//! Cliente mínimo do Google Drive para a sincronização: login OAuth de app
//! de computador (navegador do sistema, retorno em 127.0.0.1, PKCE) e
//! arquivos na pasta oculta do app (`appDataFolder`, escopo
//! `drive.appdata`): o Papeo não enxerga o resto do Drive.

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";
const FILES_URL: &str = "https://www.googleapis.com/drive/v3/files";
const UPLOAD_URL: &str = "https://www.googleapis.com/upload/drive/v3/files";
pub const SCOPE: &str = "https://www.googleapis.com/auth/drive.appdata";
/// Tempo para concluir o login no navegador.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// Arquivo maior que isto na nuvem não é baixado.
const MAX_FILE: usize = 64 * 1024 * 1024;

/// ID e "segredo" do cliente OAuth (app de computador; para a Google esse
/// segredo não é confidencial). Definidos no build: sem eles, a
/// sincronização fica indisponível.
pub fn client() -> Option<(&'static str, &'static str)> {
    match (
        option_env!("PAPEO_GOOGLE_CLIENT_ID"),
        option_env!("PAPEO_GOOGLE_CLIENT_SECRET"),
    ) {
        (Some(id), Some(secret)) if !id.is_empty() => Some((id, secret)),
        _ => None,
    }
}

fn http() -> Result<reqwest::Client, String> {
    // Um cliente só (pool de conexões reaproveitado); clonar é barato (Arc).
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client.clone());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("Papeo/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;
    Ok(CLIENT.get_or_init(|| client).clone())
}

fn random_text(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).expect("gerador aleatório do sistema");
    B64URL.encode(buf)
}

fn form(pairs: &[(&str, &str)]) -> String {
    let mut s = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in pairs {
        s.append_pair(k, v);
    }
    s.finish()
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub expires_in: u64,
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// Permissões concedidas (o usuário pode desmarcar a do Drive).
    #[serde(default)]
    pub scope: Option<String>,
}

impl TokenResponse {
    pub fn has_drive_scope(&self) -> bool {
        self.scope
            .as_deref()
            .is_none_or(|s| s.split_whitespace().any(|x| x == SCOPE))
    }
}

/// Erro de arquivo inexistente na nuvem (para quem chama decidir).
pub const NOT_FOUND: &str = "NOT_FOUND";

/// Código de login devolvido pelo navegador, pronto para trocar por tokens.
pub struct LoginCode {
    code: String,
    verifier: String,
    redirect: String,
}

/// Login: abre o navegador e espera o retorno em 127.0.0.1. Bloqueia
/// (chamar numa thread própria); depois, `exchange`.
pub fn login(open_browser: impl FnOnce(&str) -> Result<(), String>) -> Result<LoginCode, String> {
    let (client_id, _) = client().ok_or("Sincronização indisponível nesta versão.")?;
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect = format!("http://127.0.0.1:{port}/");
    let verifier = random_text(48);
    let challenge = B64URL.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_text(24);
    let url = format!(
        "{AUTH_URL}?{}",
        form(&[
            ("client_id", client_id),
            ("redirect_uri", &redirect),
            ("response_type", "code"),
            ("scope", SCOPE),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
            ("state", &state),
            ("access_type", "offline"),
            ("prompt", "consent"),
        ])
    );
    open_browser(&url)?;
    let code = wait_for_code(&listener, &state)?;
    Ok(LoginCode {
        code,
        verifier,
        redirect,
    })
}

/// Troca o código do login pelos tokens.
pub async fn exchange(login: LoginCode) -> Result<TokenResponse, String> {
    let (client_id, client_secret) = client().ok_or("Sincronização indisponível nesta versão.")?;
    token_request(form(&[
        ("code", &login.code),
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("redirect_uri", &login.redirect),
        ("grant_type", "authorization_code"),
        ("code_verifier", &login.verifier),
    ]))
    .await
}

