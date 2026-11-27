//! Estado compartilhado do app e publicação para a interface.
//!
//! Regra de concorrência: nunca segure o lock do estado enquanto chama algo
//! que espera a thread principal (`add_child`, `is_focused`, `with_webview`
//! com espera de resposta…). Callbacks da thread principal (título da
//! página, permissões, downloads) também pegam este lock.

use papeo_core::i18n::tr;
use papeo_core::{Config, Mode, ModeChoice, ModeOverride, Organizer, Service, Settings};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
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

/// Largura do divisor entre as metades da tela dividida (px lógicos).
pub const SPLIT_GAP: f64 = 6.0;
/// Faixa no alto de cada metade (marca qual está em foco), fora da webview.
pub const SPLIT_BAR: f64 = 3.0;
const SPLIT_MIN: f64 = 0.25;
const SPLIT_MAX: f64 = 0.75;

/// Tela dividida (spec 30): duas contas lado a lado; `active` é sempre
/// uma das duas (a em foco).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Split {
    pub left: String,
    pub right: String,
    /// Fração da largura da metade esquerda.
    pub ratio: f64,
}

impl Split {
    pub fn clamp_ratio(ratio: f64) -> f64 {
        if ratio.is_finite() {
            ratio.clamp(SPLIT_MIN, SPLIT_MAX)
        } else {
            0.5
        }
    }

    pub fn has(&self, id: &str) -> bool {
        self.left == id || self.right == id
    }

    /// Retângulos (esquerda, direita) dentro da área do conteúdo. O mesmo
    /// cálculo está em `App.svelte` (`paneRects`).
    pub fn panes(&self, area: Area) -> (Area, Area) {
        let usable = (area.width - SPLIT_GAP).max(0.0);
        let left_w = (usable * self.ratio).round();
        let right_x = left_w + SPLIT_GAP;
        let below = Area {
            y: area.y + SPLIT_BAR,
            height: (area.height - SPLIT_BAR).max(1.0),
            ..area
        };
        (
            Area {
                width: left_w,
                ..below
            },
            Area {
                x: area.x + right_x,
                width: (area.width - right_x).max(0.0),
                ..below
            },
        )
    }
}

/// Sinais de vida enviados pelo site (via `notification_shim.js`).
#[derive(Debug, Clone, Copy)]
pub struct Health {
    pub created: Instant,
    pub last_beat: Option<Instant>,
    pub logged_out: bool,
    /// Último sinal de vida com chamada ou áudio/vídeo em curso.
    pub media_at: Option<Instant>,
    /// Pontos de apoio do site encontrados na página (spec 14).
    pub probes: papeo_core::probes::ProbeTracker,
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
            media_at: None,
            probes: Default::default(),
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
    /// Até quando a conversa foi vista (aberta com o Papeo na frente,
    /// aberta pela lista ou respondida pelo Papeo); segundos Unix.
    pub read_at: u64,
}

impl RecentChat {
    /// Tem mensagem recebida depois da última vez que foi vista.
    pub fn has_unread(&self) -> bool {
        self.messages.iter().any(|m| !m.mine && m.at > self.read_at)
    }
}

/// Segundos que a lista do site leva para mostrar o contador de uma
/// mensagem nova.
const LIST_GRACE_SECS: u64 = 10;

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
    /// Tem mensagem recebida depois da última vez que foi vista.
    pub unread: bool,
}

