//! Configurações gerais do app e regras de "não perturbe".

use crate::privacy::NotificationPreview;
use crate::service::Service;
use serde::{Deserialize, Serialize};

/// Valor de `dnd_until` que significa "até eu reativar".
pub const DND_FOREVER: u64 = u64::MAX;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Não perturbe global até este instante (segundos Unix).
    pub dnd_until: Option<u64>,
    pub quiet_hours: QuietHours,
    /// Pasta dos downloads; sem valor, a pasta Downloads do sistema.
    pub downloads_dir: Option<String>,
    /// Fechar a janela só a esconde; o app continua na bandeja/menu bar.
    pub close_to_tray: bool,
    /// Ao abrir junto com o sistema, começa escondido na bandeja.
    pub start_hidden: bool,
    /// Ao voltar para o app logo após uma notificação, abre aquele serviço.
    pub open_on_notification_focus: bool,
    /// Quanto do conteúdo das mensagens aparece nas notificações.
    pub notification_preview: NotificationPreview,
    /// Esconde a janela em capturas e compartilhamento de tela.
    pub protect_screen: bool,
    pub lock: AppLock,
    /// Tema da interface; também define o tema claro/escuro da janela, que
    /// os sites dos serviços enxergam como `prefers-color-scheme`.
    pub appearance: Appearance,
    pub updates: UpdatePrefs,
    /// Recarrega um serviço em segundo plano que passar deste uso de
    /// memória, em MB (o WhatsApp Web cresce com o tempo). 0 = desligado.
    pub reload_above_mb: u32,
    /// Onde abrem os links que os sites mandam abrir fora.
    pub open_links: LinkTarget,
    /// O que fazer com os anexos baixados nos serviços.
    pub attachments: AttachmentMode,
    /// Textos salvos para a resposta rápida (chamados com "/").
    pub quick_replies: Vec<QuickReply>,
}

/// Resposta pronta: um atalho curto e o texto que entra na resposta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickReply {
    pub shortcut: String,
    pub text: String,
}

/// Limites das respostas prontas.
pub const QUICK_REPLIES_MAX: usize = 100;
pub const QUICK_REPLY_SHORTCUT_MAX: usize = 40;
/// Igual ao limite de uma resposta rápida.
pub const QUICK_REPLY_TEXT_MAX: usize = 4096;

impl QuickReply {
    /// Atalho sem "/" nem espaços nas pontas; texto sem espaços nas pontas e
    /// no limite. Sem texto, a resposta é descartada.
    fn sanitized(self) -> Option<Self> {
        let text: String = self
            .text
            .trim()
            .chars()
            .take(QUICK_REPLY_TEXT_MAX)
            .collect();
        if text.is_empty() {
            return None;
        }
        let shortcut: String = self
            .shortcut
            .trim()
            .trim_start_matches('/')
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .filter(|c| !c.is_control())
            .take(QUICK_REPLY_SHORTCUT_MAX)
            .collect();
        Some(Self { shortcut, text })
    }
}

/// Destino dos links externos clicados dentro dos serviços.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkTarget {
    /// Navegador padrão do sistema.
    #[default]
    Browser,
    /// Janela pequena do Papeo, sem login nem dados guardados.
    Preview,
}

/// Anexos (arquivos baixados pelos sites dos serviços).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AttachmentMode {
    /// Salva direto na pasta de downloads.
    #[default]
    Save,
    /// Mostra antes numa janela do Papeo (pasta temporária); salvar é escolha.
    Preview,
}

/// Limite padrão e faixa aceita para `reload_above_mb`.
pub const RELOAD_DEFAULT_MB: u32 = 2048;
pub const RELOAD_MIN_MB: u32 = 512;
pub const RELOAD_MAX_MB: u32 = 16 * 1024;

/// Verificação de atualizações no GitHub.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UpdatePrefs {
    /// Consulta a cada 2 horas se há versão nova (só depois do aviso).
    pub check: bool,
    /// A pessoa já viu o aviso de que a verificação consulta o GitHub.
    /// Até lá, nenhuma consulta automática é feita.
    pub notice_seen: bool,
}

impl Default for UpdatePrefs {
    fn default() -> Self {
        Self {
            check: true,
            notice_seen: false,
        }
    }
}

impl UpdatePrefs {
    /// A verificação automática pode rodar.
    pub fn auto_check(self) -> bool {
        self.check && self.notice_seen
    }
}

/// Tema escolhido. `Oled` é o escuro com preto puro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
    Oled,
}

