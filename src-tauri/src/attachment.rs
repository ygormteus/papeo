//! Visualização de anexos: com `settings.attachments = Preview`, o arquivo
//! que um serviço baixa vai para uma pasta temporária e abre numa janela do
//! Papeo antes de ir para a pasta de downloads.
//!
//! Janela `attachment` com a barra (`attachment-bar`, interface local, só os
//! comandos `attachment_*`) e o conteúdo:
//! - macOS: Quick Look nativo (`QLPreviewView`), que mostra PDF, Office,
//!   imagens e vídeo sem precisar do Office;
//! - Windows: PDF, imagem, áudio e vídeo numa webview só com o arquivo
//!   (`attachment-page`, sem comando nenhum, sem dados guardados); o resto
//!   (Excel, Word) só pela barra: "Salvar" ou "Abrir no app".
//!
//! "Abrir no app" salva antes na pasta de downloads e abre essa cópia: o
//! que for editado no Excel não fica na pasta temporária, que é apagada ao
//! fechar a janela e a cada abertura do Papeo.

use crate::state::{now_secs, publish, AppState, DownloadEntry, DownloadState};
use papeo_core::i18n::{tr, trf};
use papeo_core::{AttachmentMode, AttachmentView};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager};
use tauri_plugin_opener::OpenerExt;

pub const ATTACHMENT: &str = "attachment";
const BAR: &str = "attachment-bar";
#[cfg(not(target_os = "macos"))]
const PAGE: &str = "attachment-page";
/// Evento para a barra: o anexo atual.
const EVENT: &str = "papeo://attachment";
const BAR_HEIGHT: f64 = 44.0;
const TEMP_DIR: &str = "anexos";

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentInfo {
    file_name: String,
    /// Sem valor: nenhum anexo aberto.
    view: Option<AttachmentView>,
    /// Já copiado para a pasta de downloads.
    saved: bool,
}

struct Current {
    info: AttachmentInfo,
    service_id: String,
    /// Arquivo na pasta temporária (a pasta dele é só dele).
    temp: PathBuf,
    saved: Option<PathBuf>,
    /// Cópia para os downloads em curso (bloquear não faz outra).
    saving: bool,
    /// Janela em que está aberto (o `Destroyed` de uma janela antiga não
    /// apaga o anexo de uma nova).
    window: u64,
}

struct Pending {
    service_id: String,
    url: String,
    path: PathBuf,
}

static CURRENT: Mutex<Option<Current>> = Mutex::new(None);
static PENDING: Mutex<Vec<Pending>> = Mutex::new(Vec::new());
/// Downloads que nunca terminam (o site cancelou) não acumulam para sempre.
const MAX_PENDING: usize = 32;
static WINDOWS: AtomicU64 = AtomicU64::new(0);
/// Janela viva (0 = nenhuma ou fechando): uma janela já pedida para fechar
/// ainda aparece em `get_window` até o `Destroyed` e não é reaproveitada.
static ALIVE: AtomicU64 = AtomicU64::new(0);
/// Um anexo abre por vez (dois downloads terminando juntos).
static OPENING: Mutex<()> = Mutex::new(());

fn current() -> std::sync::MutexGuard<'static, Option<Current>> {
    CURRENT.lock().unwrap_or_else(|e| e.into_inner())
}

fn pending() -> std::sync::MutexGuard<'static, Vec<Pending>> {
    PENDING.lock().unwrap_or_else(|e| e.into_inner())
}

fn temp_root(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_cache_dir().ok().map(|d| d.join(TEMP_DIR))
}