pub struct Inner {
    pub config: Config,
    pub active: Option<String>,
    /// Tela dividida; `None` = uma conta só.
    pub split: Option<Split>,
    /// Contas em janela separada (spec 32) e a que tem a janela em foco.
    pub detached: HashSet<String>,
    pub detached_focus: Option<String>,
    /// Janelas separadas escondidas pelo bloqueio.
    pub detached_hidden: bool,
    pub area: Area,
    /// Um modal da interface está aberto: a webview ativa fica oculta para não cobri-lo.
    pub overlay: bool,
    pub window_focused: bool,
    pub autostart: bool,
    pub last_used: HashMap<String, Instant>,
    pub unread: HashMap<String, u32>,
    /// Serviços cuja página informa as não lidas (Gmail, Roundcube): o
    /// título deixa de valer (o Gmail tira o número ao abrir um e-mail).
    pub page_unread: HashSet<String>,
    pub last_notified: HashMap<String, Instant>,
    /// Última notificação mostrada com o app em segundo plano.
    pub last_notification: Option<(String, Instant)>,
    pub health: HashMap<String, Health>,
    /// Último recarregamento automático por excesso de memória.
    pub last_reload: HashMap<String, Instant>,
    pub downloads: VecDeque<DownloadEntry>,
    pub next_download_id: u64,
    pub recent: VecDeque<RecentChat>,
    /// Mensagens seguradas pelo não perturbe/horário silencioso (spec 17).
    pub silence: papeo_core::SilenceLog,
    /// Silêncio valendo na última conferência (`silence::check`).
    pub silence_active: bool,
    /// Resumo do último silêncio, até a pessoa fechar o cartão.
    pub silence_summary: Option<papeo_core::SilenceSummary>,
    /// Conversa a abrir assim que a página do serviço der sinal de vida
    /// (o serviço estava dormindo ou ainda carregando).
    pub pending_chat: HashMap<String, (String, bool)>,
    /// App bloqueado: webviews dos serviços ocultas até autenticar.
    pub locked: bool,
    /// O sistema oferece Touch ID/senha ou Windows Hello.
    pub system_auth: bool,
    /// Um pedido de autenticação (sistema ou PIN) está aberto.
    pub authenticating: bool,
    /// Depois de vários PINs errados, só aceita outra tentativa a partir daqui.
    pub pin_retry_at: Option<Instant>,
    /// Desde quando a janela está sem foco (para o bloqueio automático).
    pub unfocused_since: Option<Instant>,
    pub update: UpdateStatus,
    /// Última verificação de atualização (manual ou automática), em
    /// segundos Unix.
    pub update_checked: Option<u64>,
    /// Tarefas e anotações (`organizer.json`).
    pub organizer: Organizer,
    /// Conversa aberta em cada serviço, como o site informa.
    pub current_chat: HashMap<String, String>,
    /// `organizer.json` não pôde ser lido: nada é gravado por cima.
    pub organizer_error: Option<String>,
    /// Troca manual de modo feita no automático (até o expediente mudar).
    pub mode_override: Option<ModeOverride>,
    /// Avisos por IMAP de cada conta de e-mail que os usa.
    pub mail: HashMap<String, MailLive>,
    /// Vigia do Talk pela API (spec 29): situação e não lidas, como no IMAP.
    pub talk: HashMap<String, MailLive>,
    /// Contas de e-mail com o leitor que o usuário pediu para ver no site
    /// (até voltar ao leitor).
    pub site_view: HashSet<String>,
    /// Contas de e-mail recém-adicionadas, ainda sem o IMAP: abrem na tela
    /// de configuração do leitor, não no site (até o usuário escolher).
    pub mail_setup: HashSet<String>,
}

/// Situação dos avisos por IMAP de uma conta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum MailStatus {
    Connecting,
    /// Conectado: contador e avisos vêm do IMAP.
    Ok,
    /// Sem conexão; tenta de novo sozinho.
    Offline {
        detail: String,
    },
    /// Usuário ou senha recusados; espera uma senha nova.
    Auth,
    /// Falta a senha neste computador (backup de outro, por exemplo).
    NoPassword,
}

#[derive(Debug, Clone)]
pub struct MailLive {
    pub status: MailStatus,
    /// Não lidas informadas pelo servidor.
    pub unread: u32,
    /// Vigia dono deste registro: um vigia antigo não o sobrescreve.
    pub generation: u64,
}

impl Inner {
    /// A conta abre no leitor de e-mail do Papeo (IMAP configurado), não no
    /// site. Sem senha ou com erro, o leitor mostra o problema e oferece o
    /// site.
    /// Na janela principal: a em foco ou a outra metade da tela dividida.
    pub fn in_main(&self, id: &str) -> bool {
        self.active.as_deref() == Some(id) || self.split.as_ref().is_some_and(|s| s.has(id))
    }

    /// A conta está à vista: na janela principal ou numa janela separada.
    pub fn on_screen(&self, id: &str) -> bool {
        self.in_main(id) || self.detached.contains(id)
    }

    /// A conta sai da tela (removida, hibernada, dados apagados): a tela
    /// dividida fecha e a outra metade fica em foco.
    pub fn leave_split(&mut self, id: &str) {
        let Some(split) = self.split.take_if(|s| s.has(id)) else {
            return;
        };
        if self.active.as_deref() == Some(id) {
            let other = if split.left == id {
                split.right
            } else {
                split.left
            };
            self.active = Some(other);
        }
    }