impl Appearance {
    /// Tema da janela nativa: `None` segue o sistema; `Some(true)` = escuro.
    pub fn dark(self) -> Option<bool> {
        match self {
            Appearance::System => None,
            Appearance::Light => Some(false),
            Appearance::Dark | Appearance::Oled => Some(true),
        }
    }
}

/// Maior espera configurável antes do bloqueio automático.
pub const LOCK_MAX_MINUTES: u32 = 24 * 60;

/// Bloqueio do app com a autenticação do sistema (Touch ID, senha do Mac,
/// Windows Hello/PIN).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppLock {
    pub enabled: bool,
    /// 0 = ao esconder a janela; n = após n minutos sem usar o Papeo.
    /// Também bloqueia sempre ao abrir o app.
    pub after_minutes: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dnd_until: None,
            quiet_hours: QuietHours::default(),
            downloads_dir: None,
            close_to_tray: true,
            start_hidden: true,
            open_on_notification_focus: true,
            notification_preview: NotificationPreview::Full,
            protect_screen: false,
            lock: AppLock {
                enabled: false,
                after_minutes: 5,
            },
            appearance: Appearance::System,
            updates: UpdatePrefs::default(),
            reload_above_mb: RELOAD_DEFAULT_MB,
            open_links: LinkTarget::Browser,
            attachments: AttachmentMode::Save,
            quick_replies: Vec::new(),
        }
    }
}

/// Horário silencioso diário, em minutos desde a meia-noite (hora local).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct QuietHours {
    pub enabled: bool,
    pub start: u16,
    pub end: u16,
}

impl Default for QuietHours {
    fn default() -> Self {
        Self {
            enabled: false,
            start: 22 * 60,
            end: 7 * 60,
        }
    }
}

impl QuietHours {
    /// Também cobre faixas que atravessam a meia-noite (22:00–07:00).
    pub fn contains(&self, minute_of_day: u16) -> bool {
        if !self.enabled || self.start == self.end {
            return false;
        }
        if self.start < self.end {
            (self.start..self.end).contains(&minute_of_day)
        } else {
            minute_of_day >= self.start || minute_of_day < self.end
        }
    }

    pub fn sanitized(self) -> Self {
        Self {
            enabled: self.enabled,
            start: self.start.min(24 * 60 - 1),
            end: self.end.min(24 * 60 - 1),
        }
    }
}

impl Settings {
    pub fn dnd_active(&self, now: u64, minute_of_day: u16) -> bool {
        self.dnd_until.is_some_and(|until| now < until) || self.quiet_hours.contains(minute_of_day)
    }

    pub fn sanitized(mut self) -> Self {
        self.quiet_hours = self.quiet_hours.sanitized();
        self.downloads_dir = self
            .downloads_dir
            .map(|d| d.trim().to_owned())
            .filter(|d| !d.is_empty() && std::path::Path::new(d).is_absolute());
        self.lock.after_minutes = self.lock.after_minutes.min(LOCK_MAX_MINUTES);
        if self.reload_above_mb != 0 {
            self.reload_above_mb = self.reload_above_mb.clamp(RELOAD_MIN_MB, RELOAD_MAX_MB);
        }
        self.quick_replies = std::mem::take(&mut self.quick_replies)
            .into_iter()
            .filter_map(QuickReply::sanitized)
            .take(QUICK_REPLIES_MAX)
            .collect();
        self
    }
}

