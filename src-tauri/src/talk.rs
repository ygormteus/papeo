//! Leitor do Nextcloud Talk pela API (spec 29): login pelo Login Flow v2
//! (senha de app no cofre), comandos de conversa e mensagem e um vigia por
//! conta que mantém o contador e avisa mensagens novas sem a página aberta.
//! A leitura das respostas fica em `papeo_core::talk`.
//!
//! Locks: o do estado só por um instante (ler a conta, gravar a situação),
//! nunca durante a rede; o registro dos vigias e os caches têm `Mutex`
//! próprio e nunca são pegos junto com o do estado.

use crate::secrets::{self, Saved};
use crate::state::{publish, AppState, MailLive, MailStatus};
use papeo_core::i18n::{tr, trf};
use papeo_core::talk::{self as core, Mention, Message, Room, TalkAccount};
use papeo_core::ServiceKind;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// Resultado do login (`{ id, ok, error }`).
pub const LOGIN_EVENT: &str = "papeo://talk-login";
/// Clique num aviso: abre a conversa no leitor (`{ id, token }`).
pub const REVEAL_EVENT: &str = "papeo://talk-reveal";
/// Prefixo do `open` dos avisos do Talk (vira o token da conversa).
pub const OPEN_PREFIX: &str = "talk:";
/// Lista de conversas a cada consulta do vigia (`{ id, rooms }`): a tela
/// usa esta em vez de pedir de novo.
pub const ROOMS_EVENT: &str = "papeo://talk-rooms";

const LOGIN_WAIT: Duration = Duration::from_secs(20 * 60);
const POLL_ACTIVE: Duration = Duration::from_secs(30);
const POLL_IDLE: Duration = Duration::from_secs(60);
const RETRY_MAX: Duration = Duration::from_secs(5 * 60);
const FILE_MAX: usize = 200 * 1024 * 1024;
const UPLOAD_MAX: u64 = 100 * 1024 * 1024;
const IMAGE_MAX: usize = 2 * 1024 * 1024;
const CACHE_MAX: usize = 200;
/// Teto do cache de imagens em bytes (`data:` já em base64).
const CACHE_BYTES: usize = 20 * 1024 * 1024;

static GENERATION: AtomicU64 = AtomicU64::new(0);

struct Watcher {
    key: String,
    stop: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct Talks {
    watchers: Mutex<HashMap<String, Watcher>>,
    /// Logins em andamento: o sinal de cancelar de cada conta.
    logins: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// Fotos das conversas e miniaturas (`data:`), só na memória.
    images: Mutex<HashMap<String, String>>,
}

pub fn init(app: &AppHandle) {
    app.manage(Talks::default());
    sync(app);
}

// ---------------------------------------------------------------------------
// HTTP

fn http() -> Result<reqwest::Client, String> {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client.clone());
    }
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        // A espera longa do chat é de 30 s; o resto é bem mais curto.
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("Papeo/", env!("CARGO_PKG_VERSION")))
        // Redirecionamento para http: nunca (a senha de app iria aberta).
        .https_only(true)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(CLIENT.get_or_init(|| client).clone())
}

/// Conta e senha de app de `id`.
struct Creds {
    account: TalkAccount,
    password: String,
}

fn creds(app: &AppHandle, id: &str) -> Result<Creds, String> {
    let account = app
        .state::<AppState>()
        .lock()
        .service(id)?
        .talk
        .clone()
        .ok_or(tr("Conecte o Talk primeiro."))?;
    // Fora do lock: arquivo ou cofre do sistema.
    let saved = secrets::read(app, id)
        .filter(|s| matches(s, &account))
        .ok_or(tr("Conecte o Talk de novo neste computador."))?;
    Ok(Creds {
        account,
        password: saved.secret,
    })
}

fn matches(saved: &Saved, account: &TalkAccount) -> bool {
    saved.port == 0 && saved.host == account.server && saved.user == account.user
}

/// URL da API a partir do servidor e de pedaços de caminho (cada um
/// codificado: token e caminho de arquivo não escapam da pasta).
fn url(base: &str, fixed: &str, segments: &[&str]) -> Result<reqwest::Url, String> {
    let mut url = reqwest::Url::parse(&format!("{base}{fixed}")).map_err(|e| e.to_string())?;
    if !segments.is_empty() {
        url.path_segments_mut()
            .map_err(|_| tr("Endereço inválido.").to_string())?
            .pop_if_empty()
            .extend(segments);
    }
    Ok(url)
}