    /// Onde a webview da conta aparece (se está à vista).
    pub fn pane_area(&self, id: &str) -> Option<Area> {
        match &self.split {
            Some(split) if split.has(id) => {
                let (left, right) = split.panes(self.area);
                Some(if split.left == id { left } else { right })
            }
            _ => (self.active.as_deref() == Some(id)).then_some(self.area),
        }
    }

    pub fn reader(&self, id: &str) -> bool {
        !self.site_view.contains(id)
            && (self.mail_setup.contains(id)
                || self
                    .service(id)
                    .is_ok_and(|s| s.mail.is_some() || s.talk.is_some()))
    }

    pub fn new(config: Config) -> Self {
        let config_lock_enabled = config.settings.lock.enabled;
        // A espera entre tentativas continua depois de reabrir o app.
        let pin_retry_at = config
            .lock_pin
            .as_ref()
            .map(|p| papeo_core::pin_wait_secs(p.failures))
            .filter(|&s| s > 0)
            .map(|s| Instant::now() + std::time::Duration::from_secs(s));
        Self {
            config,
            active: None,
            split: None,
            detached: HashSet::new(),
            detached_focus: None,
            detached_hidden: false,
            area: Area::default(),
            overlay: false,
            window_focused: true,
            autostart: false,
            last_used: HashMap::new(),
            unread: HashMap::new(),
            page_unread: HashSet::new(),
            last_notified: HashMap::new(),
            last_notification: None,
            health: HashMap::new(),
            last_reload: HashMap::new(),
            downloads: VecDeque::new(),
            next_download_id: 1,
            recent: VecDeque::new(),
            silence: Default::default(),
            silence_active: false,
            silence_summary: None,
            pending_chat: HashMap::new(),
            // Bloqueia sempre ao abrir; sem PIN e sem autenticação do
            // sistema, `lock::check_availability` desfaz isso logo em seguida.
            locked: config_lock_enabled,
            system_auth: cfg!(any(target_os = "macos", windows)),
            authenticating: false,
            pin_retry_at,
            unfocused_since: None,
            update: UpdateStatus::Idle,
            update_checked: None,
            organizer: Organizer::default(),
            current_chat: HashMap::new(),
            organizer_error: None,
            mode_override: None,
            mail: HashMap::new(),
            talk: HashMap::new(),
            site_view: HashSet::new(),
            mail_setup: HashSet::new(),
        }
    }

    /// O IMAP está conectado: contador e avisos vêm dele, não do site.
    pub fn mail_live(&self, id: &str) -> bool {
        self.mail
            .get(id)
            .is_some_and(|m| m.status == MailStatus::Ok)
    }

    /// Como o app confirma a identidade: o PIN do Papeo, se definido; senão,
    /// a autenticação do sistema.
    pub fn lock_method(&self) -> LockMethod {
        if self.config.lock_pin.is_some() {
            LockMethod::Pin
        } else if self.system_auth {
            LockMethod::System
        } else {
            LockMethod::None
        }
    }

