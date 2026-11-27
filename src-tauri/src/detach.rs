//! Conta em janela separada (spec 32). A conta pelo site leva a própria
//! webview para a janela nova (`Webview::reparent`, sem recarregar) e a
//! devolve à janela principal antes de a janela fechar; a conta pelo leitor
//! do Papeo ganha uma webview local `rd-<id>` montando só o leitor.
//!
//! Janelas e webviews são criadas, movidas e fechadas sempre sem o lock do
//! estado (a thread principal também o usa).

use crate::state::{publish, AppState, Snapshot, MAIN_WINDOW};
use crate::webviews::{activate, apply_layout};
use papeo_core::i18n::tr;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, Webview, WebviewUrl};

const WINDOW_PREFIX: &str = "win-";
/// Webview local do leitor numa janela separada (capability `default`).
pub const READER_PREFIX: &str = "rd-";

/// Janela atual de cada conta separada. O rótulo leva um número novo a cada
/// abertura: o `Destroyed` atrasado de uma janela antiga não mexe na nova,
/// e reabrir logo depois de fechar não esbarra no rótulo antigo. Nunca é
/// segurado junto com o lock do estado nem durante chamadas de janela.
static LABELS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
static SEQ: AtomicU64 = AtomicU64::new(0);

fn labels() -> std::sync::MutexGuard<'static, Option<HashMap<String, String>>> {
    LABELS.lock().unwrap_or_else(|e| e.into_inner())
}

fn label_of(id: &str) -> Option<String> {
    labels().as_ref().and_then(|m| m.get(id).cloned())
}

fn window_of(app: &AppHandle, id: &str) -> Option<tauri::Window> {
    app.get_window(&label_of(id)?)
}

/// Rótulos das janelas separadas abertas (tema, proteção de tela).
pub fn windows(app: &AppHandle) -> Vec<tauri::Window> {
    let ids: Vec<String> = app
        .state::<AppState>()
        .lock()
        .detached
        .iter()
        .cloned()
        .collect();
    ids.iter().filter_map(|id| window_of(app, id)).collect()
}

/// Tamanho útil da janela (pixels lógicos).
fn inner_size(window: &tauri::Window) -> Option<LogicalSize<f64>> {
    let scale = window.scale_factor().ok()?;
    Some(window.inner_size().ok()?.to_logical::<f64>(scale))
}

/// Põe a webview do site da conta na janela separada, ocupando-a toda.
/// Conta pelo leitor: nada (a janela tem a própria tela do leitor; o site,
/// se carregado para os avisos, fica escondido na principal).
fn fit(app: &AppHandle, id: &str) {
    if app.state::<AppState>().lock().reader(id) {
        if let Some(webview) = app.get_webview(id) {
            let _ = webview.hide();
        }
        return;
    }
    let (Some(window), Some(webview)) = (window_of(app, id), app.get_webview(id)) else {
        return;
    };
    if webview.window().label() != window.label() && webview.reparent(&window).is_err() {
        return;
    }
    if let Some(size) = inner_size(&window) {
        let _ = webview.set_position(LogicalPosition::new(0.0, 0.0));
        let _ = webview.set_size(size);
    }
    let _ = webview.show();
}

/// Webview nova da conta (recarregar, acordar da hibernação): se a conta
/// está separada, vai para a janela dela. Chamado sem o lock.
pub fn adopt(app: &AppHandle, webview: &Webview) {
    let id = webview.label().to_owned();
    if app.state::<AppState>().lock().detached.contains(&id) {
        fit(app, &id);
    }
}

/// Devolve a webview do site à janela principal (escondida; o layout
/// decide). Sem efeito se ela já estiver lá ou não existir.
fn return_webview(app: &AppHandle, id: &str) {
    let (Some(main), Some(webview)) = (app.get_window(MAIN_WINDOW), app.get_webview(id)) else {
        return;
    };
    if webview.window().label() != MAIN_WINDOW {
        let _ = webview.hide();
        let _ = webview.reparent(&main);
    }
}

