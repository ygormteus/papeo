//! Criação, posicionamento e ciclo de vida das webviews dos serviços.

use crate::state::{
    publish, AppState, Area, DownloadState, Health, Snapshot, MAIN_WINDOW, UI_WEBVIEW,
};
use papeo_core::{MediaPolicy, Service};
use std::time::{Duration, Instant};
use tauri::webview::{NewWindowResponse, PermissionKind, PermissionResponse, WebviewBuilder};
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, Webview, WebviewUrl};
use tauri_plugin_opener::OpenerExt;

const SERVICE_SCRIPT: &str = include_str!("notification_shim.js");

/// WKWebView (macOS) e WebKitGTK (Linux) não trazem o token "Safari" no
/// user-agent padrão, e o WhatsApp Web recusa o navegador sem ele.
#[cfg(not(windows))]
pub(crate) const SAFARI_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.6 Safari/605.1.15";

/// Decide pedidos de permissão do site conforme a política do serviço.
/// Microfone e câmera só valem para o próprio site do serviço: uma página de
/// outro domínio aberta dentro dele recebe "negado".
fn permission_for(webview: &Webview, kind: PermissionKind) -> PermissionResponse {
    match kind {
        PermissionKind::Microphone | PermissionKind::Camera => {
            let page = webview.url().map(|u| u.to_string()).unwrap_or_default();
            let state = webview.state::<AppState>();
            let policy = state
                .lock()
                .service(webview.label())
                .ok()
                .filter(|s| papeo_core::same_site(&s.url, &page))
                .map(|s| s.media_policy())
                .unwrap_or(MediaPolicy::Block);
            match policy {
                MediaPolicy::Allow => PermissionResponse::Allow,
                MediaPolicy::Block => PermissionResponse::Deny,
                MediaPolicy::Ask => PermissionResponse::Default,
            }
        }
        // As notificações passam pelo shim (com "não perturbe"); a notificação
        // própria da webview furaria essas regras.
        PermissionKind::Notifications => PermissionResponse::Deny,
        _ => PermissionResponse::Default,
    }
}

pub fn create_service_webview(
    app: &AppHandle,
    service: &Service,
    area: Area,
) -> Result<Webview, String> {
    let window = app
        .get_window(MAIN_WINDOW)
        .ok_or("Janela principal indisponível.")?;
    let url = service
        .url
        .parse()
        .map_err(|_| "URL inválida.".to_owned())?;
    let state = app.state::<AppState>();

    let mut builder = WebviewBuilder::new(&service.id, WebviewUrl::External(url))
        .data_directory(state.sessions_dir.join(&service.session))
        .initialization_script(SERVICE_SCRIPT)
        // Sem isto o Tauri captura arquivos soltos na janela e o site
        // (WhatsApp, Telegram) nunca recebe o arrastar e soltar.
        .disable_drag_drop_handler()
        .on_document_title_changed(|webview, title| {
            let app = webview.app_handle().clone();
            let count = papeo_core::parse_unread(&title);
            let changed = {
                let state = app.state::<AppState>();
                let mut inner = state.lock();
                inner.unread.insert(webview.label().to_owned(), count) != Some(count)
            };
            if changed {
                publish(&app);
            }
        })
        .on_permission_request(|webview, kind| permission_for(&webview, kind))
        .on_download(|webview, event| crate::downloads::handle(&webview, event))
        .on_new_window({
            let app = app.clone();
            move |url, _features| {
                // Links "abrir em nova janela" vão para o navegador padrão ou,
                // se escolhido, para a prévia do Papeo (só páginas web).
                let preview = matches!(url.scheme(), "http" | "https")
                    && app.state::<AppState>().lock().config.settings.open_links
                        == papeo_core::LinkTarget::Preview;
                if preview {
                    // Fora da thread principal: criar a janela espera por ela.
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move { crate::preview::open(&app, url) });
                } else if matches!(url.scheme(), "http" | "https" | "mailto") {
                    let _ = app.opener().open_url(url.as_str(), None::<&str>);
                }
                NewWindowResponse::Deny
            }
        });

    #[cfg(target_os = "macos")]
    {
        builder = builder.data_store_identifier(service.session_bytes());
    }

    match &service.user_agent {
        Some(ua) => builder = builder.user_agent(ua),
        #[cfg(not(windows))]
        None => builder = builder.user_agent(SAFARI_UA),
        #[cfg(windows)]
        None => {}
    }

    let webview = match window.add_child(
        builder,
        LogicalPosition::new(area.x, area.y),
        LogicalSize::new(area.width, area.height),
    ) {
        Ok(webview) => webview,
        // Outra thread criou a mesma conta ao mesmo tempo (ex.: clique
        // durante a abertura escalonada): usa a dela.
        Err(err) => return app.get_webview(&service.id).ok_or_else(|| err.to_string()),
    };
    #[cfg(target_os = "macos")]
    crate::webcrypto::attach(&webview);
    if (service.zoom - 1.0).abs() > f64::EPSILON {
        let _ = webview.set_zoom(service.zoom);
    }
    // Lembra que o serviço está carregado, para reabri-lo ao iniciar o app.
    let _ = state.update(|inner| {
        inner.health.insert(service.id.clone(), Health::new());
        if !inner.config.open.contains(&service.id) {
            inner.config.open.push(service.id.clone());
        }
        Ok(())
    });
    Ok(webview)
}

