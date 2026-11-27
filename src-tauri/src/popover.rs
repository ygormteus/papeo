//! Mini janela da barra de menus (macOS) / bandeja (Windows): conversas
//! recentes com resposta rápida. Criada no clique; escondida, continua
//! pronta por 5 minutos (reabrir na hora) e depois é fechada de verdade,
//! para não deixar um processo do navegador ocupando memória o dia todo.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, Rect, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

pub const POPOVER: &str = "popover";
const WIDTH: f64 = 380.0;
const HEIGHT: f64 = 540.0;
/// Distância entre o ícone e a janela.
const GAP: f64 = 6.0;

/// Quando a janela foi escondida por perder o foco. O clique no ícone que a
/// esconde chega depois do "perdeu o foco": sem isto ela reabriria na hora.
static HIDDEN_AT: Mutex<Option<Instant>> = Mutex::new(None);

/// Aberta (ou talvez aberta): só então recebe cada estado publicado. Ao
/// abrir, recebe o estado atual antes de aparecer.
static SHOWN: AtomicBool = AtomicBool::new(false);

/// Muda a cada abrir/esconder: o fechamento agendado só vale se nada
/// aconteceu desde que ela foi escondida.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Escondida há este tempo: fecha a janela (libera o processo dela).
const CLOSE_AFTER: Duration = Duration::from_secs(5 * 60);

/// Marca como escondida e agenda o fechamento de verdade.
fn hidden(app: &AppHandle) {
    SHOWN.store(false, Ordering::SeqCst);
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(CLOSE_AFTER);
        // Confere e fecha na thread principal, a mesma do clique no ícone:
        // um clique não cabe entre a conferência e o fechamento.
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            if GENERATION.load(Ordering::SeqCst) != generation || is_shown() {
                return;
            }
            if let Some(window) = handle.get_webview_window(POPOVER) {
                let _ = window.destroy();
            }
        });
    });
}

pub fn is_shown() -> bool {
    SHOWN.load(Ordering::SeqCst)
}

fn window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window(POPOVER) {
        return Ok(window);
    }
    WebviewWindowBuilder::new(app, POPOVER, WebviewUrl::App(Default::default()))
        // A mesma interface, montando só a mini janela (`src/main.ts`).
        .initialization_script(crate::lang::view_script("popover"))
        .title("Papeo")
        .inner_size(WIDTH, HEIGHT)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .shadow(true)
        .visible(false)
        .build()
}

/// Posição junto ao ícone: abaixo dele com a barra no alto (macOS), acima
/// com a barra embaixo (Windows), sem sair da tela.
fn place(app: &AppHandle, window: &WebviewWindow, icon: Rect) {
    let pos = icon.position.to_physical::<f64>(1.0);
    let size = icon.size.to_physical::<f64>(1.0);
    let center_x = pos.x + size.width / 2.0;
    let Ok(Some(monitor)) = app.monitor_from_point(center_x, pos.y + size.height / 2.0) else {
        return;
    };
    let scale = monitor.scale_factor();
    let (w, h, gap) = (WIDTH * scale, HEIGHT * scale, GAP * scale);
    let area = monitor.work_area();
    let left = f64::from(area.position.x);
    let top = f64::from(area.position.y);
    let right = left + f64::from(area.size.width);
    let bottom = top + f64::from(area.size.height);

    let x = (center_x - w / 2.0).clamp(left + gap, (right - w - gap).max(left + gap));
    let below = pos.y + size.height + gap;
    let above = pos.y - h - gap;
    let y = if pos.y < top + (bottom - top) / 2.0 {
        below
    } else {
        above
    };
    let y = y.clamp(top + gap, (bottom - h - gap).max(top + gap));
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

/// Clique no ícone: abre ou fecha a mini janela.
pub fn toggle(app: &AppHandle, icon: Rect) {
    let just_hidden = HIDDEN_AT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
        .is_some_and(|t| t.elapsed() < Duration::from_millis(400));
    let window = match window(app) {
        Ok(window) => window,
        Err(_) => {
            crate::tray::show_main(app);
            return;
        }
    };
    if just_hidden || window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        hidden(app);
        return;
    }
    place(app, &window, icon);
    GENERATION.fetch_add(1, Ordering::SeqCst);
    SHOWN.store(true, Ordering::SeqCst);
    let _ = app.emit_to(
        POPOVER,
        crate::state::STATE_EVENT,
        crate::state::snapshot(app),
    );
    let _ = window.show();
    let _ = window.set_focus();
}

pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(POPOVER) {
        let _ = window.hide();
        hidden(app);
    }
}

/// Perdeu o foco (clique fora): esconde.
pub fn on_blur(window: &tauri::Window) {
    if window.is_visible().unwrap_or(false) {
        *HIDDEN_AT.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
        let _ = window.hide();
        hidden(window.app_handle());
    }
}