/// Abre a conta numa janela separada (ou traz a dela para a frente).
pub fn detach(app: &AppHandle, id: &str) -> Result<Snapshot, String> {
    let state = app.state::<AppState>();
    let (service, area, reader, protected, appearance, next) = {
        let mut inner = state.lock();
        let service = inner.service(id)?.clone();
        if inner.locked {
            return Err(tr("Desbloqueie o Papeo primeiro.").into());
        }
        if inner.detached.contains(id) {
            drop(inner);
            focus(app, id);
            return Ok(publish(app));
        }
        inner.leave_split(id);
        // A janela principal fica com a conta usada por último (ou o Início).
        let next = if inner.active.as_deref() == Some(id) {
            inner
                .last_used
                .iter()
                .filter(|(other, _)| {
                    other.as_str() != id
                        && !inner.detached.contains(other.as_str())
                        && inner.service(other).is_ok()
                })
                .max_by_key(|(_, at)| **at)
                .map(|(other, _)| other.clone())
        } else {
            inner.active.clone()
        };
        inner.detached.insert(id.to_owned());
        (
            service,
            inner.area,
            inner.reader(id),
            inner.config.settings.protect_screen,
            inner.config.settings.appearance,
            next,
        )
    };
    let built = build_window(app, &service, area, reader, protected, appearance);
    if let Err(err) = built {
        state.lock().detached.remove(id);
        return Err(err);
    }
    if !reader {
        if app.get_webview(id).is_none() {
            // Hibernada: nasce na principal e `adopt` a traz para cá.
            let offscreen = crate::state::Area {
                x: -(area.width + 100.0),
                ..area
            };
            if let Err(err) = crate::webviews::create_service_webview(app, &service, offscreen) {
                attach_quiet(app, id);
                return Err(err);
            }
        } else {
            fit(app, id);
        }
    }
    // Bloqueado enquanto a janela abria: some como as outras.
    let locked = state.lock().locked;
    if locked {
        if let Some(window) = window_of(app, id) {
            let _ = window.hide();
        }
        state.lock().detached_hidden = true;
    }
    // A janela principal troca de conta se a separada era a aberta.
    let snap = activate(app, next)?;
    if let Some(window) = window_of(app, id) {
        let _ = window.set_focus();
    }
    Ok(snap)
}

fn build_window(
    app: &AppHandle,
    service: &papeo_core::Service,
    area: crate::state::Area,
    reader: bool,
    protected: bool,
    appearance: papeo_core::Appearance,
) -> Result<(), String> {
    let id = service.id.clone();
    let label = format!(
        "{WINDOW_PREFIX}{id}-{}",
        SEQ.fetch_add(1, Ordering::SeqCst) + 1
    );
    labels()
        .get_or_insert_with(HashMap::new)
        .insert(id.clone(), label.clone());
    let window = tauri::window::WindowBuilder::new(app, &label)
        .title(&service.name)
        .inner_size(area.width.max(520.0), area.height.max(420.0))
        .min_inner_size(420.0, 320.0)
        .content_protected(protected)
        .theme(crate::window_theme(appearance))
        .visible(true)
        .build()
        .map_err(|e| {
            if let Some(m) = labels().as_mut() {
                m.remove(&id);
            }
            e.to_string()
        })?;
    if reader {
        let size = inner_size(&window).unwrap_or(LogicalSize::new(area.width, area.height));
        let id_json = serde_json::to_string(&id).map_err(|e| e.to_string())?;
        let script = format!(
            "{} window.__PAPEO_READER = {id_json};",
            crate::lang::view_script("reader")
        );
        window
            .add_child(
                tauri::webview::WebviewBuilder::new(
                    format!("{READER_PREFIX}{id}"),
                    WebviewUrl::App(Default::default()),
                )
                .auto_resize()
                .disable_drag_drop_handler()
                .initialization_script(script),
                LogicalPosition::new(0.0, 0.0),
                size,
            )
            .map_err(|e| {
                let _ = window.destroy();
                e.to_string()
            })?;
    }
    let app2 = app.clone();
    window.on_window_event(move |event| match event {
        tauri::WindowEvent::Resized(_) => fit(&app2, &id),
        tauri::WindowEvent::Focused(focused) => {
            let changed = {
                let state = app2.state::<AppState>();
                let mut inner = state.lock();
                let before = inner.detached_focus.clone();
                if *focused {
                    inner.detached_focus = Some(id.clone());
                } else if inner.detached_focus.as_deref() == Some(id.as_str()) {
                    inner.detached_focus = None;
                }
                before != inner.detached_focus
            };
            if changed {
                publish(&app2);
            }
        }
        // Antes de a janela sumir, a página volta (não é destruída com ela).
        tauri::WindowEvent::CloseRequested { .. } => return_webview(&app2, &id),
        tauri::WindowEvent::Destroyed => {
            let app = app2.clone();
            let id = id.clone();
            let label = label.clone();
            // Fora do evento da janela (o layout chama a thread principal).
            tauri::async_runtime::spawn(async move { forget(&app, &id, &label) });
        }
        _ => {}
    });
    Ok(())
}

