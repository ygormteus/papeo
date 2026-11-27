//! Avisos de e-mail por IMAP: um vigia (thread) por conta de e-mail que os
//! usa. Com o IMAP conectado, o contador e os avisos vêm do servidor, e o
//! site pode ficar hibernado. O protocolo fica em `papeo_core::mail`.
//!
//! Locks: o do estado pode ser pego com o registro dos vigias
//! (`MailRuntime::watchers`) seguro, nunca o contrário.

use crate::secrets::{self, Saved};
use crate::state::{publish, AppState, MailLive, MailStatus};
use papeo_core::i18n::{tr, trf};
use papeo_core::mail::{self as core, ImapError, MailAccount, MailAuth, NewMail};
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// Tempo para abrir a conexão com cada endereço do servidor.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Esperas entre tentativas depois de falhas seguidas de rede.
const BACKOFF: [u64; 6] = [5, 15, 30, 60, 120, 300];
/// Senha recusada: tenta de novo só depois disto (ou com senha nova), para
/// o provedor não bloquear a conta por tentativas.
const AUTH_RETRY: Duration = Duration::from_secs(60 * 60);
/// Com o IMAP conectado, o site parado há este tempo hiberna (se a conta
/// pedir).
pub const SLEEP_SITE_AFTER: Duration = Duration::from_secs(2 * 60);

static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Controle de um vigia: parar, acordar da espera e derrubar a conexão.
pub(crate) struct Control {
    generation: u64,
    stop: AtomicBool,
    /// Conexão em uso (para derrubar na hora ao parar ou acordar).
    socket: Mutex<Option<TcpStream>>,
    /// Sobe a cada pedido para acordar.
    wake: Mutex<u64>,
    cv: Condvar,
}

impl Control {
    /// Controle avulso (leitor de e-mail): nunca para sozinho.
    pub(crate) fn detached() -> Self {
        Self {
            generation: 0,
            stop: AtomicBool::new(false),
            socket: Mutex::new(None),
            wake: Mutex::new(0),
            cv: Condvar::new(),
        }
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    /// Derruba a conexão e acorda a espera: o vigia reconecta já.
    fn interrupt(&self) {
        if let Some(socket) = self
            .socket
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            let _ = socket.shutdown(Shutdown::Both);
        }
        *self.wake.lock().unwrap_or_else(|e| e.into_inner()) += 1;
        self.cv.notify_all();
    }

    fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        self.interrupt();
    }

    /// Quantas vezes já pediram para acordar.
    fn wakes(&self) -> u64 {
        *self.wake.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Espera até `timeout`, um pedido para acordar ou parar. `true` = parar.
    fn sleep(&self, timeout: Duration) -> bool {
        let guard = self.wake.lock().unwrap_or_else(|e| e.into_inner());
        let start = *guard;
        let _ = self
            .cv
            .wait_timeout_while(guard, timeout, |w| *w == start && !self.stopped());
        self.stopped()
    }
}

struct Watcher {
    saved: Saved,
    /// Modo de conexão e contador: mudou, o vigia recomeça.
    options: Options,
    control: Arc<Control>,
}

impl Watcher {
    /// A senha guardada é a deste vigia. No login da Microsoft o token de
    /// renovação muda a cada uso (o próprio vigia o troca): vale a mesma
    /// conta; um login novo recomeça o vigia por `restart`.
    fn same_secret(&self, saved: &Saved) -> bool {
        if self.options.microsoft {
            self.saved.host == saved.host
                && self.saved.port == saved.port
                && self.saved.user == saved.user
        } else {
            self.saved == *saved
        }
    }
}

#[derive(Default)]
pub struct MailRuntime {
    watchers: Mutex<HashMap<String, Watcher>>,
}

pub fn init(app: &AppHandle) {
    app.manage(MailRuntime::default());
    // Fora do `setup` (thread principal): lê as senhas e publica o estado.
    let app = app.clone();
    std::thread::spawn(move || sync(&app));
}

/// Liga, troca ou desliga os vigias conforme a configuração e as senhas
/// guardadas. Chamar depois de mudar contas ou senhas.
pub fn sync(app: &AppHandle) {
    sync_with(app, None);
}

