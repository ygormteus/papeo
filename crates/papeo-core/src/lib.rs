//! Núcleo do Papeo: modelo dos serviços (mensageiros) e persistência em `services.json`.
//! Não depende do Tauri, para ser testado em qualquer máquina.

use serde::{Deserialize, Serialize};
use std::collections::hash_map::RandomState;
use std::fs;
use std::hash::BuildHasher;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const STORE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceKind {
    Whatsapp,
    Telegram,
    Custom,
}

impl ServiceKind {
    fn default_name(self) -> Option<&'static str> {
        match self {
            Self::Whatsapp => Some("WhatsApp"),
            Self::Telegram => Some("Telegram"),
            Self::Custom => None,
        }
    }

    fn default_url(self) -> Option<&'static str> {
        match self {
            Self::Whatsapp => Some("https://web.whatsapp.com/"),
            Self::Telegram => Some("https://web.telegram.org/a/"),
            Self::Custom => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    /// Identificador estável; também é o rótulo da webview.
    pub id: String,
    pub kind: ServiceKind,
    pub name: String,
    pub url: String,
    /// 32 caracteres hexadecimais que identificam a sessão isolada
    /// (cookies, armazenamento local) deste serviço.
    pub session: String,
    /// Minutos sem uso até hibernar; 0 desativa a hibernação automática.
    #[serde(default)]
    pub hibernate_after_minutes: u32,
    /// Repassa as notificações do site como notificações nativas.
    #[serde(default = "enabled")]
    pub notifications: bool,
    /// Sobrescreve o user-agent padrão da plataforma.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
}

impl Service {
    pub fn new(
        kind: ServiceKind,
        name: Option<String>,
        url: Option<String>,
    ) -> Result<Self, String> {
        let url = match url.map(|u| u.trim().to_owned()).filter(|u| !u.is_empty()) {
            Some(raw) => normalize_url(&raw)?,
            None => kind
                .default_url()
                .ok_or("Informe a URL do serviço.")?
                .to_owned(),
        };
        let name = name
            .map(|n| n.trim().to_owned())
            .filter(|n| !n.is_empty())
            .or_else(|| kind.default_name().map(str::to_owned))
            .or_else(|| url::Url::parse(&url).ok()?.host_str().map(str::to_owned))
            .unwrap_or_else(|| "Serviço".to_owned());
        let session = random_hex();
        Ok(Self {
            id: format!("svc-{}", &session[..12]),
            kind,
            name,
            url,
            session,
            hibernate_after_minutes: 0,
            notifications: true,
            user_agent: None,
        })
    }

    /// Identificador de 16 bytes do armazenamento isolado (WKWebsiteDataStore no macOS).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn session_bytes(&self) -> [u8; 16] {
        let mut out = [0u8; 16];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = self
                .session
                .get(i * 2..i * 2 + 2)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
                .unwrap_or(0);
        }
        out
    }
}

fn enabled() -> bool {
    true
}

/// Evita dois serviços com o mesmo nome automático: "WhatsApp", "WhatsApp 2"…
pub fn unique_name(base: &str, existing: &[Service]) -> String {
    let taken = |name: &str| existing.iter().any(|s| s.name.eq_ignore_ascii_case(name));
    if !taken(base) {
        return base.to_owned();
    }
    (2..)
        .map(|n| format!("{base} {n}"))
        .find(|candidate| !taken(candidate))
        .unwrap_or_else(|| base.to_owned())
}

/// Limita o texto vindo dos sites (título e corpo das notificações).
pub fn clip(text: &str, max_chars: usize) -> String {
    let text = text.trim();
    match text.char_indices().nth(max_chars) {
        Some((cut, _)) => format!("{}…", text[..cut].trim_end()),
        None => text.to_owned(),
    }
}

fn normalize_url(raw: &str) -> Result<String, String> {
    let candidate = if raw.contains("://") {
        raw.to_owned()
    } else {
        format!("https://{raw}")
    };
    let url = url::Url::parse(&candidate).map_err(|_| "URL inválida.".to_owned())?;
    match url.scheme() {
        "https" | "http" if url.host_str().is_some() => Ok(url.to_string()),
        _ => Err("Use um endereço http(s) válido.".to_owned()),
    }
}

fn random_hex() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let state = RandomState::new();
    (0..2u8)
        .map(|salt| format!("{:016x}", state.hash_one((nanos, salt, std::process::id()))))
        .collect()
}

#[derive(Serialize, Deserialize)]
struct StoreFile {
    version: u32,
    services: Vec<Service>,
}

