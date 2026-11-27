//! Resumo do que chegou durante o não perturbe ou o horário silencioso
//! (spec 17). Só contagens e nomes de conversa, sempre na memória.

use serde::Serialize;

/// Conversas guardadas por conta no resumo.
pub const SILENCE_CHATS_MAX: usize = 5;

/// O que uma conta recebeu durante o silêncio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SilencedAccount {
    pub service_id: String,
    pub count: u32,
    /// Conversas, a mais recente primeiro (sem repetir).
    pub chats: Vec<String>,
}

/// Resumo de um período de silêncio, pronto para mostrar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SilenceSummary {
    /// Início e fim do silêncio (segundos Unix).
    pub since: u64,
    pub until: u64,
    pub total: u32,
    /// Contas na ordem da mais movimentada.
    pub accounts: Vec<SilencedAccount>,
}

/// Contagem do silêncio em andamento.
#[derive(Debug, Default, Clone)]
pub struct SilenceLog {
    since: Option<u64>,
    accounts: Vec<SilencedAccount>,
}

impl SilenceLog {
    /// Começa um silêncio novo (descarta uma contagem antiga).
    pub fn start(&mut self, now: u64) {
        self.since = Some(now);
        self.accounts.clear();
    }

    /// Conta uma mensagem segurada pelo silêncio.
    pub fn record(&mut self, service_id: &str, chat: &str, now: u64) {
        self.since.get_or_insert(now);
        let pos = match self
            .accounts
            .iter()
            .position(|a| a.service_id == service_id)
        {
            Some(pos) => pos,
            None => {
                self.accounts.push(SilencedAccount {
                    service_id: service_id.to_owned(),
                    count: 0,
                    chats: Vec::new(),
                });
                self.accounts.len() - 1
            }
        };
        let account = &mut self.accounts[pos];
        account.count = account.count.saturating_add(1);
        let chat = chat.trim();
        if !chat.is_empty() {
            account.chats.retain(|c| c != chat);
            account.chats.insert(0, chat.to_owned());
            account.chats.truncate(SILENCE_CHATS_MAX);
        }
    }

    /// Contas com mensagens contadas.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.accounts.iter().map(|a| a.service_id.as_str())
    }

    /// Conta removida: sai da contagem.
    pub fn forget(&mut self, service_id: &str) {
        self.accounts.retain(|a| a.service_id != service_id);
    }

    /// Fim do silêncio: o resumo, se algo chegou. Zera a contagem.
    pub fn finish(&mut self, now: u64) -> Option<SilenceSummary> {
        let since = self.since.take().unwrap_or(now);
        let mut accounts = std::mem::take(&mut self.accounts);
        if accounts.is_empty() {
            return None;
        }
        // Estável: empate fica na ordem de chegada.
        accounts.sort_by_key(|a| std::cmp::Reverse(a.count));
        let total = accounts.iter().fold(0u32, |n, a| n.saturating_add(a.count));
        Some(SilenceSummary {
            since,
            until: now,
            total,
            accounts,
        })
    }
}

impl SilenceSummary {
    /// Conta removida: sai do resumo (e o resumo some se ficar vazio).
    pub fn without(mut self, service_id: &str) -> Option<Self> {
        self.accounts.retain(|a| a.service_id != service_id);
        self.total = self
            .accounts
            .iter()
            .fold(0u32, |n, a| n.saturating_add(a.count));
        (!self.accounts.is_empty()).then_some(self)
    }

    /// Mesmo resumo sem os nomes das conversas (prévia oculta).
    pub fn without_chats(mut self) -> Self {
        for a in &mut self.accounts {
            a.chats.clear();
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_by_account_and_keeps_recent_chats() {
        let mut log = SilenceLog::default();
        log.start(100);
        for i in 0..7 {
            log.record("svc-a", &format!("Conversa {i}"), 101);
        }
        log.record("svc-a", "Conversa 3", 102);
        log.record("svc-b", "", 103);
        log.record("svc-b", "Ana", 104);
        let s = log.finish(200).unwrap();
        assert_eq!((s.since, s.until, s.total), (100, 200, 10));
        assert_eq!(s.accounts[0].service_id, "svc-a");
        assert_eq!(s.accounts[0].count, 8);
        assert_eq!(
            s.accounts[0].chats,
            [
                "Conversa 3",
                "Conversa 6",
                "Conversa 5",
                "Conversa 4",
                "Conversa 2"
            ]
        );
        assert_eq!(s.accounts[1].count, 2);
        assert_eq!(s.accounts[1].chats, ["Ana"]);
        // Zerou para o próximo silêncio.
        assert!(log.finish(300).is_none());
    }

    #[test]
    fn nothing_held_means_no_summary() {
        let mut log = SilenceLog::default();
        log.start(10);
        assert!(log.finish(20).is_none());
        // Registro sem `start` (app aberto já em silêncio) usa a hora dele.
        log.record("svc-a", "Ana", 30);
        assert_eq!(log.finish(40).unwrap().since, 30);
    }

    #[test]
    fn removed_account_leaves_log_and_summary() {
        let mut log = SilenceLog::default();
        log.record("svc-a", "Ana", 1);
        log.record("svc-b", "Bia", 1);
        log.forget("svc-a");
        let s = log.finish(2).unwrap();
        assert_eq!(s.total, 1);
        let s = s.without("svc-b");
        assert!(s.is_none());
        let mut log = SilenceLog::default();
        log.record("svc-a", "Ana", 1);
        log.record("svc-b", "Bia", 1);
        let s = log.finish(2).unwrap().without("svc-a").unwrap();
        assert_eq!((s.total, s.accounts.len()), (1, 1));
        assert!(s.without_chats().accounts[0].chats.is_empty());
    }
}
