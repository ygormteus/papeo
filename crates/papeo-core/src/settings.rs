//! Configurações gerais do app e regras de "não perturbe".

use crate::privacy::NotificationPreview;
use crate::service::{Service, ServiceContext};
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
    /// Como o microfone é aberto nas mensagens de voz (macOS).
    pub voice_capture: VoiceCapture,
    /// Textos salvos para a resposta rápida (chamados com "/").
    pub quick_replies: Vec<QuickReply>,
    /// Modo escolhido: todas as contas, só trabalho, só pessoal ou pelo
    /// expediente.
    pub mode: ModeChoice,
    /// Expediente usado no modo automático.
    pub work_hours: WorkHours,
    /// Hiberna as contas que ficam fora do modo atual (libera memória).
    pub sleep_outside_mode: bool,
    /// Moldura com a cor da conta em volta do site aberto.
    pub account_frame: bool,
    /// Modo de diagnóstico: avisa na barra do topo quando um ponto de apoio
    /// do site some (spec 14).
    pub diagnostics: bool,
}

/// Modo valendo agora: quais contas aparecem e notificam.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    All,
    Work,
    Personal,
}

impl Mode {
    /// Conta sem trabalho/pessoal marcado aparece em todos os modos.
    pub fn allows(self, context: Option<ServiceContext>) -> bool {
        match (self, context) {
            (Mode::All, _) | (_, None) => true,
            (Mode::Work, Some(c)) => c == ServiceContext::Work,
            (Mode::Personal, Some(c)) => c == ServiceContext::Personal,
        }
    }
}

/// Modo escolhido nas configurações.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModeChoice {
    #[default]
    All,
    Work,
    Personal,
    /// Trabalho no expediente, pessoal fora dele.
    Auto,
}

impl ModeChoice {
    /// Modo pela escolha e, no automático, pelo expediente.
    pub fn scheduled(self, hours: &WorkHours, weekday: u8, minute_of_day: u16) -> Mode {
        match self {
            ModeChoice::All => Mode::All,
            ModeChoice::Work => Mode::Work,
            ModeChoice::Personal => Mode::Personal,
            ModeChoice::Auto if hours.contains(weekday, minute_of_day) => Mode::Work,
            ModeChoice::Auto => Mode::Personal,
        }
    }

    /// Minutos até o modo do horário mudar (começo ou fim do expediente);
    /// `None` se não muda na próxima semana.
    pub fn minutes_until_change(
        self,
        hours: &WorkHours,
        weekday: u8,
        minute_of_day: u16,
    ) -> Option<u32> {
        let now = self.scheduled(hours, weekday, minute_of_day);
        (1..=8 * DAY_MINUTES).find(|&i| {
            let total = u32::from(minute_of_day) + i;
            let day = ((u32::from(weekday) + total / DAY_MINUTES) % 7) as u8;
            self.scheduled(hours, day, (total % DAY_MINUTES) as u16) != now
        })
    }
}

const DAY_MINUTES: u32 = 24 * 60;

/// Troca manual feita no modo automático: vale até o expediente mudar o
/// modo (começar ou terminar). Só na memória: reabrir o app volta ao horário.
/// O fim é um instante, não o modo do horário: com o computador dormindo
/// durante a virada, a troca não ressuscita no dia seguinte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeOverride {
    pub mode: Mode,
    /// Segundos Unix da próxima virada do horário (`u64::MAX`: sem virada).
    pub until: u64,
}

impl ModeOverride {
    pub fn holds(self, now: u64) -> bool {
        now < self.until
    }
}

/// Expediente: dias da semana (0 = domingo … 6 = sábado) e horário em
/// minutos desde a meia-noite (hora local). Pode atravessar a meia-noite;
/// nesse caso o dia que conta é o do início.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkHours {
    pub days: Vec<u8>,
    pub start: u16,
    pub end: u16,
}

impl Default for WorkHours {
    fn default() -> Self {
        Self {
            days: vec![1, 2, 3, 4, 5],
            start: 9 * 60,
            end: 18 * 60,
        }
    }
}

