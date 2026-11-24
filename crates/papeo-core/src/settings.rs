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
}