    pub fn service(&self, id: &str) -> Result<&Service, String> {
        self.config
            .services
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| tr("Serviço não encontrado.").to_owned())
    }

    pub fn service_mut(&mut self, id: &str) -> Result<&mut Service, String> {
        self.config
            .services
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| tr("Serviço não encontrado.").to_owned())
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
                read_at: 0,
            },
        };
        // Respondeu: viu a conversa.
        if message.mine {
            chat.read_at = chat.read_at.max(message.at);
        }
        chat.messages.push_back(message);
        while chat.messages.len() > MAX_MESSAGES {
            chat.messages.pop_front();
        }
        self.recent.push_front(chat);
        self.recent.truncate(MAX_RECENT);
    }

    /// Marca a conversa como vista agora.
    pub fn mark_read(&mut self, service_id: &str, chat: &str) {
        let chat = papeo_core::clean_chat(chat);
        let now = now_secs();
        for c in self.recent.iter_mut() {
            if c.service_id == service_id && papeo_core::clean_chat(&c.title) == chat {
                c.read_at = now;
            }
        }
    }

    /// Lista de conversas do próprio site: a que aparece sem contador de não
    /// lidas foi lida lá (ou no celular). Mensagens dos últimos segundos
    /// esperam a próxima leitura (o contador do site pode ainda não ter
    /// aparecido). Devolve se alguma conversa deixou de ser não lida.
    pub fn apply_chat_list(&mut self, service_id: &str, list: &[(String, u32)]) -> bool {
        let read: Vec<String> = list
            .iter()
            .filter(|(_, unread)| *unread == 0)
            .map(|(title, _)| papeo_core::clean_chat(title))
            .collect();
        if read.is_empty() {
            return false;
        }
        let until = now_secs().saturating_sub(LIST_GRACE_SECS);
        let mut changed = false;
        for c in self
            .recent
            .iter_mut()
            .filter(|c| c.service_id == service_id)
        {
            if c.read_at < until && read.contains(&papeo_core::clean_chat(&c.title)) {
                let before = c.has_unread();
                c.read_at = until;
                changed |= before != c.has_unread();
            }
        }
        changed
    }

    /// A conversa aberta na conta à vista, com a janela em foco, conta como
    /// vista. Chamado a cada estado publicado.
    pub fn mark_seen(&mut self) {
        if self.locked {
            return;
        }
        let mut shown: Vec<String> = Vec::new();
        if self.window_focused && !self.overlay {
            shown.extend(self.active.iter().cloned());
            if let Some(split) = &self.split {
                shown.extend([split.left.clone(), split.right.clone()]);
            }
        }
        shown.extend(self.detached_focus.iter().cloned());
        shown.sort();
        shown.dedup();
        for id in shown {
            if let Some(chat) = self.current_chat.get(&id).cloned() {
                self.mark_read(&id, &chat);
            }
        }
    }

    /// Esquece as mensagens de um serviço (removido ou "sair da conta").
    pub fn forget_recent(&mut self, service_id: &str) {
        self.recent.retain(|m| m.service_id != service_id);
        self.silence.forget(service_id);
        self.silence_summary = self
            .silence_summary
            .take()
            .and_then(|s| s.without(service_id));
        self.pending_chat.remove(service_id);
        self.current_chat.remove(service_id);
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
                    can_reply: self.can_reply(&service.id),
                    unread: chat.has_unread(),
                })
            })
            .collect()
    }

    /// Resposta rápida oferecida para a conta: mensageiro conhecido e
    /// nenhum ponto de apoio dela sumido do site há tempo demais (spec 14).
    pub fn can_reply(&self, id: &str) -> bool {
        self.service(id).is_ok_and(|s| s.kind.supports_chat())
            && self.health.get(id).is_none_or(|h| h.probes.can_reply())
    }

    pub fn push_download(&mut self, mut entry: DownloadEntry) -> u64 {
        entry.id = self.next_download_id;
        self.next_download_id += 1;
        self.downloads.push_front(entry);
        self.downloads.truncate(MAX_DOWNLOADS);
        self.next_download_id - 1
    }
}

/// Configuração desta sessão quando `services.json` existe mas está preso
/// por outro programa (ver `papeo_core::load_guarded`).
pub const FALLBACK_STORE: &str = "services.sessao.json";

pub struct AppState {
    pub store_path: PathBuf,
    pub organizer_path: PathBuf,
    pub sessions_dir: PathBuf,
    pub inner: Mutex<Inner>,
    pub config_disk: DiskWriter,
    pub organizer_disk: DiskWriter,
}

/// Grava um arquivo fora do `AppState::lock()` sem trocar a ordem: cada
/// versão recebe um número sob o lock, e uma versão mais velha que a já
/// gravada é descartada (a nova já contém a alteração dela).
#[derive(Default)]
pub struct DiskWriter {
    next: AtomicU64,
    written: Mutex<u64>,
}

/// Arquivo pronto para gravar, preparado sob o lock.
#[must_use = "grave com AppState::write depois de soltar o lock"]
pub struct Staged {
    organizer: bool,
    ticket: u64,
    body: Vec<u8>,
}

impl AppState {
    /// Sessão com a configuração provisória: as contas reais estão em
    /// `services.json`, que não pôde ser lido.
    pub fn config_fallback(&self) -> bool {
        self.store_path
            .file_name()
            .is_some_and(|n| n == FALLBACK_STORE)
    }

