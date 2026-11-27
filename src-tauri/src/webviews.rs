//! Criação, posicionamento e ciclo de vida das webviews dos serviços.

use crate::state::{
    publish, AppState, Area, DownloadState, Health, Snapshot, MAIN_WINDOW, UI_WEBVIEW,
};
use papeo_core::i18n::tr;
use papeo_core::{MediaPolicy, Service};
use std::time::{Duration, Instant};
use tauri::webview::{
    NewWindowResponse, PageLoadEvent, PermissionKind, PermissionResponse, WebviewBuilder,
};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Webview, WebviewUrl};
use tauri_plugin_opener::OpenerExt;

const SERVICE_SCRIPT: &str = include_str!("notification_shim.js");
const VOICE_ENHANCE: &str = include_str!("voice_enhance.js");
const RNNOISE_WASM: &[u8] = include_bytes!("../vendor/rnnoise/rnnoise.wasm");

/// Limpeza das mensagens de voz (só no WhatsApp), com o RNNoise embutido.
fn voice_enhance_script() -> &'static str {
    use base64::Engine;
    static SCRIPT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SCRIPT.get_or_init(|| {
        let wasm = base64::engine::general_purpose::STANDARD.encode(RNNOISE_WASM);
        VOICE_ENHANCE.replace("__PAPEO_RNNOISE_WASM__", &wasm)
    })
}

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

/// Modo da gravação de mensagens de voz lido por `notification_shim.js`.
fn voice_script(mode: papeo_core::VoiceCapture) -> &'static str {
    match mode {
        papeo_core::VoiceCapture::Instant => "window.__papeoVoice = 'instant';",
        papeo_core::VoiceCapture::Enhanced => "window.__papeoVoice = 'enhanced';",
        papeo_core::VoiceCapture::Clean => "window.__papeoVoice = 'clean';",
    }
}

/// A limpeza do Papeo (com o RNNoise, ~150 KB) só entra no WhatsApp e só com
/// esse modo escolhido; o script não roda duas vezes na mesma página.
fn wants_enhance(kind: papeo_core::ServiceKind, mode: papeo_core::VoiceCapture) -> bool {
    kind == papeo_core::ServiceKind::Whatsapp && mode == papeo_core::VoiceCapture::Enhanced
}

/// Aplica o modo de gravação nos serviços já abertos (os novos recebem na criação).
pub fn set_voice_capture(app: &AppHandle, mode: papeo_core::VoiceCapture) {
    let services: Vec<(String, papeo_core::ServiceKind)> = {
        let state = app.state::<AppState>();
        let inner = state.lock();
        inner
            .config
            .services
            .iter()
            .map(|s| (s.id.clone(), s.kind))
            .collect()
    };
    for (id, kind) in services {
        if let Some(webview) = app.get_webview(&id) {
            let _ = webview.eval(voice_script(mode));
            if wants_enhance(kind, mode) {
                let _ = webview.eval(voice_enhance_script());
            }
        }
    }
}

/// Abre no navegador padrão (no autoteste, só anota).
pub fn open_external(app: &AppHandle, url: &str) {
    #[cfg(feature = "selftest")]
    {
        let _ = app;
        crate::selftest::opened(url);
    }
    #[cfg(not(feature = "selftest"))]
    {
        let _ = app.opener().open_url(url, None::<&str>);
    }
}

/// Teto das não lidas vindas do site (título ou página).
pub const UNREAD_MAX: u32 = 999_999;

