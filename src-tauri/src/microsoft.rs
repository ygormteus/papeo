//! Login da Microsoft para os avisos do Outlook por IMAP: OAuth de app de
//! computador (navegador do sistema, retorno em `localhost`, PKCE, sem
//! segredo). O que fica guardado é o token de renovação, que a Microsoft
//! troca a cada uso; a senha da conta nunca passa pelo Papeo.

use crate::drive::{form, http, random_text, wait_for_redirect, Texts};
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine;
use papeo_core::i18n::{tr, trf};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};

/// Contas pessoais (Outlook.com, Hotmail) e de trabalho/escola.
const AUTHORIZE_URL: &str = "https://login.microsoftonline.com/common/oauth2/v2.0/authorize";
const TOKEN_URL: &str = "https://login.microsoftonline.com/common/oauth2/v2.0/token";
/// No login: ler a caixa por IMAP, enviar por SMTP, continuar com acesso
/// (renovação) e saber o e-mail.
const SCOPE: &str = "https://outlook.office.com/IMAP.AccessAsUser.All https://outlook.office.com/SMTP.Send offline_access openid email";

/// Para que serve o token de acesso pedido na renovação.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Ler por IMAP (logins antigos só liberaram isto).
    Read,
    /// Enviar por SMTP.
    Send,
}

impl Scope {
    fn text(self) -> &'static str {
        match self {
            Scope::Read => "https://outlook.office.com/IMAP.AccessAsUser.All offline_access",
            Scope::Send => "https://outlook.office.com/SMTP.Send offline_access",
        }
    }
}
/// App "Papeo" registrado no Microsoft Entra (cliente público, sem segredo).
const DEFAULT_CLIENT_ID: &str = "cd10f1fa-0501-4cd3-a402-c5bd7920c139";

fn client_id() -> &'static str {
    match option_env!("PAPEO_MICROSOFT_CLIENT_ID") {
        Some(id) if !id.is_empty() => id,
        _ => DEFAULT_CLIENT_ID,
    }
}

#[derive(Deserialize)]
pub struct Tokens {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub id_token: Option<String>,
    /// Segundos de validade do token de acesso.
    #[serde(default)]
    pub expires_in: u64,
}

/// Falha ao pedir tokens.
#[derive(Debug, PartialEq, Eq)]
pub enum TokenError {
    /// Sem conexão ou erro passageiro da Microsoft: tentar de novo.
    Network(String),
    /// Login vencido, revogado ou que pede interação: entrar de novo.
    Reauth,
    /// Recusa que um novo login não resolve sozinho (app bloqueado pela
    /// empresa, por exemplo): texto para a tela.
    Refused(String),
}

impl From<TokenError> for String {
    fn from(err: TokenError) -> Self {
        match err {
            TokenError::Network(text) | TokenError::Refused(text) => text,
            TokenError::Reauth => tr("O login da Microsoft não foi aceito. Tente de novo.").into(),
        }
    }
}

/// Desiste do login em andamento (diálogo fechado).
static CANCEL: AtomicBool = AtomicBool::new(false);

pub fn cancel_login() {
    CANCEL.store(true, Ordering::SeqCst);
}

pub struct LoginCode {
    code: String,
    verifier: String,
    redirect: String,
}

/// Abre o navegador no login da Microsoft e espera a volta. Bloqueia
/// (chamar numa thread própria); depois, `exchange`.
pub fn login(open_browser: impl FnOnce(&str) -> Result<(), String>) -> Result<LoginCode, String> {
    CANCEL.store(false, Ordering::SeqCst);
    let (listeners, port) = listen_localhost()?;
    let redirect = format!("http://localhost:{port}");
    let verifier = random_text(48);
    let challenge = B64URL.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_text(24);
    let url = format!(
        "{AUTHORIZE_URL}?{}",
        form(&[
            ("client_id", client_id()),
            ("response_type", "code"),
            ("redirect_uri", &redirect),
            ("response_mode", "query"),
            ("scope", SCOPE),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
            ("state", &state),
            ("prompt", "select_account"),
        ])
    );
    open_browser(&url)?;
    let params = wait_for_redirect(
        &listeners,
        &state,
        &Texts {
            done: tr("Papeo conectado à sua conta Microsoft. Pode fechar esta aba."),
            timeout: tr("O login na Microsoft não foi concluído a tempo."),
        },
        Some(&CANCEL),
    )?;
    if let Some(err) = params.get("error") {
        let detail = params
            .get("error_description")
            .map(String::as_str)
            .unwrap_or("");
        return Err(refusal(err, detail));
    }
    let code = params
        .get("code")
        .cloned()
        .ok_or(tr("Resposta de login inválida."))?;
    Ok(LoginCode {
        code,
        verifier,
        redirect,
    })
}

/// `localhost` é o endereço registrado; o navegador pode chegar por IPv4 ou
/// IPv6, então escuta nos dois na mesma porta. Porta já usada no IPv6 por
/// outro programa (que receberia a volta): sorteia outra.
fn listen_localhost() -> Result<(Vec<TcpListener>, u16), String> {
    for _ in 0..8 {
        let v4 = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = v4.local_addr().map_err(|e| e.to_string())?.port();
        match TcpListener::bind(("::1", port)) {
            Ok(v6) => return Ok((vec![v4, v6], port)),
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => continue,
            // Sem IPv6 no computador: `localhost` só chega pelo IPv4.
            Err(_) => return Ok((vec![v4], port)),
        }
    }
    Err(tr("Não deu para abrir a porta do login.").into())
}

