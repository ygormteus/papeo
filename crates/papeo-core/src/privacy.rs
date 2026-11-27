//! Regras de privacidade: origem dos sites, texto das notificações e
//! identificadores de sessão.

use serde::{Deserialize, Serialize};

/// Quanto do conteúdo aparece nas notificações do sistema (e na tela
/// bloqueada/central de notificações).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NotificationPreview {
    /// Remetente e texto da mensagem.
    #[default]
    Full,
    /// Só o remetente (título), sem o texto.
    Sender,
    /// Só o nome do serviço: "Nova mensagem".
    Hidden,
}

/// Título e corpo exibidos, conforme a prévia escolhida.
pub fn notification_text(
    preview: NotificationPreview,
    service_name: &str,
    title: &str,
    body: &str,
) -> (String, String) {
    let title = crate::clip(title, 80);
    let with_title = || {
        if title.is_empty() {
            service_name.to_owned()
        } else {
            format!("{service_name} · {title}")
        }
    };
    match preview {
        NotificationPreview::Full => (with_title(), crate::clip(body, 240)),
        NotificationPreview::Sender => (with_title(), "Nova mensagem".to_owned()),
        NotificationPreview::Hidden => (service_name.to_owned(), "Nova mensagem".to_owned()),
    }
}

/// Como uma mensagem recente aparece no menu da barra de menus e na busca:
/// `(conversa, texto)`. `None` = não mostrar (app bloqueado, prévia oculta ou
/// sem nome de conversa). Com "só remetente", o texto vem vazio.
pub fn recent_text(
    preview: NotificationPreview,
    locked: bool,
    title: &str,
    body: &str,
) -> Option<(String, String)> {
    let title = crate::clip(title, 60);
    if locked || title.is_empty() {
        return None;
    }
    match preview {
        NotificationPreview::Full => Some((title, crate::clip(body, 80))),
        NotificationPreview::Sender => Some((title, String::new())),
        NotificationPreview::Hidden => None,
    }
}

/// Domínio "registrável" aproximado: as duas últimas partes do host, ou três
/// quando a penúltima é um sufixo curto de país (`exemplo.com.br`).
fn site_of(host: &str) -> String {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.parse::<std::net::IpAddr>().is_ok() || host.starts_with('[') {
        return host;
    }
    let labels: Vec<&str> = host.split('.').collect();
    let n = labels.len();
    let take = if n >= 3
        && labels[n - 1].len() == 2
        && matches!(
            labels[n - 2],
            "com" | "net" | "org" | "gov" | "edu" | "co" | "ac" | "or" | "ne" | "go"
        ) {
        3
    } else {
        2
    };
    labels[n.saturating_sub(take)..].join(".")
}

/// A página aberta pertence ao mesmo site do serviço? Exige o mesmo esquema
/// (um serviço https não aceita página http) e o mesmo domínio registrável:
/// `web.whatsapp.com` aceita `*.whatsapp.com`, mas não `exemplo.com`.
pub fn same_site(service_url: &str, page_url: &str) -> bool {
    let (Ok(service), Ok(page)) = (url::Url::parse(service_url), url::Url::parse(page_url)) else {
        return false;
    };
    if service.scheme() != page.scheme() || !matches!(page.scheme(), "https" | "http") {
        return false;
    }
    match (service.host_str(), page.host_str()) {
        (Some(a), Some(b)) => site_of(a) == site_of(b) || same_alias(a, b),
        _ => false,
    }
}

/// Endereços exatos para onde a Microsoft leva o mesmo Teams/Outlook de
/// trabalho (spec 19): contam como o mesmo site, e nada além deles.
const MICROSOFT_ALIASES: [&[&str]; 2] = [
    &["teams.microsoft.com", "teams.cloud.microsoft"],
    &[
        "outlook.office.com",
        "outlook.office365.com",
        "outlook.cloud.microsoft",
    ],
];

fn same_alias(a: &str, b: &str) -> bool {
    let (a, b) = (a.to_ascii_lowercase(), b.to_ascii_lowercase());
    MICROSOFT_ALIASES
        .iter()
        .any(|set| set.contains(&a.as_str()) && set.contains(&b.as_str()))
}

/// Página de login da Microsoft: vale só para dizer "desconectado" de uma
/// conta Teams ou Outlook (nunca para avisos ou microfone).
pub fn is_microsoft_login(page_url: &str) -> bool {
    url::Url::parse(page_url).is_ok_and(|u| {
        u.scheme() == "https"
            && matches!(
                u.host_str(),
                Some("login.microsoftonline.com" | "login.live.com")
            )
    })
}