pub fn create_service_webview(
    app: &AppHandle,
    service: &Service,
    area: Area,
) -> Result<Webview, String> {
    let window = app
        .get_window(MAIN_WINDOW)
        .ok_or(tr("Janela principal indisponível."))?;
    let url = service
        .url
        .parse()
        .map_err(|_| tr("URL inválida.").to_owned())?;
    let state = app.state::<AppState>();
    let voice = state.lock().config.settings.voice_capture;
    let kind = service.kind;

    let mut builder = WebviewBuilder::new(&service.id, WebviewUrl::External(url))
        .data_directory(state.sessions_dir.join(&service.session))
        .initialization_script(voice_script(voice))
        .initialization_script(SERVICE_SCRIPT)
        // O script de inicialização guarda o modo de quando a webview nasceu;
        // a cada página carregada (o site ou o Papeo recarregam), vale o atual.
        // A limpeza escolhida depois da criação também entra a cada página.
        .on_page_load(move |webview, payload| {
            if payload.event() == PageLoadEvent::Finished {
                let mode = webview
                    .state::<AppState>()
                    .lock()
                    .config
                    .settings
                    .voice_capture;
                let _ = webview.eval(voice_script(mode));
                if wants_enhance(kind, mode) {
                    let _ = webview.eval(voice_enhance_script());
                }
            }
        })
        // Sem isto o Tauri captura arquivos soltos na janela e o site
        // (WhatsApp, Telegram) nunca recebe o arrastar e soltar.
        .disable_drag_drop_handler()
        .on_document_title_changed(|webview, title| {
            let app = webview.app_handle().clone();
            let count = papeo_core::parse_unread(&title).min(UNREAD_MAX);
            let changed = {
                let state = app.state::<AppState>();
                let mut inner = state.lock();
                let id = webview.label();
                !inner.page_unread.contains(id)
                    && inner.unread.insert(id.to_owned(), count) != Some(count)
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
                    open_external(&app, url.as_str());
                }
                NewWindowResponse::Deny
            }
        });

    #[cfg(target_os = "macos")]
    {
        builder = builder.data_store_identifier(service.session_bytes());
    }

    if wants_enhance(kind, voice) {
        builder = builder.initialization_script(voice_enhance_script());
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
pub const RESTORE_GAP: Duration = Duration::from_secs(3);

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
            // Leitor de e-mail: o site só abre quando pedido.
            .filter(|s| !inner.reader(&s.id))
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
    // Hibernada enquanto esperava.
    let open = app
        .state::<AppState>()
        .lock()
        .config
        .open
        .iter()
        .any(|o| o == id);
    open && load_hidden(app, id)
}