    pub fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Prepara `config.json`. Chamar com o lock (recebe `&Inner` dele).
    pub fn stage(&self, inner: &Inner) -> Result<Staged, String> {
        Ok(Staged {
            organizer: false,
            ticket: self.config_disk.next.fetch_add(1, Ordering::SeqCst) + 1,
            body: papeo_core::encode_config(&inner.config)?,
        })
    }

    /// Prepara `organizer.json`. Chamar com o lock (recebe `&Inner` dele).
    pub fn stage_organizer(&self, inner: &Inner) -> Result<Staged, String> {
        if let Some(err) = &inner.organizer_error {
            return Err(err.clone());
        }
        Ok(Staged {
            organizer: true,
            ticket: self.organizer_disk.next.fetch_add(1, Ordering::SeqCst) + 1,
            body: papeo_core::encode_organizer(&inner.organizer)?,
        })
    }

    /// Grava o que foi preparado, já sem o `AppState::lock()`.
    pub fn write(&self, staged: Staged) -> Result<(), String> {
        let (disk, path) = if staged.organizer {
            (&self.organizer_disk, &self.organizer_path)
        } else {
            (&self.config_disk, &self.store_path)
        };
        let mut written = disk.written.lock().unwrap_or_else(|e| e.into_inner());
        if staged.ticket <= *written {
            return Ok(());
        }
        papeo_core::write_atomic(path, &staged.body)?;
        *written = staged.ticket;
        Ok(())
    }

    /// Grava `config.json` na hora. Só na inicialização, antes de qualquer
    /// outra thread usar o estado.
    pub fn persist(&self, inner: &Inner) -> Result<(), String> {
        self.write(self.stage(inner)?)
    }