enum Fail {
    /// Senha de app recusada (revogada).
    Auth,
    Other(String),
}

impl From<Fail> for String {
    fn from(f: Fail) -> Self {
        match f {
            Fail::Auth => tr("O Nextcloud recusou o acesso. Conecte o Talk de novo.").into(),
            Fail::Other(e) => e,
        }
    }
}

fn net(e: reqwest::Error) -> Fail {
    Fail::Other(trf(
        "Sem conexão com o Nextcloud ({erro}).",
        &[("erro", &papeo_core::clip(&e.to_string(), 200))],
    ))
}

async fn send(c: &Creds, req: reqwest::RequestBuilder) -> Result<reqwest::Response, Fail> {
    let res = req
        .basic_auth(&c.account.user, Some(&c.password))
        .header("OCS-APIRequest", "true")
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(net)?;
    match res.status().as_u16() {
        401 => Err(Fail::Auth),
        _ => Ok(res),
    }
}

/// Pedido OCS que precisa dar certo; devolve o `data`.
async fn ocs(c: &Creds, req: reqwest::RequestBuilder) -> Result<Value, Fail> {
    let res = send(c, req).await?;
    let status = res.status();
    let body = res.bytes().await.map_err(net)?;
    if !status.is_success() {
        return Err(Fail::Other(trf(
            "O Nextcloud recusou o pedido ({codigo}).",
            &[("codigo", &status.as_u16().to_string())],
        )));
    }
    core::ocs_data(&body).ok_or_else(|| Fail::Other(tr("Resposta inesperada do Nextcloud.").into()))
}

fn check_token(token: &str) -> Result<(), String> {
    core::valid_token(token)
        .then_some(())
        .ok_or(tr("Conversa inválida.").into())
}

// ---------------------------------------------------------------------------
// Login Flow v2

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LoginResult {
    id: String,
    ok: bool,
    error: Option<String>,
}

/// Começa o login: abre a aprovação no navegador e espera a senha de app.
pub async fn login(app: AppHandle, id: String) -> Result<(), String> {
    let base = {
        let state = app.state::<AppState>();
        let inner = state.lock();
        let service = inner.service(&id)?;
        if service.kind != ServiceKind::Nextcloud {
            return Err(tr("Esta conta não é do Nextcloud.").into());
        }
        core::talk_base(&service.url)
            .ok_or(tr("O endereço do Nextcloud precisa começar com https://."))?
    };
    // O sinal de cancelar vale desde já (cancelar durante o pedido inicial).
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let talks = app.state::<Talks>();
        let mut logins = talks.logins.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(old) = logins.insert(id.clone(), cancel.clone()) {
            old.store(true, Ordering::SeqCst);
        }
    }
    let client = http()?;
    let res = client
        .post(format!("{base}/index.php/login/v2"))
        .send()
        .await
        .map_err(|e| String::from(net(e)))?;
    if !res.status().is_success() {
        return Err(tr("Este servidor não ofereceu o login do Nextcloud.").into());
    }
    let start: Value = res
        .json()
        .await
        .map_err(|_| tr("Resposta inesperada do Nextcloud."))?;
    let login = start["login"].as_str().unwrap_or_default().to_owned();
    let token = start["poll"]["token"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let endpoint = start["poll"]["endpoint"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    if token.is_empty()
        || !core::login_urls_ok(&base, &login, None)
        || !core::login_urls_ok(&base, &endpoint, None)
    {
        return Err(tr("O servidor mandou um login de outro endereço; nada foi aberto.").into());
    }
    if cancel.load(Ordering::SeqCst) {
        return Ok(());
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(&login, None::<&str>)
        .map_err(|e| e.to_string())?;
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = wait_login(&app2, &id, &base, &endpoint, &token, &cancel).await;
        {
            let talks = app2.state::<Talks>();
            let mut logins = talks.logins.lock().unwrap_or_else(|e| e.into_inner());
            if logins.get(&id).is_some_and(|c| Arc::ptr_eq(c, &cancel)) {
                logins.remove(&id);
            }
        }
        if cancel.load(Ordering::SeqCst) {
            return;
        }
        let (ok, error) = match result {
            Ok(()) => (true, None),
            Err(e) => (false, Some(e)),
        };
        let _ = app2.emit(LOGIN_EVENT, LoginResult { id, ok, error });
    });
    Ok(())
}

pub fn login_cancel(app: &AppHandle, id: &str) {
    let talks = app.state::<Talks>();
    let mut logins = talks.logins.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(cancel) = logins.remove(id) {
        cancel.store(true, Ordering::SeqCst);
    }
}

