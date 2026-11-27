//! Endereços conhecidos de cada conta de e-mail (spec 25), em
//! `mail-contacts.json` na pasta de dados: só neste computador, fora do
//! backup. Lock próprio (nunca junto do `AppState`); grava a cada 15 s se
//! algo mudou.

use papeo_core::address_book::{AddressBook, Seen};
use papeo_core::mailbox::Addr;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};

pub struct MailContacts {
    path: PathBuf,
    inner: Mutex<Inner>,
    /// Uma gravação por vez (o arquivo temporário é um só).
    saving: Mutex<()>,
}

#[derive(Default)]
struct Inner {
    book: Option<AddressBook>,
    dirty: bool,
    /// Contas lendo os enviados antigos agora.
    backfilling: Vec<String>,
}

pub fn init(app: &AppHandle, path: PathBuf) {
    app.manage(MailContacts {
        path,
        inner: Mutex::new(Inner::default()),
        saving: Mutex::new(()),
    });
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(15));
        save(&app);
    });
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn with_book<T>(app: &AppHandle, f: impl FnOnce(&mut AddressBook, &mut Inner) -> T) -> T {
    let contacts = app.state::<MailContacts>();
    let mut inner = contacts.inner.lock().unwrap_or_else(|e| e.into_inner());
    let mut book = inner.book.take().unwrap_or_else(|| {
        std::fs::read(&contacts.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    });
    let result = f(&mut book, &mut inner);
    inner.book = Some(book);
    result
}

/// Grava, se algo mudou desde a última vez.
pub fn save(app: &AppHandle) {
    let contacts = app.state::<MailContacts>();
    let _saving = contacts.saving.lock().unwrap_or_else(|e| e.into_inner());
    let body = with_book(app, |book, inner| {
        if !inner.dirty {
            return None;
        }
        inner.dirty = false;
        serde_json::to_vec(book).ok()
    });
    if let Some(body) = body {
        if let Err(err) = papeo_core::write_atomic(&contacts.path, &body) {
            eprintln!("papeo: não deu para gravar os contatos do e-mail: {err}");
        }
    }
}

/// Registra endereços vistos na conta `id` (`own`: o endereço da conta).
pub fn note<'a>(
    app: &AppHandle,
    id: &str,
    own: &str,
    seen: impl IntoIterator<Item = (&'a Addr, Seen)>,
) {
    let when = now();
    with_book(app, |book, inner| {
        let account = book.accounts.entry(id.to_owned()).or_default();
        for (addr, how) in seen {
            if account.note(addr, how, when, own) {
                inner.dirty = true;
            }
        }
    });
}

/// Endereços que combinam com `query`. `true` no segundo valor: falta ler
/// os enviados antigos desta conta (e ninguém está lendo).
pub fn search(app: &AppHandle, id: &str, query: &str) -> (Vec<Addr>, bool) {
    with_book(app, |book, inner| {
        let account = book.accounts.get(id);
        let found = account.map(|a| a.search(query, 8)).unwrap_or_default();
        let backfill =
            !account.is_some_and(|a| a.backfilled) && !inner.backfilling.iter().any(|b| b == id);
        if backfill {
            inner.backfilling.push(id.to_owned());
        }
        (found, backfill)
    })
}

/// Fim da leitura dos enviados antigos (`ok`: não precisa repetir).
pub fn backfilled(app: &AppHandle, id: &str, ok: bool) {
    with_book(app, |book, inner| {
        inner.backfilling.retain(|b| b != id);
        if ok {
            book.accounts.entry(id.to_owned()).or_default().backfilled = true;
            inner.dirty = true;
        }
    });
}

/// Conta removida: os endereços dela saem.
pub fn forget(app: &AppHandle, id: &str) {
    with_book(app, |book, inner| {
        if book.accounts.remove(id).is_some() {
            inner.dirty = true;
        }
    });
    save(app);
}
