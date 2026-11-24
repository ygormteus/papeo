//! Ícone na bandeja (Windows) / menu bar (macOS), fechar para a bandeja e
//! reabrir a janela.

use crate::state::{now_secs, publish, AppState, Snapshot, MAIN_WINDOW};
use papeo_core::DND_FOREVER;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Window, WindowEvent, Wry};

const TRAY_ID: &str = "papeo";

struct TrayHandles {
    dnd: CheckMenuItem<Wry>,
}

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_window(MAIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "tray:show", "Abrir o Papeo", true, None::<&str>)?;
    let dnd = CheckMenuItem::with_id(app, "tray:dnd", "Não perturbe", true, false, None::<&str>)?;
    let hour = MenuItem::with_id(
        app,
        "tray:dnd1h",
        "Silenciar por 1 hora",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "tray:quit", "Sair do Papeo", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &PredefinedMenuItem::separator(app)?,
            &dnd,
            &hour,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Papeo")
        .menu(&menu)
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray:show" => show_main(app),
            "tray:dnd" => {
                let state = app.state::<AppState>();
                let _ = state.update(|inner| {
                    let s = &mut inner.config.settings;
                    s.dnd_until = if s.dnd_until.is_some_and(|t| now_secs() < t) {
                        None
                    } else {
                        Some(DND_FOREVER)
                    };
                    Ok(())
                });
                publish(app);
            }
            "tray:dnd1h" => {
                let state = app.state::<AppState>();
                let _ = state.update(|inner| {
                    inner.config.settings.dnd_until = Some(now_secs() + 3600);
                    Ok(())
                });
                publish(app);
            }
            "tray:quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray: &TrayIcon, event| {
            // No Windows, clique esquerdo abre a janela; o direito mostra o menu.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if !cfg!(target_os = "macos") {
                    show_main(tray.app_handle());
                }
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    app.manage(TrayHandles { dnd });
    Ok(())
}

/// Atualiza contador e estado do "não perturbe" na bandeja.
pub fn refresh(app: &AppHandle, snap: &Snapshot) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let tooltip = match snap.total_unread {
        0 => "Papeo".to_owned(),
        1 => "Papeo — 1 não lida".to_owned(),
        n => format!("Papeo — {n} não lidas"),
    };
    let _ = tray.set_tooltip(Some(tooltip));
    #[cfg(target_os = "macos")]
    {
        let title = (snap.total_unread > 0).then(|| snap.total_unread.to_string());
        let _ = tray.set_title(title);
    }
    if let Some(handles) = app.try_state::<TrayHandles>() {
        // Só o não perturbe manual: o horário silencioso não é ligado/desligado aqui.
        let manual = snap.settings.dnd_until.is_some_and(|t| now_secs() < t);
        let _ = handles.dnd.set_checked(manual);
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
        WindowEvent::Focused(focused) if window.label() == MAIN_WINDOW => {
            crate::notify::on_window_focus(window.app_handle(), *focused);
        }
        _ => {}
    }
}