/// Como `sync`, recomeçando já o vigia de `id` mesmo com a mesma senha
/// (Salvar de novo depois de liberar o IMAP no provedor, por exemplo).
pub fn restart(app: &AppHandle, id: &str) {
    sync_with(app, Some(id));
}

fn sync_with(app: &AppHandle, fresh: Option<&str>) {
    let Some(runtime) = app.try_state::<MailRuntime>() else {
        return;
    };
    let state = app.state::<AppState>();
    let mut started = Vec::new();
    let mut statuses = Vec::new();
    // Sem senha e com o site para dormir: o site volta a avisar.
    let mut fallback = Vec::new();
    {
        // Tudo sob o registro: duas sincronizações ao mesmo tempo (senha
        // trocada no meio) não voltam a uma configuração ou senha antiga.
        let mut watchers = runtime.watchers.lock().unwrap_or_else(|e| e.into_inner());
        // A senha de conta sem IMAP é apagada por quem tira o IMAP
        // (`forget`), nunca aqui: uma sincronização com a configuração de um
        // instante antes apagaria a senha que acabou de ser salva.
        let accounts: Vec<(String, MailAccount)> = state
            .lock()
            .config
            .services
            .iter()
            .filter_map(|s| {
                let mail = s.mail.clone().filter(|m| s.kind.accepts_mail(m.auth))?;
                Some((s.id.clone(), mail))
            })
            .collect();
        // Senhas lidas sem o lock do estado (arquivo ou cofre do sistema).
        let wanted: HashMap<String, (MailAccount, Option<Saved>)> = accounts
            .into_iter()
            .map(|(id, account)| {
                let saved = secrets::read(app, &id).filter(|s| s.matches(&account));
                (id, (account, saved))
            })
            .collect();
        watchers.retain(|id, watcher| {
            let keep = fresh != Some(id.as_str())
                && wanted
                    .get(id)
                    .and_then(|(_, saved)| saved.as_ref())
                    .is_some_and(|saved| watcher.same_secret(saved))
                && wanted
                    .get(id)
                    .is_some_and(|(account, _)| Options::of(account) == watcher.options);
            if !keep {
                watcher.control.stop();
            }
            keep
        });
        for (id, (account, saved)) in &wanted {
            if watchers.contains_key(id) {
                continue;
            }
            let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
            let Some(saved) = saved.clone() else {
                statuses.push((id.clone(), MailStatus::NoPassword, generation));
                continue;
            };
            let control = Arc::new(Control {
                generation,
                stop: AtomicBool::new(false),
                socket: Mutex::new(None),
                wake: Mutex::new(0),
                cv: Condvar::new(),
            });
            watchers.insert(
                id.clone(),
                Watcher {
                    saved: saved.clone(),
                    options: Options::of(account),
                    control: control.clone(),
                },
            );
            statuses.push((id.clone(), MailStatus::Connecting, generation));
            started.push((id.clone(), Options::of(account), saved, control));
        }
        let mut inner = state.lock();
        inner.mail.retain(|id, _| wanted.contains_key(id));
        for (id, status, generation) in statuses {
            let before = inner.mail.get(&id);
            // Acabou de ficar sem senha: o site volta a avisar (uma vez, não
            // a cada sincronização, que desfaria uma hibernação manual).
            let sleeps = wanted.get(&id).is_some_and(|(a, _)| a.sleep_site);
            if status == MailStatus::NoPassword
                && sleeps
                && before.is_none_or(|b| b.status != MailStatus::NoPassword)
            {
                fallback.push(id.clone());
            }
            // O último número continua até a primeira consulta nova.
            let unread = before.map_or(0, |b| b.unread);
            inner.mail.insert(
                id,
                MailLive {
                    status,
                    unread,
                    generation,
                },
            );
        }
    }
    for (id, options, saved, control) in started {
        let app = app.clone();
        std::thread::Builder::new()
            .name(format!("papeo-imap-{id}"))
            .spawn(move || run(&app, &id, options, &saved, &control))
            .ok();
    }
    publish(app);
    for id in fallback {
        site_fallback(app, &id, false);
    }
}