/// Janela separada fechada: a conta volta a ser da janela principal (só se
/// for a janela atual dela).
fn forget(app: &AppHandle, id: &str, label: &str) {
    {
        let mut map = labels();
        let Some(m) = map.as_mut() else { return };
        if m.get(id).map(String::as_str) != Some(label) {
            return;
        }
        m.remove(id);
    }
    let was = {
        let state = app.state::<AppState>();
        let mut inner = state.lock();
        if inner.detached_focus.as_deref() == Some(id) {
            inner.detached_focus = None;
        }
        inner.detached.remove(id)
    };
    if !was {
        return;
    }
    return_webview(app, id);
    apply_layout(app);
    publish(app);
}

/// Traz a janela separada da conta para a frente.
pub fn focus(app: &AppHandle, id: &str) {
    if let Some(window) = window_of(app, id) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Devolve a conta à janela principal (fecha a janela separada).
/// `false` se ela não estava separada.
pub fn attach_quiet(app: &AppHandle, id: &str) -> bool {
    let was = {
        let state = app.state::<AppState>();
        let mut inner = state.lock();
        if inner.detached_focus.as_deref() == Some(id) {
            inner.detached_focus = None;
        }
        inner.detached.remove(id)
    };
    if !was {
        return false;
    }
    return_webview(app, id);
    let window = window_of(app, id);
    if let Some(m) = labels().as_mut() {
        m.remove(id);
    }
    if let Some(window) = window {
        let _ = window.destroy();
    }
    true
}

/// Todas as contas voltam à janela principal (restaurar backup, "Esqueci o
/// PIN", janela principal fechada de vez).
pub fn attach_all(app: &AppHandle) {
    let ids: Vec<String> = app
        .state::<AppState>()
        .lock()
        .detached
        .iter()
        .cloned()
        .collect();
    for id in ids {
        attach_quiet(app, &id);
    }
}

/// "Voltar para a janela principal": fecha a separada e mostra a conta.
pub fn attach(app: &AppHandle, id: &str) -> Result<Snapshot, String> {
    attach_quiet(app, id);
    activate(app, Some(id.to_owned()))
}

/// Bloqueio: as janelas separadas somem; ao desbloquear, voltam. Só na
/// mudança (mostrar a cada layout roubaria o foco).
pub fn sync_lock(app: &AppHandle) {
    let (hide, show, ids) = {
        let state = app.state::<AppState>();
        let mut inner = state.lock();
        let locked = inner.locked;
        let hide = locked && !inner.detached_hidden;
        let show = !locked && inner.detached_hidden;
        if hide || show {
            inner.detached_hidden = locked;
        }
        (
            hide,
            show,
            inner.detached.iter().cloned().collect::<Vec<_>>(),
        )
    };
    for id in ids {
        if let Some(window) = window_of(app, &id) {
            if hide {
                let _ = window.hide();
            } else if show {
                let _ = window.show();
            }
        }
    }
}

/// A janela principal sumiu (fechada sem "manter na bandeja"): fecha as
/// separadas, senão o app seguiria aberto sem a janela principal.
pub fn close_windows(app: &AppHandle) {
    let all: Vec<String> = labels()
        .as_ref()
        .map(|m| m.values().cloned().collect())
        .unwrap_or_default();
    for label in all {
        if let Some(window) = app.get_window(&label) {
            let _ = window.destroy();
        }
    }
}