/// Mensagem para uma recusa do login.
fn refusal(err: &str, detail: &str) -> String {
    // Empresa que exige aprovação do administrador para apps de fora.
    if ["AADSTS65001", "AADSTS90094", "AADSTS90095", "AADSTS65004"]
        .iter()
        .any(|code| detail.contains(code))
        || detail.contains("admin")
    {
        return tr(
            "A sua empresa precisa liberar o Papeo: peça ao administrador do Microsoft 365.",
        )
        .into();
    }
    if err == "access_denied" {
        return tr("Acesso à caixa do Outlook não autorizado.").into();
    }
    trf("A Microsoft recusou o login ({erro}).", &[("erro", &err)])
}

/// Troca o código do login pelos tokens.
pub async fn exchange(login: LoginCode) -> Result<Tokens, TokenError> {
    token_request(form(&[
        ("client_id", client_id()),
        ("grant_type", "authorization_code"),
        ("code", &login.code),
        ("redirect_uri", &login.redirect),
        ("code_verifier", &login.verifier),
        ("scope", SCOPE),
    ]))
    .await
}

/// Novo token de acesso (e, em geral, um novo de renovação).
pub async fn refresh(refresh_token: &str, scope: Scope) -> Result<Tokens, TokenError> {
    token_request(form(&[
        ("client_id", client_id()),
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("scope", scope.text()),
    ]))
    .await
}

async fn token_request(body: String) -> Result<Tokens, TokenError> {
    let network = |e: String| {
        TokenError::Network(trf(
            "Sem conexão com a Microsoft ({erro}).",
            &[("erro", &e)],
        ))
    };
    let resp = http()
        .map_err(network)?
        .post(TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| network(e.to_string()))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| network(e.to_string()))?;
    if !status.is_success() {
        let json = serde_json::from_str::<serde_json::Value>(&text).ok();
        let field = |key: &str| {
            json.as_ref()
                .and_then(|v| v.get(key)?.as_str().map(str::to_owned))
        };
        return Err(classify(
            status.as_u16(),
            &field("error").unwrap_or_default(),
            &field("error_description").unwrap_or_default(),
        ));
    }
    serde_json::from_str(&text)
        .map_err(|_| TokenError::Network(tr("Resposta inválida da Microsoft.").into()))
}

/// Tipo de falha de uma resposta de erro do servidor de tokens.
fn classify(status: u16, err: &str, detail: &str) -> TokenError {
    match err {
        // Vencido, revogado, senha trocada ou política que pede login.
        "invalid_grant" | "interaction_required" | "login_required" | "consent_required" => {
            TokenError::Reauth
        }
        "temporarily_unavailable" | "server_error" => {
            TokenError::Network(tr("A Microsoft está indisponível; tentando de novo.").into())
        }
        _ if status >= 500 || err.is_empty() => TokenError::Network(trf(
            "A Microsoft respondeu com erro ({status}).",
            &[("status", &status)],
        )),
        _ => TokenError::Refused(refusal(err, detail)),
    }
}

/// E-mail da conta, lido do `id_token` recebido direto da Microsoft (por
/// TLS, na troca do código): `email` ou, nas contas pessoais,
/// `preferred_username`.
pub fn account_email(id_token: &str) -> Option<String> {
    let payload = id_token.split('.').nth(1)?;
    let bytes = B64URL.decode(payload.trim_end_matches('=')).ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    ["email", "preferred_username"]
        .iter()
        .filter_map(|key| claims.get(*key)?.as_str())
        .map(str::trim)
        .find(|v| v.contains('@') && !v.chars().any(|c| c.is_control() || c.is_whitespace()))
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_from_id_token() {
        let token = |claims: &str| format!("x.{}.y", B64URL.encode(claims));
        assert_eq!(
            account_email(&token(r#"{"preferred_username":"ana@outlook.com"}"#)).as_deref(),
            Some("ana@outlook.com")
        );
        assert_eq!(
            account_email(&token(
                r#"{"email":"bia@empresa.com","preferred_username":"x"}"#
            ))
            .as_deref(),
            Some("bia@empresa.com")
        );
        assert_eq!(
            account_email(&token(r#"{"preferred_username":"semarroba"}"#)),
            None
        );
        assert_eq!(account_email("lixo"), None);
    }

    #[test]
    fn token_errors() {
        assert_eq!(classify(400, "invalid_grant", ""), TokenError::Reauth);
        assert_eq!(
            classify(400, "interaction_required", ""),
            TokenError::Reauth
        );
        assert!(matches!(classify(503, "", ""), TokenError::Network(_)));
        assert!(matches!(
            classify(400, "temporarily_unavailable", ""),
            TokenError::Network(_)
        ));
        assert!(matches!(
            classify(400, "unauthorized_client", "AADSTS65001"),
            TokenError::Refused(text) if text.contains("administrador")
        ));
    }

    #[test]
    fn refusal_messages() {
        assert!(refusal("access_denied", "AADSTS65001: needs consent").contains("administrador"));
        assert!(refusal("access_denied", "").contains("não autorizado"));
        assert!(refusal("server_error", "").contains("server_error"));
    }
}
