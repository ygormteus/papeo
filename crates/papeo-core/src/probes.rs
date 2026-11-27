//! Diagnóstico dos sites (spec 14): pontos de apoio que o script injetado
//! procura na página de cada conta (lista de conversas, busca, campo de
//! texto…). Um ponto ausente por muito tempo indica que o site mudou e que
//! o recurso que depende dele vai falhar em silêncio.
//!
//! Só presença: nenhum texto da página chega aqui. O estado fica na memória
//! e recomeça com a página.

use std::hash::{DefaultHasher, Hash, Hasher};

use serde::Serialize;

use crate::ServiceKind;

/// Ainda ausente numa leitura feita este tempo depois da primeira ausência
/// (sem nenhuma presença no meio) = o site provavelmente mudou. Só o tempo
/// passando, sem leitura nova, não basta.
pub const MISSING_AFTER_MS: u64 = 10 * 60 * 1000;

/// Ausências relatadas no mínimo (o script relata a cada 20 s): uma leitura
/// só, no meio de uma troca de tela, não basta.
pub const MISSING_MIN_REPORTS: u16 = 3;

/// O campo de texto precisa faltar em tantas conversas diferentes: canais e
/// grupos só de avisos não têm campo.
pub const COMPOSER_MIN_CHATS: usize = 3;

/// Ponto de apoio procurado na página.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Probe {
    /// Lista de conversas.
    List,
    /// Caixa de busca do site.
    Search,
    /// Nome da conversa aberta, no cabeçalho.
    Header,
    /// Campo de texto da conversa aberta.
    Composer,
    /// Caixa de Entrada (contador de não lidas).
    Inbox,
    /// Assunto do e-mail aberto.
    Subject,
}

impl Probe {
    const ALL: [Probe; 6] = [
        Probe::List,
        Probe::Search,
        Probe::Header,
        Probe::Composer,
        Probe::Inbox,
        Probe::Subject,
    ];

    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "list" => Probe::List,
            "search" => Probe::Search,
            "header" => Probe::Header,
            "composer" => Probe::Composer,
            "inbox" => Probe::Inbox,
            "subject" => Probe::Subject,
            _ => return None,
        })
    }
}

/// Pontos que fazem sentido em cada tipo de conta. O "Outro e-mail" só
/// tem pontos quando o webmail é um Roundcube (o script decide).
pub fn probes_for(kind: ServiceKind) -> &'static [Probe] {
    match kind {
        ServiceKind::Whatsapp | ServiceKind::Telegram => {
            &[Probe::List, Probe::Search, Probe::Header, Probe::Composer]
        }
        ServiceKind::Gmail => &[Probe::Inbox, Probe::Subject],
        ServiceKind::Outlook => &[Probe::Inbox, Probe::Search, Probe::Subject],
        ServiceKind::Email => &[Probe::Inbox, Probe::Search],
        ServiceKind::Discord
        | ServiceKind::Custom
        | ServiceKind::Slack
        | ServiceKind::Teams
        | ServiceKind::Nextcloud => &[],
    }
}

/// A resposta rápida abre a conversa pela busca, confere o cabeçalho e
/// escreve no campo de texto.
pub const REPLY_NEEDS: [Probe; 3] = [Probe::Search, Probe::Header, Probe::Composer];

