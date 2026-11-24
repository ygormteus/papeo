//! Apaga os dados locais (cookies, cache, IndexedDB) de sessões que não são
//! mais usadas: serviço removido, "sair da conta" ou restos de backups.
//!
//! Windows/Linux guardam cada sessão em `sessions/<session>`; o macOS usa um
//! `WKWebsiteDataStore` identificado pelos 16 bytes da sessão. A webview
//! recém-fechada ainda pode segurar os arquivos, então a remoção fica numa
//! lista persistida (`pending_removal`) e é tentada de novo, inclusive na
//! próxima abertura do app.

use crate::state::AppState;
use std::collections::BTreeSet;
use std::time::Duration;
use tauri::{AppHandle, Manager};

const RETRIES: u32 = 5;

#[cfg(target_os = "macos")]
fn session_bytes(session: &str) -> [u8; 16] {
    let mut out = [0u8; 16];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&session[i * 2..i * 2 + 2], 16).unwrap_or(0);
    }
    out
}

/// Sessões usadas pelos serviços atuais (nunca apagadas).
fn in_use(app: &AppHandle) -> BTreeSet<String> {
    app.state::<AppState>()
        .lock()
        .config
        .services
        .iter()
        .map(|s| s.session.clone())
        .collect()
}

/// Tenta apagar os dados de uma sessão. `true` = não resta nada dela.
async fn erase(app: &AppHandle, session: &str) -> bool {
    // Só o formato gerado pelo app: vira nome de pasta.
    if !papeo_core::is_session_id(session) {
        return true;
    }
    let dir = app.state::<AppState>().sessions_dir.join(session);
    let dir_gone = match std::fs::remove_dir_all(&dir) {
        Ok(()) => true,
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    };
    #[cfg(target_os = "macos")]
    let store_gone = {
        let id = session_bytes(session);
        match app.fetch_data_store_identifiers().await {
            Ok(ids) if !ids.contains(&id) => true,
            Ok(_) => app.remove_data_store(id).await.is_ok(),
            Err(_) => false,
        }
    };
    #[cfg(not(target_os = "macos"))]
    let store_gone = true;
    dir_gone && store_gone
}

/// Põe sessões na fila de remoção (gravada em disco) e começa a apagar.
pub fn remove_later(app: &AppHandle, sessions: Vec<String>) {
    if sessions.is_empty() {
        return;
    }
    let state = app.state::<AppState>();
    let _ = state.update(|inner| {
        for s in sessions {
            if papeo_core::is_session_id(&s) && !inner.config.pending_removal.contains(&s) {
                inner.config.pending_removal.push(s);
            }
        }
        Ok(())
    });
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        retry_pending(&app).await;
    });
}

/// Apaga o que estiver na fila, com algumas novas tentativas espaçadas.
/// Retorna quantas sessões continuam pendentes.
pub async fn retry_pending(app: &AppHandle) -> usize {
    let state = app.state::<AppState>();
    for attempt in 0..RETRIES {
        let pending = state.lock().config.pending_removal.clone();
        let mut finished = Vec::new();
        for session in &pending {
            // Relido a cada sessão: um backup restaurado durante a limpeza pode
            // ter voltado a usá-la. Sessão em uso nunca é apagada.
            if in_use(app).contains(session) || erase(app, session).await {
                finished.push(session.clone());
            }
        }
        let left = state
            .update(|inner| {
                inner
                    .config
                    .pending_removal
                    .retain(|s| !finished.contains(s));
                Ok(inner.config.pending_removal.len())
            })
            .unwrap_or(usize::MAX);
        if left == 0 || attempt + 1 == RETRIES {
            return left;
        }
        let wait = Duration::from_secs(2 * u64::from(attempt + 1));
        let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(wait)).await;
    }
    0
}

/// Sessões no disco que nenhum serviço usa (ex.: restos de antes de
/// restaurar um backup).
pub async fn orphans(app: &AppHandle) -> Vec<String> {
    let used = in_use(app);
    let mut found = BTreeSet::new();
    let dir = app.state::<AppState>().sessions_dir.clone();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if papeo_core::is_session_id(&name) && entry.path().is_dir() && !used.contains(&name) {
                found.insert(name);
            }
        }
    }
    #[cfg(target_os = "macos")]
    if let Ok(ids) = app.fetch_data_store_identifiers().await {
        for id in ids {
            let session = papeo_core::session_from_bytes(&id);
            if !used.contains(&session) {
                found.insert(session);
            }
        }
    }
    found.into_iter().collect()
}
