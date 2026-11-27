//! Modelo de um serviço (mensageiro) e validação dos campos editáveis.

use crate::i18n::{tr, trf};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceKind {
    Whatsapp,
    Telegram,
    Discord,
    /// Gmail na web.
    Gmail,
    /// Outlook na web: pessoal (Outlook.com, Hotmail) ou de trabalho/escola
    /// (Microsoft 365, pelo endereço informado ao adicionar).
    Outlook,
    /// Webmail de qualquer provedor (Roundcube e outros, contas IMAP):
    /// o endereço é informado pela pessoa.
    Email,
    Custom,
    /// Slack na web (spec 19).
    Slack,
    /// Microsoft Teams na web: trabalho/escola (padrão) ou pessoal, pelo
    /// endereço informado ao adicionar.
    Teams,
    /// Nextcloud Talk no servidor da pessoa (spec 26).
    Nextcloud,
}

impl ServiceKind {
    /// Mensageiros em que o Papeo sabe abrir conversas e responder.
    /// Discord fica de fora: as regras dele proíbem automatizar contas
    /// pessoais, e a resposta rápida escreve e envia pela página.
    pub fn supports_chat(self) -> bool {
        matches!(self, ServiceKind::Whatsapp | ServiceKind::Telegram)
    }

    /// Caixas de e-mail: o assunto aberto faz as vezes da conversa.
    pub fn is_email(self) -> bool {
        matches!(
            self,
            ServiceKind::Gmail | ServiceKind::Outlook | ServiceKind::Email
        )
    }

    /// Caixas que aceitam avisos por IMAP com usuário e senha. O Outlook
    /// só aceita o login da Microsoft (`accepts_mail`).
    pub fn supports_mail_watch(self) -> bool {
        matches!(self, ServiceKind::Gmail | ServiceKind::Email)
    }

    /// A conta aceita avisos por IMAP com esta forma de entrar.
    pub fn accepts_mail(self, auth: crate::mail::MailAuth) -> bool {
        match auth {
            crate::mail::MailAuth::Password => self.supports_mail_watch(),
            crate::mail::MailAuth::Microsoft => self == ServiceKind::Outlook,
        }
    }

    fn default_name(self) -> Option<&'static str> {
        match self {
            Self::Whatsapp => Some("WhatsApp"),
            Self::Telegram => Some("Telegram"),
            Self::Discord => Some("Discord"),
            Self::Gmail => Some("Gmail"),
            Self::Outlook => Some("Outlook"),
            Self::Slack => Some("Slack"),
            Self::Teams => Some("Teams"),
            Self::Nextcloud => Some("Nextcloud Talk"),
            Self::Email | Self::Custom => None,
        }
    }

    fn default_url(self) -> Option<&'static str> {
        match self {
            Self::Whatsapp => Some("https://web.whatsapp.com/"),
            Self::Telegram => Some("https://web.telegram.org/a/"),
            Self::Discord => Some("https://discord.com/app"),
            Self::Gmail => Some("https://mail.google.com/mail/u/0/"),
            Self::Outlook => Some("https://outlook.live.com/mail/"),
            Self::Slack => Some("https://app.slack.com/client"),
            Self::Teams => Some("https://teams.microsoft.com/v2/"),
            Self::Email | Self::Custom | Self::Nextcloud => None,
        }
    }
}

/// O que fazer quando o site pede microfone ou câmera.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaPolicy {
    /// O sistema/webview pergunta a cada pedido.
    Ask,
    Allow,
    Block,
}

/// Mundo a que a conta pertence, para os modos Trabalho e Pessoal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceContext {
    Work,
    Personal,
}