/// Como um ponto está agora.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProbeState {
    Found,
    /// Ausente há menos de `MISSING_AFTER_MS`.
    Missing,
    /// Ausente há `MISSING_AFTER_MS` ou mais.
    Broken,
    /// Ainda não deu para avaliar (ex.: campo de texto sem conversa aberta).
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeView {
    pub probe: Probe,
    pub state: ProbeState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Seen {
    /// Nunca relatado por esta página.
    #[default]
    Absent,
    Unknown,
    Found,
    Missing(Missing),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Missing {
    /// Primeira ausência (ms).
    since: u64,
    /// Última ausência relatada (ms).
    last: u64,
    /// Ausências relatadas desde então.
    reports: u16,
    /// Conversas diferentes em que faltou (resumos, não os nomes).
    chats: [u64; COMPOSER_MIN_CHATS],
    chat_count: u8,
}

impl Missing {
    fn new(now_ms: u64, chat: u64) -> Self {
        let mut chats = [0; COMPOSER_MIN_CHATS];
        chats[0] = chat;
        Missing {
            since: now_ms,
            last: now_ms,
            reports: 1,
            chats,
            chat_count: 1,
        }
    }

    fn again(&mut self, now_ms: u64, chat: u64) {
        self.last = self.last.max(now_ms);
        self.reports = self.reports.saturating_add(1);
        let n = self.chat_count as usize;
        if n < COMPOSER_MIN_CHATS && !self.chats[..n].contains(&chat) {
            self.chats[n] = chat;
            self.chat_count += 1;
        }
    }

    fn confirmed(&self, probe: Probe) -> bool {
        self.last.saturating_sub(self.since) >= MISSING_AFTER_MS
            && self.reports >= MISSING_MIN_REPORTS
            && (probe != Probe::Composer || self.chat_count as usize >= COMPOSER_MIN_CHATS)
    }
}

fn chat_key(chat: Option<&str>) -> u64 {
    let mut h = DefaultHasher::new();
    chat.unwrap_or("").hash(&mut h);
    h.finish()
}

/// Estado dos pontos de uma página (uma posição por `Probe`; `Copy` para
/// caber na saúde da conta).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProbeTracker {
    seen: [Seen; 6],
}

impl ProbeTracker {
    /// Aplica um relatório do script. `None` (não avaliado) não muda o
    /// ponto; presença zera a ausência; ausência marca o início e conta.
    /// `chat` é a conversa aberta no mesmo relatório: o campo de texto só
    /// conta como sumido se faltar em mais de uma conversa (canais e grupos
    /// só de avisos não têm campo). Pontos de fora do tipo são ignorados.
    /// Devolve se algum estado visível mudou.
    pub fn update<'a>(
        &mut self,
        kind: ServiceKind,
        report: impl IntoIterator<Item = (&'a str, Option<bool>)>,
        chat: Option<&str>,
        now_ms: u64,
    ) -> bool {
        let chat = chat_key(chat);
        let before = self.view();
        let allowed = probes_for(kind);
        for (name, value) in report {
            let Some(probe) = Probe::parse(name).filter(|p| allowed.contains(p)) else {
                continue;
            };
            let slot = &mut self.seen[probe as usize];
            if *slot == Seen::Absent {
                *slot = Seen::Unknown;
            }
            match (value, slot) {
                (Some(true), slot) => *slot = Seen::Found,
                (Some(false), Seen::Missing(m)) => m.again(now_ms, chat),
                (Some(false), slot) => *slot = Seen::Missing(Missing::new(now_ms, chat)),
                (None, _) => {}
            }
        }
        before != self.view()
    }

    pub fn state(&self, probe: Probe) -> ProbeState {
        match self.seen[probe as usize] {
            Seen::Absent | Seen::Unknown => ProbeState::Unknown,
            Seen::Found => ProbeState::Found,
            Seen::Missing(m) if m.confirmed(probe) => ProbeState::Broken,
            Seen::Missing(_) => ProbeState::Missing,
        }
    }

    /// Algum ponto ausente há tempo demais.
    pub fn any_broken(&self) -> bool {
        Probe::ALL
            .iter()
            .any(|p| self.state(*p) == ProbeState::Broken)
    }

    /// A resposta rápida continua oferecida enquanto nenhum ponto de que ela
    /// depende está ausente há tempo demais.
    pub fn can_reply(&self) -> bool {
        REPLY_NEEDS
            .iter()
            .all(|p| self.state(*p) != ProbeState::Broken)
    }

    /// Pontos que o script já relatou para esta página, na ordem fixa.
    pub fn view(&self) -> Vec<ProbeView> {
        Probe::ALL
            .iter()
            .filter(|p| self.seen[**p as usize] != Seen::Absent)
            .map(|probe| ProbeView {
                probe: *probe,
                state: self.state(*probe),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: u64 = 60 * 1000;

    /// Relata `value` para `probe` a cada minuto de `from` a `to` (inclusive).
    fn every_minute(
        t: &mut ProbeTracker,
        kind: ServiceKind,
        probe: &str,
        value: Option<bool>,
        chat: Option<&str>,
        minutes: std::ops::RangeInclusive<u64>,
    ) {
        for m in minutes {
            t.update(kind, [(probe, value)], chat, m * MIN);
        }
    }

    #[test]
    fn missing_becomes_broken_after_ten_minutes_of_reports() {
        let mut t = ProbeTracker::default();
        let kind = ServiceKind::Whatsapp;
        assert!(t.update(kind, [("search", Some(false))], None, 0));
        every_minute(&mut t, kind, "search", Some(false), None, 1..=9);
        assert_eq!(t.state(Probe::Search), ProbeState::Missing);
        assert!(t.update(kind, [("search", Some(false))], None, 10 * MIN));
        assert_eq!(t.state(Probe::Search), ProbeState::Broken);
        assert!(!t.can_reply());
        assert!(t.any_broken());
        // Encontrado de novo: tudo volta.
        assert!(t.update(kind, [("search", Some(true))], None, 11 * MIN));
        assert!(t.can_reply());
        assert!(!t.any_broken());
    }

    #[test]
    fn time_alone_does_not_confirm_an_absence() {
        let mut t = ProbeTracker::default();
        let kind = ServiceKind::Telegram;
        every_minute(&mut t, kind, "header", Some(false), Some("Ana"), 0..=3);
        // Depois só "não avaliado" (conversa fechada): nunca confirma.
        every_minute(&mut t, kind, "header", None, None, 4..=30);
        assert_eq!(t.state(Probe::Header), ProbeState::Missing);
        assert!(t.can_reply());
        // Volta a faltar mais de 10 min depois da primeira ausência: confirma.
        t.update(kind, [("header", Some(false))], Some("Ana"), 31 * MIN);
        assert_eq!(t.state(Probe::Header), ProbeState::Broken);
        // Nunca avaliado aparece como desconhecido.
        t.update(kind, [("composer", None)], None, 31 * MIN);
        assert_eq!(t.state(Probe::Composer), ProbeState::Unknown);
    }

    #[test]
    fn composer_needs_absence_in_three_chats() {
        let mut t = ProbeTracker::default();
        let kind = ServiceKind::Telegram;
        // Canais só de leitura, um depois do outro: não é o site mudando.
        every_minute(
            &mut t,
            kind,
            "composer",
            Some(false),
            Some("Canal A"),
            0..=2,
        );
        every_minute(
            &mut t,
            kind,
            "composer",
            Some(false),
            Some("Canal B"),
            3..=30,
        );
        assert_eq!(t.state(Probe::Composer), ProbeState::Missing);
        assert!(t.can_reply());
        // Falta também numa terceira conversa: aí sim.
        t.update(kind, [("composer", Some(false))], Some("Ana"), 31 * MIN);
        assert_eq!(t.state(Probe::Composer), ProbeState::Broken);
        assert!(!t.can_reply());
        // Uma conversa normal com o campo de texto desfaz tudo.
        t.update(kind, [("composer", Some(true))], Some("Bia"), 32 * MIN);
        assert!(t.can_reply());
    }

    #[test]
    fn probes_outside_the_kind_are_ignored() {
        let mut t = ProbeTracker::default();
        assert!(!t.update(
            ServiceKind::Gmail,
            [("composer", Some(false)), ("bogus", Some(false))],
            None,
            0
        ));
        assert!(t.view().is_empty());
        assert!(!t.update(ServiceKind::Discord, [("list", Some(false))], None, 0));
        assert!(t.update(ServiceKind::Gmail, [("inbox", Some(true))], None, 0));
        assert_eq!(
            t.view(),
            vec![ProbeView {
                probe: Probe::Inbox,
                state: ProbeState::Found
            }]
        );
    }

    #[test]
    fn reply_ignores_probes_it_does_not_need() {
        let mut t = ProbeTracker::default();
        every_minute(
            &mut t,
            ServiceKind::Whatsapp,
            "list",
            Some(false),
            None,
            0..=20,
        );
        assert_eq!(t.state(Probe::List), ProbeState::Broken);
        assert!(t.can_reply());
    }
}
