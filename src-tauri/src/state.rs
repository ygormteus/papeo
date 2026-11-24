//! Estado compartilhado do app e publicação para a interface.
//!
//! Regra de concorrência: nunca segure o lock do estado enquanto chama algo
//! que espera a thread principal (`add_child`, `is_focused`, `with_webview`
//! com espera de resposta…). Callbacks da thread principal (título da
//! página, permissões, downloads) também pegam este lock.

use papeo_core::{Config, Service, Settings};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

pub const MAIN_WINDOW: &str = "main";
pub const UI_WEBVIEW: &str = "ui";
pub const STATE_EVENT: &str = "papeo://state";
/// Comandos do menu nativo (só macOS) para a interface.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub const COMMAND_EVENT: &str = "papeo://command";
pub const DOWNLOAD_EVENT: &str = "papeo://download";

/// Sem sinal de vida do serviço visível por este tempo = "não responde".
pub const UNRESPONSIVE_AFTER: Duration = Duration::from_secs(75);
const MAX_DOWNLOADS: usize = 20;
/// Conversas recentes guardadas (só na memória, somem ao fechar o app).
const MAX_RECENT: usize = 8;
/// Mensagens guardadas por conversa.
const MAX_MESSAGES: usize = 6;

/// Área (em pixels lógicos, relativa à janela) onde o serviço ativo aparece.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Area {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
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

/// Sinais de vida enviados pelo site (via `notification_shim.js`).
#[derive(Debug, Clone, Copy)]
pub struct Health {
    pub created: Instant,
    pub last_beat: Option<Instant>,
    pub logged_out: bool,
}

impl Health {
    /// Recomeça a contagem de "não responde" (serviço voltou a ser exibido;
    /// enquanto escondido, o navegador pode atrasar os timers do site).
    pub fn grace(&mut self) {
        self.last_beat = Some(Instant::now());
    }

