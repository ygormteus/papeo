//! Lista do leitor de e-mail guardada no disco (spec 27): pastas e as
//! mensagens mais novas das pastas usadas há pouco, para o leitor abrir na
//! hora enquanto o servidor responde. Só um adiantamento: nada aqui vale
//! como verdade para abrir, mover ou excluir.

use crate::mailbox::{Folder, Page, Summary, PAGE_SIZE};
use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;
/// Pastas com mensagens guardadas, por conta.
pub const FOLDERS_MAX: usize = 8;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CachedList {
    pub path: String,
    pub total: u32,
    pub messages: Vec<Summary>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MailCache {
    pub version: u32,
    /// `usuário@servidor:porta` de quem gravou: outra conta não lê.
    pub account: String,
    pub folders: Vec<Folder>,
    /// A usada mais recentemente primeiro.
    pub lists: Vec<CachedList>,
}

impl MailCache {
    pub fn new(account: &str) -> Self {
        MailCache {
            version: VERSION,
            account: account.to_owned(),
            ..Default::default()
        }
    }

    /// Lê o arquivo de `account`; ilegível, de outra versão ou de outra
    /// conta vira vazio.
    pub fn read(bytes: Option<&[u8]>, account: &str) -> Self {
        bytes
            .and_then(|b| serde_json::from_slice::<MailCache>(b).ok())
            .filter(|c| c.version == VERSION && c.account == account)
            .unwrap_or_else(|| MailCache::new(account))
    }

    pub fn set_folders(&mut self, folders: Vec<Folder>) {
        // Pasta que sumiu do servidor sai também da lista guardada.
        self.lists
            .retain(|l| folders.iter().any(|f| f.path == l.path));
        self.folders = folders;
    }

    /// Guarda as mais novas de `path` (a primeira leva, sem busca).
    pub fn set_list(&mut self, path: &str, page: &Page) {
        self.lists.retain(|l| l.path != path);
        let mut messages = page.messages.clone();
        messages.truncate(PAGE_SIZE as usize);
        self.lists.insert(
            0,
            CachedList {
                path: path.to_owned(),
                total: page.total,
                messages,
            },
        );
        self.lists.truncate(FOLDERS_MAX);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(path: &str) -> Folder {
        Folder {
            path: path.into(),
            name: path.into(),
            depth: 0,
            role: None,
        }
    }

    fn page(n: u32) -> Page {
        Page {
            messages: (0..n)
                .map(|uid| Summary {
                    uid,
                    subject: format!("s{uid}"),
                    ..Default::default()
                })
                .collect(),
            total: n,
        }
    }

    #[test]
    fn keeps_recent_folders() {
        let mut cache = MailCache::new("a@x:993");
        for i in 0..10 {
            cache.set_list(&format!("F{i}"), &page(1));
        }
        cache.set_list("F5", &page(2));
        assert_eq!(cache.lists.len(), FOLDERS_MAX);
        assert_eq!(cache.lists[0].path, "F5");
        assert_eq!(cache.lists[0].messages.len(), 2);
        assert_eq!(cache.lists.iter().filter(|l| l.path == "F5").count(), 1);
        assert!(!cache.lists.iter().any(|l| l.path == "F0"));

        cache.set_folders(vec![folder("F5"), folder("F9")]);
        let paths: Vec<_> = cache.lists.iter().map(|l| l.path.as_str()).collect();
        assert_eq!(paths, ["F5", "F9"]);
    }

    #[test]
    fn trims_messages() {
        let mut cache = MailCache::new("a@x:993");
        cache.set_list("INBOX", &page(80));
        assert_eq!(cache.lists[0].messages.len(), PAGE_SIZE as usize);
        assert_eq!(cache.lists[0].total, 80);
    }

    #[test]
    fn other_account_is_ignored() {
        let mut cache = MailCache::new("a@x:993");
        cache.set_folders(vec![folder("INBOX")]);
        cache.set_list("INBOX", &page(3));
        let bytes = serde_json::to_vec(&cache).unwrap();

        assert_eq!(MailCache::read(Some(&bytes), "a@x:993"), cache);
        let other = MailCache::read(Some(&bytes), "b@x:993");
        assert!(other.folders.is_empty() && other.lists.is_empty());
        assert_eq!(other.account, "b@x:993");
        assert!(MailCache::read(Some(b"{lixo"), "a@x:993").lists.is_empty());
        let newer = br#"{"version":99,"account":"a@x:993","folders":[]}"#;
        assert_eq!(MailCache::read(Some(newer), "a@x:993").version, VERSION);
        assert!(MailCache::read(None, "a@x:993").folders.is_empty());
    }
}