pub const ZOOM_MIN: f64 = 0.5;
pub const ZOOM_MAX: f64 = 2.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    /// Identificador estável (`svc-` + 12 hex); também é o rótulo da webview.
    pub id: String,
    pub kind: ServiceKind,
    pub name: String,
    pub url: String,
    /// 32 caracteres hexadecimais que identificam a sessão isolada
    /// (cookies, armazenamento local) deste serviço. Vira nome de pasta.
    pub session: String,
    /// Minutos sem uso até hibernar; 0 desativa a hibernação automática.
    #[serde(default)]
    pub hibernate_after_minutes: u32,
    /// Repassa as notificações do site como notificações nativas.
    #[serde(default = "enabled")]
    pub notifications: bool,
    /// Silencia as notificações até este instante (segundos Unix).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted_until: Option<u64>,
    /// Cor do ícone (`#rrggbb`); sem ela, usa a cor do tipo de serviço.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Emoji (ou letra curta) mostrado no ícone no lugar do símbolo do serviço.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    /// Microfone/câmera; sem valor, usa o padrão do tipo (`media_policy`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaPolicy>,
    /// Zoom da página (1.0 = 100%).
    #[serde(default = "one")]
    pub zoom: f64,
    /// Sobrescreve o user-agent padrão da plataforma.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    /// Trabalho ou pessoal; sem valor, a conta aparece em todos os modos.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<ServiceContext>,
    /// Avisos por IMAP (só caixas de e-mail). A senha fica fora daqui.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mail: Option<crate::mail::MailAccount>,
    /// Leitor do Talk pela API (spec 29; só Nextcloud). A senha de app fica
    /// fora daqui.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub talk: Option<crate::talk::TalkAccount>,
}

fn enabled() -> bool {
    true
}

fn one() -> f64 {
    1.0
}

impl Service {
    pub fn new(
        kind: ServiceKind,
        name: Option<String>,
        url: Option<String>,
    ) -> Result<Self, String> {
        let url = match url.map(|u| u.trim().to_owned()).filter(|u| !u.is_empty()) {
            Some(raw) if kind == ServiceKind::Nextcloud => talk_url(&normalize_url(&raw)?),
            Some(raw) => normalize_url(&raw)?,
            None => kind
                .default_url()
                .ok_or(tr("Informe a URL do serviço."))?
                .to_owned(),
        };
        let name = name
            .map(|n| n.trim().to_owned())
            .filter(|n| !n.is_empty())
            .or_else(|| kind.default_name().map(str::to_owned))
            .or_else(|| url::Url::parse(&url).ok()?.host_str().map(str::to_owned))
            .map(|n| fit_name(&n))
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
            muted_until: None,
            color: None,
            emoji: None,
            media: None,
            zoom: 1.0,
            user_agent: None,
            context: None,
            mail: None,
            talk: None,
        })
    }

    /// Mensageiros conhecidos liberam microfone/câmera (mensagens de voz);
    /// e-mail e sites personalizados perguntam.
    pub fn media_policy(&self) -> MediaPolicy {
        self.media.unwrap_or(match self.kind {
            ServiceKind::Custom
            | ServiceKind::Gmail
            | ServiceKind::Outlook
            | ServiceKind::Email => MediaPolicy::Ask,
            _ => MediaPolicy::Allow,
        })
    }

    /// Identificador de 16 bytes do armazenamento isolado (WKWebsiteDataStore no macOS).
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

    /// Confere e normaliza um serviço vindo de arquivo (backup ou
    /// `services.json` editado à mão). `id` e `session` viram rótulo de
    /// webview e nome de pasta, então só aceitam o formato gerado pelo app.
    pub fn sanitized(mut self) -> Result<Self, String> {
        let hex = |s: &str, len: usize| s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit());
        if !hex(&self.session, 32) {
            return Err(trf(
                "sessão inválida em \"{nome}\"",
                &[("nome", &self.name)],
            ));
        }
        self.session.make_ascii_lowercase();
        match self.id.strip_prefix("svc-") {
            Some(rest) if hex(rest, 12) => self.id.make_ascii_lowercase(),
            _ => {
                return Err(trf(
                    "identificador inválido em \"{nome}\"",
                    &[("nome", &self.name)],
                ))
            }
        }
        // Nome longo/vazio é reparado (não invalida o arquivo inteiro).
        self.name = normalize_name(&self.name).unwrap_or_else(|_| {
            let fallback = url::Url::parse(&self.url)
                .ok()
                .and_then(|u| u.host_str().map(str::to_owned))
                .unwrap_or_else(|| "Serviço".to_owned());
            fit_name(if self.name.trim().is_empty() {
                &fallback
            } else {
                &self.name
            })
        });
        self.url = normalize_url(&self.url)?;
        self.color = normalize_color(self.color.as_deref())?;
        self.emoji = normalize_emoji(self.emoji.as_deref())?;
        self.zoom = clamp_zoom(self.zoom);
        self.hibernate_after_minutes = self.hibernate_after_minutes.min(24 * 60);
        self.user_agent = self
            .user_agent
            .map(|ua| ua.trim().to_owned())
            .filter(|ua| !ua.is_empty() && ua.len() <= 512 && !ua.chars().any(char::is_control));
        // IMAP inválido (arquivo editado à mão) só desliga os avisos por IMAP.
        self.mail = self
            .mail
            .filter(|m| self.kind.accepts_mail(m.auth))
            .and_then(|m| m.sanitized().ok());
        // Talk pela API só no Nextcloud, com servidor `https:`.
        self.talk = self
            .talk
            .filter(|_| self.kind == ServiceKind::Nextcloud)
            .and_then(|t| t.sanitized());
        Ok(self)
    }
}