/// O IMAP não cobre a conta (desligado, sem senha, senha recusada ou sem
/// conexão há um tempo): reabre o site escondido para os avisos voltarem
/// por ele. `only_if_sleeping`: só se a conta pede o site hibernado (o
/// IMAP é quem o tinha feito dormir).
pub fn site_fallback(app: &AppHandle, id: &str, only_if_sleeping: bool) {
    if only_if_sleeping {
        let sleeps = app
            .state::<AppState>()
            .lock()
            .service(id)
            .is_ok_and(|s| s.mail.as_ref().is_some_and(|m| m.sleep_site));
        if !sleeps {
            return;
        }
    }
    if crate::webviews::load_hidden(app, id) {
        crate::webviews::apply_layout(app);
        publish(app);
    }
}

/// Computador acordou ou a rede voltou: reconecta todos já.
pub fn kick(app: &AppHandle) {
    let Some(runtime) = app.try_state::<MailRuntime>() else {
        return;
    };
    let watchers = runtime.watchers.lock().unwrap_or_else(|e| e.into_inner());
    for watcher in watchers.values() {
        watcher.control.interrupt();
    }
}

/// Esquece o IMAP de uma conta (removida ou com os dados apagados).
pub fn forget(app: &AppHandle, id: &str) {
    secrets::delete(app, id);
    crate::address_book::forget(app, id);
    crate::mail_cache::forget(app, id);
}

/// Grava a situação, se este vigia ainda é o dono do registro. Publica se
/// algo visível mudou.
fn set_status(app: &AppHandle, id: &str, generation: u64, status: MailStatus) {
    let changed = {
        let state = app.state::<AppState>();
        let mut inner = state.lock();
        match inner.mail.get_mut(id) {
            Some(live) if live.generation == generation && live.status != status => {
                live.status = status;
                true
            }
            _ => false,
        }
    };
    if changed {
        publish(app);
    }
}

fn set_unread(app: &AppHandle, id: &str, generation: u64, count: u32) {
    let count = count.min(crate::webviews::UNREAD_MAX);
    let changed = {
        let state = app.state::<AppState>();
        let mut inner = state.lock();
        match inner.mail.get_mut(id) {
            Some(live) if live.generation == generation && live.unread != count => {
                live.unread = count;
                true
            }
            _ => false,
        }
    };
    if changed {
        publish(app);
    }
}

/// Falhas de rede seguidas até o site voltar a avisar (≈ 50 s sem IMAP).
const FALLBACK_AFTER: usize = 3;

/// Como conectar e contar (o vigia recomeça quando a conta muda).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Options {
    /// Gmail: só a aba Principal.
    primary_only: bool,
    /// Conexão comum cifrada com STARTTLS, em vez de TLS direto.
    starttls: bool,
    /// Login da Microsoft (XOAUTH2) em vez de senha.
    microsoft: bool,
}

impl Options {
    fn of(account: &MailAccount) -> Self {
        Self {
            primary_only: account.primary_only(),
            starttls: account.starttls,
            microsoft: account.auth == MailAuth::Microsoft,
        }
    }
}

fn run(app: &AppHandle, id: &str, options: Options, saved: &Saved, control: &Arc<Control>) {
    let mut failures = 0usize;
    let mut warned = false;
    // Até onde já avisou, de uma conexão para a outra.
    let mut cursor = None;
    // Login da Microsoft: token de renovação atual e token de acesso.
    let mut saved = saved.clone();
    let mut access = None;
    // Uma reconexão rápida, sem mostrar erro, depois de uma sessão que
    // chegou a conectar (servidor que fecha conexões antigas, token vencido).
    let mut quick = false;
    loop {
        if control.stopped() {
            return;
        }
        let mut connected = false;
        let wakes = control.wakes();
        let result = session(
            app,
            id,
            options,
            &mut saved,
            &mut access,
            control,
            &mut cursor,
            &mut connected,
        );
        if control.stopped() {
            return;
        }
        if connected {
            failures = 0;
            quick = true;
        }
        match result {
            Ok(()) => {}
            Err(ImapError::Io(_)) if connected && quick => {
                quick = false;
                if control.sleep(Duration::from_secs(1)) {
                    return;
                }
            }
            // Conexão derrubada de propósito (volta do repouso): reconecta
            // logo, sem mostrar erro.
            Err(ImapError::Io(_)) if control.wakes() != wakes => {
                if control.sleep(Duration::from_secs(1)) {
                    return;
                }
            }
            Err(ImapError::Auth(_)) => {
                set_status(app, id, control.generation, MailStatus::Auth);
                if !warned {
                    warned = true;
                    auth_failed(app, id, options.microsoft);
                }
                let until = Instant::now() + AUTH_RETRY;
                while let Some(left) = until.checked_duration_since(Instant::now()) {
                    if control.sleep(left) {
                        return;
                    }
                }
            }
            Err(err) => {
                set_status(
                    app,
                    id,
                    control.generation,
                    MailStatus::Offline {
                        detail: describe(&err),
                    },
                );
                let wait = BACKOFF[failures.min(BACKOFF.len() - 1)];
                failures += 1;
                if failures == FALLBACK_AFTER {
                    site_fallback(app, id, true);
                }
                if control.sleep(Duration::from_secs(wait)) {
                    return;
                }
            }
        }
    }
}

