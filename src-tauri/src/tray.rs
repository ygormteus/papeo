//! Ícone na bandeja (Windows) / menu bar (macOS), fechar para a bandeja e
//! reabrir a janela.

use crate::state::{now_secs, publish, snapshot, AppState, Snapshot, MAIN_WINDOW};
use papeo_core::i18n::{tk, tr, trf};
use papeo_core::{Mode, ModeChoice, DND_FOREVER};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Window, WindowEvent, Wry};

const TRAY_ID: &str = "papeo";
/// Mensagens recentes mostradas no menu.
const MENU_RECENT: usize = 6;

/// Estado do menu montado: assinatura (para só remontar quando algo visível
/// muda) e as conversas por posição, para o clique saber qual abrir.
#[derive(Default)]
struct TrayHandles {
    signature: Mutex<String>,
    chats: Mutex<Chats>,
}

/// (id do serviço, conversa) de cada mensagem recente do menu, na ordem.
type Chats = Vec<(String, String)>;

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_window(MAIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn manual_dnd(snap: &Snapshot) -> bool {
    snap.settings.dnd_until.is_some_and(|t| now_secs() < t)
}

fn clock(at: u64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(i64::try_from(at).unwrap_or_default(), 0)
        .single()
        .map(|t| t.format("%H:%M").to_string())
        .unwrap_or_default()
}

/// Tudo o que aparece no menu; se não mudou, o menu não é remontado.
fn signature(snap: &Snapshot) -> String {
    let services: Vec<String> = snap
        .services
        .iter()
        .map(|s| {
            format!(
                "{}|{}|{:?}|{}|{}|{}|{:?}",
                s.service.id,
                s.service.name,
                s.service.emoji,
                s.unread,
                s.loaded,
                s.in_mode,
                s.service.context
            )
        })
        .collect();
    let recent: Vec<String> = snap
        .recent
        .iter()
        .take(MENU_RECENT)
        .map(|m| format!("{}|{}|{}|{}", m.service_id, m.title, m.body, m.at))
        .collect();
    format!(
        "{:?}#{}#{:?}#{:?}#{}#{}#{}#{}#{}",
        snap.lang,
        snap.locked,
        snap.mode,
        snap.settings.mode,
        snap.total_unread,
        manual_dnd(snap),
        snap.settings.lock.enabled,
        services.join(";"),
        recent.join(";")
    )
}

fn disabled(app: &AppHandle, text: &str) -> tauri::Result<MenuItem<Wry>> {
    MenuItem::new(app, text, false, None::<&str>)
}

fn item(app: &AppHandle, id: &str, text: &str) -> tauri::Result<MenuItem<Wry>> {
    MenuItem::with_id(app, id, text, true, None::<&str>)
}

/// Monta o menu a partir do estado. Bloqueado: só contagens, sem conversas.
fn build_menu(app: &AppHandle, snap: &Snapshot) -> tauri::Result<(Menu<Wry>, Chats)> {
    let mut items: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::new();
    let sep = || PredefinedMenuItem::separator(app);

    let header = if snap.locked {
        tr("Papeo bloqueado").to_owned()
    } else {
        match snap.total_unread {
            0 => tr("Nenhuma mensagem nova").to_owned(),
            1 => tr("1 mensagem não lida").to_owned(),
            n => trf("{n} mensagens não lidas", &[("n", &n)]),
        }
    };
    items.push(Box::new(disabled(app, &header)?));

    let name_of = |id: &str| {
        snap.services
            .iter()
            .find(|s| s.service.id == id)
            .map(|s| s.service.name.clone())
            .unwrap_or_default()
    };
    let mut chats = Vec::new();
    if !snap.recent.is_empty() {
        items.push(Box::new(sep()?));
        items.push(Box::new(disabled(app, tr("Mensagens recentes"))?));
        let many = snap.services.len() > 1;
        for (i, m) in snap.recent.iter().take(MENU_RECENT).enumerate() {
            let mut label = format!("{}  {}", clock(m.at), m.title);
            if !m.body.is_empty() {
                label.push_str(" — ");
                label.push_str(&papeo_core::clip(&m.body, 40));
            }
            if many {
                label.push_str(&format!("  ({})", name_of(&m.service_id)));
            }
            items.push(Box::new(item(app, &format!("tray:chat:{i}"), &label)?));
            chats.push((m.service_id.clone(), m.chat.clone()));
        }
    }

    if snap.services.iter().any(|s| s.in_mode) {
        items.push(Box::new(sep()?));
        items.push(Box::new(disabled(app, tr("Contas"))?));
        for s in snap.services.iter().filter(|s| s.in_mode) {
            let mut label = match &s.service.emoji {
                Some(emoji) => format!("{emoji}  {}", s.service.name),
                None => s.service.name.clone(),
            };
            if s.unread > 0 {
                label.push_str(&format!(" — {}", s.unread));
            } else if !s.loaded {
                label.push_str(tr(" (dormindo)"));
            }
            items.push(Box::new(item(
                app,
                &format!("tray:svc:{}", s.service.id),
                &label,
            )?));
        }
    }

    items.push(Box::new(sep()?));
    if uses_modes(snap) {
        let mark = |mode: Mode, text: &str| {
            let text = if snap.mode == mode {
                format!("✓ {text}")
            } else {
                text.to_owned()
            };
            item(app, &format!("tray:mode:{}", mode_id(mode)), &text)
        };
        let mut title = trf("Modo: {modo}", &[("modo", &tr(mode_label(snap.mode)))]);
        if snap.settings.mode == ModeChoice::Auto {
            title.push_str(if snap.mode_manual {
                tr(" (até o expediente mudar)")
            } else {
                tr(" (automático)")
            });
        }
        items.push(Box::new(Submenu::with_items(
            app,
            &title,
            true,
            &[
                &mark(Mode::Work, tr("Trabalho"))?,
                &mark(Mode::Personal, tr("Pessoal"))?,
                &mark(Mode::All, tr("Todas as contas"))?,
            ],
        )?));
    }
    let dnd_on = manual_dnd(snap);
    let dnd = if dnd_on {
        Submenu::with_items(
            app,
            tr("Não perturbe (ligado)"),
            true,
            &[&item(app, "tray:dndoff", tr("Desligar"))?],
        )?
    } else {
        Submenu::with_items(
            app,
            tr("Não perturbe"),
            true,
            &[
                &item(app, "tray:dnd1h", tr("Por 1 hora"))?,
                &item(app, "tray:dnd8h", tr("Por 8 horas"))?,
                &item(app, "tray:dndon", tr("Até eu reativar"))?,
            ],
        )?
    };
    items.push(Box::new(dnd));
    if snap.settings.lock.enabled && !snap.locked {
        items.push(Box::new(item(app, "tray:lock", tr("Bloquear agora"))?));
    }
    items.push(Box::new(sep()?));
    items.push(Box::new(item(app, "tray:show", tr("Abrir o Papeo"))?));
    items.push(Box::new(item(app, "tray:quit", tr("Sair do Papeo"))?));

    let refs: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|i| i.as_ref()).collect();
    Ok((Menu::with_items(app, &refs)?, chats))
}