/// Valida o nome exibido do serviço.
pub fn normalize_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(tr("O nome não pode ficar vazio.").into());
    }
    if name.chars().count() > NAME_MAX {
        return Err(tr("Use um nome com até 40 caracteres.").into());
    }
    Ok(name.to_owned())
}

/// Aceita `#rgb` ou `#rrggbb` (devolve `#rrggbb` minúsculo); vazio remove a cor.
pub fn normalize_color(raw: Option<&str>) -> Result<Option<String>, String> {
    let Some(raw) = raw.map(str::trim).filter(|c| !c.is_empty()) else {
        return Ok(None);
    };
    let hex = raw.strip_prefix('#').unwrap_or(raw);
    let full: String = match hex.len() {
        3 => hex.chars().flat_map(|c| [c, c]).collect(),
        6 => hex.to_owned(),
        _ => String::new(),
    };
    if full.len() == 6 && full.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(Some(format!("#{}", full.to_ascii_lowercase())))
    } else {
        Err(tr("Cor inválida; use o formato #rrggbb.").into())
    }
}

/// Aceita um emoji (inclusive compostos, como famílias e bandeiras) ou até
/// 2 letras; vazio remove.
pub fn normalize_emoji(raw: Option<&str>) -> Result<Option<String>, String> {
    let Some(text) = raw.map(str::trim).filter(|e| !e.is_empty()) else {
        return Ok(None);
    };
    let chars = text.chars().count();
    let has_symbol = text.chars().any(|c| !c.is_alphanumeric());
    let ok = !text.chars().any(|c| c.is_control() || c.is_whitespace())
        && ((has_symbol && chars <= 12) || (!has_symbol && chars <= 2));
    if ok {
        Ok(Some(text.to_owned()))
    } else {
        Err(tr("Use um emoji ou até 2 letras.").into())
    }
}

pub const NAME_MAX: usize = 40;

/// Encurta um nome para caber em `NAME_MAX` caracteres.
pub(crate) fn fit_name(name: &str) -> String {
    let name = name.trim();
    if name.chars().count() <= NAME_MAX {
        return name.to_owned();
    }
    let cut: String = name.chars().take(NAME_MAX - 1).collect();
    format!("{}…", cut.trim_end())
}

/// Limita o zoom a 50–200%, em passos de 10%.
pub fn clamp_zoom(zoom: f64) -> f64 {
    if !zoom.is_finite() {
        return 1.0;
    }
    (zoom.clamp(ZOOM_MIN, ZOOM_MAX) * 10.0).round() / 10.0
}

