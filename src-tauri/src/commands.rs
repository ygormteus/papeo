//! Comandos chamados pela interface (webview `ui`). Os sites dos serviços não
//! alcançam nenhum deles (ver `capabilities/`).

use crate::state::{now_secs, publish, snapshot, AppState, Area, LockMethod, Snapshot};
use crate::webviews::{activate, apply_layout, close_all, hibernate};
use papeo_core::{
    AppLock, MediaPolicy, NotificationPreview, QuietHours, Service, ServiceKind, DND_FOREVER,
    LOCK_MAX_MINUTES,
};
use serde::Deserialize;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_opener::OpenerExt;

type CmdResult<T = Snapshot> = Result<T, String>;

/// Com o app bloqueado, a interface só oferece "Desbloquear"; o backend
/// também recusa mudanças e acesso a arquivos (defesa em profundidade).
fn ensure_unlocked(app: &AppHandle) -> CmdResult<()> {
    if app.state::<AppState>().lock().locked {
        Err("Desbloqueie o Papeo primeiro.".into())
    } else {
        Ok(())
    }
}

#[tauri::command]
pub async fn get_state(app: AppHandle) -> Snapshot {
    snapshot(&app)
}

#[tauri::command]
pub async fn add_service(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: ServiceKind,
    name: Option<String>,
    url: Option<String>,
) -> CmdResult {
    ensure_unlocked(&app)?;
    let auto_name = name.as_deref().is_none_or(|n| n.trim().is_empty());
    let mut service = Service::new(kind, name, url)?;
    if !auto_name {
        service.name = papeo_core::normalize_name(&service.name)?;
    }
    let id = service.id.clone();
    state.update(|inner| {
        if auto_name {
            // Segunda conta do mesmo mensageiro: "WhatsApp 2", "WhatsApp 3"…
            service.name = papeo_core::unique_name(&service.name, &inner.config.services);
        }
        inner.config.services.push(service);
        Ok(())
    })?;
    activate(&app, Some(id))
}