/// Intervalo entre as contas reabertas ao iniciar.
const RESTORE_GAP: Duration = Duration::from_secs(3);

/// Ao iniciar, recarrega (escondidos) os serviços que estavam carregados
/// quando o app foi fechado, para as notificações continuarem chegando.
/// A primeira conta da barra lateral abre na hora; as outras, uma a uma em
/// seguida, para não disputarem a abertura do app.
pub fn restore_open(app: &AppHandle) {
    let ids: Vec<String> = {
        let state = app.state::<AppState>();
        let inner = state.lock();
        inner
            .config
            .services
            .iter()
            .filter(|s| inner.config.open.contains(&s.id))
            .map(|s| s.id.clone())
            .collect()
    };
    let Some((first, rest)) = ids.split_first() else {
        return;
    };
    restore_one(app, first);
    apply_layout(app);
    if rest.is_empty() {
        return;
    }
    let app = app.clone();
    let rest = rest.to_vec();
    std::thread::spawn(move || {
        for id in rest {
            std::thread::sleep(RESTORE_GAP);
            if restore_one(&app, &id) {
                apply_layout(&app);
                publish(&app);
            }
        }
    });
}

/// Cria a webview de uma conta a reabrir, se ainda fizer sentido.
fn restore_one(app: &AppHandle, id: &str) -> bool {
    let state = app.state::<AppState>();
    let (service, area) = {
        let inner = state.lock();
        // Removida, hibernada ou já aberta pelo usuário enquanto esperava.
        if !inner.config.open.iter().any(|o| o == id) || app.get_webview(id).is_some() {
            return false;
        }
        let Ok(service) = inner.service(id).cloned() else {
            return false;
        };
        (service, inner.area)
    };
    // Nasce fora de vista (o layout decide se aparece): não cobre a tela atual.
    let offscreen = Area {
        x: -(area.width + 100.0),
        ..area
    };
    // Sem o lock: add_child espera a thread principal.
    if create_service_webview(app, &service, offscreen).is_err() {
        return false;
    }
    let mut inner = state.lock();
    // Removida, com os dados apagados ou trocada por um backup durante a
    // criação: desfaz.
    if inner.service(&service.id).map(|s| &s.session) != Ok(&service.session) {
        inner.config.open.retain(|o| o != &service.id);
        drop(inner);
        if let Some(webview) = app.get_webview(&service.id) {
            let _ = webview.close();
        }
        return false;
    }
    // Conta como usado agora, para a hibernação automática esperar.
    inner.last_used.insert(service.id, Instant::now());
    true
}

/// Recarrega o site do serviço (zera a memória acumulada pela página).
pub fn reload(app: &AppHandle, id: &str) -> Result<bool, String> {
    let Some(webview) = app.get_webview(id) else {
        return Ok(false);
    };
    if let Some(health) = app.state::<AppState>().lock().health.get_mut(id) {
        *health = Health::new();
    }
    webview.reload().map_err(|e| e.to_string())?;
    Ok(true)
}

/// Posiciona e exibe apenas o serviço ativo; os demais carregados ficam ocultos.
pub fn apply_layout(app: &AppHandle) {
    let state = app.state::<AppState>();
    let (ids, active, area, overlay) = {
        let inner = state.lock();
        let ids: Vec<String> = inner.config.services.iter().map(|s| s.id.clone()).collect();
        // Bloqueado: nenhuma conversa aparece.
        (
            ids,
            inner.active.clone(),
            inner.area,
            inner.overlay || inner.locked,
        )
    };
    for id in ids {
        let Some(webview) = app.get_webview(&id) else {
            continue;
        };
        if active.as_deref() == Some(id.as_str()) && !overlay {
            let _ = webview.set_position(LogicalPosition::new(area.x, area.y));
            let _ = webview.set_size(LogicalSize::new(area.width, area.height));
            let _ = webview.show();
        } else {
            let _ = webview.hide();
        }
    }
}