    /// Altera a configuração e grava em seguida (fora do lock).
    pub fn update<T>(&self, f: impl FnOnce(&mut Inner) -> Result<T, String>) -> Result<T, String> {
        let (out, staged) = {
            let mut inner = self.lock();
            let out = f(&mut inner)?;
            let staged = self.stage(&inner)?;
            (out, staged)
        };
        self.write(staged)?;
        Ok(out)
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// Dia da semana (0 = domingo … 6 = sábado) e minuto do dia locais, do
/// mesmo instante (sem parear segunda com 00:00 de terça na virada).
pub fn local_clock() -> (u8, u16) {
    use chrono::{Datelike, Timelike};
    let now = chrono::Local::now();
    (
        now.weekday().num_days_from_sunday() as u8,
        (now.hour() * 60 + now.minute()) as u16,
    )
}

impl Inner {
    /// Modo valendo agora (trabalho, pessoal ou todas as contas).
    pub fn current_mode(&self) -> Mode {
        let (weekday, minute) = local_clock();
        self.config
            .settings
            .current_mode(weekday, minute, now_secs(), self.mode_override)
    }

    /// A troca manual do automático ainda vale.
    pub fn mode_manual(&self) -> bool {
        self.config.settings.mode == ModeChoice::Auto
            && self.mode_override.is_some_and(|o| o.holds(now_secs()))
    }
}

/// Troca o modo. No automático, vale até o expediente mudar (voltar ao
/// modo do horário desfaz a troca); fora dele, vira a escolha gravada.
pub fn choose_mode(inner: &mut Inner, mode: Mode) {
    let s = &inner.config.settings;
    if s.mode == ModeChoice::Auto {
        let (weekday, minute) = local_clock();
        let scheduled = s.mode.scheduled(&s.work_hours, weekday, minute);
        let now = now_secs();
        let until = s
            .mode
            .minutes_until_change(&s.work_hours, weekday, minute)
            .map_or(u64::MAX, |m| now - now % 60 + u64::from(m) * 60);
        inner.mode_override = (mode != scheduled).then_some(ModeOverride { mode, until });
    } else {
        inner.config.settings.mode = match mode {
            Mode::All => ModeChoice::All,
            Mode::Work => ModeChoice::Work,
            Mode::Personal => ModeChoice::Personal,
        };
        inner.mode_override = None;
    }
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
        /// As mesmas notas em inglês (`notesEn` do `latest.json`, spec 20).
        #[serde(rename = "notesEn")]
        notes_en: Option<String>,
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
    /// Conversa aberta no site (sem valor com o app bloqueado).
    pub chat: Option<String>,
    /// A conta faz parte do modo atual (trabalho/pessoal).
    pub in_mode: bool,
    /// Avisos por IMAP (só contas que os usam).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mail_status: Option<MailStatus>,
    /// Tem assinatura formatada (spec 22); o HTML dela fica fora do estado
    /// publicado (`mail_signature`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub signature_rich: bool,
    /// Abre no leitor de e-mail do Papeo (o site só quando pedido).
    pub reader: bool,
    /// Aberta numa janela separada (spec 32).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub detached: bool,
    /// Servidor de envio (SMTP) da conta, `host:porta`, para mostrar.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mail_smtp: Option<String>,
    /// Pontos de apoio do site relatados pela página (spec 14).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub probes: Vec<papeo_core::probes::ProbeView>,
}

/// Como o bloqueio confirma a identidade.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum LockMethod {
    None,
    System,
    Pin,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub services: Vec<ServiceView>,
    pub active: Option<String>,
    pub split: Option<Split>,
    pub total_unread: u32,
    pub settings: Settings,
    pub dnd_active: bool,
    pub autostart: bool,
    pub downloads: Vec<DownloadEntry>,
    pub downloads_dir: String,
    pub locked: bool,
    /// Há como desbloquear (PIN do Papeo ou autenticação do sistema).
    pub lock_available: bool,
    pub lock_method: LockMethod,
    /// O sistema oferece Touch ID/senha ou Windows Hello.
    pub system_auth: bool,
    pub update: UpdateStatus,
    pub app_version: String,
    /// Mensagens recentes, mais nova primeiro (vazia com o app bloqueado).
    pub recent: Vec<RecentView>,
    /// Tarefas atrasadas e para hoje (zero com o app bloqueado).
    pub tasks: crate::organizer::TaskCounts,
    /// Resumo do último silêncio (spec 17); vazio com o app bloqueado, sem
    /// conversas com a prévia oculta.
    pub silence_summary: Option<papeo_core::SilenceSummary>,
    /// Modo valendo agora.
    pub mode: Mode,
    /// O modo foi trocado à mão no automático (até o expediente mudar).
    pub mode_manual: bool,
    /// Versão do estado: sobe a cada mudança enviada. A interface ignora um
    /// estado mais velho que chegue depois (resposta de comando atrasada).
    pub rev: u64,
    /// Idioma efetivo (spec 20).
    pub lang: papeo_core::Lang,
}

/// Versão do último estado enviado (ver `Snapshot::rev`).
static REV: AtomicU64 = AtomicU64::new(1);

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
    let visible = !inner.locked
        && ((inner.in_main(id) && inner.window_focused && !inner.overlay)
            || inner.detached_focus.as_deref() == Some(id));
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
    let mut inner = state.lock();
    inner.mark_seen();
    let mode = inner.current_mode();
    let services = inner
        .config
        .services
        .iter()
        .map(|s| {
            let loaded = app.get_webview(&s.id).is_some();
            // A assinatura formatada (imagens) fica fora do estado publicado.
            let mut service = s.clone();
            let signature_rich = match service.mail.as_mut() {
                Some(m) => !std::mem::take(&mut m.signature_html).is_empty(),
                None => false,
            };
            ServiceView {
                service,
                signature_rich,
                loaded,
                // Com o IMAP conectado, vale o número do servidor (o site
                // pode estar hibernado).
                unread: match inner.mail.get(&s.id).or_else(|| inner.talk.get(&s.id)) {
                    Some(m) if m.status == MailStatus::Ok => m.unread,
                    // Site hibernado e IMAP reconectando: o último número dele.
                    Some(m)
                        if !loaded
                            && !matches!(m.status, MailStatus::Auth | MailStatus::NoPassword) =>
                    {
                        m.unread
                    }
                    _ => inner.unread.get(&s.id).copied().unwrap_or(0),
                },
                status: status_of(&inner, &s.id, loaded),
                idle_minutes: if inner.on_screen(&s.id) {
                    Some(0)
                } else {
                    inner
                        .last_used
                        .get(&s.id)
                        .map(|t| t.elapsed().as_secs() / 60)
                },
                chat: if inner.locked || !loaded {
                    None
                } else {
                    inner.current_chat.get(&s.id).cloned()
                },
                in_mode: mode.allows(s.context),
                mail_status: if s.mail.is_some() {
                    Some(
                        inner
                            .mail
                            .get(&s.id)
                            .map_or(MailStatus::Connecting, |m| m.status.clone()),
                    )
                } else if s.talk.is_some() {
                    Some(
                        inner
                            .talk
                            .get(&s.id)
                            .map_or(MailStatus::Connecting, |m| m.status.clone()),
                    )
                } else {
                    None
                },
                reader: inner.reader(&s.id),
                detached: inner.detached.contains(&s.id),
                mail_smtp: s.mail.as_ref().map(|m| {
                    let smtp = m.smtp();
                    format!("{}:{}", smtp.host, smtp.port)
                }),
                probes: inner
                    .health
                    .get(&s.id)
                    .filter(|_| loaded)
                    .map(|h| h.probes.view())
                    .unwrap_or_default(),
            }
        })
        .collect::<Vec<_>>();
    // Contas fora do modo não contam (nem no ícone do Dock/bandeja).
    let total_unread = services
        .iter()
        .filter(|s| s.in_mode)
        .map(|s| s.unread)
        .sum();
    let hidden: HashSet<&str> = services
        .iter()
        .filter(|s| !s.in_mode)
        .map(|s| s.service.id.as_str())
        .collect();
    let mut recent = inner.recent_views();
    recent.retain(|r| !hidden.contains(r.service_id.as_str()));
    Snapshot {
        services,
        active: inner.active.clone(),
        split: inner.split.clone(),
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
        lock_available: inner.lock_method() != LockMethod::None,
        lock_method: inner.lock_method(),
        system_auth: inner.system_auth,
        update: inner.update.clone(),
        app_version: app.package_info().version.to_string(),
        recent,
        silence_summary: silence_view(&inner, mode),
        tasks: crate::organizer::counts(&inner),
        mode,
        mode_manual: inner.mode_manual(),
        lang: papeo_core::i18n::lang(),
        rev: REV.load(Ordering::SeqCst),
    }
}