impl WorkHours {
    pub fn contains(&self, weekday: u8, minute_of_day: u16) -> bool {
        if self.start == self.end {
            return false;
        }
        let on = |day: u8| self.days.contains(&day);
        if self.start < self.end {
            on(weekday) && (self.start..self.end).contains(&minute_of_day)
        } else {
            (on(weekday) && minute_of_day >= self.start)
                || (on((weekday + 6) % 7) && minute_of_day < self.end)
        }
    }

    pub fn sanitized(mut self) -> Self {
        self.days.retain(|d| *d < 7);
        self.days.sort_unstable();
        self.days.dedup();
        self.start = self.start.min(24 * 60 - 1);
        self.end = self.end.min(24 * 60 - 1);
        self
    }
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

/// Gravação das mensagens de voz. No WebKit (macOS), a redução de eco e
/// ruído do sistema leva ~1 s para começar e o começo do áudio sai mudo; por
/// isso o padrão é sem ela. A limpeza do Papeo (RNNoise e nivelador) começa
/// na hora e vale também no Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VoiceCapture {
    /// Começa na hora: som original no macOS, padrão do sistema no Windows.
    #[default]
    Instant,
    /// Começa na hora, com a limpeza do Papeo.
    Enhanced,
    /// Redução de eco e ruído do navegador (no macOS, a do sistema).
    Clean,
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
            voice_capture: VoiceCapture::Instant,
            quick_replies: Vec::new(),
            mode: ModeChoice::All,
            work_hours: WorkHours::default(),
            sleep_outside_mode: false,
            account_frame: true,
            diagnostics: false,
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

    /// Modo valendo agora (`weekday`: 0 = domingo), com a troca manual do
    /// automático, se ainda valer.
    pub fn current_mode(
        &self,
        weekday: u8,
        minute_of_day: u16,
        now: u64,
        manual: Option<ModeOverride>,
    ) -> Mode {
        match manual {
            Some(o) if self.mode == ModeChoice::Auto && o.holds(now) => o.mode,
            _ => self
                .mode
                .scheduled(&self.work_hours, weekday, minute_of_day),
        }
    }