/// Espera a volta do navegador com `code` e o `state` certo.
fn wait_for_code(listener: &TcpListener, state: &str) -> Result<String, String> {
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let started = Instant::now();
    loop {
        if started.elapsed() > LOGIN_TIMEOUT {
            return Err("O login no Google não foi concluído a tempo.".into());
        }
        let (mut stream, _) = match listener.accept() {
            Ok(conn) => conn,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
            Err(e) => return Err(e.to_string()),
        };
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut buf = [0u8; 8192];
        let n = stream.read(&mut buf).unwrap_or(0);
        let request = String::from_utf8_lossy(&buf[..n]);
        let path = request
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or("/");
        let query = url::Url::parse(&format!("http://127.0.0.1{path}")).ok();
        let get = |name: &str| {
            query.as_ref().and_then(|u| {
                u.query_pairs()
                    .find(|(k, _)| k == name)
                    .map(|(_, v)| v.into_owned())
            })
        };
        // Pedidos que não são a volta deste login (ícone da aba, outro
        // programa com `state` errado): ignora e continua esperando.
        if get("state").as_deref() != Some(state) {
            let _ = stream.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
            continue;
        }
        let ok = get("code").is_some();
        let page = if ok {
            "Papeo conectado ao Google Drive. Pode fechar esta aba."
        } else {
            "O login não foi concluído. Volte ao Papeo e tente de novo."
        };
        let html = format!(
            "<!doctype html><meta charset=utf-8><title>Papeo</title><body style=\"font:16px system-ui;padding:40px\">{page}</body>"
        );
        let _ = stream.write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
                html.len()
            )
            .as_bytes(),
        );
        if let Some(err) = get("error") {
            return Err(if err == "access_denied" {
                "Acesso ao Google Drive não autorizado.".into()
            } else {
                format!("O Google recusou o login ({err}).")
            });
        }
        return match (ok, get("code")) {
            (true, Some(code)) => Ok(code),
            _ => Err("Resposta de login inválida.".into()),
        };
    }
}

async fn token_request(body: String) -> Result<TokenResponse, String> {
    let resp = http()?
        .post(TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("Sem conexão com o Google ({e})."))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        if text.contains("invalid_grant") {
            return Err("REAUTH".into());
        }
        return Err(format!("O Google recusou o acesso ({status})."));
    }
    serde_json::from_str(&text).map_err(|_| "Resposta inválida do Google.".to_owned())
}

/// Novo token de acesso a partir do de renovação. `Err("REAUTH")`: o acesso
/// foi revogado; é preciso entrar de novo.
pub async fn refresh(refresh_token: &str) -> Result<TokenResponse, String> {
    let (client_id, client_secret) = client().ok_or("Sincronização indisponível nesta versão.")?;
    token_request(form(&[
        ("refresh_token", refresh_token),
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("grant_type", "refresh_token"),
    ]))
    .await
}

pub async fn revoke(token: &str) {
    if let Ok(client) = http() {
        let _ = client
            .post(REVOKE_URL)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form(&[("token", token)]))
            .send()
            .await;
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFile {
    pub id: String,
    pub name: String,
    /// RFC 3339; compara como texto (sempre em UTC, mesmo formato).
    #[serde(default)]
    pub created_time: String,
    #[serde(default)]
    pub modified_time: String,
}

#[derive(Deserialize)]
struct FileList {
    #[serde(default)]
    files: Vec<RemoteFile>,
    #[serde(rename = "nextPageToken")]
    next: Option<String>,
}

async fn check(resp: reqwest::Response) -> Result<reqwest::Response, String> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let body = resp.text().await.unwrap_or_default();
    Err(match status.as_u16() {
        401 => "REAUTH".to_owned(),
        404 => NOT_FOUND.to_owned(),
        403 if body.contains("insufficientPermissions")
            || body.contains("ACCESS_TOKEN_SCOPE_INSUFFICIENT") =>
        {
            "Falta a permissão do Google Drive. Desconecte e conecte de novo, marcando o acesso à pasta do Papeo.".to_owned()
        }
        403 if body.contains("storageQuotaExceeded") => {
            "O Google Drive está sem espaço. Libere espaço na conta Google.".to_owned()
        }
        403 | 429 => {
            "O Google Drive limitou o acesso por agora. Tentamos de novo em instantes.".to_owned()
        }
        _ => format!("Erro do Google Drive ({status})."),
    })
}