/// Resumo do silêncio como a interface pode ver (spec 17): nada bloqueado;
/// sem contas fora do modo atual (como as recentes); sem conversas com a
/// prévia oculta.
fn silence_view(inner: &Inner, mode: Mode) -> Option<papeo_core::SilenceSummary> {
    if inner.locked {
        return None;
    }
    let mut summary = inner.silence_summary.clone()?;
    let hidden: Vec<String> = summary
        .accounts
        .iter()
        .filter(|a| {
            inner
                .service(&a.service_id)
                .map_or(true, |svc| !mode.allows(svc.context))
        })
        .map(|a| a.service_id.clone())
        .collect();
    for id in hidden {
        summary = summary.without(&id)?;
    }
    if inner.config.settings.notification_preview == papeo_core::NotificationPreview::Hidden {
        summary = summary.without_chats();
    }
    Some(summary)
}

/// Envia o estado à interface e atualiza badge e bandeja.
/// Só envia se o estado mudou desde o último envio: a interface não refaz a
/// tela à toa (muitos avisos do site não mudam nada visível). Convertido
/// uma vez só para a janela principal e a mini janela.
pub fn publish(app: &AppHandle) -> Snapshot {
    // Último estado enviado (com a versão de então). Montar, comparar e
    // enviar sob o mesmo lock: dois envios simultâneos não chegam trocados.
    // Ordem: este lock e depois o do estado; quem segura o do estado nunca
    // chama `publish`.
    static LAST: Mutex<String> = Mutex::new(String::new());
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let mut snap = snapshot(app);
    let body = serde_json::to_string(&snap).unwrap_or_default();
    if body != *last {
        snap.rev = REV.fetch_add(1, Ordering::SeqCst) + 1;
        *last = serde_json::to_string(&snap).unwrap_or_default();
        if let Ok(raw) = serde_json::value::RawValue::from_string(last.clone()) {
            let _ = app.emit_to(UI_WEBVIEW, STATE_EVENT, &raw);
            // Leitores em janela separada (spec 32).
            for s in snap.services.iter().filter(|s| s.detached && s.reader) {
                let label = format!("{}{}", crate::detach::READER_PREFIX, s.service.id);
                let _ = app.emit_to(label.as_str(), STATE_EVENT, &raw);
            }
            // Fechada, a mini janela recebe o estado atual ao abrir.
            if crate::popover::is_shown() {
                let _ = app.emit_to(crate::popover::POPOVER, STATE_EVENT, &raw);
            }
        }
    }
    drop(last);
    static BADGE: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(-1);
    if let Some(window) = app.get_window(MAIN_WINDOW) {
        let total = i64::from(snap.total_unread);
        if BADGE.swap(total, Ordering::SeqCst) != total {
            let _ = window.set_badge_count((total > 0).then_some(total));
        }
    }
    crate::tray::refresh(app, &snap);
    snap
}