/// Evita dois serviços com o mesmo nome automático: "WhatsApp", "WhatsApp 2"…
pub fn unique_name(base: &str, existing: &[Service]) -> String {
    let taken = |name: &str| existing.iter().any(|s| s.name.eq_ignore_ascii_case(name));
    if !taken(base) {
        return base.to_owned();
    }
    (2..)
        .map(|n| {
            let suffix = format!(" {n}");
            let room = NAME_MAX - suffix.chars().count();
            let stem: String = base.chars().take(room).collect();
            format!("{}{suffix}", stem.trim_end())
        })
        .find(|candidate| !taken(candidate))
        .unwrap_or_else(|| base.to_owned())
}

pub(crate) fn normalize_url(raw: &str) -> Result<String, String> {
    let candidate = if raw.contains("://") {
        raw.to_owned()
    } else {
        format!("https://{raw}")
    };
    let url = url::Url::parse(&candidate).map_err(|_| tr("URL inválida.").to_owned())?;
    match url.scheme() {
        "https" | "http" if url.host_str().is_some() => Ok(url.to_string()),
        _ => Err(tr("Use um endereço http(s) válido.").to_owned()),
    }
}

/// Endereço do Talk no servidor Nextcloud informado (spec 26): aceita o
/// servidor ("cloud.empresa.com", com ou sem pasta) ou qualquer página
/// dele (Arquivos, uma conversa) e leva à lista de conversas.
fn talk_url(normalized: &str) -> String {
    let Ok(mut url) = url::Url::parse(normalized) else {
        return normalized.to_owned();
    };
    let path = url.path().to_owned();
    // Já é do Talk (lista ou conversa `/call/<token>`): fica.
    if path.contains("/apps/spreed") || path.contains("/call/") {
        return normalized.to_owned();
    }
    // Página de outro app: o que vem antes de `/index.php` ou `/apps/`.
    let base = ["/index.php", "/apps/", "/login"]
        .iter()
        .filter_map(|marker| path.find(marker))
        .min()
        .map_or(path.as_str(), |at| &path[..at]);
    let base = base.trim_end_matches('/');
    url.set_path(&format!("{base}/index.php/apps/spreed/"));
    url.set_query(None);
    url.set_fragment(None);
    url.to_string()
}