fn defaults() -> Vec<Service> {
    [ServiceKind::Whatsapp, ServiceKind::Telegram]
        .into_iter()
        .filter_map(|kind| Service::new(kind, None, None).ok())
        .collect()
}

/// Lê a lista de serviços. Na primeira execução, cria WhatsApp e Telegram.
/// Um arquivo ilegível é preservado como `.corrompido` em vez de sobrescrito.
pub fn load(path: &Path) -> Vec<Service> {
    match fs::read(path) {
        Ok(bytes) => match serde_json::from_slice::<StoreFile>(&bytes) {
            Ok(file) => file.services,
            Err(err) => {
                eprintln!("papeo: services.json ilegível ({err}); usando padrões");
                let _ = fs::rename(path, path.with_extension("json.corrompido"));
                defaults()
            }
        },
        Err(_) => defaults(),
    }
}

/// Grava de forma atômica (arquivo temporário + rename).
pub fn save(path: &Path, services: &[Service]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let body = serde_json::to_vec_pretty(&StoreFile {
        version: STORE_VERSION,
        services: services.to_vec(),
    })
    .map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, body).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Extrai a contagem de não lidas de títulos como "(3) WhatsApp" ou "Telegram (12)".
pub fn parse_unread(title: &str) -> u32 {
    let mut rest = title;
    while let Some(start) = rest.find('(') {
        let after = &rest[start + 1..];
        if let Some(end) = after.find(')') {
            let inner = after[..end].trim().trim_end_matches('+');
            if let Ok(n) = inner.parse::<u32>() {
                return n;
            }
            rest = &after[end + 1..];
        } else {
            break;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unread_from_titles() {
        assert_eq!(parse_unread("(3) WhatsApp"), 3);
        assert_eq!(parse_unread("Telegram (12)"), 12);
        assert_eq!(parse_unread("(99+) Mensagens"), 99);
        assert_eq!(parse_unread("WhatsApp"), 0);
        assert_eq!(parse_unread("Grupo (família) (4)"), 4);
    }

    #[test]
    fn new_service_validates_url() {
        assert!(Service::new(ServiceKind::Custom, None, None).is_err());
        assert!(Service::new(
            ServiceKind::Custom,
            None,
            Some("javascript:alert(1)".into())
        )
        .is_err());
        let s = Service::new(ServiceKind::Custom, None, Some("app.slack.com".into())).unwrap();
        assert_eq!(s.url, "https://app.slack.com/");
        assert_eq!(s.name, "app.slack.com");
        assert_eq!(s.session.len(), 32);
        assert!(s.id.starts_with("svc-"));
    }

    #[test]
    fn second_account_gets_numbered_name() {
        let first = Service::new(ServiceKind::Whatsapp, None, None).unwrap();
        assert_eq!(unique_name("WhatsApp", &[]), "WhatsApp");
        let mut second = Service::new(ServiceKind::Whatsapp, None, None).unwrap();
        second.name = unique_name("WhatsApp", std::slice::from_ref(&first));
        assert_eq!(second.name, "WhatsApp 2");
        assert_eq!(unique_name("whatsapp", &[first, second]), "whatsapp 3");
    }

    #[test]
    fn clip_limits_chars() {
        assert_eq!(clip("  oi  ", 10), "oi");
        assert_eq!(clip("ação longa demais", 4), "ação…");
    }

    #[test]
    fn old_store_without_notifications_defaults_on() {
        let json = r#"{"id":"svc-1","kind":"telegram","name":"T","url":"https://web.telegram.org/a/","session":"00"}"#;
        let s: Service = serde_json::from_str(json).unwrap();
        assert!(s.notifications);
        assert_eq!(s.hibernate_after_minutes, 0);
    }

    #[test]
    fn sessions_are_unique() {
        let a = Service::new(ServiceKind::Whatsapp, None, None).unwrap();
        let b = Service::new(ServiceKind::Whatsapp, None, None).unwrap();
        assert_ne!(a.session, b.session);
        assert_ne!(a.session_bytes(), [0u8; 16]);
    }

    #[test]
    fn store_roundtrip_and_corruption() {
        let dir = std::env::temp_dir().join(format!("papeo-test-{}", random_hex()));
        let path = dir.join("services.json");
        let first = load(&path);
        assert_eq!(first.len(), 2);
        save(&path, &first).unwrap();
        assert_eq!(load(&path)[0].id, first[0].id);
        fs::write(&path, b"{oops").unwrap();
        assert_eq!(load(&path).len(), 2);
        assert!(path.with_extension("json.corrompido").exists());
        let _ = fs::remove_dir_all(dir);
    }
}