#[cfg(test)]
mod tests {
    use super::*;
    use papeo_core::{Service, ServiceKind};

    #[test]
    fn split_panes_divide_the_area() {
        let area = Area {
            x: 100.0,
            y: 50.0,
            width: 1006.0,
            height: 700.0,
        };
        let split = Split {
            left: "a".into(),
            right: "b".into(),
            ratio: 0.5,
        };
        let (l, r) = split.panes(area);
        assert_eq!((l.x, l.width, l.y, l.height), (100.0, 500.0, 53.0, 697.0));
        assert_eq!((r.x, r.width), (606.0, 500.0));
        assert_eq!(l.width + SPLIT_GAP + r.width, area.width);
        assert_eq!(Split::clamp_ratio(0.9), 0.75);
        assert_eq!(Split::clamp_ratio(f64::NAN), 0.5);
    }

    fn inner_with_service() -> (Inner, String) {
        let service = Service::new(ServiceKind::Whatsapp, None, None).unwrap();
        let id = service.id.clone();
        let config = Config {
            services: vec![service],
            ..Config::default()
        };
        (Inner::new(config), id)
    }

    fn unread(inner: &Inner) -> Vec<bool> {
        inner.recent_views().iter().map(|v| v.unread).collect()
    }

    fn incoming(at: u64) -> ChatMessage {
        ChatMessage {
            body: "Oi".into(),
            at,
            mine: false,
        }
    }

    #[test]
    fn chats_read_when_seen_or_answered() {
        let (mut inner, id) = inner_with_service();
        let past = now_secs() - 60;
        inner.push_recent(&id, "Ana", incoming(past));
        assert_eq!(unread(&inner), [true]);

        // Respondida pelo Papeo: lida.
        inner.push_recent(
            &id,
            "Ana",
            ChatMessage {
                body: "Já vou".into(),
                at: past + 1,
                mine: true,
            },
        );
        assert_eq!(unread(&inner), [false]);

        // Mensagem nova: não lida de novo.
        inner.push_recent(&id, "Ana", incoming(past + 2));
        assert_eq!(unread(&inner), [true]);

        // Aberta numa conta em segundo plano ou com a janela sem foco: não conta.
        inner.current_chat.insert(id.clone(), "Ana".into());
        inner.window_focused = false;
        inner.active = Some(id.clone());
        inner.mark_seen();
        assert_eq!(unread(&inner), [true]);

        // Aberta na conta à vista, com a janela em foco: lida.
        inner.window_focused = true;
        inner.mark_seen();
        assert_eq!(unread(&inner), [false]);
    }

    #[test]
    fn chats_read_on_the_site_leave_unread() {
        let (mut inner, id) = inner_with_service();
        let old = now_secs() - 60;
        inner.push_recent(&id, "Ana", incoming(old));
        inner.push_recent(&id, "Bruno", incoming(old));
        // Bruno ainda com contador no site; Ana lida (no celular, por ex.).
        let list = vec![("Ana".to_owned(), 0), ("Bruno".to_owned(), 2)];
        assert!(inner.apply_chat_list(&id, &list));
        let by_title: Vec<(String, bool)> = inner
            .recent_views()
            .into_iter()
            .map(|v| (v.chat, v.unread))
            .collect();
        assert!(by_title.contains(&("Ana".to_owned(), false)));
        assert!(by_title.contains(&("Bruno".to_owned(), true)));
        // Nada mudou: não pede para publicar de novo.
        assert!(!inner.apply_chat_list(&id, &list));

        // Mensagem que acabou de chegar: o contador do site pode não ter
        // aparecido ainda, então não conta como lida.
        inner.push_recent(&id, "Ana", incoming(now_secs()));
        inner.apply_chat_list(&id, &list);
        assert!(inner
            .recent_views()
            .iter()
            .any(|v| v.chat == "Ana" && v.unread));

        // Outra conta com o mesmo nome não é afetada.
        let other = "svc-outraconta0".to_owned();
        assert!(!inner.apply_chat_list(&other, &list));
    }
}