pub(crate) fn random_hex() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let state = RandomState::new();
    (0..2u8)
        .map(|salt| format!("{:016x}", state.hash_one((nanos, salt, std::process::id()))))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn talk_account_only_on_nextcloud() {
        let mut nc =
            Service::new(ServiceKind::Nextcloud, None, Some("nuvem.x.com".into())).unwrap();
        // Arquivo antigo (sem `talk`) lê igual.
        let old = serde_json::to_value(&nc).unwrap();
        assert!(old.get("talk").is_none());
        let back: Service = serde_json::from_value(old).unwrap();
        assert!(back.talk.is_none());
        nc.talk = Some(crate::talk::TalkAccount {
            server: "https://nuvem.x.com".into(),
            user: "ana".into(),
            uid: "ana".into(),
        });
        let json = serde_json::to_value(&nc).unwrap();
        let read: Service = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(read.sanitized().unwrap().talk, nc.talk);
        // Em outro tipo, ou com servidor sem https, o Talk é descartado.
        let mut other = Service::new(ServiceKind::Slack, None, None).unwrap();
        other.talk = nc.talk.clone();
        assert!(other.sanitized().unwrap().talk.is_none());
        let mut plain = nc.clone();
        plain.talk.as_mut().unwrap().server = "http://nuvem.x.com".into();
        assert!(plain.sanitized().unwrap().talk.is_none());
    }

    #[test]
    fn nextcloud_talk_url() {
        let talk = |raw: &str| {
            Service::new(ServiceKind::Nextcloud, None, Some(raw.into()))
                .unwrap()
                .url
        };
        assert!(Service::new(ServiceKind::Nextcloud, None, None).is_err());
        assert_eq!(
            talk("cloud.empresa.com"),
            "https://cloud.empresa.com/index.php/apps/spreed/"
        );
        assert_eq!(
            talk("https://empresa.com/nextcloud/"),
            "https://empresa.com/nextcloud/index.php/apps/spreed/"
        );
        assert_eq!(
            talk("https://cloud.empresa.com/index.php/apps/files/?dir=/x"),
            "https://cloud.empresa.com/index.php/apps/spreed/"
        );
        assert_eq!(
            talk("https://cloud.empresa.com/apps/dashboard/"),
            "https://cloud.empresa.com/index.php/apps/spreed/"
        );
        assert_eq!(
            talk("https://cloud.empresa.com/login?redirect_url=/x"),
            "https://cloud.empresa.com/index.php/apps/spreed/"
        );
        // Já no Talk: fica como está.
        assert_eq!(
            talk("https://cloud.empresa.com/apps/spreed/"),
            "https://cloud.empresa.com/apps/spreed/"
        );
        assert_eq!(
            talk("https://cloud.empresa.com/call/abc123"),
            "https://cloud.empresa.com/call/abc123"
        );
        let s = Service::new(ServiceKind::Nextcloud, None, Some("cloud.x.com".into())).unwrap();
        assert_eq!(s.name, "Nextcloud Talk");
        assert_eq!(s.media_policy(), MediaPolicy::Allow);
        assert!(!ServiceKind::Nextcloud.supports_chat() && !ServiceKind::Nextcloud.is_email());
        assert_eq!(
            serde_json::to_string(&ServiceKind::Nextcloud).unwrap(),
            r#""nextcloud""#
        );
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
    fn discord_defaults_without_quick_reply() {
        let svc = Service::new(ServiceKind::Discord, None, None).unwrap();
        assert_eq!(svc.name, "Discord");
        assert_eq!(svc.url, "https://discord.com/app");
        assert!(!ServiceKind::Discord.supports_chat());
        assert!(ServiceKind::Telegram.supports_chat());
        let kind: ServiceKind = serde_json::from_str(r#""discord""#).unwrap();
        assert_eq!(kind, ServiceKind::Discord);
    }

    #[test]
    fn email_kinds() {
        let gmail = Service::new(ServiceKind::Gmail, None, None).unwrap();
        assert_eq!(gmail.name, "Gmail");
        assert_eq!(gmail.url, "https://mail.google.com/mail/u/0/");
        assert_eq!(gmail.media_policy(), MediaPolicy::Ask);
        // Webmail genérico: o endereço é obrigatório e o nome vem dele.
        assert!(Service::new(ServiceKind::Email, None, None).is_err());
        let webmail = Service::new(
            ServiceKind::Email,
            None,
            Some("webmail.exemplo.com.br".into()),
        )
        .unwrap();
        assert_eq!(webmail.name, "webmail.exemplo.com.br");
        assert!(ServiceKind::Gmail.is_email() && ServiceKind::Email.is_email());
        let outlook = Service::new(ServiceKind::Outlook, None, None).unwrap();
        assert_eq!(outlook.name, "Outlook");
        assert_eq!(outlook.url, "https://outlook.live.com/mail/");
        assert_eq!(outlook.media_policy(), MediaPolicy::Ask);
        assert!(ServiceKind::Outlook.is_email() && !ServiceKind::Outlook.supports_chat());
        let work = Service::new(
            ServiceKind::Outlook,
            None,
            Some("https://outlook.office.com/mail/".into()),
        )
        .unwrap();
        assert_eq!(work.url, "https://outlook.office.com/mail/");
        assert_eq!(
            serde_json::to_string(&ServiceKind::Outlook).unwrap(),
            r#""outlook""#
        );
        assert!(!ServiceKind::Gmail.supports_chat() && !ServiceKind::Whatsapp.is_email());
        // Slack e Teams (spec 19): chamadas liberadas, sem conversa nem e-mail.
        let slack = Service::new(ServiceKind::Slack, None, None).unwrap();
        assert_eq!(
            (slack.name.as_str(), slack.url.as_str()),
            ("Slack", "https://app.slack.com/client")
        );
        let teams = Service::new(ServiceKind::Teams, None, None).unwrap();
        assert_eq!(
            (teams.name.as_str(), teams.url.as_str()),
            ("Teams", "https://teams.microsoft.com/v2/")
        );
        let personal = Service::new(
            ServiceKind::Teams,
            None,
            Some("https://teams.live.com/v2/".into()),
        )
        .unwrap();
        assert_eq!(personal.url, "https://teams.live.com/v2/");
        for kind in [ServiceKind::Slack, ServiceKind::Teams] {
            let s = Service::new(kind, None, None).unwrap();
            assert_eq!(s.media_policy(), MediaPolicy::Allow);
            assert!(!kind.supports_chat() && !kind.is_email() && !kind.supports_mail_watch());
        }
        assert_eq!(
            serde_json::to_string(&ServiceKind::Slack).unwrap(),
            r#""slack""#
        );
        assert_eq!(
            serde_json::to_string(&ServiceKind::Teams).unwrap(),
            r#""teams""#
        );
        let kind: ServiceKind = serde_json::from_str(r#""email""#).unwrap();
        assert_eq!(kind, ServiceKind::Email);
        assert_eq!(
            serde_json::to_string(&ServiceKind::Gmail).unwrap(),
            r#""gmail""#
        );
    }

    #[test]
    fn mail_watch_only_for_password_mailboxes() {
        let account = crate::mail::MailAccount {
            host: "IMAP.gmail.com".into(),
            port: 993,
            user: "ana@gmail.com".into(),
            sleep_site: true,
            starttls: false,
            whole_inbox: false,
            auth: crate::mail::MailAuth::Password,
            signature: String::new(),
            signature_html: Default::default(),
        };
        let mut gmail = Service::new(ServiceKind::Gmail, None, None).unwrap();
        gmail.mail = Some(account.clone());
        let json = serde_json::to_string(&gmail).unwrap();
        assert!(json.contains(r#""mail":{"host":"IMAP.gmail.com""#));
        let back: Service = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.sanitized().unwrap().mail.unwrap().host,
            "imap.gmail.com"
        );
        // Outlook (só OAuth) e mensageiros: descartado.
        for kind in [ServiceKind::Outlook, ServiceKind::Whatsapp] {
            let mut other = Service::new(kind, None, None).unwrap();
            other.mail = Some(account.clone());
            assert!(other.sanitized().unwrap().mail.is_none());
        }
        // Servidor inválido: só o IMAP sai, a conta continua.
        let mut bad = Service::new(ServiceKind::Gmail, None, None).unwrap();
        bad.mail = Some(crate::mail::MailAccount {
            host: "x y".into(),
            ..account
        });
        assert!(bad.sanitized().unwrap().mail.is_none());
        // Sem IMAP, nada novo aparece no arquivo.
        let plain = Service::new(ServiceKind::Gmail, None, None).unwrap();
        assert!(!serde_json::to_string(&plain).unwrap().contains("\"mail\""));
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
    fn appearance_validation() {
        assert_eq!(
            normalize_color(Some("#ABC")).unwrap().as_deref(),
            Some("#aabbcc")
        );
        assert_eq!(
            normalize_color(Some("25D366")).unwrap().as_deref(),
            Some("#25d366")
        );
        assert_eq!(normalize_color(Some("  ")).unwrap(), None);
        assert!(normalize_color(Some("red")).is_err());
        assert!(normalize_color(Some("#12345g")).is_err());
        assert!(normalize_color(Some("#fff;background:url(x)")).is_err());

        assert_eq!(normalize_emoji(Some("💼")).unwrap().as_deref(), Some("💼"));
        assert_eq!(normalize_emoji(Some("👨‍👩‍👧")).unwrap().as_deref(), Some("👨‍👩‍👧"));
        assert_eq!(normalize_emoji(Some("🇧🇷")).unwrap().as_deref(), Some("🇧🇷"));
        assert_eq!(normalize_emoji(Some(" P ")).unwrap().as_deref(), Some("P"));
        assert_eq!(normalize_emoji(None).unwrap(), None);
        assert!(normalize_emoji(Some("Pessoal")).is_err());
        assert!(normalize_emoji(Some("a b")).is_err());

        assert_eq!(normalize_name("  Pessoal ").unwrap(), "Pessoal");
        assert!(normalize_name("   ").is_err());
        assert!(normalize_name(&"x".repeat(41)).is_err());
    }

    #[test]
    fn old_record_gets_defaults() {
        let json = r#"{"id":"svc-1","kind":"telegram","name":"T","url":"https://web.telegram.org/a/","session":"00"}"#;
        let s: Service = serde_json::from_str(json).unwrap();
        assert!(s.notifications);
        assert_eq!(s.hibernate_after_minutes, 0);
        assert_eq!(s.zoom, 1.0);
        assert_eq!(s.media_policy(), MediaPolicy::Allow);
    }

    #[test]
    fn media_policy_defaults_by_kind() {
        let custom = Service::new(ServiceKind::Custom, None, Some("x.com".into())).unwrap();
        assert_eq!(custom.media_policy(), MediaPolicy::Ask);
        let mut wa = Service::new(ServiceKind::Whatsapp, None, None).unwrap();
        assert_eq!(wa.media_policy(), MediaPolicy::Allow);
        wa.media = Some(MediaPolicy::Block);
        assert_eq!(wa.media_policy(), MediaPolicy::Block);
    }

    #[test]
    fn zoom_is_clamped_and_stepped() {
        assert_eq!(clamp_zoom(1.04), 1.0);
        assert_eq!(clamp_zoom(1.15), 1.2);
        assert_eq!(clamp_zoom(9.0), ZOOM_MAX);
        assert_eq!(clamp_zoom(0.1), ZOOM_MIN);
        assert_eq!(clamp_zoom(f64::NAN), 1.0);
    }

    #[test]
    fn sanitized_rejects_path_tricks() {
        let good = Service::new(ServiceKind::Whatsapp, None, None).unwrap();
        assert!(good.clone().sanitized().is_ok());

        let mut bad = good.clone();
        bad.session = "../../../etc/passwd/aaaaaaaaaaaaa".into();
        assert!(bad.sanitized().is_err());

        let mut bad = good.clone();
        bad.id = "ui".into();
        assert!(bad.sanitized().is_err());

        let mut bad = good;
        bad.url = "file:///etc/passwd".into();
        assert!(bad.sanitized().is_err());
    }

    #[test]
    fn long_host_names_fit() {
        let long = "mattermost.departamento.universidade.exemplo.edu.br";
        let s = Service::new(ServiceKind::Custom, None, Some(long.into())).unwrap();
        assert_eq!(s.name.chars().count(), NAME_MAX);
        assert!(normalize_name(&s.name).is_ok());
        let second = unique_name(&s.name, std::slice::from_ref(&s));
        assert!(second.ends_with(" 2") && second.chars().count() <= NAME_MAX);

        // Nome longo gravado por versão antiga é reparado, não rejeitado.
        let mut old = s.clone();
        old.name = "x".repeat(60);
        assert_eq!(old.sanitized().unwrap().name.chars().count(), NAME_MAX);
    }

    #[test]
    fn sessions_are_unique() {
        let a = Service::new(ServiceKind::Whatsapp, None, None).unwrap();
        let b = Service::new(ServiceKind::Whatsapp, None, None).unwrap();
        assert_ne!(a.session, b.session);
        assert_ne!(a.session_bytes(), [0u8; 16]);
    }
}
