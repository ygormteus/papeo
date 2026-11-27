//! Lista do leitor guardada no disco (spec 27): um arquivo por conta em
//! `mail-cache/<id>.json`, só do usuário (0600), fora do backup. Lock
//! próprio, nunca junto do `AppState` nem de uma conexão de leitura.

use papeo_core::mail::MailAccount;
use papeo_core::mail_cache::MailCache;
use papeo_core::mailbox::{Folder, Page};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub struct MailCaches {
    dir: PathBuf,
    /// Um ler-mudar-gravar por vez.
    lock: Mutex<()>,
}

pub fn init(app: &AppHandle, dir: PathBuf) {
    app.manage(MailCaches {
        dir,
        lock: Mutex::new(()),
    });
}

/// Quem gravou: trocar usuário, servidor ou porta invalida o guardado.
fn owner(account: &MailAccount) -> String {
    format!("{}@{}:{}", account.user, account.host, account.port)
}

fn path(app: &AppHandle, id: &str) -> Option<PathBuf> {
    // O `id` vem só do formato gerado pelo app (`svc-` + hex); confere
    // mesmo assim, porque vira nome de arquivo.
    let ok = id.len() <= 40
        && id.strip_prefix("svc-").is_some_and(|rest| {
            !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_alphanumeric())
        });
    ok.then(|| app.state::<MailCaches>().dir.join(format!("{id}.json")))
}

fn update(app: &AppHandle, id: &str, account: &MailAccount, f: impl FnOnce(&mut MailCache)) {
    let Some(path) = path(app, id) else {
        return;
    };
    let caches = app.state::<MailCaches>();
    let _lock = caches.lock.lock().unwrap_or_else(|e| e.into_inner());
    let old = std::fs::read(&path).ok();
    let mut cache = MailCache::read(old.as_deref(), &owner(account));
    f(&mut cache);
    if let Ok(body) = serde_json::to_vec(&cache) {
        // Nada mudou (atualização sem e-mail novo): não grava nem força o
        // disco à toa.
        if old.as_deref() == Some(body.as_slice()) {
            return;
        }
        if let Err(err) = crate::secrets::write_private(&path, &body) {
            eprintln!("papeo: não deu para guardar a lista do e-mail: {err}");
        }
    }
}

pub fn put_folders(app: &AppHandle, id: &str, account: &MailAccount, folders: &[Folder]) {
    update(app, id, account, |c| c.set_folders(folders.to_vec()));
}

pub fn put_list(app: &AppHandle, id: &str, account: &MailAccount, folder: &str, page: &Page) {
    update(app, id, account, |c| c.set_list(folder, page));
}

/// O guardado da conta (vazio se não há ou é de outra).
pub fn get(app: &AppHandle, id: &str, account: &MailAccount) -> MailCache {
    let caches = app.state::<MailCaches>();
    let _lock = caches.lock.lock().unwrap_or_else(|e| e.into_inner());
    let bytes = path(app, id).and_then(|p| std::fs::read(p).ok());
    MailCache::read(bytes.as_deref(), &owner(account))
}

/// Conta removida ou sem o IMAP: o arquivo sai.
pub fn forget(app: &AppHandle, id: &str) {
    if let Some(path) = path(app, id) {
        let caches = app.state::<MailCaches>();
        let _lock = caches.lock.lock().unwrap_or_else(|e| e.into_inner());
        let _ = std::fs::remove_file(path);
    }
}
