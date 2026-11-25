//! Prévia de links: janela pequena sobre o app para os links que os sites
//! mandam abrir fora (quando escolhido nas Configurações).
//!
//! Duas webviews numa janela: a barra (`preview-bar`, interface local, só os
//! comandos `preview_*`) e a página (`preview-page`, o site, sem nenhum
//! comando e sem dados guardados: nada de login, cookies ou cache em disco).
//! Uma única janela, reaproveitada a cada link; fechar destrói as duas
//! webviews e devolve a memória.
//!
//! Tamanho: o último em que a pessoa deixou a janela (`preview-size.json`,
//! só deste computador); na primeira vez, boa parte da tela.

use crate::state::AppState;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::webview::{NewWindowResponse, PageLoadEvent, WebviewBuilder};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Url, WebviewUrl};
use tauri_plugin_opener::OpenerExt;

pub const PREVIEW: &str = "preview";
const BAR: &str = "preview-bar";
const PAGE: &str = "preview-page";
/// Evento para a barra: endereço e título atuais.
const EVENT: &str = "papeo://preview";
const BAR_HEIGHT: f64 = 44.0;
const MIN_WIDTH: f64 = 420.0;
const MIN_HEIGHT: f64 = 320.0;
/// Margem até a borda da tela.
const SCREEN_GAP: f64 = 40.0;
const SIZE_FILE: &str = "preview-size.json";

#[derive(Serialize, Clone, Default)]
pub struct PreviewInfo {
    url: String,
    title: String,
}

static CURRENT: Mutex<Option<PreviewInfo>> = Mutex::new(None);
/// Tamanho atual (lógico), gravado quando a janela fecha.
static LAST_SIZE: Mutex<Option<(f64, f64)>> = Mutex::new(None);

#[derive(Serialize, serde::Deserialize)]
struct SavedSize {
    width: f64,
    height: f64,
}

fn size_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join(SIZE_FILE))
}

fn saved_size(app: &AppHandle) -> Option<(f64, f64)> {
    let text = std::fs::read_to_string(size_path(app)?).ok()?;
    let s: SavedSize = serde_json::from_str(&text).ok()?;
    (s.width.is_finite() && s.height.is_finite()).then_some((s.width, s.height))
}

fn save_size(app: &AppHandle) {
    let Some((width, height)) = LAST_SIZE.lock().unwrap_or_else(|e| e.into_inner()).take() else {
        return;
    };
    if let (Some(path), Ok(json)) = (
        size_path(app),
        serde_json::to_string(&SavedSize { width, height }),
    ) {
        let _ = std::fs::write(path, json);
    }
}

/// Área útil (lógica) da tela da janela principal.
fn screen_area(app: &AppHandle) -> Option<(f64, f64)> {
    let monitor = app
        .get_window(crate::state::MAIN_WINDOW)
        .and_then(|w| w.current_monitor().ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten())?;
    let area = monitor
        .work_area()
        .size
        .to_logical::<f64>(monitor.scale_factor());
    Some((area.width, area.height))
}

/// Último tamanho usado ou, na primeira vez, ~70% × 85% da tela; sempre
/// cabendo na tela atual.
fn initial_size(app: &AppHandle) -> (f64, f64) {
    let (screen_w, screen_h) = screen_area(app).unwrap_or((1440.0, 900.0));
    let max_w = (screen_w - SCREEN_GAP).max(MIN_WIDTH);
    let max_h = (screen_h - SCREEN_GAP).max(MIN_HEIGHT);
    let (w, h) =
        saved_size(app).unwrap_or(((screen_w * 0.7).max(1100.0), (screen_h * 0.85).max(760.0)));
    (w.clamp(MIN_WIDTH, max_w), h.clamp(MIN_HEIGHT, max_h))
}

fn current() -> std::sync::MutexGuard<'static, Option<PreviewInfo>> {
    CURRENT.lock().unwrap_or_else(|e| e.into_inner())
}

/// Endereços que a página pode carregar; o resto (apps, arquivos locais) não.
fn web(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
}

/// Pedido da página para fechar a prévia (atalho de teclado). A página não
/// tem comando nenhum: o pedido é uma navegação que o Papeo intercepta. Um
/// site só consegue, no máximo, fechar a própria prévia.
const CLOSE_URL: &str = "papeo-preview://close";

/// Na página: ⌘W/Ctrl+W fecha a prévia mesmo com o foco no site (no Mac o
/// menu já faz isso; no Windows não há menu).
const PAGE_SCRIPT: &str = r#"
addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "w") {
    e.preventDefault();
    e.stopImmediatePropagation();
    location.href = "papeo-preview://close";
  }
}, true);
"#;

fn update(app: &AppHandle, change: impl FnOnce(&mut PreviewInfo)) {
    let info = {
        let mut cur = current();
        let info = cur.get_or_insert_with(PreviewInfo::default);
        change(info);
        info.clone()
    };
    let _ = app.emit_to(BAR, EVENT, info);
}

/// Abre o link na prévia (ou troca o link da prévia já aberta). Chamar fora
/// da thread principal: criar a janela espera por ela.
pub fn open(app: &AppHandle, url: Url) {
    if !web(&url) {
        return;
    }
    let (locked, protected, appearance) = {
        let state = app.state::<AppState>();
        let inner = state.lock();
        let s = &inner.config.settings;
        (inner.locked, s.protect_screen, s.appearance)
    };
    if locked {
        return;
    }
    let host = url.host_str().unwrap_or_default().to_owned();
    update(app, |info| {
        info.url = url.to_string();
        info.title = host.clone();
    });

    if let Some(window) = app.get_window(PREVIEW) {
        if let Some(page) = app.get_webview(PAGE) {
            let _ = page.navigate(url);
        }
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    } else if create(app, url, protected, appearance).is_err() {
        // Sem prévia, o link ainda abre: no navegador (nunca com o app bloqueado).
        let url = current().take().map(|i| i.url).unwrap_or_default();
        close(app);
        if !app.state::<AppState>().lock().locked {
            let _ = app.opener().open_url(url, None::<&str>);
        }
        return;
    }
    // O app pode ter bloqueado enquanto a janela abria: o `close` do bloqueio
    // não a teria encontrado. Conferido depois de ela existir, não escapa.
    if app.state::<AppState>().lock().locked {
        close(app);
    }
}