/// O submenu de modo só aparece para quem separa trabalho e pessoal.
fn uses_modes(snap: &Snapshot) -> bool {
    snap.settings.mode != ModeChoice::All
        || snap.services.iter().any(|s| s.service.context.is_some())
}

fn mode_id(mode: Mode) -> &'static str {
    match mode {
        Mode::All => "all",
        Mode::Work => "work",
        Mode::Personal => "personal",
    }
}

fn mode_label(mode: Mode) -> &'static str {
    match mode {
        Mode::All => tk("todas as contas"),
        Mode::Work => tk("Trabalho"),
        Mode::Personal => tk("Pessoal"),
    }
}

fn set_mode(app: &AppHandle, id: &str) {
    let mode = match id {
        "work" => Mode::Work,
        "personal" => Mode::Personal,
        "all" => Mode::All,
        _ => return,
    };
    let state = app.state::<AppState>();
    let _ = state.update(|inner| {
        crate::state::choose_mode(inner, mode);
        Ok(())
    });
    publish(app);
}

fn set_dnd(app: &AppHandle, until: Option<u64>) {
    let state = app.state::<AppState>();
    let _ = state.update(|inner| {
        inner.config.settings.dnd_until = until;
        Ok(())
    });
    publish(app);
    // Fora do evento de menu (thread principal): a notificação do resumo.
    let app = app.clone();
    tauri::async_runtime::spawn(async move { crate::silence::check(&app) });
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "tray:show" => show_main(app),
        "tray:quit" => app.exit(0),
        "tray:dnd1h" => set_dnd(app, Some(now_secs() + 3600)),
        "tray:dnd8h" => set_dnd(app, Some(now_secs() + 8 * 3600)),
        "tray:dndon" => set_dnd(app, Some(DND_FOREVER)),
        "tray:dndoff" => set_dnd(app, None),
        "tray:lock" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move { crate::lock::lock_app(&app) });
        }
        _ if id.starts_with("tray:mode:") => set_mode(app, &id["tray:mode:".len()..]),
        _ => {
            // Conta ou conversa: abre a janela e, fora da thread principal
            // (criar webview espera por ela), o serviço.
            let target = if let Some(service) = id.strip_prefix("tray:svc:") {
                Some((service.to_owned(), None))
            } else if let Some(index) = id.strip_prefix("tray:chat:") {
                let chats = app.state::<TrayHandles>();
                let chats = chats.chats.lock().unwrap_or_else(|e| e.into_inner());
                index
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| chats.get(i).cloned())
                    .map(|(service, title)| (service, Some(title)))
            } else {
                None
            };
            let Some((service, chat)) = target else {
                return;
            };
            show_main(app);
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                // Bloqueado: a janela mostra só a tela de desbloqueio.
                if app.state::<AppState>().lock().locked {
                    return;
                }
                let _ = match chat {
                    Some(title) => crate::webviews::open_chat(&app, &service, &title, true),
                    None => crate::webviews::activate(&app, Some(service)),
                };
            });
        }
    }
}

pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let snap = snapshot(app);
    let (menu, chats) = build_menu(app, &snap)?;
    let handles = TrayHandles::default();
    *handles.signature.lock().unwrap_or_else(|e| e.into_inner()) = signature(&snap);
    *handles.chats.lock().unwrap_or_else(|e| e.into_inner()) = chats;
    app.manage(handles);

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Papeo")
        .menu(&menu)
        // Clique esquerdo: mini janela com as conversas; direito: o menu.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray: &TrayIcon, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                crate::popover::toggle(tray.app_handle(), rect);
            }
        });
    // macOS: silhueta "template", que o sistema pinta conforme o tema da
    // barra de menus. Windows: o ícone colorido do app.
    #[cfg(target_os = "macos")]
    {
        let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-template.png"))?;
        builder = builder.icon(icon).icon_as_template(true);
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Atualiza contador, dica e, se algo visível mudou, o menu.
/// Nunca chamar com o lock do estado seguro (espera a thread principal).
pub fn refresh(app: &AppHandle, snap: &Snapshot) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    // Dica e número só quando o total muda (cada um espera a thread principal).
    // O idioma entra no valor guardado: trocar o idioma refaz a dica.
    static SHOWN_UNREAD: AtomicU64 = AtomicU64::new(u64::MAX);
    let shown = u64::from(snap.total_unread) | (u64::from(snap.lang == papeo_core::Lang::En) << 32);
    if SHOWN_UNREAD.swap(shown, Ordering::SeqCst) != shown {
        let tooltip = match snap.total_unread {
            0 => "Papeo".to_owned(),
            1 => tr("Papeo — 1 não lida").to_owned(),
            n => trf("Papeo — {n} não lidas", &[("n", &n)]),
        };
        let _ = tray.set_tooltip(Some(tooltip));
        #[cfg(target_os = "macos")]
        {
            let title = (snap.total_unread > 0).then(|| snap.total_unread.to_string());
            let _ = tray.set_title(title);
        }
    }
    let Some(handles) = app.try_state::<TrayHandles>() else {
        return;
    };
    let sig = signature(snap);
    {
        let mut current = handles.signature.lock().unwrap_or_else(|e| e.into_inner());
        if *current == sig {
            return;
        }
        *current = sig;
    }
    if let Ok((menu, chats)) = build_menu(app, snap) {
        *handles.chats.lock().unwrap_or_else(|e| e.into_inner()) = chats;
        let _ = tray.set_menu(Some(menu));
    }
}

/// Fechar a janela só a esconde quando "manter na bandeja" está ligado.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    match event {
        WindowEvent::CloseRequested { api, .. } if window.label() == MAIN_WINDOW => {
            let app = window.app_handle();
            let keep = app.state::<AppState>().lock().config.settings.close_to_tray;
            if keep {
                api.prevent_close();
                let _ = window.hide();
                // Bloqueio "ao esconder a janela".
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let hide_locks = app
                        .state::<AppState>()
                        .lock()
                        .config
                        .settings
                        .lock
                        .after_minutes
                        == 0;
                    if hide_locks {
                        crate::lock::lock_app(&app);
                    }
                });
            }
        }
        // Alt+F4/Cmd+W na mini janela só esconde (ela fecha sozinha depois).
        WindowEvent::CloseRequested { api, .. } if window.label() == crate::popover::POPOVER => {
            api.prevent_close();
            crate::popover::hide(window.app_handle());
        }
        WindowEvent::Focused(false) if window.label() == crate::popover::POPOVER => {
            crate::popover::on_blur(window);
        }
        WindowEvent::Focused(focused) if window.label() == MAIN_WINDOW => {
            crate::notify::on_window_focus(window.app_handle(), *focused);
            // Voltando das configurações do Windows com o PIN do Hello criado:
            // reavalia (só quando ainda não havia autenticação do sistema).
            let app = window.app_handle();
            if *focused && cfg!(windows) && !app.state::<AppState>().lock().system_auth {
                crate::lock::check_availability(app);
            }
        }
        // Lendo na prévia de links também é usar o Papeo: o bloqueio
        // automático não conta esse tempo.
        WindowEvent::Focused(focused) if window.label() == crate::preview::PREVIEW => {
            let state = window.app_handle().state::<AppState>();
            let mut inner = state.lock();
            if *focused {
                inner.unfocused_since = None;
            } else if !inner.window_focused && inner.unfocused_since.is_none() {
                inner.unfocused_since = Some(std::time::Instant::now());
            }
        }
        _ => {}
    }
}