/// Apaga a pasta de um anexo (só dentro da pasta temporária do Papeo).
fn remove_temp(app: &AppHandle, file: &Path) {
    let (Some(root), Some(dir)) = (temp_root(app), file.parent()) else {
        return;
    };
    if dir.starts_with(&root) && dir != root {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Ao abrir o app: sobras de anexos de uma sessão anterior.
pub fn clear_temp(app: &AppHandle) {
    if let Some(root) = temp_root(app) {
        let _ = std::fs::remove_dir_all(root);
    }
}

fn quick_look() -> bool {
    cfg!(target_os = "macos")
}

/// Pedido de download de um serviço: se for para visualizar, devolve o
/// destino na pasta temporária. Sem lock do estado ao chamar.
pub fn intercept(app: &AppHandle, service_id: &str, url: &str, file_name: &str) -> Option<PathBuf> {
    {
        let state = app.state::<AppState>();
        let inner = state.lock();
        if inner.locked || inner.config.settings.attachments != AttachmentMode::Preview {
            return None;
        }
    }
    papeo_core::attachment_view(file_name, quick_look())?;
    let dir = temp_root(app)?.join(papeo_core::new_item_id());
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(file_name);
    let mut list = pending();
    list.push(Pending {
        service_id: service_id.to_owned(),
        url: url.to_owned(),
        path: path.clone(),
    });
    if list.len() > MAX_PENDING {
        list.remove(0);
    }
    Some(path)
}

/// Anexo de e-mail (spec 21): grava numa pasta nova do temporário e mostra
/// na janela. `Ok(false)` sem gravar nada se o tipo não tem visualização.
/// Nunca na thread principal (o `open` espera a janela).
pub fn show_bytes(
    app: &AppHandle,
    service_id: &str,
    name: &str,
    bytes: &[u8],
) -> Result<bool, String> {
    let file_name = papeo_core::safe_file_name(name);
    if papeo_core::attachment_view(&file_name, quick_look()).is_none() {
        return Ok(false);
    }
    let dir = temp_root(app)
        .ok_or(tr("Não foi possível abrir o anexo."))?
        .join(papeo_core::new_item_id());
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(&file_name);
    if let Err(e) = std::fs::write(&path, bytes) {
        remove_temp(app, &path);
        return Err(trf(
            "Não deu para salvar o anexo ({erro}).",
            &[("erro", &e)],
        ));
    }
    crate::mailbox::mark_downloaded(&path);
    open(app, service_id, path);
    Ok(true)
}

/// Fim de um download. `true` se era um anexo para visualizar (já tratado).
pub fn finished(
    app: &AppHandle,
    service_id: &str,
    url: &str,
    path: Option<&Path>,
    success: bool,
) -> bool {
    // Pelo caminho; pelo endereço só se o caminho não veio ou é da pasta
    // temporária (o mesmo endereço pode estar baixando para os downloads).
    let in_temp = path.is_none_or(|p| temp_root(app).is_some_and(|r| p.starts_with(r)));
    let item = {
        let mut list = pending();
        let by_path = path.and_then(|p| {
            list.iter()
                .position(|x| x.service_id == service_id && x.path == p)
        });
        let by_url = || {
            in_temp
                .then(|| {
                    list.iter()
                        .position(|x| x.service_id == service_id && x.url == url)
                })
                .flatten()
        };
        let Some(i) = by_path.or_else(by_url) else {
            return false;
        };
        list.remove(i)
    };
    let file = path.map(Path::to_path_buf).unwrap_or(item.path);
    if success && file.is_file() {
        let app = app.clone();
        // Fora da thread principal: criar a janela espera por ela.
        tauri::async_runtime::spawn_blocking(move || open(&app, &item.service_id, file));
    } else {
        remove_temp(app, &file);
        use tauri_plugin_notification::NotificationExt;
        let _ = app
            .notification()
            .builder()
            .title(tr("Anexo"))
            .body(tr("Não foi possível abrir o anexo."))
            .show();
    }
    true
}

fn emit(app: &AppHandle) {
    let info = current()
        .as_ref()
        .map(|c| c.info.clone())
        .unwrap_or_default();
    let _ = app.emit_to(BAR, EVENT, info);
}

/// Mostra o anexo (troca o que estiver aberto). Nunca na thread principal.
fn open(app: &AppHandle, service_id: &str, file: PathBuf) {
    let _one = OPENING.lock().unwrap_or_else(|e| e.into_inner());
    let (locked, protected, appearance) = {
        let state = app.state::<AppState>();
        let inner = state.lock();
        let s = &inner.config.settings;
        (inner.locked, s.protect_screen, s.appearance)
    };
    let file_name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Nome final sem visualização (o navegador trocou o nome): vai para os
    // downloads, como sem a visualização.
    let Some(view) = papeo_core::attachment_view(&file_name, quick_look()) else {
        let _ = save_to_downloads(app, service_id, &file, &file_name);
        remove_temp(app, &file);
        return;
    };
    // Bloqueou durante o download: nada à vista, mas o arquivo não se perde.
    if locked {
        let _ = save_to_downloads(app, service_id, &file, &file_name);
        remove_temp(app, &file);
        return;
    }

    // Janela fechando: espera ela sumir antes de criar outra (mesmo rótulo).
    // O id é lido uma vez: um "Fechar" no meio do caminho zera `ALIVE`, e a
    // janela vira "fechando" (nunca reaproveitada com id 0).
    let alive = ALIVE.load(Ordering::SeqCst);
    let mut existing = app.get_window(ATTACHMENT);
    if existing.is_some() && alive == 0 {
        wait_gone(app);
        existing = None;
    }
    let window_id = match &existing {
        Some(_) => alive,
        None => WINDOWS.fetch_add(1, Ordering::SeqCst) + 1,
    };
    let previous = current().replace(Current {
        info: AttachmentInfo {
            file_name,
            view: Some(view),
            saved: false,
        },
        service_id: service_id.to_owned(),
        temp: file.clone(),
        saved: None,
        saving: false,
        window: window_id,
    });
    if let Some(prev) = previous {
        discard(app, prev);
    }
    // "Fechar" entre a leitura de `ALIVE` e agora: a janela está fechando e
    // o `Destroyed` dela levaria este anexo. Vai para uma janela nova.
    let (existing, window_id) = if existing.is_some() && ALIVE.load(Ordering::SeqCst) != window_id {
        wait_gone(app);
        let id = WINDOWS.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(c) = current().as_mut().filter(|c| c.temp == file) {
            c.window = id;
        }
        (None, id)
    } else {
        (existing, window_id)
    };

    let window = match existing {
        Some(window) => window,
        None => match create(app, window_id, protected, appearance) {
            Ok(window) => {
                ALIVE.store(window_id, Ordering::SeqCst);
                window
            }
            Err(_) => {
                // Sem janela, o anexo não se perde: vai para os downloads.
                let _ = save(app);
                close(app);
                return;
            }
        },
    };
    show_content(app, &window, view, &file);
    layout(app);
    emit(app);
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
    // O app pode ter bloqueado enquanto a janela abria.
    let locked_now = app.state::<AppState>().lock().locked;
    if locked_now {
        close_for_lock(app);
    }
}

fn create(
    app: &AppHandle,
    window_id: u64,
    protected: bool,
    appearance: papeo_core::Appearance,
) -> tauri::Result<tauri::Window> {
    let (width, height) = crate::preview::initial_size(app);
    let builder = tauri::window::WindowBuilder::new(app, ATTACHMENT)
        .title(tr("Anexo"))
        .inner_size(width, height)
        .min_inner_size(420.0, 320.0)
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
        tauri::webview::WebviewBuilder::new(BAR, tauri::WebviewUrl::App(Default::default()))
            // A mesma interface, montando só a barra do anexo (`src/main.ts`).
            .initialization_script(crate::lang::view_script("attachment")),
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(size.width, BAR_HEIGHT),
    )?;
    let app = app.clone();
    window.on_window_event(move |event| match event {
        tauri::WindowEvent::Resized(_) => layout(&app),
        tauri::WindowEvent::Destroyed => {
            let _ = ALIVE.compare_exchange(window_id, 0, Ordering::SeqCst, Ordering::SeqCst);
            forget(&app, window_id);
        }
        _ => {}
    });
    Ok(window)
}

/// Conteúdo abaixo da barra.
#[cfg(target_os = "macos")]
fn show_content(app: &AppHandle, _window: &tauri::Window, _view: AttachmentView, file: &Path) {
    let file = file.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        // A janela é procurada aqui: pode ter fechado antes de esta vez chegar.
        let shown = handle
            .get_window(ATTACHMENT)
            .filter(|_| ALIVE.load(Ordering::SeqCst) != 0)
            .and_then(|w| w.ns_window().ok())
            // SAFETY: thread principal, janela viva (só é destruída nesta thread).
            .map(|w| unsafe { quick_look::show(w, &file, BAR_HEIGHT) })
            .unwrap_or(true); // fechou: nada a mostrar, nada a abrir
        let _ = tx.send(shown);
    });
    // Sem Quick Look (não deveria acontecer): abre no app padrão.
    if rx.recv() == Ok(false) {
        let _ = open_in_app(app);
    }
}