    pub fn new() -> Self {
        Self {
            created: Instant::now(),
            last_beat: None,
            logged_out: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadState {
    Downloading,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadEntry {
    pub id: u64,
    pub service_id: String,
    pub file_name: String,
    #[serde(skip)]
    pub url: String,
    #[serde(skip)]
    pub path: PathBuf,
    pub state: DownloadState,
    /// Segundos Unix do início.
    pub at: u64,
}

/// Mensagem de uma conversa recente: recebida (notificação) ou enviada pelo
/// Papeo (resposta rápida). Só na memória: nunca vai para o disco.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub body: String,
    /// Segundos Unix.
    pub at: u64,
    /// Enviada por você pelo Papeo.
    pub mine: bool,
}

#[derive(Debug, Clone)]
pub struct RecentChat {
    pub service_id: String,
    /// Nome da conversa (contato ou grupo), como veio na notificação.
    pub title: String,
    /// Mais antiga primeiro.
    pub messages: VecDeque<ChatMessage>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MessageView {
    pub body: String,
    pub at: u64,
    pub mine: bool,
}

/// Conversa recente como a interface e a barra de menus a veem, já com a
/// prévia escolhida aplicada.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecentView {
    pub service_id: String,
    /// Nome para exibir (pode vir encurtado).
    pub title: String,
    /// Nome completo da conversa, para abrir e responder.
    pub chat: String,
    /// Última mensagem recebida (vazia com a prévia "só remetente").
    pub body: String,
    /// Última atividade.
    pub at: u64,
    /// Mensagens recentes, mais antiga primeiro (vazia com "só remetente").
    pub messages: Vec<MessageView>,
    /// Dá para responder daqui (WhatsApp/Telegram).
    pub can_reply: bool,
}

pub struct Inner {
    pub config: Config,
    pub active: Option<String>,
    pub area: Area,
    /// Um modal da interface está aberto: a webview ativa fica oculta para não cobri-lo.
    pub overlay: bool,
    pub window_focused: bool,
    pub autostart: bool,
    pub last_used: HashMap<String, Instant>,
    pub unread: HashMap<String, u32>,
    pub last_notified: HashMap<String, Instant>,
    /// Última notificação mostrada com o app em segundo plano.
    pub last_notification: Option<(String, Instant)>,
    pub health: HashMap<String, Health>,
    /// Último recarregamento automático por excesso de memória.
    pub last_reload: HashMap<String, Instant>,
    pub downloads: VecDeque<DownloadEntry>,
    pub next_download_id: u64,
    pub recent: VecDeque<RecentChat>,
    /// Conversa a abrir assim que a página do serviço der sinal de vida
    /// (o serviço estava dormindo ou ainda carregando).
    pub pending_chat: HashMap<String, (String, bool)>,
    /// App bloqueado: webviews dos serviços ocultas até autenticar.
    pub locked: bool,
    /// O sistema oferece Touch ID/senha ou Windows Hello.
    pub lock_available: bool,
    /// Um pedido de autenticação do sistema está aberto.
    pub authenticating: bool,
    /// Desde quando a janela está sem foco (para o bloqueio automático).
    pub unfocused_since: Option<Instant>,
    pub update: UpdateStatus,
    /// Última verificação de atualização (manual ou automática), em
    /// segundos Unix.
    pub update_checked: Option<u64>,
}

impl Inner {
    pub fn new(config: Config) -> Self {
        let config_lock_enabled = config.settings.lock.enabled;
        Self {
            config,
            active: None,
            area: Area::default(),
            overlay: false,
            window_focused: true,
            autostart: false,
            last_used: HashMap::new(),
            unread: HashMap::new(),
            last_notified: HashMap::new(),
            last_notification: None,
            health: HashMap::new(),
            last_reload: HashMap::new(),
            downloads: VecDeque::new(),
            next_download_id: 1,
            recent: VecDeque::new(),
            pending_chat: HashMap::new(),
            // Bloqueia sempre ao abrir; se o sistema não tiver autenticação,
            // `lock::check_availability` desfaz isso logo em seguida.
            locked: config_lock_enabled,
            lock_available: cfg!(any(target_os = "macos", windows)),
            authenticating: false,
            unfocused_since: None,
            update: UpdateStatus::Idle,
            update_checked: None,
        }
    }

    pub fn service(&self, id: &str) -> Result<&Service, String> {
        self.config
            .services
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| "Serviço não encontrado.".to_owned())
    }

    pub fn service_mut(&mut self, id: &str) -> Result<&mut Service, String> {
        self.config
            .services
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| "Serviço não encontrado.".to_owned())
    }

    /// Acrescenta uma mensagem à conversa (que sobe para o topo).
    pub fn push_recent(&mut self, service_id: &str, title: &str, message: ChatMessage) {
        let pos = self
            .recent
            .iter()
            .position(|c| c.service_id == service_id && c.title == title);
        let mut chat = match pos.and_then(|i| self.recent.remove(i)) {
            Some(chat) => chat,
            None => RecentChat {
                service_id: service_id.to_owned(),
                title: title.to_owned(),
                messages: VecDeque::new(),
            },
        };
        chat.messages.push_back(message);
        while chat.messages.len() > MAX_MESSAGES {
            chat.messages.pop_front();
        }
        self.recent.push_front(chat);
        self.recent.truncate(MAX_RECENT);
    }

    /// Esquece as mensagens de um serviço (removido ou "sair da conta").
    pub fn forget_recent(&mut self, service_id: &str) {
        self.recent.retain(|m| m.service_id != service_id);
        self.pending_chat.remove(service_id);
    }

    fn recent_views(&self) -> Vec<RecentView> {
        let preview = self.config.settings.notification_preview;
        self.recent
            .iter()
            .filter_map(|chat| {
                let service = self.service(&chat.service_id).ok()?;
                let last = chat.messages.back()?;
                let last_in = chat.messages.iter().rev().find(|m| !m.mine).unwrap_or(last);
                let (title, body) =
                    papeo_core::recent_text(preview, self.locked, &chat.title, &last_in.body)?;
                let full = preview == papeo_core::NotificationPreview::Full;
                let messages = if full {
                    chat.messages
                        .iter()
                        .map(|m| MessageView {
                            body: papeo_core::clip(&m.body, 240),
                            at: m.at,
                            mine: m.mine,
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                Some(RecentView {
                    service_id: chat.service_id.clone(),
                    title,
                    chat: chat.title.clone(),
                    body: if last_in.mine { String::new() } else { body },
                    at: last.at,
                    messages,
                    can_reply: service.kind.supports_chat(),
                })
            })
            .collect()
    }

    pub fn push_download(&mut self, mut entry: DownloadEntry) -> u64 {
        entry.id = self.next_download_id;
        self.next_download_id += 1;
        self.downloads.push_front(entry);
        self.downloads.truncate(MAX_DOWNLOADS);
        self.next_download_id - 1
    }
}

pub struct AppState {
    pub store_path: PathBuf,
    pub sessions_dir: PathBuf,
    pub inner: Mutex<Inner>,
}

impl AppState {
    pub fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn persist(&self, inner: &Inner) -> Result<(), String> {
        papeo_core::save(&self.store_path, &inner.config)
    }

    /// Altera a configuração e grava em seguida.
    pub fn update<T>(&self, f: impl FnOnce(&mut Inner) -> Result<T, String>) -> Result<T, String> {
        let mut inner = self.lock();
        let out = f(&mut inner)?;
        self.persist(&inner)?;
        Ok(out)
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// Minutos desde a meia-noite, na hora local.
pub fn local_minute_of_day() -> u16 {
    use chrono::Timelike;
    let now = chrono::Local::now();
    (now.hour() * 60 + now.minute()) as u16
}

/// Situação da atualização do app, mostrada na interface.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available {
        version: String,
        notes: Option<String>,
    },
    /// Baixando e instalando; `percent` sem valor se o tamanho é desconhecido.
    Installing {
        percent: Option<u8>,
    },
    Failed {
        message: String,
    },
}

impl UpdateStatus {
    pub fn busy(&self) -> bool {
        matches!(self, Self::Checking | Self::Installing { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceStatus {
    Ok,
    Sleeping,
    LoggedOut,
    Unresponsive,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ServiceView {
    #[serde(flatten)]
    pub service: Service,
    pub loaded: bool,
    pub unread: u32,
    pub status: ServiceStatus,
    /// Minutos desde o último uso (None = não usado nesta sessão).
    pub idle_minutes: Option<u64>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub services: Vec<ServiceView>,
    pub active: Option<String>,
    pub total_unread: u32,
    pub settings: Settings,
    pub dnd_active: bool,
    pub autostart: bool,
    pub downloads: Vec<DownloadEntry>,
    pub downloads_dir: String,
    pub locked: bool,
    pub lock_available: bool,
    pub update: UpdateStatus,
    pub app_version: String,
    /// Mensagens recentes, mais nova primeiro (vazia com o app bloqueado).
    pub recent: Vec<RecentView>,
}

pub fn status_of(inner: &Inner, id: &str, loaded: bool) -> ServiceStatus {
    if !loaded {
        return ServiceStatus::Sleeping;
    }
    let Some(health) = inner.health.get(id) else {
        return ServiceStatus::Ok;
    };
    if health.logged_out {
        return ServiceStatus::LoggedOut;
    }
    // Só o serviço visível, com a janela em foco, é julgado: os ocultos têm
    // timers desacelerados pelo sistema e dariam falso alarme.
    let visible = inner.active.as_deref() == Some(id)
        && inner.window_focused
        && !inner.overlay
        && !inner.locked;
    let since = health.last_beat.unwrap_or(health.created);
    if visible
        && health.created.elapsed() > UNRESPONSIVE_AFTER
        && since.elapsed() > UNRESPONSIVE_AFTER
    {
        return ServiceStatus::Unresponsive;
    }
    ServiceStatus::Ok
}

pub fn snapshot(app: &AppHandle) -> Snapshot {
    let state = app.state::<AppState>();
    let downloads_dir = crate::downloads::downloads_dir(app);
    let inner = state.lock();
    let services = inner
        .config
        .services
        .iter()
        .map(|s| {
            let loaded = app.get_webview(&s.id).is_some();
            ServiceView {
                service: s.clone(),
                loaded,
                unread: inner.unread.get(&s.id).copied().unwrap_or(0),
                status: status_of(&inner, &s.id, loaded),
                idle_minutes: if inner.active.as_deref() == Some(s.id.as_str()) {
                    Some(0)
                } else {
                    inner
                        .last_used
                        .get(&s.id)
                        .map(|t| t.elapsed().as_secs() / 60)
                },
            }
        })
        .collect::<Vec<_>>();
    let total_unread = services.iter().map(|s| s.unread).sum();
    Snapshot {
        services,
        active: inner.active.clone(),
        total_unread,
        dnd_active: inner
            .config
            .settings
            .dnd_active(now_secs(), local_minute_of_day()),
        settings: inner.config.settings.clone(),
        autostart: inner.autostart,
        downloads: inner.downloads.iter().cloned().collect(),
        downloads_dir: downloads_dir.to_string_lossy().into_owned(),
        locked: inner.locked,
        lock_available: inner.lock_available,
        update: inner.update.clone(),
        app_version: app.package_info().version.to_string(),
        recent: inner.recent_views(),
    }
}

/// Envia o estado à interface e atualiza badge e bandeja.
pub fn publish(app: &AppHandle) -> Snapshot {
    let snap = snapshot(app);
    let _ = app.emit_to(UI_WEBVIEW, STATE_EVENT, snap.clone());
    let _ = app.emit_to(crate::popover::POPOVER, STATE_EVENT, snap.clone());
    if let Some(window) = app.get_window(MAIN_WINDOW) {
        let count = (snap.total_unread > 0).then_some(i64::from(snap.total_unread));
        let _ = window.set_badge_count(count);
    }
    crate::tray::refresh(app, &snap);
    snap
}