/// Mostra um serviço (criando a webview se estiver hibernado) ou o Início.
pub fn activate(app: &AppHandle, id: Option<String>) -> Result<Snapshot, String> {
    let state = app.state::<AppState>();
    let (to_create, area) = {
        let mut inner = state.lock();
        let service = match &id {
            Some(id) => Some(inner.service(id)?.clone()),
            None => None,
        };
        let now = Instant::now();
        if let Some(prev) = inner.active.take() {
            inner.last_used.insert(prev, now);
        }
        if let Some(id) = &id {
            inner.last_used.insert(id.clone(), now);
            if let Some(health) = inner.health.get_mut(id) {
                health.grace();
            }
        }
        inner.active = id.clone();
        inner.overlay = false;
        let to_create = service.filter(|s| app.get_webview(&s.id).is_none());
        (to_create, inner.area)
    };
    // Sem o lock: add_child espera a thread principal, que também usa o estado.
    if let Some(service) = to_create {
        create_service_webview(app, &service, area)?;
    }
    apply_layout(app);
    // Bloqueado: o teclado não vai para a conversa escondida.
    let locked = state.lock().locked;
    let focus = if locked { None } else { id.as_deref() };
    if let Some(webview) = focus.and_then(|id| app.get_webview(id)) {
        let _ = webview.set_focus();
    } else if let Some(ui) = app.get_webview(UI_WEBVIEW) {
        let _ = ui.set_focus();
    }
    Ok(publish(app))
}

/// Mostra o serviço e procura uma conversa nele (busca do próprio site).
/// Se a página ainda não está pronta, a busca espera o primeiro sinal de vida
/// dela (`service_status`).
/// `exact`: o nome inteiro da conversa (mensagens recentes); senão, abre só
/// se a busca achar uma única conversa.
pub fn open_chat(app: &AppHandle, id: &str, query: &str, exact: bool) -> Result<Snapshot, String> {
    let query: String = query.chars().filter(|c| !c.is_control()).collect();
    let query = papeo_core::clip(&query, 100);
    let snap = activate(app, Some(id.to_owned()))?;
    if query.is_empty() {
        return Ok(snap);
    }
    let ready = {
        let state = app.state::<AppState>();
        let mut inner = state.lock();
        let ready = inner.health.get(id).is_some_and(|h| h.last_beat.is_some());
        if !ready {
            inner
                .pending_chat
                .insert(id.to_owned(), (query.clone(), exact));
        }
        ready
    };
    if ready {
        if let Some(webview) = app.get_webview(id) {
            run_open_chat(&webview, &query, exact);
        }
    }
    Ok(snap)
}

/// Pede ao script do serviço (`notification_shim.js`) para buscar a conversa.
/// O texto vai como JSON: não é interpretado como código.
pub fn run_open_chat(webview: &Webview, query: &str, exact: bool) {
    let Ok(arg) = serde_json::to_string(query) else {
        return;
    };
    let _ = webview.eval(format!(
        "window.__papeoOpenChat && window.__papeoOpenChat({arg}, {exact})"
    ));
}

/// Descarrega a webview (libera a memória); a sessão continua salva.
pub fn hibernate(app: &AppHandle, id: &str) {
    if let Some(webview) = app.get_webview(id) {
        let _ = webview.close();
    }
    let state = app.state::<AppState>();
    let mut inner = state.lock();
    inner.unread.remove(id);
    inner.health.remove(id);
    inner.last_reload.remove(id);
    inner.pending_chat.remove(id);
    if let Some(pos) = inner.config.open.iter().position(|s| s == id) {
        inner.config.open.remove(pos);
        let _ = state.persist(&inner);
    }
    // Downloads em andamento são interrompidos junto com a webview.
    for d in inner.downloads.iter_mut() {
        if d.service_id == id && d.state == DownloadState::Downloading {
            d.state = DownloadState::Failed;
        }
    }
}

/// Hiberna tudo e volta ao Início (usado ao restaurar um backup).
pub fn close_all(app: &AppHandle) {
    let state = app.state::<AppState>();
    let ids: Vec<String> = state
        .lock()
        .config
        .services
        .iter()
        .map(|s| s.id.clone())
        .collect();
    for id in ids {
        hibernate(app, &id);
    }
    state.lock().active = None;
}