/// Texto curto para a tela.
pub(crate) fn describe(err: &ImapError) -> String {
    match err {
        ImapError::Io(e) if e.kind() == io::ErrorKind::Other && e.to_string().contains("TLS") => {
            tr("Conexão segura recusada: confira o servidor e a porta.").into()
        }
        ImapError::Io(_) => tr("Sem conexão com o servidor; tentando de novo.").into(),
        ImapError::Auth(_) => tr("Usuário ou senha recusados.").into(),
        ImapError::Rejected(text) | ImapError::Protocol(text) => {
            trf("O servidor respondeu: {texto}", &[("texto", &text)])
        }
    }
}

/// Senha recusada: avisa uma vez (respeitando não perturbe, silêncio e
/// modo) e, se o site dormia pelo IMAP, reabre-o escondido.
fn auth_failed(app: &AppHandle, id: &str, microsoft: bool) {
    let (name, allowed) = {
        let state = app.state::<AppState>();
        let inner = state.lock();
        let Ok(service) = inner.service(id) else {
            return;
        };
        let allowed = inner.current_mode().allows(service.context)
            && papeo_core::should_notify(
                &inner.config.settings,
                service,
                crate::state::now_secs(),
                crate::state::local_minute_of_day(),
                papeo_core::Mode::All,
            );
        (service.name.clone(), allowed)
    };
    let target = crate::notifications::Target {
        service_id: id.to_owned(),
        reply: false,
        later: false,
        chat: None,
        task: None,
        remind_at: None,
    };
    let text = if microsoft {
        tr("Avisos por IMAP: o login da Microsoft expirou. Entre de novo em Avisos por IMAP; até lá, os avisos vêm pelo site.")
    } else {
        tr("Avisos por IMAP: usuário ou senha recusados. Os avisos voltam pelo site até você corrigir.")
    };
    if allowed {
        let _ = crate::notifications::show(app, &name, text, None, target);
    }
    site_fallback(app, id, true);
}

/// Conexão TLS (só TLS direto, porta 993 em geral).
pub(crate) struct Tls(native_tls::TlsStream<TcpStream>);

impl Read for Tls {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for Tls {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl core::Transport for Tls {
    fn set_timeout(&mut self, timeout: Duration) -> io::Result<()> {
        let socket = self.0.get_ref();
        socket.set_read_timeout(Some(timeout))?;
        socket.set_write_timeout(Some(timeout))
    }
}

/// Abre a conexão cifrada: TLS direto ou, com `starttls`, conexão comum
/// que pede STARTTLS antes de qualquer outra coisa (sem ele, falha).
fn connect(host: &str, port: u16, starttls: bool, control: &Control) -> Result<Tls, ImapError> {
    let addrs: Vec<_> = (host, port).to_socket_addrs()?.collect();
    let mut last = io::Error::new(io::ErrorKind::NotFound, "servidor não encontrado");
    let socket = addrs
        .iter()
        .find_map(
            |addr| match TcpStream::connect_timeout(addr, CONNECT_TIMEOUT) {
                Ok(socket) => Some(socket),
                Err(err) => {
                    last = err;
                    None
                }
            },
        )
        .ok_or(last)?;
    socket.set_read_timeout(Some(Duration::from_secs(30)))?;
    socket.set_write_timeout(Some(Duration::from_secs(30)))?;
    // Registrada antes do aperto de mão: parar derruba até ele.
    *control.socket.lock().unwrap_or_else(|e| e.into_inner()) = Some(socket.try_clone()?);
    if control.stopped() {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "parado").into());
    }
    let mut socket = socket;
    if starttls {
        core::starttls_preamble(&mut socket)?;
    }
    let connector =
        native_tls::TlsConnector::new().map_err(|e| io::Error::other(format!("TLS: {e}")))?;
    let stream = connector
        .connect(host, socket)
        .map_err(|e| io::Error::other(format!("TLS: {e}")))?;
    Ok(Tls(stream))
}