async fn wait_login(
    app: &AppHandle,
    id: &str,
    base: &str,
    endpoint: &str,
    token: &str,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let client = http()?;
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("token", token)
        .finish();
    let until = Instant::now() + LOGIN_WAIT;
    let found = loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(String::new());
        }
        if Instant::now() >= until {
            return Err(tr("O login não foi aprovado a tempo. Tente de novo.").into());
        }
        let _ = tauri::async_runtime::spawn_blocking(|| std::thread::sleep(Duration::from_secs(2)))
            .await;
        let Ok(res) = client
            .post(endpoint)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body.clone())
            .send()
            .await
        else {
            continue;
        };
        if res.status().as_u16() == 200 {
            let v: Value = res
                .json()
                .await
                .map_err(|_| tr("Resposta inesperada do Nextcloud."))?;
            break v;
        }
    };
    let server = found["server"].as_str().unwrap_or_default();
    let user = found["loginName"].as_str().unwrap_or_default().to_owned();
    let password = found["appPassword"].as_str().unwrap_or_default().to_owned();
    if !core::login_urls_ok(base, base, Some(server)) || user.is_empty() || password.is_empty() {
        return Err(tr("O servidor devolveu o login de outro endereço; nada foi salvo.").into());
    }
    // Id do usuário (WebDAV e autoria das mensagens).
    let mut creds = Creds {
        account: TalkAccount {
            server: base.to_owned(),
            user: user.clone(),
            uid: user.clone(),
        },
        password: password.clone(),
    };
    let me = ocs(
        &creds,
        client.get(url(base, "/ocs/v2.php/cloud/user", &[])?),
    )
    .await
    .map_err(String::from)?;
    if let Some(uid) = me["id"].as_str().filter(|u| !u.is_empty()) {
        creds.account.uid = uid.to_owned();
    }
    let account = creds
        .account
        .clone()
        .sanitized()
        .ok_or(tr("Resposta inesperada do Nextcloud."))?;
    let saved = Saved {
        host: account.server.clone(),
        port: 0,
        user: account.user.clone(),
        secret: password,
    };
    // Cancelado enquanto confirmava o usuário: a senha nova não fica valendo.
    if cancel.load(Ordering::SeqCst) {
        revoke(&creds).await;
        return Ok(());
    }
    // Site numa janela separada vira leitor: volta à principal (spec 32).
    crate::detach::attach_quiet(app, id);
    // Conta antes da senha (como no e-mail): uma sincronização no meio não
    // apaga nada; sem a senha, só pede conectar de novo.
    let state = app.state::<AppState>();
    let previous = state.update(|inner| {
        let service = inner.service_mut(id)?;
        let previous = service.talk.replace(account.clone());
        inner.mail_setup.remove(id);
        Ok(previous)
    })?;
    if let Err(err) = secrets::write(app, id, &saved) {
        let _ = state.update(|inner| {
            inner.service_mut(id)?.talk = previous;
            Ok(())
        });
        return Err(err);
    }
    restart(app, id);
    Ok(())
}

/// Desconecta: revoga a senha de app (melhor esforço) e esquece a conta.
pub async fn logout(app: AppHandle, id: String) -> Result<(), String> {
    // Leitor numa janela separada: volta à principal (spec 32).
    crate::detach::attach_quiet(&app, &id);
    if let Ok(c) = creds(&app, &id) {
        revoke(&c).await;
    }
    app.state::<AppState>().update(|inner| {
        inner.service_mut(&id)?.talk = None;
        // Continua no leitor, na tela de conectar (não volta para o site).
        inner.mail_setup.insert(id.clone());
        Ok(())
    })?;
    secrets::delete(&app, &id);
    sync(&app);
    Ok(())
}

/// Conta removida: revoga a senha de app (em segundo plano).
pub fn forget_remote(app: &AppHandle, id: &str) {
    if let Ok(c) = creds(app, id) {
        tauri::async_runtime::spawn(async move { revoke(&c).await });
    }
}

async fn revoke(c: &Creds) {
    let Ok(client) = http() else { return };
    if let Ok(u) = url(&c.account.server, "/ocs/v2.php/core/apppassword", &[]) {
        let _ = send(c, client.delete(u)).await;
    }
}

// ---------------------------------------------------------------------------
// Conversas e mensagens

