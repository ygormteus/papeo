//! Comandos chamados pela interface (webview `ui`). Os sites dos serviços não
//! alcançam nenhum deles (ver `capabilities/`).

use crate::state::{now_secs, publish, snapshot, AppState, Area, LockMethod, Snapshot};
use crate::webviews::{activate, apply_layout, close_all, hibernate};
use papeo_core::i18n::{tr, trf};
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
        Err(tr("Desbloqueie o Papeo primeiro.").into())
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
        // E-mail: começa pela configuração do IMAP (leve), não pelo site.
        if service.kind.supports_mail_watch() || service.kind == ServiceKind::Outlook {
            inner.mail_setup.insert(service.id.clone());
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
            .ok_or(tr("Serviço não encontrado."))?;
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
    crate::mail::forget(&app, &id);
    crate::mail::sync(&app);
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
            .ok_or(tr("Serviço não encontrado."))?;
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

/// Avisos por IMAP de uma caixa de e-mail.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailInput {
    host: String,
    port: Option<u16>,
    user: String,
    /// Vazia: mantém a senha guardada (só se servidor e usuário não mudaram).
    password: Option<String>,
    sleep_site: bool,
    #[serde(default)]
    starttls: bool,
    #[serde(default)]
    whole_inbox: bool,
}

/// Liga, altera (`Some`) ou desliga (`None`) os avisos por IMAP da conta.
/// A senha vai para o cofre do sistema (ou arquivo só do usuário), nunca
/// para `services.json`.
#[tauri::command]
pub async fn set_mail_account(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    account: Option<MailInput>,
) -> CmdResult {
    ensure_unlocked(&app)?;
    let (kind, previous) = {
        let inner = state.lock();
        let service = inner.service(&id)?;
        (service.kind, service.mail.clone())
    };
    // O site dormia pelo IMAP: volta a avisar quando o IMAP sai de cena.
    let slept = previous.as_ref().is_some_and(|m| m.sleep_site);
    let Some(input) = account else {
        state.update(|inner| {
            inner.service_mut(&id)?.mail = None;
            Ok(())
        })?;
        crate::mail::forget(&app, &id);
        crate::mail::sync(&app);
        {
            let mut inner = state.lock();
            inner.site_view.remove(&id);
            inner.mail_setup.remove(&id);
        }
        // Sem o leitor, a conta aberta volta a ser o site.
        if state.lock().active.as_deref() == Some(id.as_str()) {
            return crate::webviews::activate(&app, Some(id));
        }
        if slept {
            crate::mail::site_fallback(&app, &id, false);
        }
        return Ok(publish(&app));
    };
    // Login da Microsoft: por aqui só muda o site hibernado (servidor e
    // usuário vêm do login, em `mail_microsoft_login`).
    if let Some(mut account) = previous
        .clone()
        .filter(|m| m.auth == papeo_core::mail::MailAuth::Microsoft)
    {
        account.sleep_site = input.sleep_site;
        state.update(|inner| {
            inner.service_mut(&id)?.mail = Some(account.clone());
            Ok(())
        })?;
        crate::mail::sync(&app);
        if slept && !account.sleep_site {
            crate::mail::site_fallback(&app, &id, false);
        }
        return Ok(publish(&app));
    }
    if !kind.supports_mail_watch() {
        return Err(tr("Esta conta ainda não aceita avisos por IMAP.").into());
    }
    let account = papeo_core::mail::MailAccount {
        host: input.host,
        port: input.port.unwrap_or(if input.starttls {
            papeo_core::mail::IMAP_PORT
        } else {
            papeo_core::mail::IMAPS_PORT
        }),
        user: input.user,
        sleep_site: input.sleep_site,
        starttls: input.starttls,
        whole_inbox: input.whole_inbox,
        auth: papeo_core::mail::MailAuth::Password,
        signature: previous
            .as_ref()
            .map(|m| m.signature.clone())
            .unwrap_or_default(),
    }
    .sanitized()?;
    let secret = input.password.filter(|p| !p.is_empty());
    match &secret {
        Some(secret) => papeo_core::mail::check_secret(secret)?,
        None => {
            // Fora do lock: arquivo ou cofre do sistema.
            let kept = crate::secrets::read(&app, &id).is_some_and(|s| s.matches(&account));
            if !kept {
                return Err(tr(
                    "Digite a senha: ela vale só para o servidor e o usuário em que foi informada.",
                )
                .into());
            }
        }
    }
    let sleeps = account.sleep_site;
    // Conta primeiro, senha depois: uma sincronização no meio não apaga a
    // senha nova por ver a conta ainda sem IMAP.
    state.update(|inner| {
        inner.service_mut(&id)?.mail = Some(account.clone());
        Ok(())
    })?;
    if let Some(secret) = secret {
        let saved = crate::secrets::Saved {
            host: account.host.clone(),
            port: account.port,
            user: account.user.clone(),
            secret,
        };
        if let Err(err) = crate::secrets::write(&app, &id, &saved) {
            let _ = state.update(|inner| {
                inner.service_mut(&id)?.mail = previous;
                Ok(())
            });
            crate::mail::sync(&app);
            return Err(err);
        }
    }
    // Salvar de novo tenta já, mesmo com a mesma senha.
    crate::mail::restart(&app, &id);
    if slept && !sleeps {
        crate::mail::site_fallback(&app, &id, false);
    }
    Ok(publish(&app))
}