fn create(
    app: &AppHandle,
    url: Url,
    protected: bool,
    appearance: papeo_core::Appearance,
) -> tauri::Result<()> {
    let (width, height) = initial_size(app);
    let builder = tauri::window::WindowBuilder::new(app, PREVIEW)
        .title("Prévia")
        .inner_size(width, height)
        .min_inner_size(MIN_WIDTH, MIN_HEIGHT)
        .center()
        .content_protected(protected)
        .theme(crate::window_theme(appearance))
        .visible(true);
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true);
    let window = builder.build()?;
    let size = window
        .inner_size()?
        .to_logical::<f64>(window.scale_factor()?);

    window.add_child(
        WebviewBuilder::new(BAR, WebviewUrl::App(Default::default()))
            // A mesma interface, montando só a barra da prévia (`src/main.ts`).
            .initialization_script("window.__PAPEO_VIEW = 'preview';"),
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(size.width, BAR_HEIGHT),
    )?;

    let page = WebviewBuilder::new(PAGE, WebviewUrl::External(url))
        // Sem dados em disco: a prévia não guarda login, cookies nem cache.
        .incognito(true)
        .initialization_script(PAGE_SCRIPT)
        .on_navigation({
            let app = app.clone();
            move |url| {
                if url.as_str().starts_with(CLOSE_URL) {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move { close(&app) });
                    return false;
                }
                web(url)
            }
        })
        .on_new_window({
            let app = app.clone();
            move |url, _features| {
                // Dentro da prévia, "nova janela" vai para o navegador.
                if web(&url) || url.scheme() == "mailto" {
                    crate::webviews::open_external(&app, url.as_str());
                }
                NewWindowResponse::Deny
            }
        })
        // Nada é salvo pela prévia; para baixar, "Abrir no navegador".
        .on_download(|_, _| false)
        .on_page_load(|webview, payload| {
            if payload.event() == PageLoadEvent::Finished {
                let url = payload.url().to_string();
                update(webview.app_handle(), |info| info.url = url);
            }
        })
        .on_document_title_changed(|webview, title| {
            update(webview.app_handle(), |info| info.title = title);
        });
    #[cfg(not(windows))]
    let page = page.user_agent(crate::webviews::SAFARI_UA);
    window.add_child(
        page,
        LogicalPosition::new(0.0, BAR_HEIGHT),
        LogicalSize::new(size.width, (size.height - BAR_HEIGHT).max(0.0)),
    )?;

    let app = app.clone();
    // `CURRENT` não é limpo no `Destroyed`: ele pode chegar depois de um
    // link novo já ter sido gravado (`close` limpa).
    window.on_window_event(move |event| match event {
        tauri::WindowEvent::Resized(_) => layout(&app),
        tauri::WindowEvent::Destroyed => save_size(&app),
        _ => {}
    });
    let _ = window.set_focus();
    Ok(())
}

/// Barra no alto, página no resto.
fn layout(app: &AppHandle) {
    let Some(window) = app.get_window(PREVIEW) else {
        return;
    };
    let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) else {
        return;
    };
    let size = size.to_logical::<f64>(scale);
    // Minimizada (tamanho zero) não conta como o tamanho escolhido.
    if size.width >= MIN_WIDTH && size.height >= MIN_HEIGHT {
        *LAST_SIZE.lock().unwrap_or_else(|e| e.into_inner()) = Some((size.width, size.height));
    }
    if let Some(bar) = app.get_webview(BAR) {
        let _ = bar.set_size(LogicalSize::new(size.width, BAR_HEIGHT));
    }
    if let Some(page) = app.get_webview(PAGE) {
        let _ = page.set_position(LogicalPosition::new(0.0, BAR_HEIGHT));
        let _ = page.set_size(LogicalSize::new(
            size.width,
            (size.height - BAR_HEIGHT).max(0.0),
        ));
    }
}

/// Fecha de verdade: as webviews são destruídas e a memória volta.
pub fn close(app: &AppHandle) {
    if let Some(window) = app.get_window(PREVIEW) {
        let _ = window.destroy();
    }
    *current() = None;
}

/// Endereço e título atuais, para a barra ao abrir.
#[tauri::command]
pub fn preview_state() -> PreviewInfo {
    current().clone().unwrap_or_default()
}

/// Leva a página atual da prévia para o navegador padrão e fecha a prévia.
#[tauri::command]
pub async fn preview_open_browser(app: AppHandle) -> Result<(), String> {
    let url = app
        .get_webview(PAGE)
        .and_then(|page| page.url().ok())
        .filter(web)
        .or_else(|| {
            current()
                .as_ref()
                .and_then(|i| Url::parse(&i.url).ok())
                .filter(web)
        })
        .ok_or("Nenhum link aberto na prévia.")?;
    app.opener()
        .open_url(url.as_str(), None::<&str>)
        .map_err(|e| e.to_string())?;
    close(&app);
    Ok(())
}

#[tauri::command]
pub async fn preview_close(app: AppHandle) {
    close(&app);
}