async fn rooms_of(c: &Creds) -> Result<Vec<Room>, Fail> {
    let client = http().map_err(Fail::Other)?;
    let u = url(
        &c.account.server,
        "/ocs/v2.php/apps/spreed/api/v4/room",
        &[],
    )
    .map_err(Fail::Other)?;
    let data = ocs(c, client.get(u)).await?;
    Ok(core::parse_rooms(&data, &c.account.uid))
}

pub async fn rooms(app: AppHandle, id: String) -> Result<Vec<Room>, String> {
    let c = creds(&app, &id)?;
    match rooms_of(&c).await {
        Err(Fail::Auth) => {
            mark_auth(&app, &id);
            Err(Fail::Auth.into())
        }
        other => other.map_err(String::from),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Chunk {
    pub messages: Vec<Message>,
    /// Maior id visto (escondidas inclusive), para a próxima espera.
    pub last: i64,
    /// Menor id visto, para pedir as anteriores.
    pub first: i64,
    /// Pode haver anteriores.
    pub more: bool,
}

const CHUNK: usize = 50;

fn chunk(data: &Value, uid: &str) -> Chunk {
    let ids: Vec<i64> = data
        .as_array()
        .map(|a| a.iter().filter_map(|m| m["id"].as_i64()).collect())
        .unwrap_or_default();
    Chunk {
        messages: core::parse_messages(data, uid),
        last: core::last_id(data),
        first: ids.iter().copied().min().unwrap_or(0),
        more: ids.len() >= CHUNK,
    }
}

/// As últimas mensagens, ou as anteriores a `before`.
pub async fn messages(
    app: AppHandle,
    id: String,
    token: String,
    before: Option<i64>,
) -> Result<Chunk, String> {
    check_token(&token)?;
    let c = creds(&app, &id)?;
    let client = http()?;
    let mut u = url(
        &c.account.server,
        "/ocs/v2.php/apps/spreed/api/v1/chat",
        &[&token],
    )?;
    {
        let mut q = u.query_pairs_mut();
        q.append_pair("lookIntoFuture", "0")
            .append_pair("limit", &CHUNK.to_string())
            .append_pair("setReadMarker", "0");
        if let Some(before) = before.filter(|b| *b > 0) {
            q.append_pair("lastKnownMessageId", &before.to_string());
        }
    }
    let res = send(&c, client.get(u)).await.map_err(String::from)?;
    if res.status().as_u16() == 304 {
        return Ok(Chunk {
            messages: Vec::new(),
            last: 0,
            first: 0,
            more: false,
        });
    }
    let status = res.status();
    let body = res.bytes().await.map_err(|e| String::from(net(e)))?;
    if !status.is_success() {
        return Err(trf(
            "O Nextcloud recusou o pedido ({codigo}).",
            &[("codigo", &status.as_u16().to_string())],
        ));
    }
    let data = core::ocs_data(&body).ok_or(tr("Resposta inesperada do Nextcloud."))?;
    Ok(chunk(&data, &c.account.uid))
}

/// Espera (até 30 s) mensagens depois de `after`.
pub async fn wait(app: AppHandle, id: String, token: String, after: i64) -> Result<Chunk, String> {
    check_token(&token)?;
    let c = creds(&app, &id)?;
    let client = http()?;
    let mut u = url(
        &c.account.server,
        "/ocs/v2.php/apps/spreed/api/v1/chat",
        &[&token],
    )?;
    u.query_pairs_mut()
        .append_pair("lookIntoFuture", "1")
        .append_pair("timeout", "30")
        .append_pair("limit", "100")
        .append_pair("setReadMarker", "0")
        .append_pair("lastKnownMessageId", &after.max(0).to_string());
    let res = send(&c, client.get(u).timeout(Duration::from_secs(45)))
        .await
        .map_err(String::from)?;
    if res.status().as_u16() == 304 {
        return Ok(Chunk {
            messages: Vec::new(),
            last: after,
            first: after,
            more: false,
        });
    }
    let status = res.status();
    let body = res.bytes().await.map_err(|e| String::from(net(e)))?;
    if !status.is_success() {
        return Err(trf(
            "O Nextcloud recusou o pedido ({codigo}).",
            &[("codigo", &status.as_u16().to_string())],
        ));
    }
    let data = core::ocs_data(&body).ok_or(tr("Resposta inesperada do Nextcloud."))?;
    let mut out = chunk(&data, &c.account.uid);
    out.last = out.last.max(after);
    Ok(out)
}

pub async fn send_message(
    app: AppHandle,
    id: String,
    token: String,
    text: String,
    reply_to: Option<i64>,
) -> Result<Option<Message>, String> {
    check_token(&token)?;
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    if text.chars().count() > 32_000 {
        return Err(tr("Mensagem longa demais.").into());
    }
    let c = creds(&app, &id)?;
    let client = http()?;
    let u = url(
        &c.account.server,
        "/ocs/v2.php/apps/spreed/api/v1/chat",
        &[&token],
    )?;
    let mut body = serde_json::json!({ "message": text });
    if let Some(r) = reply_to.filter(|r| *r > 0) {
        body["replyTo"] = r.into();
    }
    let data = ocs(&c, client.post(u).json(&body))
        .await
        .map_err(String::from)?;
    Ok(
        core::parse_messages(&Value::Array(vec![data]), &c.account.uid)
            .into_iter()
            .next(),
    )
}

pub async fn mark_read(app: AppHandle, id: String, token: String, last: i64) -> Result<(), String> {
    check_token(&token)?;
    let c = creds(&app, &id)?;
    let client = http()?;
    let u = url(
        &c.account.server,
        "/ocs/v2.php/apps/spreed/api/v1/chat",
        &[&token, "read"],
    )?;
    ocs(
        &c,
        client
            .post(u)
            .json(&serde_json::json!({ "lastReadMessage": last })),
    )
    .await
    .map_err(String::from)?;
    Ok(())
}

pub async fn react(
    app: AppHandle,
    id: String,
    token: String,
    message: i64,
    emoji: String,
    on: bool,
) -> Result<(), String> {
    check_token(&token)?;
    if emoji.is_empty() || emoji.chars().count() > 8 || emoji.chars().any(char::is_control) {
        return Err(tr("Reação inválida.").into());
    }
    let c = creds(&app, &id)?;
    let client = http()?;
    let u = url(
        &c.account.server,
        "/ocs/v2.php/apps/spreed/api/v1/reaction",
        &[&token, &message.to_string()],
    )?;
    let body = serde_json::json!({ "reaction": emoji });
    let req = if on {
        client.post(u).json(&body)
    } else {
        client.delete(u).json(&body)
    };
    ocs(&c, req).await.map_err(String::from)?;
    Ok(())
}

pub async fn mentions(
    app: AppHandle,
    id: String,
    token: String,
    search: String,
) -> Result<Vec<Mention>, String> {
    check_token(&token)?;
    let c = creds(&app, &id)?;
    let client = http()?;
    let mut u = url(
        &c.account.server,
        "/ocs/v2.php/apps/spreed/api/v1/chat",
        &[&token, "mentions"],
    )?;
    u.query_pairs_mut()
        .append_pair("search", &papeo_core::clip(search.trim(), 100))
        .append_pair("limit", "10");
    let data = ocs(&c, client.get(u)).await.map_err(String::from)?;
    Ok(core::parse_mentions(&data))
}

// ---------------------------------------------------------------------------
// Imagens e arquivos

fn remember_image(app: &AppHandle, key: String, data: String) {
    let talks = app.state::<Talks>();
    let mut images = talks.images.lock().unwrap_or_else(|e| e.into_inner());
    let used: usize = images.values().map(String::len).sum();
    if images.len() >= CACHE_MAX || used + data.len() > CACHE_BYTES {
        images.clear();
    }
    images.insert(key, data);
}

fn cached_image(app: &AppHandle, key: &str) -> Option<String> {
    let talks = app.state::<Talks>();
    let images = talks.images.lock().unwrap_or_else(|e| e.into_inner());
    images.get(key).cloned()
}

/// Corpo da resposta lido aos pedaços, parando no teto: sem tamanho
/// anunciado (ou mentindo), nunca carrega mais que `max` na memória.
/// `None` = maior que o teto.
async fn read_capped(mut res: reqwest::Response, max: usize) -> Result<Option<Vec<u8>>, String> {
    if res.content_length().is_some_and(|n| n > max as u64) {
        return Ok(None);
    }
    let mut out = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(|e| String::from(net(e)))? {
        if out.len() + chunk.len() > max {
            return Ok(None);
        }
        out.extend_from_slice(&chunk);
    }
    Ok(Some(out))
}

/// Imagem pequena como `data:` (só tipos de imagem, com teto).
async fn image(c: &Creds, u: reqwest::Url) -> Result<Option<String>, String> {
    let client = http()?;
    let res = send(c, client.get(u)).await.map_err(String::from)?;
    if !res.status().is_success() {
        return Ok(None);
    }
    let mime = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|v| {
            v.split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase()
        })
        .unwrap_or_default();
    if !matches!(
        mime.as_str(),
        "image/png" | "image/jpeg" | "image/gif" | "image/webp" | "image/svg+xml"
    ) {
        return Ok(None);
    }
    let Some(bytes) = read_capped(res, IMAGE_MAX).await? else {
        return Ok(None);
    };
    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(Some(format!("data:{mime};base64,{data}")))
}

/// Foto da conversa (`version` muda quando a foto muda).
pub async fn avatar(
    app: AppHandle,
    id: String,
    token: String,
    version: String,
) -> Result<Option<String>, String> {
    check_token(&token)?;
    let key = format!("a\u{0}{id}\u{0}{token}\u{0}{version}");
    if let Some(hit) = cached_image(&app, &key) {
        return Ok(Some(hit));
    }
    let c = creds(&app, &id)?;
    let u = url(
        &c.account.server,
        "/ocs/v2.php/apps/spreed/api/v1/room",
        &[&token, "avatar"],
    )?;
    let found = image(&c, u).await?;
    if let Some(data) = &found {
        remember_image(&app, key, data.clone());
    }
    Ok(found)
}

/// Miniatura de um arquivo compartilhado.
pub async fn preview(app: AppHandle, id: String, file: String) -> Result<Option<String>, String> {
    if file.is_empty() || file.len() > 20 || !file.bytes().all(|b| b.is_ascii_digit()) {
        return Err(tr("Arquivo inválido.").into());
    }
    let key = format!("p\u{0}{id}\u{0}{file}");
    if let Some(hit) = cached_image(&app, &key) {
        return Ok(Some(hit));
    }
    let c = creds(&app, &id)?;
    let mut u = url(&c.account.server, "/index.php/core/preview", &[])?;
    u.query_pairs_mut()
        .append_pair("fileId", &file)
        .append_pair("x", "480")
        .append_pair("y", "480")
        .append_pair("a", "1");
    let found = image(&c, u).await?;
    if let Some(data) = &found {
        remember_image(&app, key, data.clone());
    }
    Ok(found)
}

fn dav_url(c: &Creds, path: &str) -> Result<reqwest::Url, String> {
    if !core::valid_path(path) {
        return Err(tr("Arquivo inválido.").into());
    }
    let mut segments: Vec<&str> = vec![&c.account.uid];
    segments.extend(path.split('/').filter(|p| !p.is_empty()));
    url(&c.account.server, "/remote.php/dav/files", &segments)
}

/// Baixa um arquivo da conversa e abre na visualização (`open`) ou salva
/// em Downloads. Devolve o caminho salvo, ou vazio se abriu.
pub async fn file(
    app: AppHandle,
    id: String,
    path: String,
    name: String,
    open: bool,
) -> Result<String, String> {
    let c = creds(&app, &id)?;
    let client = http()?;
    let u = dav_url(&c, &path)?;
    let res = send(&c, client.get(u).timeout(Duration::from_secs(600)))
        .await
        .map_err(String::from)?;
    if !res.status().is_success() {
        return Err(trf(
            "O Nextcloud recusou o pedido ({codigo}).",
            &[("codigo", &res.status().as_u16().to_string())],
        ));
    }
    let bytes = read_capped(res, FILE_MAX)
        .await?
        .ok_or(tr("Arquivo grande demais para abrir pelo Papeo."))?;
    let name = if name.trim().is_empty() {
        path.rsplit('/').next().unwrap_or("arquivo").to_owned()
    } else {
        name
    };
    tauri::async_runtime::spawn_blocking(move || {
        if open && crate::attachment::show_bytes(&app, &id, &name, &bytes)? {
            return Ok(String::new());
        }
        let dir = crate::downloads::downloads_dir(&app);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let target = papeo_core::unique_path(&dir, &papeo_core::safe_file_name(&name), &[]);
        std::fs::write(&target, &bytes)
            .map_err(|e| trf("Não deu para salvar o anexo ({erro}).", &[("erro", &e)]))?;
        crate::mailbox::mark_downloaded(&target);
        Ok(target.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Envia um arquivo do computador: grava na pasta de anexos do Talk da
/// pessoa e compartilha na conversa.
pub async fn upload(app: AppHandle, id: String, token: String, path: String) -> Result<(), String> {
    check_token(&token)?;
    let local = std::path::PathBuf::from(&path);
    let meta = std::fs::metadata(&local).map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err(tr("Escolha um arquivo.").into());
    }
    if meta.len() > UPLOAD_MAX {
        return Err(tr("Arquivo grande demais (até 100 MB).").into());
    }
    let name = local
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .map(|n| papeo_core::safe_file_name(&n))
        .ok_or(tr("Escolha um arquivo."))?;
    let bytes = tauri::async_runtime::spawn_blocking(move || std::fs::read(local))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    // Uma cópia só, dividida entre as tentativas de nome.
    let bytes = bytes::Bytes::from(bytes);
    let c = creds(&app, &id)?;
    let client = http()?;
    // Pasta de anexos escolhida no Talk (padrão "/Talk").
    let caps = ocs(
        &c,
        client.get(url(
            &c.account.server,
            "/ocs/v2.php/cloud/capabilities",
            &[],
        )?),
    )
    .await
    .map_err(String::from)?;
    let folder = caps["capabilities"]["spreed"]["config"]["attachments"]["folder"]
        .as_str()
        .unwrap_or("/Talk")
        .trim_matches('/')
        .to_owned();
    let folder = if folder.is_empty() || !core::valid_path(&folder) {
        "Talk".to_owned()
    } else {
        folder
    };
    // Cria a pasta (já existir não é erro).
    let _ = send(
        &c,
        client.request(
            reqwest::Method::from_bytes(b"MKCOL").expect("método válido"),
            dav_url(&c, &folder)?,
        ),
    )
    .await;
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_owned(), format!(".{e}")),
        _ => (name.clone(), String::new()),
    };
    let mut stored = None;
    for n in 1..=20 {
        let candidate = if n == 1 {
            name.clone()
        } else {
            format!("{stem} ({n}){ext}")
        };
        let target = format!("{folder}/{candidate}");
        // Nunca sobrescreve um arquivo da pessoa.
        let res = send(
            &c,
            client
                .put(dav_url(&c, &target)?)
                .header("If-None-Match", "*")
                .timeout(Duration::from_secs(600))
                .body(bytes.clone()),
        )
        .await
        .map_err(String::from)?;
        match res.status().as_u16() {
            201 | 204 => {
                stored = Some(target);
                break;
            }
            412 => continue,
            code => {
                return Err(trf(
                    "O Nextcloud recusou o pedido ({codigo}).",
                    &[("codigo", &code.to_string())],
                ))
            }
        }
    }
    let stored = stored.ok_or(tr("Já há arquivos demais com esse nome na pasta do Talk."))?;
    let share = serde_json::json!({
        "shareType": 10,
        "shareWith": token,
        "path": format!("/{stored}"),
    });
    ocs(
        &c,
        client
            .post(url(
                &c.account.server,
                "/ocs/v2.php/apps/files_sharing/api/v1/shares",
                &[],
            )?)
            .json(&share),
    )
    .await
    .map_err(String::from)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Vigia: contador e avisos

fn set_live(
    app: &AppHandle,
    id: &str,
    generation: u64,
    status: Option<MailStatus>,
    unread: Option<u32>,
) {
    let changed = {
        let state = app.state::<AppState>();
        let mut inner = state.lock();
        match inner.talk.get_mut(id) {
            Some(live) if live.generation == generation => {
                let mut changed = false;
                if let Some(s) = status.filter(|s| *s != live.status) {
                    live.status = s;
                    changed = true;
                }
                if let Some(u) = unread
                    .map(|u| u.min(crate::webviews::UNREAD_MAX))
                    .filter(|u| *u != live.unread)
                {
                    live.unread = u;
                    changed = true;
                }
                changed
            }
            _ => false,
        }
    };
    if changed {
        publish(app);
    }
}

/// Senha recusada fora do vigia (num comando): marca para conectar de novo.
fn mark_auth(app: &AppHandle, id: &str) {
    let generation = {
        let state = app.state::<AppState>();
        let inner = state.lock();
        inner.talk.get(id).map(|l| l.generation)
    };
    if let Some(g) = generation {
        set_live(app, id, g, Some(MailStatus::Auth), None);
    }
}

/// Liga e desliga os vigias conforme as contas com Talk.
pub fn sync(app: &AppHandle) {
    sync_with(app, None);
}

/// Recomeça o vigia de `id` (login novo).
pub fn restart(app: &AppHandle, id: &str) {
    sync_with(app, Some(id));
}

fn sync_with(app: &AppHandle, fresh: Option<&str>) {
    let Some(talks) = app.try_state::<Talks>() else {
        return;
    };
    let state = app.state::<AppState>();
    let mut started = Vec::new();
    {
        let mut watchers = talks.watchers.lock().unwrap_or_else(|e| e.into_inner());
        let accounts: Vec<(String, TalkAccount)> = state
            .lock()
            .config
            .services
            .iter()
            .filter_map(|s| Some((s.id.clone(), s.talk.clone()?)))
            .collect();
        let wanted: HashMap<String, (TalkAccount, Option<Saved>)> = accounts
            .into_iter()
            .map(|(id, account)| {
                let saved = secrets::read(app, &id).filter(|s| matches(s, &account));
                (id, (account, saved))
            })
            .collect();
        let key =
            |a: &TalkAccount, s: &Saved| format!("{}\u{0}{}\u{0}{}", a.server, a.user, s.secret);
        watchers.retain(|id, w| {
            let keep = fresh != Some(id.as_str())
                && wanted
                    .get(id)
                    .and_then(|(a, s)| s.as_ref().map(|s| key(a, s)))
                    .is_some_and(|k| k == w.key);
            if !keep {
                w.stop.store(true, Ordering::SeqCst);
            }
            keep
        });
        let mut statuses = Vec::new();
        for (id, (account, saved)) in &wanted {
            if watchers.contains_key(id) {
                continue;
            }
            let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
            let Some(saved) = saved.clone() else {
                statuses.push((id.clone(), MailStatus::NoPassword, generation));
                continue;
            };
            let stop = Arc::new(AtomicBool::new(false));
            watchers.insert(
                id.clone(),
                Watcher {
                    key: key(account, &saved),
                    stop: stop.clone(),
                },
            );
            statuses.push((id.clone(), MailStatus::Connecting, generation));
            started.push((
                id.clone(),
                Creds {
                    account: account.clone(),
                    password: saved.secret,
                },
                generation,
                stop,
            ));
        }
        let mut inner = state.lock();
        inner.talk.retain(|id, _| wanted.contains_key(id));
        for (id, status, generation) in statuses {
            let unread = inner.talk.get(&id).map_or(0, |b| b.unread);
            inner.talk.insert(
                id,
                MailLive {
                    status,
                    unread,
                    generation,
                },
            );
        }
    }
    for (id, creds, generation, stop) in started {
        let app = app.clone();
        std::thread::Builder::new()
            .name(format!("papeo-talk-{id}"))
            .spawn(move || watch(&app, &id, &creds, generation, &stop))
            .ok();
    }
    publish(app);
}

/// Espera `wait`, acordando a cada segundo para ver se deve parar.
fn nap(stop: &AtomicBool, wait: Duration) -> bool {
    let until = Instant::now() + wait;
    while Instant::now() < until {
        if stop.load(Ordering::SeqCst) {
            return false;
        }
        std::thread::sleep(Duration::from_secs(1).min(until - Instant::now()));
    }
    !stop.load(Ordering::SeqCst)
}

fn watch(app: &AppHandle, id: &str, c: &Creds, generation: u64, stop: &AtomicBool) {
    let mut seen: HashMap<String, i64> = HashMap::new();
    let mut first = true;
    let mut retry = Duration::from_secs(5);
    while !stop.load(Ordering::SeqCst) {
        match tauri::async_runtime::block_on(rooms_of(c)) {
            Ok(rooms) => {
                retry = Duration::from_secs(5);
                set_live(
                    app,
                    id,
                    generation,
                    Some(MailStatus::Ok),
                    Some(core::unread_total(&rooms)),
                );
                let _ = app.emit(
                    ROOMS_EVENT,
                    serde_json::json!({ "id": id, "rooms": &rooms }),
                );
                for f in core::fresh_messages(&mut seen, &rooms, first) {
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    let body = if f.author.is_empty() || f.author == f.room {
                        f.text
                    } else {
                        format!("{}: {}", f.author, f.text)
                    };
                    let open = format!("{OPEN_PREFIX}{}", f.token);
                    let _ = crate::notify::deliver(app, id, &f.room, &body, Some(&open), false);
                }
                first = false;
                // Conta aberta na tela: consulta mais vezes.
                let active = app.state::<AppState>().lock().on_screen(id);
                if !nap(stop, if active { POLL_ACTIVE } else { POLL_IDLE }) {
                    return;
                }
            }
            Err(Fail::Auth) => {
                // Senha de app revogada: para até conectar de novo.
                set_live(app, id, generation, Some(MailStatus::Auth), None);
                return;
            }
            Err(Fail::Other(detail)) => {
                set_live(
                    app,
                    id,
                    generation,
                    Some(MailStatus::Offline { detail }),
                    None,
                );
                if !nap(stop, retry) {
                    return;
                }
                retry = (retry * 2).min(RETRY_MAX);
            }
        }
    }
}