/// `session` no formato gerado pelo app (32 hex minúsculos): seguro como
/// nome de pasta e identificador do armazenamento do macOS.
pub fn is_session_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Inverso de `Service::session_bytes`.
pub fn session_from_bytes(bytes: &[u8; 16]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Nova sessão aleatória (para "sair da conta e apagar dados").
pub fn new_session() -> String {
    crate::service::random_hex()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Service, ServiceKind};

    #[test]
    fn same_site_rules() {
        let wa = "https://web.whatsapp.com/";
        assert!(same_site(wa, "https://web.whatsapp.com/abc"));
        assert!(same_site(wa, "https://static.whatsapp.com/x"));
        assert!(!same_site(wa, "https://whatsapp.com.evil.io/"));
        assert!(!same_site(wa, "http://web.whatsapp.com/"));
        assert!(!same_site(wa, "https://exemplo.com/"));
        assert!(!same_site(wa, "about:blank"));
        let br = "https://chat.empresa.com.br/";
        assert!(same_site(br, "https://login.empresa.com.br/"));
        assert!(!same_site(br, "https://outra.com.br/"));
        assert!(same_site(
            "http://192.168.0.5:8065/",
            "http://192.168.0.5:8065/x"
        ));
        assert!(!same_site("http://192.168.0.5/", "http://192.168.0.6/"));
        // Teams/Outlook de trabalho trocam de domínio da Microsoft.
        let teams = "https://teams.microsoft.com/v2/";
        assert!(same_site(teams, "https://teams.cloud.microsoft/"));
        assert!(same_site(
            "https://outlook.office.com/mail/",
            "https://outlook.cloud.microsoft/mail/"
        ));
        assert!(!same_site(teams, "https://teams.live.com/v2/"));
        assert!(!same_site(teams, "https://cloud.microsoft.evil.io/"));
        // Só os endereços exatos: outro produto da Microsoft não entra.
        assert!(!same_site(teams, "https://forms.cloud.microsoft/"));
        assert!(!same_site(
            "https://outlook.office.com/mail/",
            "https://sway.cloud.microsoft/"
        ));
        assert!(!same_site(teams, "https://outlook.cloud.microsoft/"));
        assert!(is_microsoft_login(
            "https://login.microsoftonline.com/common/oauth2"
        ));
        assert!(!is_microsoft_login("http://login.live.com/"));
        assert!(!is_microsoft_login(
            "https://login.microsoftonline.com.evil.io/"
        ));
        assert!(!same_site(
            "https://outlook.live.com/mail/",
            "https://outlook.office.com/mail/"
        ));
    }

    #[test]
    fn preview_levels() {
        let full = notification_text(NotificationPreview::Full, "WhatsApp", "Ana", "Oi!");
        assert_eq!(full, ("WhatsApp · Ana".into(), "Oi!".into()));
        let sender = notification_text(NotificationPreview::Sender, "WhatsApp", "Ana", "Oi!");
        assert_eq!(sender, ("WhatsApp · Ana".into(), "Nova mensagem".into()));
        let hidden = notification_text(NotificationPreview::Hidden, "WhatsApp", "Ana", "Oi!");
        assert_eq!(hidden, ("WhatsApp".into(), "Nova mensagem".into()));
        let untitled = notification_text(NotificationPreview::Full, "Telegram", " ", "x");
        assert_eq!(untitled.0, "Telegram");
    }

    #[test]
    fn session_ids_roundtrip() {
        let s = Service::new(ServiceKind::Whatsapp, None, None).unwrap();
        assert!(is_session_id(&s.session));
        assert_eq!(session_from_bytes(&s.session_bytes()), s.session);
        assert!(is_session_id(&new_session()));
        assert!(!is_session_id("../../etc"));
        assert!(!is_session_id(&"A".repeat(32)));
    }

    #[test]
    fn recent_text_respects_privacy() {
        use NotificationPreview::*;
        let full = recent_text(Full, false, "Ana", "Oi, tudo bem?").unwrap();
        assert_eq!(full, ("Ana".into(), "Oi, tudo bem?".into()));
        assert_eq!(recent_text(Sender, false, "Ana", "Oi").unwrap().1, "");
        assert!(recent_text(Hidden, false, "Ana", "Oi").is_none());
        assert!(recent_text(Full, true, "Ana", "Oi").is_none());
        assert!(recent_text(Full, false, "  ", "Oi").is_none());
    }
}