/// Carrega a conta escondida (fora de vista), se ainda não está aberta.
pub fn load_hidden(app: &AppHandle, id: &str) -> bool {
    let state = app.state::<AppState>();
    let (service, area) = {
        let inner = state.lock();
        // Já aberta pelo usuário enquanto esperava.
        if app.get_webview(id).is_some() {
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
    {
        let state = app.state::<AppState>();
        let mut inner = state.lock();
        if let Some(health) = inner.health.get_mut(id) {
            *health = Health::new();
        }
        // A página volta a informar o contador (ou o título, se não informar).
        inner.page_unread.remove(id);
    }
    webview.reload().map_err(|e| e.to_string())?;
    Ok(true)
}

/// Reabre a conta do zero: fecha a webview e cria outra, como ao reabrir o
/// app. Recarregar a página mantém o processo do site e o estado da conta, e
/// depois de horas parado o WhatsApp Web pode carregar a página mas ficar
/// preso na tela de carregamento, sem nunca conectar.
/// Com download em curso, só recarrega a página (fechar o interromperia).
pub fn restart(app: &AppHandle, id: &str) -> Result<bool, String> {
    if app.get_webview(id).is_none() {
        return Ok(false);
    }
    let state = app.state::<AppState>();
    let (service, area, shown, focus) = {
        let mut inner = state.lock();
        let downloading = inner
            .downloads
            .iter()
            .any(|d| d.service_id == id && d.state == DownloadState::Downloading);
        if downloading {
            drop(inner);
            return reload(app, id);
        }
        let service = inner.service(id)?.clone();
        let shown = inner.on_screen(id) && !inner.overlay && !inner.locked;
        // A página nova volta a informar o contador (ou o título).
        inner.page_unread.remove(id);
        inner.last_reload.insert(id.to_owned(), Instant::now());
        // Foco só com a janela em foco: escondida (bandeja), nada aparece.
        let focus = shown && inner.window_focused && inner.active.as_deref() == Some(id);
        (
            service,
            inner.pane_area(id).unwrap_or(inner.area),
            shown,
            focus,
        )
    };
    if let Some(old) = app.get_webview(id) {
        let _ = old.close();
    }
    // Escondida, nasce fora de vista (o layout decide se aparece).
    let area = if shown {
        area
    } else {
        Area {
            x: -(area.width + 100.0),
            ..area
        }
    };
    // Sem o lock: add_child espera a thread principal.
    let webview = create_service_webview(app, &service, area)?;
    // Removida, com os dados apagados ou trocada por um backup durante a
    // criação: desfaz.
    let changed = state.lock().service(id).map(|s| &s.session) != Ok(&service.session);
    if changed {
        hibernate(app, id);
        return Ok(false);
    }
    apply_layout(app);
    if focus {
        let _ = webview.set_focus();
    }
    Ok(true)
}

/// Posiciona e exibe o serviço ativo (e a outra metade da tela dividida);
/// os demais carregados ficam ocultos.
pub fn apply_layout(app: &AppHandle) {
    let state = app.state::<AppState>();
    let (ids, overlay) = {
        let inner = state.lock();
        // Contas no leitor de e-mail: o site (se carregado para os avisos)
        // fica escondido; a tela é do Papeo.
        let ids: Vec<(String, bool, Option<Area>)> = inner
            .config
            .services
            .iter()
            .map(|s| (s.id.clone(), inner.reader(&s.id), inner.pane_area(&s.id)))
            .collect();
        // Bloqueado: nenhuma conversa aparece.
        (ids, inner.overlay || inner.locked)
    };
    for (id, reader, pane) in ids {
        let Some(webview) = app.get_webview(&id) else {
            continue;
        };
        if let Some(area) = pane.filter(|_| !overlay && !reader) {
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
        let prev = inner.active.take();
        // Tela dividida: conta que não é uma das duas troca a metade em
        // foco; uma das duas só muda o foco; o Início fecha a divisão.
        match (&id, inner.split.as_mut()) {
            (Some(new), Some(split)) if !split.has(new) => {
                if prev.as_deref() == Some(split.left.as_str()) {
                    split.left = new.clone();
                } else {
                    split.right = new.clone();
                }
            }
            (None, Some(_)) => inner.split = None,
            _ => {}
        }
        if let Some(prev) = prev {
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
        let to_create = service
            .filter(|s| app.get_webview(&s.id).is_none())
            .filter(|s| !inner.reader(&s.id));
        let area = id
            .as_deref()
            .and_then(|id| inner.pane_area(id))
            .unwrap_or(inner.area);
        (to_create, area)
    };
    // Sem o lock: add_child espera a thread principal, que também usa o estado.
    if let Some(service) = to_create {
        create_service_webview(app, &service, area)?;
    }
    apply_layout(app);
    // Bloqueado: o teclado não vai para a conversa escondida.
    let locked = state.lock().locked;
    let reader = id.as_deref().is_some_and(|id| state.lock().reader(id));
    let focus = if locked || reader {
        None
    } else {
        id.as_deref()
    };
    if let Some(webview) = focus.and_then(|id| app.get_webview(id)) {
        let _ = webview.set_focus();
    } else if let Some(ui) = app.get_webview(UI_WEBVIEW) {
        let _ = ui.set_focus();
    }
    Ok(publish(app))
}

/// Tela dividida (spec 30): `id` à direita da conta aberta, em foco.
pub fn split_open(app: &AppHandle, id: &str, ratio: f64) -> Result<Snapshot, String> {
    let state = app.state::<AppState>();
    {
        let mut inner = state.lock();
        inner.service(id)?;
        // Já é uma das duas: só o foco muda.
        if inner.split.as_ref().is_some_and(|s| s.has(id)) {
            drop(inner);
            return activate(app, Some(id.to_owned()));
        }
        // Já dividida: a em foco fica e a outra metade vira `id`.
        let left = inner
            .active
            .clone()
            .filter(|left| left != id)
            .ok_or(tr("Abra outra conta antes de dividir a tela."))?;
        inner.split = Some(crate::state::Split {
            left,
            right: id.to_owned(),
            ratio: crate::state::Split::clamp_ratio(ratio),
        });
    }
    activate(app, Some(id.to_owned()))
}

/// Volta a uma conta só (a em foco).
pub fn split_close(app: &AppHandle) -> Snapshot {
    app.state::<AppState>().lock().split = None;
    apply_layout(app);
    publish(app)
}

/// Divisor arrastado: só reposiciona (sem publicar a cada quadro).
pub fn split_ratio(app: &AppHandle, ratio: f64) {
    if let Some(split) = app.state::<AppState>().lock().split.as_mut() {
        split.ratio = crate::state::Split::clamp_ratio(ratio);
    }
    apply_layout(app);
}

/// A página da metade de trás ganhou foco (clique): vira a em foco.
pub fn split_focus(app: &AppHandle, id: &str) {
    let changed = {
        let state = app.state::<AppState>();
        let mut inner = state.lock();
        let switch =
            inner.split.as_ref().is_some_and(|s| s.has(id)) && inner.active.as_deref() != Some(id);
        if switch {
            if let Some(prev) = inner.active.replace(id.to_owned()) {
                inner.last_used.insert(prev, Instant::now());
            }
        }
        switch
    };
    if changed {
        publish(app);
    }
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
    // Leitor do Talk (spec 29): o aviso traz o token da conversa.
    let nextcloud = app
        .state::<AppState>()
        .lock()
        .service(id)
        .is_ok_and(|s| s.kind == papeo_core::ServiceKind::Nextcloud);
    if let Some(token) = query
        .strip_prefix(crate::talk::OPEN_PREFIX)
        .filter(|_| nextcloud)
    {
        if app.state::<AppState>().lock().reader(id) {
            let _ = app.emit(
                crate::talk::REVEAL_EVENT,
                serde_json::json!({ "id": id, "token": token }),
            );
        }
        // Pelo site: só mostra a conta (o token não serve para a busca dele).
        return Ok(snap);
    }
    // Leitor de e-mail: a tela do Papeo procura (pelo assunto).
    if app.state::<AppState>().lock().reader(id) {
        // Aviso de e-mail novo: o UID abre a mensagem direto (spec 28).
        let uid = crate::mail::notified_uid(id, &query);
        let _ = app.emit(
            "papeo://mail-reveal",
            serde_json::json!({ "id": id, "query": query, "uid": uid }),
        );
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

/// Conta com o leitor de e-mail: mostra o site (`true`, carregando-o) ou
/// volta ao leitor (`false`, descarregando o site).
pub fn show_site(app: &AppHandle, id: &str, site: bool) -> Result<Snapshot, String> {
    {
        let state = app.state::<AppState>();
        let mut inner = state.lock();
        let service = inner.service(id)?;
        // Nextcloud ainda sem o Talk pela API: o leitor começa pelo
        // "Conectar ao Talk" (spec 29).
        let talk_setup =
            service.kind == papeo_core::ServiceKind::Nextcloud && service.talk.is_none();
        if site {
            inner.site_view.insert(id.to_owned());
            inner.mail_setup.remove(id);
        } else {
            inner.site_view.remove(id);
            if talk_setup {
                inner.mail_setup.insert(id.to_owned());
            }
        }
    }
    if !site {
        hibernate(app, id);
    }
    activate(app, Some(id.to_owned()))
}

/// Descarrega a webview (libera a memória); a sessão continua salva.
pub fn hibernate(app: &AppHandle, id: &str) {
    if let Some(webview) = app.get_webview(id) {
        let _ = webview.close();
    }
    let state = app.state::<AppState>();
    let mut inner = state.lock();
    inner.unread.remove(id);
    inner.page_unread.remove(id);
    inner.health.remove(id);
    inner.last_reload.remove(id);
    inner.pending_chat.remove(id);
    let staged = inner
        .config
        .open
        .iter()
        .position(|s| s == id)
        .and_then(|pos| {
            inner.config.open.remove(pos);
            state.stage(&inner).ok()
        });
    // Downloads em andamento são interrompidos junto com a webview.
    for d in inner.downloads.iter_mut() {
        if d.service_id == id && d.state == DownloadState::Downloading {
            d.state = DownloadState::Failed;
        }
    }
    drop(inner);
    if let Some(staged) = staged {
        let _ = state.write(staged);
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
    let mut inner = state.lock();
    inner.active = None;
    inner.split = None;
}

#[cfg(test)]
mod voice_tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn voice_enhance_embeds_rnnoise() {
        let script = voice_enhance_script();
        assert!(!script.contains("__PAPEO_RNNOISE_WASM__"));
        let start = script.find("const WASM_BASE64 = \"").unwrap() + 21;
        let end = start + script[start..].find('"').unwrap();
        let wasm = base64::engine::general_purpose::STANDARD
            .decode(&script[start..end])
            .unwrap();
        assert_eq!(wasm, RNNOISE_WASM);
    }

    #[test]
    fn enhance_only_for_whatsapp_with_cleaning() {
        use papeo_core::{ServiceKind, VoiceCapture};
        assert!(wants_enhance(ServiceKind::Whatsapp, VoiceCapture::Enhanced));
        assert!(!wants_enhance(ServiceKind::Whatsapp, VoiceCapture::Instant));
        assert!(!wants_enhance(ServiceKind::Whatsapp, VoiceCapture::Clean));
        assert!(!wants_enhance(
            ServiceKind::Telegram,
            VoiceCapture::Enhanced
        ));
    }

    #[test]
    fn voice_script_per_mode() {
        use papeo_core::VoiceCapture::*;
        assert!(voice_script(Instant).contains("'instant'"));
        assert!(voice_script(Enhanced).contains("'enhanced'"));
        assert!(voice_script(Clean).contains("'clean'"));
    }
}