#[cfg(not(target_os = "macos"))]
fn show_content(app: &AppHandle, window: &tauri::Window, view: AttachmentView, file: &Path) {
    use tauri::webview::{NewWindowResponse, WebviewBuilder};
    if let Some(page) = app.get_webview(PAGE) {
        let _ = page.close();
    }
    if view != AttachmentView::Web {
        return; // a barra ocupa a janela, com "Abrir no app"
    }
    let Ok(url) = tauri::Url::from_file_path(file) else {
        return;
    };
    let Ok(size) = window
        .inner_size()
        .and_then(|s| Ok(s.to_logical::<f64>(window.scale_factor()?)))
    else {
        return;
    };
    // O mesmo arquivo (sem a parte depois de "#", que o PDF usa para páginas;
    // o WebView2 pode trocar maiúsculas da unidade ou a codificação do nome).
    let own = file.to_string_lossy().to_lowercase();
    let same_file = move |url: &tauri::Url| {
        url.scheme() == "file"
            && url
                .to_file_path()
                .is_ok_and(|p| p.to_string_lossy().to_lowercase() == own)
    };
    let page = WebviewBuilder::new(PAGE, tauri::WebviewUrl::External(url))
        .incognito(true)
        .on_navigation({
            let app = app.clone();
            move |url| {
                if same_file(url) {
                    return true;
                }
                // Link dentro do PDF: vai para o navegador, nunca para a página.
                if matches!(url.scheme(), "http" | "https" | "mailto") {
                    crate::webviews::open_external(&app, url.as_str());
                }
                false
            }
        })
        .on_new_window({
            let app = app.clone();
            move |url, _features| {
                if matches!(url.scheme(), "http" | "https" | "mailto") {
                    crate::webviews::open_external(&app, url.as_str());
                }
                NewWindowResponse::Deny
            }
        })
        .on_download(|_, _| false);
    let _ = window.add_child(
        page,
        LogicalPosition::new(0.0, BAR_HEIGHT),
        LogicalSize::new(size.width, (size.height - BAR_HEIGHT).max(0.0)),
    );
}

