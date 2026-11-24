//! Menu nativo do macOS: atalhos que funcionam mesmo com o foco dentro de um
//! serviço. No Windows os atalhos são tratados pela interface.

use crate::state::{AppState, COMMAND_EVENT, UI_WEBVIEW};
use crate::webviews::activate;
use tauri::menu::{Menu, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager};

pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let item = |id: &str, label: &str, accel: &str| {
        MenuItemBuilder::with_id(id, label)
            .accelerator(accel)
            .build(app)
    };
    let mut go = SubmenuBuilder::new(app, "Ir").item(&item("go:home", "Início", "CmdOrCtrl+0")?);
    for i in 1..=9 {
        go = go.item(&item(
            &format!("go:{i}"),
            &format!("Serviço {i}"),
            &format!("CmdOrCtrl+{i}"),
        )?);
    }
    let go = go
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&item("go:reload", "Recarregar serviço", "CmdOrCtrl+R")?)
        .item(&item("go:search", "Buscar serviço…", "CmdOrCtrl+K")?)
        .item(&item("go:settings", "Configurações…", "CmdOrCtrl+,")?)
        .build()?;
    let view = SubmenuBuilder::new(app, "Zoom")
        .item(&item("zoom:in", "Aumentar", "CmdOrCtrl+=")?)
        .item(&item("zoom:out", "Diminuir", "CmdOrCtrl+-")?)
        .item(&item("zoom:reset", "Tamanho real", "CmdOrCtrl+Shift+0")?)
        .build()?;

    // O menu padrão traz Editar (copiar/colar), necessário dentro das webviews.
    let menu = Menu::default(app)?;
    let position = menu.items()?.len().saturating_sub(1);
    menu.insert(&view, position)?;
    menu.insert(&go, position)?;
    app.set_menu(menu)?;
    app.on_menu_event(|app, event| {
        let id = event.id().as_ref().to_owned();
        if !(id.starts_with("go:") || id.starts_with("zoom:")) {
            return;
        }
        let app = app.clone();
        // Fora da thread principal: criar webview bloqueia esperando por ela.
        tauri::async_runtime::spawn(async move { handle(&app, &id) });
    });
    Ok(())
}

fn handle(app: &AppHandle, id: &str) {
    let state = app.state::<AppState>();
    let active = state.lock().active.clone();
    match id {
        "go:home" => {
            let _ = activate(app, None);
        }
        "go:reload" => {
            if let Some(webview) = active.and_then(|id| app.get_webview(&id)) {
                let _ = webview.reload();
            }
        }
        "go:search" => {
            let _ = app.emit_to(UI_WEBVIEW, COMMAND_EVENT, "search");
        }
        "go:settings" => {
            let _ = activate(app, None);
            let _ = app.emit_to(UI_WEBVIEW, COMMAND_EVENT, "settings");
        }
        "zoom:in" | "zoom:out" | "zoom:reset" => {
            let _ = app.emit_to(UI_WEBVIEW, COMMAND_EVENT, id);
        }
        other => {
            let Some(index) = other
                .strip_prefix("go:")
                .and_then(|n| n.parse::<usize>().ok())
            else {
                return;
            };
            let target = state
                .lock()
                .config
                .services
                .get(index.saturating_sub(1))
                .map(|s| s.id.clone());
            if let Some(target) = target {
                let _ = activate(app, Some(target));
            }
        }
    }
}