struct AppSink<'a> {
    app: &'a AppHandle,
    id: &'a str,
    control: &'a Control,
    connected: &'a mut bool,
}

impl core::Sink for AppSink<'_> {
    fn connected(&mut self) {
        *self.connected = true;
        set_status(self.app, self.id, self.control.generation, MailStatus::Ok);
    }

    fn unread(&mut self, count: u32) {
        set_unread(self.app, self.id, self.control.generation, count);
    }

    fn new_mail(&mut self, mails: Vec<NewMail>) {
        for mail in mails {
            if self.control.stopped() {
                return;
            }
            let from = if mail.from.is_empty() {
                tr("E-mail novo").to_owned()
            } else {
                mail.from
            };
            let subject = if mail.subject.is_empty() {
                tr("(sem assunto)").to_owned()
            } else {
                mail.subject
            };
            // Clique: no leitor, abre esta mensagem (pelo UID guardado);
            // no site, busca o assunto.
            remember_notified(self.id, &subject, mail.uid);
            let _ =
                crate::notify::deliver(self.app, self.id, &from, &subject, Some(&subject), false);
        }
    }

    fn stopped(&self) -> bool {
        self.control.stopped()
    }

    fn wait(&mut self, timeout: Duration) -> bool {
        self.control.sleep(timeout)
    }
}

/// Avisos recentes de e-mail novo (conta, assunto como a busca recebe,
/// UID na Caixa de Entrada): o clique abre a mensagem certa no leitor.
type Notified = (String, String, u32, Instant);
static NOTIFIED: std::sync::Mutex<Vec<Notified>> = std::sync::Mutex::new(Vec::new());
const NOTIFIED_MAX: usize = 100;
/// Aviso mais velho que isto não abre pelo UID (o clique procura o assunto).
const NOTIFIED_TTL: Duration = Duration::from_secs(30 * 60);

/// Assunto como `open_chat` o recebe (sem controles, cortado).
fn notified_key(subject: &str) -> String {
    let clean: String = subject.chars().filter(|c| !c.is_control()).collect();
    papeo_core::clip(&clean, 100).to_string()
}

fn remember_notified(id: &str, subject: &str, uid: u32) {
    let mut list = NOTIFIED.lock().unwrap_or_else(|e| e.into_inner());
    list.retain(|n| n.3.elapsed() < NOTIFIED_TTL);
    list.push((id.to_owned(), notified_key(subject), uid, Instant::now()));
    let extra = list.len().saturating_sub(NOTIFIED_MAX);
    list.drain(..extra);
}

/// UID do aviso recente da conta com este assunto; usado uma vez só (um
/// clique depois, numa lista de recentes com o mesmo assunto, procura).
pub fn notified_uid(id: &str, subject: &str) -> Option<u32> {
    let key = notified_key(subject);
    let mut list = NOTIFIED.lock().unwrap_or_else(|e| e.into_inner());
    list.retain(|n| n.3.elapsed() < NOTIFIED_TTL);
    let at = list.iter().rposition(|(i, s, _, _)| i == id && *s == key)?;
    Some(list.remove(at).2)
}

/// Token de acesso da Microsoft e quando vence.
pub(crate) type Access = Option<(String, Instant)>;