/// Barra no alto; sem conteúdo (Windows, Excel/Word), a barra ocupa tudo e
/// mostra o aviso.
fn layout(app: &AppHandle) {
    let Some(window) = app.get_window(ATTACHMENT) else {
        return;
    };
    let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) else {
        return;
    };
    let size = size.to_logical::<f64>(scale);
    let external = current()
        .as_ref()
        .is_some_and(|c| c.info.view == Some(AttachmentView::External));
    if let Some(bar) = app.get_webview(BAR) {
        let height = if external { size.height } else { BAR_HEIGHT };
        let _ = bar.set_size(LogicalSize::new(size.width, height));
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(page) = app.get_webview(PAGE) {
        let _ = page.set_position(LogicalPosition::new(0.0, BAR_HEIGHT));
        let _ = page.set_size(LogicalSize::new(
            size.width,
            (size.height - BAR_HEIGHT).max(0.0),
        ));
    }
}

/// Espera a janela que está fechando sumir (até 3 s; o rótulo é o mesmo).
fn wait_gone(app: &AppHandle) {
    for _ in 0..30 {
        if app.get_window(ATTACHMENT).is_none() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Larga um anexo: apaga o temporário, a não ser que uma cópia para os
/// downloads esteja lendo dele (aí `save` apaga ao terminar).
fn discard(app: &AppHandle, c: Current) {
    if !c.saving {
        remove_temp(app, &c.temp);
    }
}

/// A janela fechou: apaga o arquivo temporário dela.
fn forget(app: &AppHandle, window_id: u64) {
    let gone = {
        let mut cur = current();
        if cur.as_ref().is_some_and(|c| c.window == window_id) {
            cur.take()
        } else {
            None
        }
    };
    if let Some(c) = gone {
        discard(app, c);
    }
}

/// Fecha a janela e apaga o arquivo temporário (o salvo continua salvo).
pub fn close(app: &AppHandle) {
    ALIVE.store(0, Ordering::SeqCst);
    if let Some(window) = app.get_window(ATTACHMENT) {
        let _ = window.destroy();
    }
    let taken = current().take();
    if let Some(c) = taken {
        discard(app, c);
    }
}

/// Ao bloquear: o anexo à vista sai da tela, mas um que ainda não foi
/// salvo vai para a pasta de downloads (como sem a visualização).
pub fn close_for_lock(app: &AppHandle) {
    let unsaved = current()
        .as_ref()
        .filter(|c| c.saved.is_none() && !c.saving)
        .map(|c| {
            (
                c.service_id.clone(),
                c.temp.clone(),
                c.info.file_name.clone(),
            )
        });
    if let Some((service_id, temp, name)) = unsaved {
        let _ = save_to_downloads(app, &service_id, &temp, &name);
    }
    close(app);
}

/// Copia para a pasta de downloads e põe na lista de downloads recentes. A
/// cópia é feita sem o lock do estado (arquivo grande não trava o app).
fn save_to_downloads(
    app: &AppHandle,
    service_id: &str,
    temp: &Path,
    file_name: &str,
) -> Result<PathBuf, String> {
    let dir = crate::downloads::downloads_dir(app);
    let state = app.state::<AppState>();
    let (path, id) = {
        let mut inner = state.lock();
        let reserved: Vec<_> = inner
            .downloads
            .iter()
            .filter(|d| d.state == DownloadState::Downloading)
            .map(|d| d.path.clone())
            .collect();
        let path = papeo_core::unique_path(&dir, file_name, &reserved);
        let id = inner.push_download(DownloadEntry {
            id: 0,
            service_id: service_id.to_owned(),
            file_name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| file_name.to_owned()),
            url: String::new(),
            path: path.clone(),
            state: DownloadState::Downloading,
            at: now_secs(),
        });
        (path, id)
    };
    let copied = std::fs::copy(temp, &path);
    {
        let mut inner = state.lock();
        if let Some(entry) = inner.downloads.iter_mut().find(|d| d.id == id) {
            entry.state = if copied.is_ok() {
                DownloadState::Done
            } else {
                DownloadState::Failed
            };
        }
    }
    publish(app);
    copied.map_err(|e| trf("Não foi possível salvar: {erro}", &[("erro", &e)]))?;
    Ok(path)
}

/// Copia o anexo para a pasta de downloads (uma vez) e o põe na lista de
/// downloads recentes. Devolve o caminho salvo.
fn save(app: &AppHandle) -> Result<PathBuf, String> {
    if app.state::<AppState>().lock().locked {
        return Err(tr("O Papeo está bloqueado.").to_owned());
    }
    let (temp, service_id, file_name) = {
        let mut cur = current();
        let c = cur.as_mut().ok_or(tr("Nenhum anexo aberto."))?;
        if let Some(path) = c.saved.clone().filter(|p| p.is_file()) {
            return Ok(path);
        }
        if c.saving {
            return Err(tr("Ainda salvando o anexo.").to_owned());
        }
        c.saving = true;
        (
            c.temp.clone(),
            c.service_id.clone(),
            c.info.file_name.clone(),
        )
    };
    let result = save_to_downloads(app, &service_id, &temp, &file_name);
    let still_open = {
        let mut cur = current();
        match cur.as_mut().filter(|c| c.temp == temp) {
            Some(c) => {
                c.saving = false;
                if let Ok(path) = &result {
                    c.saved = Some(path.clone());
                    c.info.saved = true;
                }
                true
            }
            None => false,
        }
    };
    // Fechado ou trocado durante a cópia: o temporário ficou para ela.
    if !still_open {
        remove_temp(app, &temp);
    }
    let path = result?;
    emit(app);
    Ok(path)
}

fn open_in_app(app: &AppHandle) -> Result<(), String> {
    let path = save(app)?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// Roda trabalho de disco fora do executor assíncrono.
async fn blocking<F>(f: F) -> Result<(), String>
where
    F: FnOnce() -> Result<(), String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

/// Anexo aberto, para a barra ao montar.
#[tauri::command]
pub fn attachment_state() -> AttachmentInfo {
    current()
        .as_ref()
        .map(|c| c.info.clone())
        .unwrap_or_default()
}

#[tauri::command]
pub async fn attachment_save(app: AppHandle) -> Result<(), String> {
    blocking(move || save(&app).map(|_| ())).await
}

/// Salva na pasta de downloads, abre essa cópia no app padrão (Excel,
/// Numbers, leitor de PDF) e fecha a visualização.
#[tauri::command]
pub async fn attachment_open_app(app: AppHandle) -> Result<(), String> {
    let handle = app.clone();
    blocking(move || open_in_app(&handle)).await?;
    close(&app);
    Ok(())
}

/// Mostra o arquivo salvo na pasta (Finder/Explorer).
#[tauri::command]
pub async fn attachment_reveal(app: AppHandle) -> Result<(), String> {
    blocking(move || {
        let path = save(&app)?;
        app.opener()
            .reveal_item_in_dir(path)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn attachment_close(app: AppHandle) {
    close(&app);
}

/// Autoteste: anexo aberto (nome, arquivo temporário) e se o conteúdo
/// apareceu (Quick Look no Mac, página no Windows).
#[cfg(feature = "selftest")]
pub fn inspect(app: &AppHandle) -> Option<(String, PathBuf, bool)> {
    let (name, temp) = {
        let cur = current();
        let c = cur.as_ref()?;
        (c.info.file_name.clone(), c.temp.clone())
    };
    let window = app.get_window(ATTACHMENT)?;
    #[cfg(target_os = "macos")]
    let shown = {
        let ns_window = window.ns_window().ok()? as usize;
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = app.run_on_main_thread(move || {
            // SAFETY: thread principal, janela viva.
            let _ = tx.send(unsafe { quick_look::has_view(ns_window as *mut std::ffi::c_void) });
        });
        rx.recv().unwrap_or(false)
    };
    #[cfg(not(target_os = "macos"))]
    let shown = {
        let _ = window;
        app.get_webview(PAGE).is_some()
    };
    Some((name, temp, shown))
}

#[cfg(target_os = "macos")]
mod quick_look {
    use objc2::msg_send;
    use objc2::rc::{Allocated, Retained};
    use objc2::runtime::{AnyClass, AnyObject, Bool};
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString, NSURL};
    use std::path::Path;

    #[link(name = "Quartz", kind = "framework")]
    extern "C" {}

    /// `QLPreviewViewStyleNormal`: com os controles do Quick Look.
    const STYLE_NORMAL: usize = 0;
    /// `NSViewWidthSizable | NSViewHeightSizable`: acompanha a janela.
    const RESIZE_WITH_WINDOW: usize = 2 | 16;

    /// Mostra `file` na janela, abaixo da barra (`top`), criando a
    /// `QLPreviewView` na primeira vez. Só na thread principal.
    pub unsafe fn show(ns_window: *mut std::ffi::c_void, file: &Path, top: f64) -> bool {
        let window = ns_window as *mut AnyObject;
        if window.is_null() {
            return false;
        }
        let Some(class) = AnyClass::get(c"QLPreviewView") else {
            return false;
        };
        let content: *mut AnyObject = msg_send![window, contentView];
        if content.is_null() {
            return false;
        }
        let view = match find(content, class) {
            Some(view) => view,
            None => {
                let Some(view) = create(content, class, top) else {
                    return false;
                };
                view
            }
        };
        let url = NSURL::fileURLWithPath(&NSString::from_str(&file.to_string_lossy()));
        let _: () = msg_send![&*view, setPreviewItem: &*url];
        true
    }

    /// Autoteste: a janela tem a `QLPreviewView`.
    #[cfg(feature = "selftest")]
    pub unsafe fn has_view(ns_window: *mut std::ffi::c_void) -> bool {
        let window = ns_window as *mut AnyObject;
        let Some(class) = AnyClass::get(c"QLPreviewView") else {
            return false;
        };
        if window.is_null() {
            return false;
        }
        let content: *mut AnyObject = msg_send![window, contentView];
        if content.is_null() {
            return false;
        }
        let Some(view) = find(content, class) else {
            return false;
        };
        // Em coordenadas da janela (origem embaixo): ocupa da base até a
        // barra, sem cobri-la.
        let nil: *mut AnyObject = std::ptr::null_mut();
        let bounds: NSRect = msg_send![&*view, bounds];
        let rect: NSRect = msg_send![&*view, convertRect: bounds, toView: nil];
        let all: NSRect = msg_send![content, bounds];
        let all: NSRect = msg_send![content, convertRect: all, toView: nil];
        let top = rect.origin.y + rect.size.height;
        let bar_bottom = all.origin.y + all.size.height - super::BAR_HEIGHT;
        rect.origin.y.abs() < 1.0 && (top - bar_bottom).abs() < 1.0 && rect.size.width > 100.0
    }

    unsafe fn find(content: *mut AnyObject, class: &AnyClass) -> Option<Retained<AnyObject>> {
        let subviews: Option<Retained<AnyObject>> = msg_send![content, subviews];
        let subviews = subviews?;
        let count: usize = msg_send![&*subviews, count];
        for i in 0..count {
            let view: Option<Retained<AnyObject>> = msg_send![&*subviews, objectAtIndex: i];
            let Some(view) = view else { continue };
            let is: Bool = msg_send![&*view, isKindOfClass: class];
            if is.as_bool() {
                return Some(view);
            }
        }
        None
    }

    unsafe fn create(
        content: *mut AnyObject,
        class: &AnyClass,
        top: f64,
    ) -> Option<Retained<AnyObject>> {
        let bounds: NSRect = msg_send![content, bounds];
        let flipped: Bool = msg_send![content, isFlipped];
        let height = (bounds.size.height - top).max(0.0);
        let y = if flipped.as_bool() { top } else { 0.0 };
        let frame = NSRect::new(NSPoint::new(0.0, y), NSSize::new(bounds.size.width, height));
        let alloc: Allocated<AnyObject> = msg_send![class, alloc];
        let view: Option<Retained<AnyObject>> =
            msg_send![alloc, initWithFrame: frame, style: STYLE_NORMAL];
        let view = view?;
        let _: () = msg_send![&*view, setAutoresizingMask: RESIZE_WITH_WINDOW];
        let _: () = msg_send![content, addSubview: &*view];
        Some(view)
    }
}

/// Troca de idioma com a janela aberta (spec 20): título e barra.
pub fn relabel(app: &AppHandle) {
    if let Some(window) = app.get_window(ATTACHMENT) {
        let _ = window.set_title(tr("Anexo"));
    }
    if let Some(bar) = app.get_webview(BAR) {
        let _ = bar.eval(crate::lang::set_script());
    }
}