/// Arquivos da pasta do app.
pub async fn list(token: &str) -> Result<Vec<RemoteFile>, String> {
    let client = http()?;
    let mut out = Vec::new();
    let mut page: Option<String> = None;
    for _ in 0..20 {
        let mut req = client.get(FILES_URL).bearer_auth(token).query(&[
            ("spaces", "appDataFolder"),
            (
                "fields",
                "nextPageToken,files(id,name,createdTime,modifiedTime)",
            ),
            ("pageSize", "100"),
        ]);
        if let Some(p) = &page {
            req = req.query(&[("pageToken", p.as_str())]);
        }
        let resp = check(
            req.send()
                .await
                .map_err(|e| format!("Sem conexão ({e})."))?,
        )
        .await?;
        let list: FileList = resp.json().await.map_err(|e| e.to_string())?;
        out.extend(list.files);
        match list.next {
            Some(next) => page = Some(next),
            None => break,
        }
    }
    Ok(out)
}

pub async fn download(token: &str, id: &str) -> Result<Vec<u8>, String> {
    let resp = http()?
        .get(format!("{FILES_URL}/{id}"))
        .bearer_auth(token)
        .query(&[("alt", "media")])
        .send()
        .await
        .map_err(|e| format!("Sem conexão ({e})."))?;
    let mut resp = check(resp).await?;
    if resp.content_length().is_some_and(|n| n as usize > MAX_FILE) {
        return Err("Arquivo grande demais na nuvem.".into());
    }
    // Para no limite mesmo sem `Content-Length` (não lê o resto).
    let mut body = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
        if body.len() + chunk.len() > MAX_FILE {
            return Err("Arquivo grande demais na nuvem.".into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Cria (`id` nulo) ou substitui um arquivo JSON na pasta do app.
pub async fn upload(
    token: &str,
    id: Option<&str>,
    name: &str,
    body: Vec<u8>,
) -> Result<String, String> {
    let client = http()?;
    let resp = match id {
        Some(id) => {
            client
                .patch(format!("{UPLOAD_URL}/{id}"))
                .bearer_auth(token)
                .query(&[("uploadType", "media"), ("fields", "id")])
                .header("Content-Type", "application/json")
                .body(body)
                .send()
                .await
        }
        None => {
            let boundary = format!("papeo{}", random_text(12).replace(['-', '_'], "x"));
            let meta = serde_json::json!({ "name": name, "parents": ["appDataFolder"] });
            let mut multipart = format!(
                "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n--{boundary}\r\nContent-Type: application/json\r\n\r\n"
            )
            .into_bytes();
            multipart.extend_from_slice(&body);
            multipart.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
            client
                .post(UPLOAD_URL)
                .bearer_auth(token)
                .query(&[("uploadType", "multipart"), ("fields", "id")])
                .header("Content-Type", format!("multipart/related; boundary={boundary}"))
                .body(multipart)
                .send()
                .await
        }
    }
    .map_err(|e| format!("Sem conexão ({e})."))?;
    #[derive(Deserialize)]
    struct Created {
        id: String,
    }
    let created: Created = check(resp).await?.json().await.map_err(|e| e.to_string())?;
    Ok(created.id)
}

pub async fn delete(token: &str, id: &str) -> Result<(), String> {
    let resp = http()?
        .delete(format!("{FILES_URL}/{id}"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("Sem conexão ({e})."))?;
    if resp.status().as_u16() == 404 {
        return Ok(());
    }
    check(resp).await.map(|_| ())
}