/// Decide se a notificação de um serviço deve ser exibida agora.
pub fn should_notify(settings: &Settings, service: &Service, now: u64, minute_of_day: u16) -> bool {
    service.notifications
        && !service.muted_until.is_some_and(|until| now < until)
        && !settings.dnd_active(now, minute_of_day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::ServiceKind;

    #[test]
    fn quiet_hours_across_midnight() {
        let q = QuietHours {
            enabled: true,
            start: 22 * 60,
            end: 7 * 60,
        };
        assert!(q.contains(23 * 60));
        assert!(q.contains(0));
        assert!(q.contains(6 * 60 + 59));
        assert!(!q.contains(7 * 60));
        assert!(!q.contains(12 * 60));
        assert!(!QuietHours {
            enabled: false,
            ..q
        }
        .contains(23 * 60));
    }

    #[test]
    fn quiet_hours_same_day() {
        let q = QuietHours {
            enabled: true,
            start: 12 * 60,
            end: 13 * 60,
        };
        assert!(q.contains(12 * 60 + 30));
        assert!(!q.contains(13 * 60));
        assert!(!q.contains(11 * 60));
    }

    #[test]
    fn notify_rules() {
        let mut s = Settings::default();
        let mut svc = Service::new(ServiceKind::Whatsapp, None, None).unwrap();
        assert!(should_notify(&s, &svc, 1000, 600));

        svc.muted_until = Some(2000);
        assert!(!should_notify(&s, &svc, 1000, 600));
        assert!(should_notify(&s, &svc, 2000, 600));
        svc.muted_until = None;

        s.dnd_until = Some(DND_FOREVER);
        assert!(!should_notify(&s, &svc, 1000, 600));
        s.dnd_until = Some(500);
        assert!(should_notify(&s, &svc, 1000, 600));

        s.quiet_hours.enabled = true;
        assert!(!should_notify(&s, &svc, 1000, 23 * 60));

        svc.notifications = false;
        assert!(!should_notify(&Settings::default(), &svc, 1000, 600));
    }

    #[test]
    fn downloads_dir_must_be_absolute() {
        let s = Settings {
            downloads_dir: Some("relativo/pasta".into()),
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(s.downloads_dir, None);
    }

    #[test]
    fn appearance_maps_to_window_theme() {
        assert_eq!(Appearance::System.dark(), None);
        assert_eq!(Appearance::Light.dark(), Some(false));
        assert_eq!(Appearance::Oled.dark(), Some(true));
        let old: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(old.appearance, Appearance::System);
        let oled: Settings = serde_json::from_str(r#"{"appearance":"oled"}"#).unwrap();
        assert_eq!(oled.appearance, Appearance::Oled);
    }

    #[test]
    fn reload_limit_default_and_range() {
        let old: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(old.reload_above_mb, RELOAD_DEFAULT_MB);
        let off: Settings = serde_json::from_str(r#"{"reloadAboveMb":0}"#).unwrap();
        assert_eq!(off.sanitized().reload_above_mb, 0);
        let low: Settings = serde_json::from_str(r#"{"reloadAboveMb":10}"#).unwrap();
        assert_eq!(low.sanitized().reload_above_mb, RELOAD_MIN_MB);
    }

    #[test]
    fn quick_replies_are_cleaned() {
        let old: Settings = serde_json::from_str("{}").unwrap();
        assert!(old.quick_replies.is_empty());
        let s = Settings {
            quick_replies: vec![
                QuickReply {
                    shortcut: "  /orça mento ".into(),
                    text: "  Segue o orçamento.  ".into(),
                },
                QuickReply {
                    shortcut: "vazio".into(),
                    text: "   ".into(),
                },
            ],
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(
            s.quick_replies,
            vec![QuickReply {
                shortcut: "orça mento".into(),
                text: "Segue o orçamento.".into(),
            }]
        );
        let many = Settings {
            quick_replies: vec![
                QuickReply {
                    shortcut: "x".repeat(99),
                    text: "y".repeat(5000),
                };
                QUICK_REPLIES_MAX + 5
            ],
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(many.quick_replies.len(), QUICK_REPLIES_MAX);
        assert_eq!(
            many.quick_replies[0].shortcut.chars().count(),
            QUICK_REPLY_SHORTCUT_MAX
        );
        assert_eq!(
            many.quick_replies[0].text.chars().count(),
            QUICK_REPLY_TEXT_MAX
        );
    }

    #[test]
    fn links_open_in_browser_by_default() {
        let old: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(old.open_links, LinkTarget::Browser);
        let preview: Settings = serde_json::from_str(r#"{"openLinks":"preview"}"#).unwrap();
        assert_eq!(preview.open_links, LinkTarget::Preview);
    }

    #[test]
    fn attachments_saved_by_default() {
        let old: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(old.attachments, AttachmentMode::Save);
        let shown: Settings = serde_json::from_str(r#"{"attachments":"preview"}"#).unwrap();
        assert_eq!(shown.attachments, AttachmentMode::Preview);
    }

    #[test]
    fn updates_wait_for_notice() {
        let fresh: Settings = serde_json::from_str("{}").unwrap();
        assert!(fresh.updates.check);
        assert!(!fresh.updates.notice_seen);
        assert!(!fresh.updates.auto_check());
        let seen = UpdatePrefs {
            check: true,
            notice_seen: true,
        };
        assert!(seen.auto_check());
        assert!(!UpdatePrefs {
            check: false,
            ..seen
        }
        .auto_check());
    }
}