/// Token de acesso válido (renovando se preciso). O token de renovação
/// novo vai para o cofre só se o guardado ainda é o que este vigia usou.
pub(crate) fn microsoft_token(
    app: &AppHandle,
    id: &str,
    saved: &mut Saved,
    access: &mut Access,
    control: &Control,
    scope: crate::microsoft::Scope,
) -> Result<String, ImapError> {
    use crate::microsoft::TokenError;
    if let Some((token, until)) = access.as_ref() {
        if Instant::now() < *until {
            return Ok(token.clone());
        }
    }
    let tokens = tauri::async_runtime::block_on(crate::microsoft::refresh(&saved.secret, scope))
        .map_err(|err| match err {
            TokenError::Reauth => ImapError::Auth("login da Microsoft vencido".into()),
            TokenError::Network(text) => ImapError::Io(io::Error::other(text)),
            TokenError::Refused(text) => ImapError::Rejected(text),
        })?;
    // Parado durante a renovação: não grava nada (outro vigia pode já ter
    // assumido a conta).
    if control.stopped() {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "parado").into());
    }
    if let Some(next) = tokens.refresh_token.filter(|t| !t.is_empty()) {
        if next != saved.secret {
            let fresh = Saved {
                secret: next,
                ..saved.clone()
            };
            match secrets::replace(app, id, &saved.secret, &fresh) {
                Ok(true) => *saved = fresh,
                // Outro vigia ou login já trocou: adota o guardado, se é da
                // mesma conta (o deste vigia pode não ser mais renovado).
                Ok(false) => {
                    if let Some(stored) = secrets::read(app, id).filter(|s| {
                        s.host == saved.host && s.port == saved.port && s.user == saved.user
                    }) {
                        *saved = stored;
                    }
                }
                // Cofre falhou: segue com o antigo, que a Microsoft ainda
                // aceita por um tempo; tenta gravar na próxima renovação.
                Err(_) => {}
            }
        }
    }
    // Margem de 5 min antes do vencimento informado (em geral 1 h).
    let life = Duration::from_secs(tokens.expires_in.clamp(60, 24 * 3600));
    let until = Instant::now() + life.saturating_sub(Duration::from_secs(300));
    *access = Some((tokens.access_token.clone(), until));
    Ok(tokens.access_token)
}

/// Conecta e entra (senha ou login da Microsoft).
pub(crate) fn open(
    app: &AppHandle,
    id: &str,
    starttls: bool,
    microsoft: bool,
    saved: &mut Saved,
    access: &mut Access,
    control: &Control,
) -> Result<core::Imap<Tls>, ImapError> {
    use crate::microsoft::Scope;
    // Token antes da conexão: a renovação não a deixa parada esperando.
    let cached = access.is_some();
    let token = if microsoft {
        Some(microsoft_token(
            app,
            id,
            saved,
            access,
            control,
            Scope::Read,
        )?)
    } else {
        None
    };
    let stream = connect(&saved.host, saved.port, starttls, control)?;
    let mut imap = if starttls {
        core::Imap::after_starttls(stream)
    } else {
        core::Imap::connect(stream)?
    };
    if let Some(token) = token {
        match imap.authenticate_xoauth2(&saved.user, &token) {
            // Token guardado recusado (revogado antes da hora): renova
            // e tenta de novo uma vez na mesma conexão.
            Err(ImapError::Auth(_)) if cached => {
                *access = None;
                let token = microsoft_token(app, id, saved, access, control, Scope::Read)?;
                imap.authenticate_xoauth2(&saved.user, &token)?;
            }
            other => other?,
        }
    } else {
        let account = MailAccount {
            host: saved.host.clone(),
            port: saved.port,
            user: saved.user.clone(),
            sleep_site: false,
            starttls,
            whole_inbox: false,
            auth: MailAuth::Password,
            signature: String::new(),
            signature_html: Default::default(),
        };
        imap.login(&saved.user, &account.login_secret(&saved.secret))?;
    }
    Ok(imap)
}

#[allow(clippy::too_many_arguments)]
fn session(
    app: &AppHandle,
    id: &str,
    options: Options,
    saved: &mut Saved,
    access: &mut Access,
    control: &Control,
    cursor: &mut Option<core::Cursor>,
    connected: &mut bool,
) -> Result<(), ImapError> {
    let result = (|| {
        let mut imap = open(
            app,
            id,
            options.starttls,
            options.microsoft,
            saved,
            access,
            control,
        )?;
        let mut sink = AppSink {
            app,
            id,
            control,
            connected,
        };
        core::watch(
            &mut imap,
            options.primary_only,
            core::WatchTimes::default(),
            cursor,
            &mut sink,
        )
    })();
    // Em qualquer saída: a cópia guardada para derrubar a conexão não a
    // mantém aberta durante a espera.
    *control.socket.lock().unwrap_or_else(|e| e.into_inner()) = None;
    result
}
