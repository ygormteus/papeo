//! Endereços que cada conta de e-mail já usou (spec 25): para completar
//! Para, Cc e Cco no leitor. Vêm dos e-mails enviados, abertos e listados;
//! ficam só neste computador (fora do backup e da sincronização).

use crate::mailbox::Addr;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Endereços guardados por conta (os de menor uso saem primeiro).
pub const BOOK_MAX: usize = 3000;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownAddress {
    pub email: String,
    #[serde(default)]
    pub name: String,
    /// Vezes que a pessoa escreveu para este endereço.
    #[serde(default)]
    pub sent: u32,
    /// Vezes que o endereço apareceu num e-mail recebido.
    #[serde(default)]
    pub received: u32,
    /// Última vez (segundos desde 1970).
    #[serde(default)]
    pub last: i64,
}

impl KnownAddress {
    fn score(&self) -> u64 {
        u64::from(self.sent) * 20 + u64::from(self.received.min(50))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountBook {
    /// Já leu os enviados antigos (uma vez por conta).
    #[serde(default)]
    pub backfilled: bool,
    #[serde(default)]
    pub addresses: Vec<KnownAddress>,
}

/// Como o endereço apareceu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seen {
    Sent,
    Received,
}

/// Endereços de robôs: não servem para escrever.
fn automated(email: &str) -> bool {
    let local = email.split('@').next().unwrap_or_default();
    [
        "noreply",
        "no-reply",
        "no_reply",
        "donotreply",
        "do-not-reply",
        "do_not_reply",
        "mailer-daemon",
        "postmaster",
        "bounce",
        "naoresponda",
        "nao-responda",
        "nao_responda",
    ]
    .iter()
    .any(|robot| local.contains(robot))
}

fn valid(email: &str) -> bool {
    let mut parts = email.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && email.len() <= 254
        && !email
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || "<>,;\"".contains(c))
}

/// Nome que vale guardar: sem aspas, diferente do endereço.
fn clean_name(name: &str, email: &str) -> String {
    let name: String = name
        .trim()
        .trim_matches(['"', '\''])
        .chars()
        .filter(|c| !c.is_control())
        .take(120)
        .collect();
    let name = name.trim();
    if name.eq_ignore_ascii_case(email) || name.contains('@') {
        String::new()
    } else {
        name.to_owned()
    }
}

impl AccountBook {
    /// Registra um endereço; `own` é o da própria conta (não entra).
    /// Devolve se algo mudou.
    pub fn note(&mut self, addr: &Addr, how: Seen, when: i64, own: &str) -> bool {
        let email = addr.email.trim();
        if !valid(email)
            || email.eq_ignore_ascii_case(own.trim())
            || automated(&email.to_lowercase())
        {
            return false;
        }
        let name = clean_name(&addr.name, email);
        let entry = match self
            .addresses
            .iter_mut()
            .find(|a| a.email.eq_ignore_ascii_case(email))
        {
            Some(entry) => entry,
            None => {
                self.addresses.push(KnownAddress {
                    email: email.to_owned(),
                    ..Default::default()
                });
                self.addresses.last_mut().expect("acabou de entrar")
            }
        };
        // O nome de quem a pessoa escreveu vale mais que o do remetente.
        if !name.is_empty() && (entry.name.is_empty() || how == Seen::Sent) {
            entry.name = name;
        }
        match how {
            Seen::Sent => entry.sent = entry.sent.saturating_add(1),
            Seen::Received => entry.received = entry.received.saturating_add(1),
        }
        entry.last = entry.last.max(when);
        if self.addresses.len() > BOOK_MAX + BOOK_MAX / 10 {
            self.trim();
        }
        true
    }

    fn trim(&mut self) {
        self.addresses
            .sort_by(|a, b| b.score().cmp(&a.score()).then(b.last.cmp(&a.last)));
        self.addresses.truncate(BOOK_MAX);
    }

