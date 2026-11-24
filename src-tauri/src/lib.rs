mod commands;
mod downloads;
mod lock;
mod memory;
#[cfg(target_os = "macos")]
mod menu;
mod notify;
mod sessions;
mod state;
mod tray;
mod webviews;

use state::{publish, status_of, AppState, Inner, MAIN_WINDOW, UI_WEBVIEW};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tauri::webview::WebviewBuilder;
use tauri::{AppHandle, LogicalPosition, Manager, WebviewUrl};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

/// Argumento usado pelo início automático com o sistema.
const HIDDEN_ARG: &str = "--hidden";

/// A cada 15 s: hiberna serviços parados, reavalia "não responde" e aplica o
/// bloqueio automático.
fn spawn_maintenance(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last_status = HashMap::new();
        loop {
            std::thread::sleep(Duration::from_secs(15));
            let state = app.state::<AppState>();
            let (due, status) = {
                let inner = state.lock();
                let due: Vec<String> = inner
                    .config
                    .services
                    .iter()
                    .filter(|s| s.hibernate_after_minutes > 0)
                    .filter(|s| inner.active.as_deref() != Some(s.id.as_str()))
                    .filter(|s| {
                        inner.last_used.get(&s.id).is_some_and(|t| {
                            t.elapsed()
                                >= Duration::from_secs(u64::from(s.hibernate_after_minutes) * 60)
                        })
                    })
                    .filter(|s| app.get_webview(&s.id).is_some())
                    .map(|s| s.id.clone())
                    .collect();
                let status: HashMap<String, _> = inner
                    .config
                    .services
                    .iter()
                    .map(|s| {
                        let loaded = app.get_webview(&s.id).is_some();
                        (s.id.clone(), status_of(&inner, &s.id, loaded))
                    })
                    .collect();
                (due, status)
            };
            for id in &due {
                webviews::hibernate(&app, id);
            }
            if !due.is_empty() || status != last_status {
                last_status = status;
                publish(&app);
            }
            // Fora do lock: `is_visible` espera a thread principal.
            // Minimizada conta como escondida (no Windows ela segue "visível").
            let visible = app.get_window(MAIN_WINDOW).is_none_or(|w| {
                w.is_visible().unwrap_or(true) && !w.is_minimized().unwrap_or(false)
            });
            let lock_due = {
                let inner = state.lock();
                let lock = inner.config.settings.lock;
                lock.enabled
                    && !inner.locked
                    && match lock.after_minutes {
                        0 => !visible,
                        n => inner
                            .unfocused_since
                            .is_some_and(|t| t.elapsed() >= Duration::from_secs(u64::from(n) * 60)),
                    }
            };
            if lock_due {
                lock::lock_app(&app);
            }
        }
    });
}

/// Tema da janela nativa conforme a aparência escolhida (`None` = sistema).
/// As webviews dos sites herdam esse tema como `prefers-color-scheme`.
pub fn window_theme(appearance: papeo_core::Appearance) -> Option<tauri::Theme> {
    appearance.dark().map(|dark| {
        if dark {
            tauri::Theme::Dark
        } else {
            tauri::Theme::Light
        }
    })
}

fn build_main_window(
    app: &AppHandle,
    visible: bool,
    protected: bool,
    appearance: papeo_core::Appearance,
) -> tauri::Result<()> {
    let builder = tauri::window::WindowBuilder::new(app, MAIN_WINDOW)
        .title("Papeo")
        .inner_size(1280.0, 820.0)
        .min_inner_size(960.0, 600.0)
        // Fora de capturas e do compartilhamento de tela, se pedido.
        .content_protected(protected)
        .theme(window_theme(appearance))
        .visible(visible);
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true);
    let window = builder.build()?;
    let size = window
        .inner_size()?
        .to_logical::<f64>(window.scale_factor()?);
    window.add_child(
        WebviewBuilder::new(UI_WEBVIEW, WebviewUrl::App(Default::default()))
            .auto_resize()
            // Arrastar para reordenar usa o drag and drop do HTML.
            .disable_drag_drop_handler(),
        LogicalPosition::new(0.0, 0.0),
        size,
    )?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![HIDDEN_ARG]),
        ))
        .setup(|app| {
            let handle = app.handle();
            let store_path = app.path().app_config_dir()?.join("services.json");
            let config = papeo_core::load(&store_path);
            let started_hidden =
                config.settings.start_hidden && std::env::args().any(|a| a == HIDDEN_ARG);
            let protected = config.settings.protect_screen;
            let appearance = config.settings.appearance;
            let state = AppState {
                store_path,
                sessions_dir: app.path().app_data_dir()?.join("sessions"),
                inner: Mutex::new(Inner::new(config)),
            };
            {
                let mut inner = state.lock();
                inner.autostart = handle.autolaunch().is_enabled().unwrap_or(false);
                inner.window_focused = !started_hidden;
                if started_hidden {
                    inner.unfocused_since = Some(std::time::Instant::now());
                }
                state.persist(&inner)?;
            }
            app.manage(state);
            build_main_window(handle, !started_hidden, protected, appearance)?;
            lock::check_availability(handle);
            // Termina de apagar sessões de serviços removidos (nada está aberto ainda).
            let cleanup = handle.clone();
            tauri::async_runtime::spawn(async move {
                sessions::retry_pending(&cleanup).await;
            });
            tray::install(handle)?;
            #[cfg(target_os = "macos")]
            menu::install(handle)?;
            spawn_maintenance(handle.clone());
            Ok(())
        })
        .on_window_event(tray::on_window_event)
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::add_service,
            commands::remove_service,
            commands::customize_service,
            commands::move_service,
            commands::set_hibernation,
            commands::set_notifications,
            commands::mute_service,
            commands::set_media_policy,
            commands::set_zoom,
            commands::activate_service,
            commands::hibernate_service,
            commands::reload_service,
            commands::set_content_area,
            commands::set_overlay,
            commands::set_dnd,
            commands::set_quiet_hours,
            commands::set_preferences,
            commands::set_autostart,
            commands::set_downloads_dir,
            commands::reveal_download,
            commands::open_downloads_dir,
            commands::clear_downloads,
            commands::get_memory,
            commands::export_config,
            commands::preview_backup,
            commands::import_config,
            commands::clear_service_data,
            commands::get_orphan_sessions,
            commands::clean_orphan_sessions,
            commands::set_lock,
            commands::lock_now,
            commands::unlock,
            commands::set_appearance,
            notify::service_notify,
            notify::service_status,
        ])
        .build(tauri::generate_context!())
        .expect("erro ao iniciar o Papeo");

    app.run(|_app, _event| {
        // macOS: clicar no ícone do Dock com a janela escondida a reabre.
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = _event {
            tray::show_main(_app);
        }
    });
}