    pub fn sanitized(mut self) -> Self {
        self.quiet_hours = self.quiet_hours.sanitized();
        self.work_hours = std::mem::take(&mut self.work_hours).sanitized();
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

/// Decide se a notificação de um serviço deve ser exibida agora. `mode`:
/// o modo valendo agora (conta de fora dele fica em silêncio).
pub fn should_notify(
    settings: &Settings,
    service: &Service,
    now: u64,
    minute_of_day: u16,
    mode: Mode,
) -> bool {
    service.notifications
        && mode.allows(service.context)
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
        assert!(should_notify(&s, &svc, 1000, 600, Mode::All));

        svc.muted_until = Some(2000);
        assert!(!should_notify(&s, &svc, 1000, 600, Mode::All));
        assert!(should_notify(&s, &svc, 2000, 600, Mode::All));
        svc.muted_until = None;

        s.dnd_until = Some(DND_FOREVER);
        assert!(!should_notify(&s, &svc, 1000, 600, Mode::All));
        s.dnd_until = Some(500);
        assert!(should_notify(&s, &svc, 1000, 600, Mode::All));

        s.quiet_hours.enabled = true;
        assert!(!should_notify(&s, &svc, 1000, 23 * 60, Mode::All));

        svc.notifications = false;
        assert!(!should_notify(
            &Settings::default(),
            &svc,
            1000,
            600,
            Mode::All
        ));
    }

    #[test]
    fn mode_filters_accounts_and_notifications() {
        let s = Settings::default();
        let mut work = Service::new(ServiceKind::Whatsapp, None, None).unwrap();
        work.context = Some(ServiceContext::Work);
        let both = Service::new(ServiceKind::Telegram, None, None).unwrap();
        assert!(should_notify(&s, &work, 1000, 600, Mode::Work));
        assert!(!should_notify(&s, &work, 1000, 600, Mode::Personal));
        assert!(should_notify(&s, &work, 1000, 600, Mode::All));
        assert!(should_notify(&s, &both, 1000, 600, Mode::Personal));
        // Arquivo antigo: sem modo nem contexto.
        let old: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(old.mode, ModeChoice::All);
        assert!(old.account_frame);
        assert!(!old.diagnostics);
        assert_eq!(old.work_hours, WorkHours::default());
        let json = r#"{"id":"svc-0123456789ab","kind":"whatsapp","name":"W","url":"https://web.whatsapp.com/","session":"0123456789abcdef0123456789abcdef"}"#;
        let svc: Service = serde_json::from_str(json).unwrap();
        assert_eq!(svc.context, None);
        assert!(!serde_json::to_string(&svc).unwrap().contains("context"));
        let ctx: Service =
            serde_json::from_str(&json.replace("}", r#","context":"personal"}"#)).unwrap();
        assert_eq!(ctx.context, Some(ServiceContext::Personal));
    }

    #[test]
    fn work_hours_and_auto_mode() {
        let h = WorkHours::default();
        // Segunda 10h: trabalho; segunda 18h, sábado 10h: fora.
        assert!(h.contains(1, 10 * 60));
        assert!(!h.contains(1, 18 * 60));
        assert!(!h.contains(6, 10 * 60));
        // Turno da noite (sexta 22h às 6h): sábado 5h ainda conta.
        let night = WorkHours {
            days: vec![5],
            start: 22 * 60,
            end: 6 * 60,
        };
        assert!(night.contains(5, 23 * 60));
        assert!(night.contains(6, 5 * 60));
        assert!(!night.contains(6, 23 * 60));
        assert!(!night.contains(5, 5 * 60));

        let s = Settings {
            mode: ModeChoice::Auto,
            ..Settings::default()
        };
        assert_eq!(s.current_mode(1, 10 * 60, 0, None), Mode::Work);
        assert_eq!(s.current_mode(1, 20 * 60, 0, None), Mode::Personal);
        // Troca manual no expediente vale até o instante da virada.
        let manual = Some(ModeOverride {
            mode: Mode::Personal,
            until: 1_000,
        });
        assert_eq!(s.current_mode(1, 11 * 60, 999, manual), Mode::Personal);
        // Passou da virada (mesmo dormindo durante ela): volta ao horário.
        assert_eq!(s.current_mode(2, 10 * 60, 1_000, manual), Mode::Work);
        // Fora do automático, a troca manual não conta.
        let fixed = Settings {
            mode: ModeChoice::Work,
            ..Settings::default()
        };
        assert_eq!(fixed.current_mode(6, 10 * 60, 0, manual), Mode::Work);

        // Viradas: segunda 10h → fim às 18h; segunda 20h → terça 9h;
        // sexta 20h → segunda 9h; sem expediente, nunca.
        let h = WorkHours::default();
        let auto = ModeChoice::Auto;
        assert_eq!(auto.minutes_until_change(&h, 1, 10 * 60), Some(8 * 60));
        assert_eq!(auto.minutes_until_change(&h, 1, 20 * 60), Some(13 * 60));
        assert_eq!(
            auto.minutes_until_change(&h, 5, 20 * 60),
            Some(2 * 1440 + 13 * 60)
        );
        let none = WorkHours {
            days: vec![],
            ..WorkHours::default()
        };
        assert_eq!(auto.minutes_until_change(&none, 1, 10 * 60), None);
        assert_eq!(ModeChoice::Work.minutes_until_change(&h, 1, 10 * 60), None);

        let messy = WorkHours {
            days: vec![9, 3, 1, 3],
            start: 5000,
            end: 60,
        }
        .sanitized();
        assert_eq!(messy.days, vec![1, 3]);
        assert_eq!(messy.start, 24 * 60 - 1);
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
        assert_eq!(old.voice_capture, VoiceCapture::Instant);
        let clean: Settings = serde_json::from_str(r#"{"voiceCapture":"clean"}"#).unwrap();
        assert_eq!(clean.voice_capture, VoiceCapture::Clean);
        let enhanced: Settings = serde_json::from_str(r#"{"voiceCapture":"enhanced"}"#).unwrap();
        assert_eq!(enhanced.voice_capture, VoiceCapture::Enhanced);
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