    /// Endereços que combinam com o que foi digitado: começo do endereço,
    /// do nome ou de uma palavra do nome; os mais usados primeiro.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Addr> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let mut found: Vec<(u8, &KnownAddress)> = self
            .addresses
            .iter()
            .filter_map(|a| {
                let email = a.email.to_lowercase();
                let name = a.name.to_lowercase();
                let rank = if email.starts_with(&q) || name.starts_with(&q) {
                    0
                } else if name.split_whitespace().any(|w| w.starts_with(&q))
                    || email.split(['.', '_', '-', '@']).any(|w| w.starts_with(&q))
                {
                    1
                } else if q.chars().count() >= 3 && (email.contains(&q) || name.contains(&q)) {
                    2
                } else {
                    return None;
                };
                Some((rank, a))
            })
            .collect();
        found.sort_by(|(ra, a), (rb, b)| {
            ra.cmp(rb)
                .then(b.score().cmp(&a.score()))
                .then(b.last.cmp(&a.last))
        });
        found
            .into_iter()
            .take(limit)
            .map(|(_, a)| Addr {
                name: a.name.clone(),
                email: a.email.clone(),
            })
            .collect()
    }
}

/// O arquivo inteiro: um caderno por conta (pelo `id` do serviço).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressBook {
    #[serde(default)]
    pub accounts: BTreeMap<String, AccountBook>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(name: &str, email: &str) -> Addr {
        Addr {
            name: name.into(),
            email: email.into(),
        }
    }

    #[test]
    fn keeps_people_and_skips_robots_and_self() {
        let mut book = AccountBook::default();
        assert!(book.note(
            &addr("Ana Souza", "ana@exemplo.com"),
            Seen::Received,
            1,
            "eu@exemplo.com"
        ));
        assert!(!book.note(&addr("", "eu@exemplo.com"), Seen::Sent, 1, "EU@exemplo.com"));
        assert!(!book.note(
            &addr("Banco", "no-reply@banco.com"),
            Seen::Received,
            1,
            "eu@exemplo.com"
        ));
        assert!(!book.note(&addr("", "sem-arroba"), Seen::Received, 1, "eu@exemplo.com"));
        assert!(!book.note(&addr("", "a@b@c.com"), Seen::Received, 1, "eu@exemplo.com"));
        assert_eq!(book.addresses.len(), 1);
        // Mesmo endereço com outra caixa: um só; o nome de quem recebeu
        // mensagem da pessoa vale mais.
        book.note(
            &addr("Ana S.", "ANA@exemplo.com"),
            Seen::Sent,
            5,
            "eu@exemplo.com",
        );
        assert_eq!(book.addresses.len(), 1);
        assert_eq!(book.addresses[0].name, "Ana S.");
        assert_eq!(
            (
                book.addresses[0].sent,
                book.addresses[0].received,
                book.addresses[0].last
            ),
            (1, 1, 5)
        );
        // Nome igual ao endereço não vale como nome.
        book.note(&addr("carlos@x.com", "carlos@x.com"), Seen::Received, 1, "");
        assert_eq!(book.addresses[1].name, "");
    }

    #[test]
    fn search_ranks_prefix_and_use() {
        let mut book = AccountBook::default();
        book.note(&addr("Mariana Lima", "mari@x.com"), Seen::Received, 1, "");
        book.note(&addr("Ana Maria", "ana@x.com"), Seen::Sent, 1, "");
        book.note(&addr("", "contato.marcos@y.com"), Seen::Received, 1, "");
        book.note(&addr("Pedro", "pedro@marte.com"), Seen::Received, 1, "");
        let emails = |q: &str| {
            book.search(q, 10)
                .into_iter()
                .map(|a| a.email)
                .collect::<Vec<_>>()
        };
        // Começo do nome/endereço antes de palavra do meio.
        assert_eq!(
            emails("mar"),
            [
                "mari@x.com",
                "ana@x.com",
                "contato.marcos@y.com",
                "pedro@marte.com"
            ]
        );
        assert_eq!(
            emails("ma"),
            [
                "mari@x.com",
                "ana@x.com",
                "contato.marcos@y.com",
                "pedro@marte.com"
            ]
        );
        assert!(emails("").is_empty());
        assert!(emails("zz").is_empty());
        // Duas letras no meio não bastam; três sim.
        assert!(emails("ri").is_empty());
        assert_eq!(emails("ria"), ["ana@x.com", "mari@x.com"]);
        assert_eq!(book.search("ma", 2).len(), 2);
    }

    #[test]
    fn trims_the_least_used() {
        let mut book = AccountBook::default();
        book.note(&addr("", "fica@x.com"), Seen::Sent, 1, "");
        for i in 0..(BOOK_MAX + BOOK_MAX / 10) {
            book.note(
                &addr("", &format!("p{i}@x.com")),
                Seen::Received,
                i as i64,
                "",
            );
        }
        assert!(book.addresses.len() <= BOOK_MAX);
        assert!(book.addresses.iter().any(|a| a.email == "fica@x.com"));
    }
}
