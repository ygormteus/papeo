use papeo_core::{self as services, Service, ServiceKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tauri::webview::{NewWindowResponse, WebviewBuilder};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, State, Webview, WebviewUrl,
};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

const MAIN_WINDOW: &str = "main";
const UI_WEBVIEW: &str = "ui";
const STATE_EVENT: &str = "papeo://state";
const NOTIFICATION_SHIM: &str = include_str!("notification_shim.js");
/// Intervalo mínimo entre notificações do mesmo serviço (contra excesso de alertas).
const NOTIFY_MIN_GAP: Duration = Duration::from_millis(1500);

/// WKWebView (macOS) e WebKitGTK (Linux) não trazem o token "Safari" no
/// user-agent padrão, e o WhatsApp Web recusa o navegador sem ele.
#[cfg(not(windows))]
const SAFARI_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.6 Safari/605.1.15";

/// Área (em pixels lógicos, relativa à janela) onde o serviço ativo aparece.
#[derive(Debug, Clone, Copy, Deserialize)]
struct Area {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl Default for Area {
    fn default() -> Self {
        Self {
            x: 260.0,
            y: 56.0,
            width: 1020.0,
            height: 764.0,
        }
    }
}

struct Inner {
    services: Vec<Service>,
    active: Option<String>,
    area: Area,
    /// Um modal da interface está aberto: a webview ativa fica oculta para não cobri-lo.
    overlay: bool,
    last_used: HashMap<String, Instant>,
    unread: HashMap<String, u32>,
    last_notified: HashMap<String, Instant>,
}

struct AppState {
    store_path: PathBuf,
    sessions_dir: PathBuf,
    inner: Mutex<Inner>,
}

impl AppState {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ServiceView {
    #[serde(flatten)]
    service: Service,
    loaded: bool,
    unread: u32,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    services: Vec<serde_json::Value>,
    active: Option<String>,
    total_unread: u32,
}

fn snapshot(app: &AppHandle) -> Snapshot {
    let state = app.state::<AppState>();
    let inner = state.lock();
    let services = inner
        .services
        .iter()
        .map(|s| {
            let view = ServiceView {
                service: s.clone(),
                loaded: app.get_webview(&s.id).is_some(),
                unread: inner.unread.get(&s.id).copied().unwrap_or(0),
            };
            serde_json::to_value(view).unwrap_or_default()
        })
        .collect();
    let total_unread = inner
        .services
        .iter()
        .filter_map(|s| inner.unread.get(&s.id))
        .sum();
    Snapshot {
        services,
        active: inner.active.clone(),
        total_unread,
    }
}

/// Envia o estado à interface e atualiza o badge do ícone do app.
fn publish(app: &AppHandle) -> Snapshot {
    let snap = snapshot(app);
    let _ = app.emit_to(UI_WEBVIEW, STATE_EVENT, snap.clone());
    if let Some(window) = app.get_window(MAIN_WINDOW) {
        let count = (snap.total_unread > 0).then_some(i64::from(snap.total_unread));
        let _ = window.set_badge_count(count);
    }
    snap
}

fn persist(state: &AppState, inner: &Inner) -> Result<(), String> {
    services::save(&state.store_path, &inner.services)
}

fn create_service_webview(
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
        .initialization_script(NOTIFICATION_SHIM)
        .on_document_title_changed(|webview, title| {
            let app = webview.app_handle().clone();
            let count = services::parse_unread(&title);
            let changed = {
                let state = app.state::<AppState>();
                let mut inner = state.lock();
                inner.unread.insert(webview.label().to_owned(), count) != Some(count)
            };
            if changed {
                publish(&app);
            }
        })
        .on_new_window({
            let app = app.clone();
            move |url, _features| {
                // Links "abrir em nova janela" vão para o navegador padrão.
                if matches!(url.scheme(), "http" | "https" | "mailto") {
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

    window
        .add_child(
            builder,
            LogicalPosition::new(area.x, area.y),
            LogicalSize::new(area.width, area.height),
        )
        .map_err(|e| e.to_string())
}

/// Posiciona e exibe apenas o serviço ativo; os demais carregados ficam ocultos.
fn apply_layout(app: &AppHandle) {
    let state = app.state::<AppState>();
    let (ids, active, area, overlay) = {
        let inner = state.lock();
        let ids: Vec<String> = inner.services.iter().map(|s| s.id.clone()).collect();
        (ids, inner.active.clone(), inner.area, inner.overlay)
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

fn activate(app: &AppHandle, id: Option<String>) -> Result<Snapshot, String> {
    let state = app.state::<AppState>();
    let (to_create, area) = {
        let mut inner = state.lock();
        let service = match &id {
            Some(id) => Some(
                inner
                    .services
                    .iter()
                    .find(|s| &s.id == id)
                    .cloned()
                    .ok_or("Serviço não encontrado.")?,
            ),
            None => None,
        };
        let now = Instant::now();
        if let Some(prev) = inner.active.take() {
            inner.last_used.insert(prev, now);
        }
        if let Some(id) = &id {
            inner.last_used.insert(id.clone(), now);
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
    if let Some(webview) = id.as_deref().and_then(|id| app.get_webview(id)) {
        let _ = webview.set_focus();
    } else if let Some(ui) = app.get_webview(UI_WEBVIEW) {
        let _ = ui.set_focus();
    }
    Ok(publish(app))
}

fn hibernate(app: &AppHandle, id: &str) {
    if let Some(webview) = app.get_webview(id) {
        let _ = webview.close();
    }
    let state = app.state::<AppState>();
    state.lock().unread.remove(id);
}

#[tauri::command]
async fn get_state(app: AppHandle) -> Snapshot {
    snapshot(&app)
}

#[tauri::command]
async fn add_service(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: ServiceKind,
    name: Option<String>,
    url: Option<String>,
) -> Result<Snapshot, String> {
    let auto_name = name.as_deref().is_none_or(|n| n.trim().is_empty());
    let mut service = Service::new(kind, name, url)?;
    let id = service.id.clone();
    {
        let mut inner = state.lock();
        if auto_name {
            // Segunda conta do mesmo mensageiro: "WhatsApp 2", "WhatsApp 3"…
            service.name = services::unique_name(&service.name, &inner.services);
        }
        inner.services.push(service);
        persist(&state, &inner)?;
    }
    activate(&app, Some(id))
}

#[tauri::command]
async fn remove_service(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<Snapshot, String> {
    hibernate(&app, &id);
    let removed = {
        let mut inner = state.lock();
        let pos = inner
            .services
            .iter()
            .position(|s| s.id == id)
            .ok_or("Serviço não encontrado.")?;
        let removed = inner.services.remove(pos);
        if inner.active.as_deref() == Some(id.as_str()) {
            inner.active = None;
        }
        inner.last_used.remove(&id);
        persist(&state, &inner)?;
        removed
    };
    // Apaga cookies e dados locais da sessão removida (Windows/Linux).
    let _ = std::fs::remove_dir_all(state.sessions_dir.join(&removed.session));
    apply_layout(&app);
    Ok(publish(&app))
}

/// Nome, cor e emoji do serviço (para diferenciar contas do mesmo mensageiro).
/// `color`/`emoji` vazios voltam ao padrão do tipo de serviço.
#[tauri::command]
async fn customize_service(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    name: String,
    color: Option<String>,
    emoji: Option<String>,
) -> Result<Snapshot, String> {
    let name = services::normalize_name(&name)?;
    let color = services::normalize_color(color.as_deref())?;
    let emoji = services::normalize_emoji(emoji.as_deref())?;
    {
        let mut inner = state.lock();
        let service = inner
            .services
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or("Serviço não encontrado.")?;
        service.name = name;
        service.color = color;
        service.emoji = emoji;
        persist(&state, &inner)?;
    }
    Ok(publish(&app))
}

#[tauri::command]
async fn move_service(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    to: usize,
) -> Result<Snapshot, String> {
    {
        let mut inner = state.lock();
        let from = inner
            .services
            .iter()
            .position(|s| s.id == id)
            .ok_or("Serviço não encontrado.")?;
        let service = inner.services.remove(from);
        let to = to.min(inner.services.len());
        inner.services.insert(to, service);
        persist(&state, &inner)?;
    }
    Ok(publish(&app))
}

#[tauri::command]
async fn set_hibernation(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    minutes: u32,
) -> Result<Snapshot, String> {
    {
        let mut inner = state.lock();
        let service = inner
            .services
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or("Serviço não encontrado.")?;
        service.hibernate_after_minutes = minutes.min(24 * 60);
        persist(&state, &inner)?;
    }
    Ok(publish(&app))
}

#[tauri::command]
async fn set_notifications(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<Snapshot, String> {
    {
        let mut inner = state.lock();
        let service = inner
            .services
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or("Serviço não encontrado.")?;
        service.notifications = enabled;
        persist(&state, &inner)?;
    }
    Ok(publish(&app))
}

/// Chamado pelo site do serviço (via `notification_shim.js`). O serviço é
/// identificado pelo rótulo da webview que chamou, nunca por um argumento.
#[tauri::command]
async fn service_notify(
    app: AppHandle,
    webview: Webview,
    state: State<'_, AppState>,
    title: String,
    body: String,
    silent: Option<bool>,
) -> Result<(), String> {
    let id = webview.label().to_owned();
    // Lido antes do lock: `is_focused` espera a thread principal, que também
    // pega o lock (callback de título) — segurá-lo aqui causaria deadlock.
    let focused = app
        .get_window(MAIN_WINDOW)
        .and_then(|w| w.is_focused().ok())
        .unwrap_or(false);
    let name = {
        let mut inner = state.lock();
        let Some(service) = inner.services.iter().find(|s| s.id == id) else {
            return Ok(());
        };
        if !service.notifications {
            return Ok(());
        }
        let name = service.name.clone();
        let viewing = focused && inner.active.as_deref() == Some(id.as_str());
        if viewing {
            // Você já está olhando para este serviço.
            return Ok(());
        }
        let now = Instant::now();
        if inner
            .last_notified
            .get(&id)
            .is_some_and(|t| now.duration_since(*t) < NOTIFY_MIN_GAP)
        {
            return Ok(());
        }
        inner.last_notified.insert(id, now);
        name
    };

    let title = services::clip(&title, 80);
    let heading = if title.is_empty() {
        name
    } else {
        format!("{name} · {title}")
    };
    let mut notification = app
        .notification()
        .builder()
        .title(heading)
        .body(services::clip(&body, 240));
    if !silent.unwrap_or(false) {
        notification = notification.sound(DEFAULT_SOUND);
    }
    notification.show().map_err(|e| e.to_string())
}

/// Som do sistema: nome de som do macOS ou variante do toast do Windows.
#[cfg(target_os = "macos")]
const DEFAULT_SOUND: &str = "Glass";
#[cfg(not(target_os = "macos"))]
const DEFAULT_SOUND: &str = "Default";

#[tauri::command]
async fn activate_service(app: AppHandle, id: Option<String>) -> Result<Snapshot, String> {
    activate(&app, id)
}

#[tauri::command]
async fn hibernate_service(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<Snapshot, String> {
    let was_active = state.lock().active.as_deref() == Some(id.as_str());
    hibernate(&app, &id);
    if was_active {
        return activate(&app, None);
    }
    Ok(publish(&app))
}

#[tauri::command]
async fn reload_service(app: AppHandle, id: String) -> Result<(), String> {
    match app.get_webview(&id) {
        Some(webview) => webview.reload().map_err(|e| e.to_string()),
        None => Ok(()),
    }
}

#[tauri::command]
async fn set_content_area(
    app: AppHandle,
    state: State<'_, AppState>,
    area: Area,
) -> Result<(), String> {
    if !(area.width.is_finite() && area.height.is_finite()) || area.width < 1.0 || area.height < 1.0
    {
        return Ok(());
    }
    state.lock().area = area;
    apply_layout(&app);
    Ok(())
}

#[tauri::command]
async fn set_overlay(app: AppHandle, state: State<'_, AppState>, open: bool) -> Result<(), String> {
    state.lock().overlay = open;
    apply_layout(&app);
    if !open {
        let active = state.lock().active.clone();
        if let Some(webview) = active.and_then(|id| app.get_webview(&id)) {
            let _ = webview.set_focus();
        }
    }
    Ok(())
}

/// Fecha (hiberna) serviços sem uso há mais tempo que o limite configurado.
fn spawn_hibernation_timer(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(60));
        let due: Vec<String> = {
            let state = app.state::<AppState>();
            let inner = state.lock();
            inner
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
                .collect()
        };
        if !due.is_empty() {
            for id in &due {
                hibernate(&app, id);
            }
            publish(&app);
        }
    });
}

#[cfg(target_os = "macos")]
fn install_menu(app: &AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};

    let mut go = SubmenuBuilder::new(app, "Ir").item(
        &MenuItemBuilder::with_id("go:home", "Início")
            .accelerator("CmdOrCtrl+0")
            .build(app)?,
    );
    for i in 1..=9 {
        go = go.item(
            &MenuItemBuilder::with_id(format!("go:{i}"), format!("Serviço {i}"))
                .accelerator(format!("CmdOrCtrl+{i}"))
                .build(app)?,
        );
    }
    let go = go
        .item(&PredefinedMenuItem::separator(app)?)
        .item(
            &MenuItemBuilder::with_id("go:reload", "Recarregar serviço")
                .accelerator("CmdOrCtrl+R")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("go:search", "Buscar serviço…")
                .accelerator("CmdOrCtrl+K")
                .build(app)?,
        )
        .build()?;

    // O menu padrão traz Editar (copiar/colar), necessário dentro das webviews.
    let menu = Menu::default(app)?;
    let position = menu.items()?.len().saturating_sub(1);
    menu.insert(&go, position)?;
    app.set_menu(menu)?;
    app.on_menu_event(|app, event| {
        let id = event.id().as_ref().to_owned();
        let app = app.clone();
        // Fora da thread principal: criar webview bloqueia esperando por ela.
        tauri::async_runtime::spawn(async move { handle_menu(&app, &id) });
    });
    Ok(())
}

#[cfg(target_os = "macos")]
fn handle_menu(app: &AppHandle, id: &str) {
    let state = app.state::<AppState>();
    match id {
        "go:home" => {
            let _ = activate(app, None);
        }
        "go:reload" => {
            let active = state.lock().active.clone();
            if let Some(webview) = active.and_then(|id| app.get_webview(&id)) {
                let _ = webview.reload();
            }
        }
        "go:search" => {
            let _ = app.emit_to(UI_WEBVIEW, "papeo://command", "search");
        }
        other => {
            let Some(index) = other
                .strip_prefix("go:")
                .and_then(|n| n.parse::<usize>().ok())
            else {
                return;
            };
            let target = state.lock().services.get(index - 1).map(|s| s.id.clone());
            if let Some(target) = target {
                let _ = activate(app, Some(target));
            }
        }
    }
}

fn build_main_window(app: &AppHandle) -> tauri::Result<()> {
    let builder = tauri::window::WindowBuilder::new(app, MAIN_WINDOW)
        .title("Papeo")
        .inner_size(1280.0, 820.0)
        .min_inner_size(960.0, 600.0);
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true);
    let window = builder.build()?;
    let size = window
        .inner_size()?
        .to_logical::<f64>(window.scale_factor()?);
    window.add_child(
        WebviewBuilder::new(UI_WEBVIEW, WebviewUrl::App(Default::default())).auto_resize(),
        LogicalPosition::new(0.0, 0.0),
        size,
    )?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let handle = app.handle();
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            let store_path = config_dir.join("services.json");
            let services = services::load(&store_path);
            let state = AppState {
                store_path,
                sessions_dir: data_dir.join("sessions"),
                inner: Mutex::new(Inner {
                    services,
                    active: None,
                    area: Area::default(),
                    overlay: false,
                    last_used: HashMap::new(),
                    unread: HashMap::new(),
                    last_notified: HashMap::new(),
                }),
            };
            {
                let inner = state.lock();
                persist(&state, &inner)?;
            }
            app.manage(state);
            build_main_window(handle)?;
            #[cfg(target_os = "macos")]
            install_menu(handle)?;
            spawn_hibernation_timer(handle.clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            add_service,
            remove_service,
            customize_service,
            move_service,
            set_hibernation,
            activate_service,
            hibernate_service,
            reload_service,
            set_content_area,
            set_overlay,
            set_notifications,
            service_notify,
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o Papeo");
}