#[tauri::command]
pub async fn remove_service(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult {
    ensure_unlocked(&app)?;
    hibernate(&app, &id);
    let removed = state.update(|inner| {
        let pos = inner
            .config
            .services
            .iter()
            .position(|s| s.id == id)
            .ok_or("Serviço não encontrado.")?;
        if inner.active.as_deref() == Some(id.as_str()) {
            inner.active = None;
        }
        inner.last_used.remove(&id);
        inner.forget_recent(&id);
        Ok(inner.config.services.remove(pos))
    })?;
    // Apaga cookies e dados locais da sessão (pasta e, no macOS, o
    // armazenamento do WebKit), com novas tentativas se ainda estiver em uso.
    crate::sessions::remove_later(&app, vec![removed.session]);
    apply_layout(&app);
    Ok(publish(&app))
}

/// Nome, cor e emoji do serviço (para diferenciar contas do mesmo mensageiro).
#[tauri::command]
pub async fn customize_service(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    name: String,
    color: Option<String>,
    emoji: Option<String>,
    // Trabalho/pessoal; sem valor, a conta aparece em todos os modos.
    context: Option<papeo_core::ServiceContext>,
) -> CmdResult {
    ensure_unlocked(&app)?;
    let name = papeo_core::normalize_name(&name)?;
    let color = papeo_core::normalize_color(color.as_deref())?;
    let emoji = papeo_core::normalize_emoji(emoji.as_deref())?;
    state.update(|inner| {
        let service = inner.service_mut(&id)?;
        service.name = name;
        service.color = color;
        service.emoji = emoji;
        service.context = context;
        Ok(())
    })?;
    Ok(publish(&app))
}

#[tauri::command]
pub async fn move_service(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    to: usize,
) -> CmdResult {
    ensure_unlocked(&app)?;
    state.update(|inner| {
        let services = &mut inner.config.services;
        let from = services
            .iter()
            .position(|s| s.id == id)
            .ok_or("Serviço não encontrado.")?;
        let service = services.remove(from);
        let to = to.min(services.len());
        services.insert(to, service);
        Ok(())
    })?;
    Ok(publish(&app))
}

#[tauri::command]
pub async fn set_hibernation(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    minutes: u32,
) -> CmdResult {
    state.update(|inner| {
        inner.service_mut(&id)?.hibernate_after_minutes = minutes.min(24 * 60);
        Ok(())
    })?;
    Ok(publish(&app))
}

#[tauri::command]
pub async fn set_notifications(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> CmdResult {
    state.update(|inner| {
        let service = inner.service_mut(&id)?;
        service.notifications = enabled;
        if enabled {
            service.muted_until = None;
        }
        Ok(())
    })?;
    Ok(publish(&app))
}

/// Silencia um serviço por `minutes` (0 = remove o silêncio temporário).
#[tauri::command]
pub async fn mute_service(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    minutes: u32,
) -> CmdResult {
    state.update(|inner| {
        inner.service_mut(&id)?.muted_until =
            (minutes > 0).then(|| now_secs() + u64::from(minutes.min(7 * 24 * 60)) * 60);
        Ok(())
    })?;
    Ok(publish(&app))
}

#[tauri::command]
pub async fn set_media_policy(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    policy: MediaPolicy,
) -> CmdResult {
    ensure_unlocked(&app)?;
    state.update(|inner| {
        inner.service_mut(&id)?.media = Some(policy);
        Ok(())
    })?;
    Ok(publish(&app))
}

/// Zoom absoluto (`zoom`) ou relativo (`delta`, ex.: +0.1).
#[tauri::command]
pub async fn set_zoom(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    zoom: Option<f64>,
    delta: Option<f64>,
) -> CmdResult {
    let zoom = state.update(|inner| {
        let service = inner.service_mut(&id)?;
        let target = zoom.unwrap_or(service.zoom + delta.unwrap_or(0.0));
        service.zoom = papeo_core::clamp_zoom(target);
        Ok(service.zoom)
    })?;
    if let Some(webview) = app.get_webview(&id) {
        webview.set_zoom(zoom).map_err(|e| e.to_string())?;
    }
    Ok(publish(&app))
}

#[tauri::command]
pub async fn activate_service(app: AppHandle, id: Option<String>) -> CmdResult {
    activate(&app, id)
}

/// Resposta rápida (mini janela): envia sem trazer o app para frente.
#[tauri::command]
pub async fn send_reply(app: AppHandle, id: String, chat: String, text: String) -> CmdResult {
    // Espera o site enviar: fora da thread principal e do executor.
    tauri::async_runtime::spawn_blocking(move || crate::reply::send(&app, &id, &chat, &text))
        .await
        .map_err(|e| e.to_string())?
}

/// Da mini janela para o app: mostra a janela principal e, se pedido, a
/// conta ou a conversa. Bloqueado, mostra só a tela de desbloqueio.
#[tauri::command]
pub async fn show_app(app: AppHandle, id: Option<String>, chat: Option<String>) -> CmdResult {
    crate::popover::hide(&app);
    crate::tray::show_main(&app);
    if app.state::<AppState>().lock().locked {
        return Ok(snapshot(&app));
    }
    match (id, chat) {
        (Some(id), Some(chat)) => crate::webviews::open_chat(&app, &id, &chat, true),
        (Some(id), None) => activate(&app, Some(id)),
        _ => Ok(snapshot(&app)),
    }
}

#[tauri::command]
pub async fn hide_popover(app: AppHandle) {
    crate::popover::hide(&app);
}

/// Abre o serviço e procura a conversa pelo nome (busca do próprio site).
#[tauri::command]
pub async fn open_chat(app: AppHandle, id: String, query: String, exact: bool) -> CmdResult {
    ensure_unlocked(&app)?;
    // Abrir pela lista (nome exato) conta como ler.
    if exact {
        state_mark_read(&app, &id, &query);
    }
    crate::webviews::open_chat(&app, &id, &query, exact)
}

fn state_mark_read(app: &AppHandle, id: &str, chat: &str) {
    app.state::<AppState>().lock().mark_read(id, chat);
}

#[tauri::command]
pub async fn hibernate_service(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult {
    let was_active = state.lock().active.as_deref() == Some(id.as_str());
    hibernate(&app, &id);
    if was_active {
        return activate(&app, None);
    }
    Ok(publish(&app))
}

#[tauri::command]
pub async fn reload_service(app: AppHandle, id: String) -> CmdResult<()> {
    if crate::webviews::reload(&app, &id)? {
        publish(&app);
    }
    Ok(())
}

#[tauri::command]
pub async fn set_content_area(
    app: AppHandle,
    state: State<'_, AppState>,
    area: Area,
) -> CmdResult<()> {
    if !(area.width.is_finite() && area.height.is_finite()) || area.width < 1.0 || area.height < 1.0
    {
        return Ok(());
    }
    state.lock().area = area;
    apply_layout(&app);
    Ok(())
}

#[tauri::command]
pub async fn set_overlay(app: AppHandle, state: State<'_, AppState>, open: bool) -> CmdResult<()> {
    state.lock().overlay = open;
    apply_layout(&app);
    if !open {
        let (active, locked) = {
            let inner = state.lock();
            (inner.active.clone(), inner.locked)
        };
        // Bloqueado: o teclado fica na interface (tela de desbloqueio).
        let target = if locked { None } else { active };
        if let Some(webview) = target.and_then(|id| app.get_webview(&id)) {
            let _ = webview.set_focus();
        } else if let Some(ui) = app.get_webview(crate::state::UI_WEBVIEW) {
            let _ = ui.set_focus();
        }
    }
    Ok(())
}

/// Não perturbe global: `None` desliga, `Some(0)` até reativar, `Some(n)` por n minutos.
#[tauri::command]
pub async fn set_dnd(
    app: AppHandle,
    state: State<'_, AppState>,
    minutes: Option<u32>,
) -> CmdResult {
    state.update(|inner| {
        inner.config.settings.dnd_until = match minutes {
            None => None,
            Some(0) => Some(DND_FOREVER),
            Some(m) => Some(now_secs() + u64::from(m.min(7 * 24 * 60)) * 60),
        };
        Ok(())
    })?;
    Ok(publish(&app))
}

#[tauri::command]
pub async fn set_quiet_hours(
    app: AppHandle,
    state: State<'_, AppState>,
    quiet_hours: QuietHours,
) -> CmdResult {
    state.update(|inner| {
        inner.config.settings.quiet_hours = quiet_hours.sanitized();
        Ok(())
    })?;
    Ok(publish(&app))
}

/// Troca rápida de modo (barra lateral, atalho, bandeja).
#[tauri::command]
pub async fn set_mode(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: papeo_core::Mode,
) -> CmdResult {
    state.update(|inner| {
        crate::state::choose_mode(inner, mode);
        Ok(())
    })?;
    Ok(publish(&app))
}

/// Escolha do modo e expediente (Configurações); desfaz a troca manual.
#[tauri::command]
pub async fn set_mode_schedule(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: papeo_core::ModeChoice,
    work_hours: papeo_core::WorkHours,
) -> CmdResult {
    ensure_unlocked(&app)?;
    state.update(|inner| {
        inner.config.settings.mode = mode;
        inner.config.settings.work_hours = work_hours.sanitized();
        inner.mode_override = None;
        Ok(())
    })?;
    Ok(publish(&app))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    close_to_tray: Option<bool>,
    start_hidden: Option<bool>,
    open_on_notification_focus: Option<bool>,
    notification_preview: Option<NotificationPreview>,
    protect_screen: Option<bool>,
    reload_above_mb: Option<u32>,
    open_links: Option<papeo_core::LinkTarget>,
    attachments: Option<papeo_core::AttachmentMode>,
    sleep_outside_mode: Option<bool>,
    account_frame: Option<bool>,
    voice_capture: Option<papeo_core::VoiceCapture>,
}

#[tauri::command]
pub async fn set_preferences(
    app: AppHandle,
    state: State<'_, AppState>,
    prefs: Preferences,
) -> CmdResult {
    ensure_unlocked(&app)?;
    state.update(|inner| {
        let s = &mut inner.config.settings;
        if let Some(v) = prefs.close_to_tray {
            s.close_to_tray = v;
        }
        if let Some(v) = prefs.start_hidden {
            s.start_hidden = v;
        }
        if let Some(v) = prefs.open_on_notification_focus {
            s.open_on_notification_focus = v;
        }
        if let Some(v) = prefs.notification_preview {
            s.notification_preview = v;
        }
        if let Some(v) = prefs.protect_screen {
            s.protect_screen = v;
        }
        if let Some(v) = prefs.open_links {
            s.open_links = v;
        }
        if let Some(v) = prefs.attachments {
            s.attachments = v;
        }
        if let Some(v) = prefs.sleep_outside_mode {
            s.sleep_outside_mode = v;
        }
        if let Some(v) = prefs.account_frame {
            s.account_frame = v;
        }
        if let Some(v) = prefs.voice_capture {
            s.voice_capture = v;
        }
        if let Some(v) = prefs.reload_above_mb {
            s.reload_above_mb = v;
            *s = std::mem::take(s).sanitized();
        }
        Ok(())
    })?;
    if let Some(v) = prefs.protect_screen {
        if let Some(window) = app.get_window(crate::state::MAIN_WINDOW) {
            window.set_content_protected(v).map_err(|e| e.to_string())?;
        }
        for label in [crate::preview::PREVIEW, crate::attachment::ATTACHMENT] {
            if let Some(window) = app.get_window(label) {
                let _ = window.set_content_protected(v);
            }
        }
    }
    if prefs.open_links == Some(papeo_core::LinkTarget::Browser) {
        crate::preview::close(&app);
    }
    if let Some(v) = prefs.voice_capture {
        crate::webviews::set_voice_capture(&app, v);
    }
    Ok(publish(&app))
}

/// Respostas prontas (lista inteira, já na ordem desejada).
#[tauri::command]
pub async fn set_quick_replies(
    app: AppHandle,
    state: State<'_, AppState>,
    replies: Vec<papeo_core::QuickReply>,
) -> CmdResult {
    ensure_unlocked(&app)?;
    state.update(|inner| {
        let s = &mut inner.config.settings;
        s.quick_replies = replies;
        *s = std::mem::take(s).sanitized();
        Ok(())
    })?;
    Ok(publish(&app))
}

#[tauri::command]
pub async fn set_autostart(app: AppHandle, state: State<'_, AppState>, enabled: bool) -> CmdResult {
    ensure_unlocked(&app)?;
    let launcher = app.autolaunch();
    if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    }
    .map_err(|e| format!("Não foi possível alterar o início automático: {e}"))?;
    state.lock().autostart = launcher.is_enabled().unwrap_or(enabled);
    Ok(publish(&app))
}

/// `path` vem do seletor de pastas nativo; `None` volta à pasta Downloads.
#[tauri::command]
pub async fn set_downloads_dir(
    app: AppHandle,
    state: State<'_, AppState>,
    path: Option<String>,
) -> CmdResult {
    ensure_unlocked(&app)?;
    let dir = match path.map(|p| p.trim().to_owned()).filter(|p| !p.is_empty()) {
        Some(p) => {
            let dir = PathBuf::from(&p);
            if !dir.is_absolute() || !dir.is_dir() {
                return Err("Escolha uma pasta existente.".into());
            }
            Some(p)
        }
        None => None,
    };
    state.update(|inner| {
        inner.config.settings.downloads_dir = dir;
        Ok(())
    })?;
    Ok(publish(&app))
}

#[tauri::command]
pub async fn reveal_download(app: AppHandle, id: u64) -> CmdResult<()> {
    ensure_unlocked(&app)?;
    let path = crate::downloads::path_of(&app, id)?;
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|_| "O arquivo não está mais nessa pasta.".to_owned())
}

#[tauri::command]
pub async fn open_downloads_dir(app: AppHandle) -> CmdResult<()> {
    ensure_unlocked(&app)?;
    let dir = crate::downloads::downloads_dir(&app);
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_downloads(app: AppHandle, state: State<'_, AppState>) -> CmdResult {
    state
        .lock()
        .downloads
        .retain(|d| d.state == crate::state::DownloadState::Downloading);
    Ok(publish(&app))
}

#[tauri::command]
pub async fn get_memory(app: AppHandle) -> crate::memory::MemoryReport {
    crate::memory::report(&app)
}

/// `path` vem do diálogo nativo "Salvar como".
#[tauri::command]
pub async fn export_config(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<()> {
    ensure_unlocked(&app)?;
    let (config, organizer) = {
        let inner = state.lock();
        // Tarefas não lidas: um backup sem elas apagaria as de outro computador.
        if let Some(err) = &inner.organizer_error {
            return Err(err.clone());
        }
        (inner.config.clone(), inner.organizer.clone())
    };
    papeo_core::export_backup(&PathBuf::from(path), &config, &organizer)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupPreview {
    services: Vec<String>,
    /// `None`: backup sem tarefas (de uma versão antiga); as atuais ficam.
    tasks: Option<usize>,
    notes: Option<usize>,
}

#[tauri::command]
pub async fn preview_backup(app: AppHandle, path: String) -> CmdResult<BackupPreview> {
    ensure_unlocked(&app)?;
    let backup = papeo_core::read_backup(&PathBuf::from(path), crate::organizer::now_ms())?;
    Ok(BackupPreview {
        services: backup.config.services.into_iter().map(|s| s.name).collect(),
        tasks: backup.organizer.as_ref().map(|o| o.open_count()),
        notes: backup.organizer.as_ref().map(|o| o.note_count()),
    })
}

/// Substitui serviços e configurações pelos do backup. Sessões que já
/// existirem neste computador continuam logadas; as outras pedem login.
#[tauri::command]
pub async fn import_config(app: AppHandle, state: State<'_, AppState>, path: String) -> CmdResult {
    ensure_unlocked(&app)?;
    let backup = papeo_core::read_backup(&PathBuf::from(path), crate::organizer::now_ms())?;
    let config = backup.config;
    if backup.organizer.is_some() {
        if let Some(err) = &state.lock().organizer_error {
            return Err(err.clone());
        }
    }
    close_all(&app);
    state.update(|inner| {
        // Tarefas primeiro (grava e troca na memória sob o mesmo lock, antes
        // de mexer na configuração): se falhar, nada mudou. Backup sem
        // tarefas (versão antiga): as atuais continuam.
        // Grava pela mesma fila das outras gravações (ainda sob o lock): uma
        // versão anterior já preparada não sobrescreve a restaurada.
        if let Some(organizer) = backup.organizer {
            let old = std::mem::replace(&mut inner.organizer, organizer);
            if let Err(err) = state.stage_organizer(inner).and_then(|s| state.write(s)) {
                inner.organizer = old;
                return Err(err);
            }
        }
        // A limpeza pendente é deste computador; o backup não a traz.
        let pending = std::mem::take(&mut inner.config.pending_removal);
        // O aviso e a escolha sobre atualizações valem para este computador.
        let updates = inner.config.settings.updates;
        // O bloqueio também: o backup nunca traz o PIN, e restaurar não
        // pode desligar a proteção deste computador.
        let lock = inner.config.settings.lock;
        let lock_pin = inner.config.lock_pin.take();
        inner.config = config;
        // Sessão na fila de remoção (ex.: saída por "Esqueci o PIN") não
        // volta a ser usada: a conta recebe uma sessão nova.
        for service in &mut inner.config.services {
            if pending.contains(&service.session) {
                service.session = papeo_core::new_session();
            }
        }
        inner.config.pending_removal = pending;
        inner.config.settings.updates = updates;
        inner.config.settings.lock = lock;
        inner.config.lock_pin = lock_pin;
        inner.last_used.clear();
        inner.unread.clear();
        inner.page_unread.clear();
        inner.current_chat.clear();
        Ok(())
    })?;
    crate::sync::mark_dirty(&app);
    crate::organizer::publish_organizer(&app);
    activate(&app, None)
}

/// "Sair da conta e apagar dados": fecha o serviço, troca a sessão por uma
/// nova (vazia) e apaga cookies, cache e conversas guardadas da antiga.
#[tauri::command]
pub async fn clear_service_data(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult {
    ensure_unlocked(&app)?;
    hibernate(&app, &id);
    let (old, was_active) = state.update(|inner| {
        let was_active = inner.active.as_deref() == Some(id.as_str());
        inner.forget_recent(&id);
        let service = inner.service_mut(&id)?;
        let old = std::mem::replace(&mut service.session, papeo_core::new_session());
        Ok((old, was_active))
    })?;
    crate::sessions::remove_later(&app, vec![old]);
    if was_active {
        // Reabre já na tela de login da sessão nova.
        activate(&app, Some(id))
    } else {
        Ok(publish(&app))
    }
}

/// Quantas sessões no disco não pertencem a nenhum serviço.
#[tauri::command]
pub async fn get_orphan_sessions(app: AppHandle) -> usize {
    crate::sessions::orphans(&app).await.len()
}

/// Agenda a remoção das sessões sem serviço. Retorna quantas eram.
#[tauri::command]
pub async fn clean_orphan_sessions(app: AppHandle) -> CmdResult<usize> {
    ensure_unlocked(&app)?;
    let orphans = crate::sessions::orphans(&app).await;
    let count = orphans.len();
    crate::sessions::remove_later(&app, orphans);
    Ok(count)
}

/// Liga/desliga o bloqueio. Ligar com a autenticação do sistema exige
/// autenticar antes, para provar que esta pessoa consegue desbloquear
/// depois (o PIN do Papeo já foi digitado duas vezes ao ser criado).
#[tauri::command]
pub async fn set_lock(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
    after_minutes: u32,
) -> CmdResult {
    let (was_enabled, locked, method) = {
        let inner = state.lock();
        (
            inner.config.settings.lock.enabled,
            inner.locked,
            inner.lock_method(),
        )
    };
    if locked {
        return Err("Desbloqueie o Papeo primeiro.".into());
    }
    if enabled && !was_enabled {
        match method {
            LockMethod::None => return Err(crate::lock::NO_METHOD.into()),
            LockMethod::System => {
                crate::lock::confirm_identity(&app, "ativar o bloqueio do Papeo").await?
            }
            LockMethod::Pin => {}
        }
    }
    state.update(|inner| {
        inner.config.settings.lock = AppLock {
            enabled,
            after_minutes: after_minutes.min(LOCK_MAX_MINUTES),
        };
        Ok(())
    })?;
    Ok(publish(&app))
}

#[tauri::command]
pub async fn lock_now(app: AppHandle) -> CmdResult {
    crate::lock::lock_app(&app);
    Ok(snapshot(&app))
}

/// Cria, troca (`current` = PIN atual) ou remove (`pin` = None) o PIN do
/// Papeo. Criar liga o bloqueio; remover sem autenticação do sistema o
/// desliga.
#[tauri::command]
pub async fn set_lock_pin(
    app: AppHandle,
    state: State<'_, AppState>,
    current: Option<String>,
    pin: Option<String>,
) -> CmdResult {
    ensure_unlocked(&app)?;
    if let Some(pin) = &pin {
        papeo_core::check_pin(pin)?;
    }
    let method = state.lock().lock_method();
    match method {
        LockMethod::Pin => crate::lock::verify_pin(&app, current).await?,
        LockMethod::System if pin.is_some() => {
            crate::lock::confirm_identity(&app, "criar um PIN do Papeo").await?
        }
        LockMethod::System | LockMethod::None => {}
    }
    let new_pin = match pin {
        Some(pin) => Some(
            tauri::async_runtime::spawn_blocking(move || papeo_core::LockPin::new(&pin))
                .await
                .map_err(|e| e.to_string())??,
        ),
        None => None,
    };
    state.update(|inner| {
        let creating = inner.config.lock_pin.is_none() && new_pin.is_some();
        inner.config.lock_pin = new_pin;
        inner.pin_retry_at = None;
        if creating {
            inner.config.settings.lock.enabled = true;
        } else if inner.lock_method() == LockMethod::None {
            inner.config.settings.lock.enabled = false;
        }
        Ok(())
    })?;
    Ok(publish(&app))
}

/// "Esqueci o PIN" na tela de bloqueio: sai de todas as contas e apaga as
/// tarefas deste computador antes de desbloquear (ver `lock::forget_pin`).
#[tauri::command]
pub async fn forget_lock_pin(app: AppHandle) -> CmdResult {
    crate::lock::forget_pin(&app).await?;
    activate(&app, None)
}

/// Abre as opções de entrada do Windows (para configurar o PIN do Windows
/// Hello, que serve ao bloqueio mesmo sem câmera ou leitor de digital).
#[tauri::command]
pub async fn open_signin_settings(app: AppHandle) -> CmdResult<()> {
    if !cfg!(windows) {
        return Err("Disponível só no Windows.".into());
    }
    app.opener()
        .open_url("ms-settings:signinoptions", None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn unlock(app: AppHandle, state: State<'_, AppState>, pin: Option<String>) -> CmdResult {
    if !state.lock().locked {
        return Ok(snapshot(&app));
    }
    crate::lock::confirm(&app, "desbloquear o Papeo", pin).await?;
    {
        let mut inner = state.lock();
        inner.locked = false;
        inner.unfocused_since = None;
        if let Some(health) = inner
            .active
            .clone()
            .and_then(|id| inner.health.get_mut(&id))
        {
            health.grace();
        }
    }
    apply_layout(&app);
    Ok(publish(&app))
}

/// Tema da interface e da janela (os sites seguem o claro/escuro da janela).
#[tauri::command]
pub async fn set_appearance(
    app: AppHandle,
    state: State<'_, AppState>,
    appearance: papeo_core::Appearance,
) -> CmdResult {
    state.update(|inner| {
        inner.config.settings.appearance = appearance;
        Ok(())
    })?;
    if let Some(window) = app.get_window(crate::state::MAIN_WINDOW) {
        window
            .set_theme(crate::window_theme(appearance))
            .map_err(|e| e.to_string())?;
    }
    for label in [crate::preview::PREVIEW, crate::attachment::ATTACHMENT] {
        if let Some(window) = app.get_window(label) {
            let _ = window.set_theme(crate::window_theme(appearance));
        }
    }
    Ok(publish(&app))
}

/// Verificação manual (vale mesmo com a automática desligada).
#[tauri::command]
pub async fn check_update(app: AppHandle) -> CmdResult {
    crate::updater::check(&app).await?;
    Ok(snapshot(&app))
}

/// Instala a versão nova e reinicia o app. Só por clique na interface.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> CmdResult {
    ensure_unlocked(&app)?;
    crate::updater::install(&app).await?;
    Ok(snapshot(&app))
}

/// Liga/desliga a verificação automática; também registra que o aviso foi visto.
#[tauri::command]
pub async fn set_updates(app: AppHandle, state: State<'_, AppState>, check: bool) -> CmdResult {
    ensure_unlocked(&app)?;
    state.update(|inner| {
        inner.config.settings.updates = papeo_core::UpdatePrefs {
            check,
            notice_seen: true,
        };
        Ok(())
    })?;
    Ok(publish(&app))
}