/// Um login da Microsoft por vez (cada um espera o navegador até 5 min).
static MICROSOFT_LOGIN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

struct LoginBusy;

impl LoginBusy {
    fn take() -> Result<Self, String> {
        use std::sync::atomic::Ordering;
        MICROSOFT_LOGIN
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| LoginBusy)
            .map_err(|_| tr("Já há um login da Microsoft aberto no navegador.").to_owned())
    }
}

impl Drop for LoginBusy {
    fn drop(&mut self) {
        MICROSOFT_LOGIN.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Avisos do Outlook por IMAP com o login da Microsoft: abre o navegador,
/// guarda o token de renovação (nunca a senha) e liga o vigia.
#[tauri::command]
pub async fn mail_microsoft_login(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    sleep_site: bool,
) -> CmdResult {
    use papeo_core::mail::{MailAccount, MailAuth, IMAPS_PORT, OUTLOOK_HOST};
    ensure_unlocked(&app)?;
    let kind = state.lock().service(&id)?.kind;
    if !kind.accepts_mail(MailAuth::Microsoft) {
        return Err(tr("O login da Microsoft vale só para contas do Outlook.").into());
    }
    let _busy = LoginBusy::take()?;
    let opener = app.clone();
    let code = tauri::async_runtime::spawn_blocking(move || {
        crate::microsoft::login(|url| {
            use tauri_plugin_opener::OpenerExt;
            opener
                .opener()
                .open_url(url, None::<&str>)
                .map_err(|e| e.to_string())
        })
    })
    .await
    .map_err(|e| e.to_string())??;
    let tokens = crate::microsoft::exchange(code).await?;
    let refresh = tokens.refresh_token.filter(|t| !t.is_empty()).ok_or(tr(
        "A Microsoft não liberou o acesso contínuo. Tente de novo.",
    ))?;
    let user = tokens
        .id_token
        .as_deref()
        .and_then(crate::microsoft::account_email)
        .ok_or(tr(
            "A Microsoft não informou o e-mail da conta. Tente de novo.",
        ))?;
    // O login levou minutos: confere de novo o bloqueio e a conta.
    ensure_unlocked(&app)?;
    let previous = state.lock().service(&id)?.mail.clone();
    let slept = previous.as_ref().is_some_and(|m| m.sleep_site);
    let account = MailAccount {
        host: OUTLOOK_HOST.to_owned(),
        port: IMAPS_PORT,
        user,
        sleep_site,
        starttls: false,
        whole_inbox: false,
        auth: MailAuth::Microsoft,
        signature: previous
            .as_ref()
            .map(|m| m.signature.clone())
            .unwrap_or_default(),
    }
    .sanitized()?;
    // Conta primeiro, token depois (como em `set_mail_account`).
    state.update(|inner| {
        inner.service_mut(&id)?.mail = Some(account.clone());
        Ok(())
    })?;
    let saved = crate::secrets::Saved {
        host: account.host.clone(),
        port: account.port,
        user: account.user.clone(),
        secret: refresh,
    };
    if let Err(err) = crate::secrets::write(&app, &id, &saved) {
        let _ = state.update(|inner| {
            inner.service_mut(&id)?.mail = previous;
            Ok(())
        });
        crate::mail::sync(&app);
        return Err(err);
    }
    // Conta removida ou com os dados apagados enquanto gravava: o `forget`
    // dela pode ter vindo antes desta gravação.
    let still = state.lock().service(&id).is_ok_and(|s| {
        s.mail
            .as_ref()
            .is_some_and(|m| m.auth == MailAuth::Microsoft && m.user == saved.user)
    });
    if !still {
        crate::secrets::delete(&app, &id);
        crate::mail::sync(&app);
        return Err(tr("A conta mudou durante o login. Tente de novo.").into());
    }
    crate::mail::restart(&app, &id);
    if slept && !sleep_site {
        crate::mail::site_fallback(&app, &id, false);
    }
    Ok(publish(&app))
}

/// Desiste do login da Microsoft aberto no navegador (diálogo fechado).
#[tauri::command]
pub fn mail_microsoft_cancel() {
    crate::microsoft::cancel_login();
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
    if crate::webviews::restart(&app, &id)? {
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
    crate::silence::check(&app);
    Ok(publish(&app))
}

/// Fecha o cartão do resumo do silêncio (spec 17).
#[tauri::command]
pub async fn dismiss_silence_summary(app: AppHandle, state: State<'_, AppState>) -> CmdResult {
    ensure_unlocked(&app)?;
    state.lock().silence_summary = None;
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
    crate::silence::check(&app);
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
    diagnostics: Option<bool>,
    mail_undo_secs: Option<u8>,
    silence_summary: Option<bool>,
    voice_capture: Option<papeo_core::VoiceCapture>,
    language: Option<papeo_core::Language>,
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
        if let Some(v) = prefs.diagnostics {
            s.diagnostics = v;
        }
        if let Some(v) = prefs.voice_capture {
            s.voice_capture = v;
        }
        if let Some(v) = prefs.language {
            s.language = v;
        }
        if let Some(v) = prefs.silence_summary {
            s.silence_summary = v;
            // Desligado: o cartão do último resumo sai também.
            if !v {
                inner.silence_summary = None;
            }
        }
        if let Some(v) = prefs.mail_undo_secs {
            s.mail_undo_secs = v;
            *s = std::mem::take(s).sanitized();
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
    if let Some(v) = prefs.language {
        let before = papeo_core::i18n::lang();
        if crate::lang::apply(v) != before {
            crate::notifications::relabel();
            crate::preview::relabel(&app);
            crate::attachment::relabel(&app);
            // Menu do macOS na thread principal, fora do lock do estado; a
            // bandeja refaz o menu no `publish`.
            #[cfg(target_os = "macos")]
            {
                let handle = app.clone();
                let _ = app.run_on_main_thread(move || {
                    let _ = crate::menu::rebuild(&handle);
                });
            }
        }
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
    .map_err(|e| {
        trf(
            "Não foi possível alterar o início automático: {erro}",
            &[("erro", &e)],
        )
    })?;
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
                return Err(tr("Escolha uma pasta existente.").into());
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
        .map_err(|_| tr("O arquivo não está mais nessa pasta.").to_owned())
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
    // Espera a thread principal: fora dos trabalhadores do runtime.
    tauri::async_runtime::spawn_blocking(move || crate::memory::report(&app))
        .await
        .unwrap_or_default()
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
    tauri::async_runtime::spawn_blocking(move || {
        papeo_core::export_backup(&PathBuf::from(path), &config, &organizer)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Lê e confere o backup fora dos trabalhadores do runtime (arquivo de até
/// alguns MB, talvez num disco lento ou na rede).
async fn read_backup(path: String) -> CmdResult<papeo_core::Backup> {
    tauri::async_runtime::spawn_blocking(move || {
        papeo_core::read_backup(&PathBuf::from(path), crate::organizer::now_ms())
    })
    .await
    .map_err(|e| e.to_string())?
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
    let backup = read_backup(path).await?;
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
    let backup = read_backup(path).await?;
    let config = backup.config;
    // Tarefas primeiro (grava e troca na memória sob o mesmo lock, antes de
    // fechar as contas e mexer na configuração): se falhar, nada mudou.
    // Backup sem tarefas (versão antiga): as atuais continuam.
    // Grava pela mesma fila das outras gravações (ainda sob o lock): uma
    // versão anterior já preparada não sobrescreve a restaurada.
    if let Some(organizer) = backup.organizer {
        let mut inner = state.lock();
        // Bloqueado enquanto lia o arquivo: não restaura.
        if inner.locked {
            return Err(tr("Desbloqueie o Papeo primeiro.").into());
        }
        if let Some(err) = &inner.organizer_error {
            return Err(err.clone());
        }
        let old = std::mem::replace(&mut inner.organizer, organizer);
        if let Err(err) = state.stage_organizer(&inner).and_then(|s| state.write(s)) {
            inner.organizer = old;
            return Err(err);
        }
    }
    close_all(&app);
    let removed = state.update(|inner| {
        // Contas que somem com a restauração (ou voltam sem IMAP) levam a
        // senha do IMAP junto.
        let removed: Vec<String> = inner
            .config
            .services
            .iter()
            .filter(|old| {
                !config
                    .services
                    .iter()
                    .any(|new| new.id == old.id && new.mail.is_some())
            })
            .map(|old| old.id.clone())
            .collect();
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
        Ok(removed)
    })?;
    for id in removed {
        crate::mail::forget(&app, &id);
    }
    crate::mail::sync(&app);
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
        // Sair da conta também desliga o IMAP e apaga a senha dele.
        service.mail = None;
        Ok((old, was_active))
    })?;
    crate::sessions::remove_later(&app, vec![old]);
    crate::mail::forget(&app, &id);
    crate::mail::sync(&app);
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
        return Err(tr("Desbloqueie o Papeo primeiro.").into());
    }
    if enabled && !was_enabled {
        match method {
            LockMethod::None => return Err(tr(crate::lock::NO_METHOD).into()),
            LockMethod::System => {
                crate::lock::confirm_identity(&app, tr("ativar o bloqueio do Papeo")).await?
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
            crate::lock::confirm_identity(&app, tr("criar um PIN do Papeo")).await?
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
        return Err(tr("Disponível só no Windows.").into());
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
    crate::lock::confirm(&app, tr("desbloquear o Papeo"), pin).await?;
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

// ---------------------------------------------------------------------------
// Leitor de e-mail (IMAP/SMTP), para contas com o e-mail configurado.

#[tauri::command]
pub async fn mail_folders(
    app: AppHandle,
    id: String,
    refresh: bool,
) -> CmdResult<Vec<papeo_core::mailbox::Folder>> {
    ensure_unlocked(&app)?;
    crate::mailbox::folders(app, id, refresh).await
}

#[tauri::command]
pub async fn mail_list(
    app: AppHandle,
    id: String,
    folder: String,
    page: u32,
    query: String,
) -> CmdResult<papeo_core::mailbox::Page> {
    ensure_unlocked(&app)?;
    crate::mailbox::list(app, id, folder, page, query).await
}

#[tauri::command]
pub async fn mail_open(
    app: AppHandle,
    id: String,
    folder: String,
    uid: u32,
    peek: Option<bool>,
) -> CmdResult<papeo_core::mailbox::MessageView> {
    ensure_unlocked(&app)?;
    crate::mailbox::open(app, id, folder, uid, peek.unwrap_or(false)).await
}

#[tauri::command]
pub async fn mail_set_flag(
    app: AppHandle,
    id: String,
    folder: String,
    uids: Vec<u32>,
    flag: String,
    on: bool,
) -> CmdResult<()> {
    ensure_unlocked(&app)?;
    crate::mailbox::set_flag(app, id, folder, uids, flag, on).await
}

#[tauri::command]
pub async fn mail_move(
    app: AppHandle,
    id: String,
    folder: String,
    uids: Vec<u32>,
    target: String,
) -> CmdResult<()> {
    ensure_unlocked(&app)?;
    crate::mailbox::move_to(app, id, folder, uids, target).await
}

#[tauri::command]
pub async fn mail_delete(
    app: AppHandle,
    id: String,
    folder: String,
    uids: Vec<u32>,
) -> CmdResult<()> {
    ensure_unlocked(&app)?;
    crate::mailbox::delete(app, id, folder, uids).await
}

#[tauri::command]
pub async fn mail_save_attachment(
    app: AppHandle,
    id: String,
    folder: String,
    uid: u32,
    index: u32,
) -> CmdResult<String> {
    ensure_unlocked(&app)?;
    crate::mailbox::save_attachment(app, id, folder, uid, index).await
}

#[tauri::command]
pub async fn mail_send(app: AppHandle, id: String, draft: crate::mailbox::Draft) -> CmdResult<()> {
    ensure_unlocked(&app)?;
    crate::mailbox::send(app, id, draft).await
}

/// Salva o rascunho na pasta Rascunhos do servidor (spec 18).
#[tauri::command]
pub async fn mail_save_draft(
    app: AppHandle,
    id: String,
    draft: crate::mailbox::Draft,
) -> CmdResult<String> {
    // Sem conferir o bloqueio: o editor salva o que já estava escrito quando
    // o app bloqueia (a tela some); não abre nem mostra nada.
    crate::mailbox::save_draft(app, id, draft).await
}

/// Descarta o rascunho (apaga as versões salvas no servidor).
#[tauri::command]
pub async fn mail_discard_draft(
    app: AppHandle,
    id: String,
    token: Option<String>,
    replaces: Option<crate::mailbox::DraftRef>,
) -> CmdResult<()> {
    ensure_unlocked(&app)?;
    crate::mailbox::discard_draft(app, id, token, replaces).await
}

/// Abre uma mensagem da pasta Rascunhos no editor.
#[tauri::command]
pub async fn mail_open_draft(
    app: AppHandle,
    id: String,
    folder: String,
    uid: u32,
) -> CmdResult<crate::mailbox::OpenedDraft> {
    ensure_unlocked(&app)?;
    crate::mailbox::open_draft(app, id, folder, uid).await
}

/// Envio com espera para desfazer (spec 16): confere, guarda e agenda.
#[tauri::command]
pub async fn mail_send_later(
    app: AppHandle,
    id: String,
    draft: crate::mailbox::Draft,
) -> CmdResult<crate::outbox::Hold> {
    ensure_unlocked(&app)?;
    crate::outbox::hold(&app, id, draft)
}

/// Desfaz um envio em espera; `None` se ele já saiu.
#[tauri::command]
pub async fn mail_send_cancel(
    app: AppHandle,
    hold_id: String,
) -> CmdResult<Option<crate::mailbox::Draft>> {
    ensure_unlocked(&app)?;
    crate::outbox::cancel(&app, &hold_id)
}

/// E-mails guardados da conta que ainda não voltaram ao editor (desfeitos,
/// com falha ou que ficaram na espera quando o app fechou).
#[tauri::command]
pub async fn mail_unsent(app: AppHandle, id: String) -> CmdResult<Vec<crate::outbox::Unsent>> {
    ensure_unlocked(&app)?;
    crate::outbox::unsent(&app, &id)
}

/// O e-mail guardado voltou ao editor: apaga o arquivo.
#[tauri::command]
pub async fn mail_unsent_taken(app: AppHandle, hold_id: String) -> CmdResult<()> {
    ensure_unlocked(&app)?;
    crate::outbox::taken(&app, &hold_id)
}

/// Assinatura do leitor de e-mail da conta (spec 16).
#[tauri::command]
pub async fn set_mail_signature(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    signature: String,
) -> CmdResult {
    ensure_unlocked(&app)?;
    state.update(|inner| {
        let mail = inner
            .service_mut(&id)?
            .mail
            .as_mut()
            .ok_or(tr("Esta conta não tem o e-mail por IMAP configurado."))?;
        mail.signature = papeo_core::mail::normalize_signature(&signature);
        Ok(())
    })?;
    Ok(publish(&app))
}

/// Conta com o leitor: mostra o site (`true`) ou volta ao leitor.
#[tauri::command]
pub async fn mail_show_site(app: AppHandle, id: String, site: bool) -> CmdResult {
    ensure_unlocked(&app)?;
    crate::webviews::show_site(&app, &id, site)
}

/// Mostra um arquivo salvo pelo leitor (anexo) na pasta dele.
#[tauri::command]
pub async fn mail_reveal_file(app: AppHandle, path: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let path = std::path::PathBuf::from(path);
    // Só o que está na pasta de downloads (onde o leitor salva).
    let dir = crate::downloads::downloads_dir(&app);
    let inside = path
        .canonicalize()
        .ok()
        .zip(dir.canonicalize().ok())
        .is_some_and(|(p, d)| p.starts_with(d));
    if !inside {
        return Err(tr("Arquivo fora da pasta de downloads.").into());
    }
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| e.to_string())
}
