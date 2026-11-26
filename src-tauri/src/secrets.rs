//! Senhas das contas IMAP. Ficam fora de `services.json`, do backup e da
//! sincronização: no Windows, no Gerenciador de Credenciais; no macOS e no
//! Linux, num arquivo só do usuário (permissão 0600), como os cookies das
//! contas e o `sync.json`. As Chaves do macOS pediriam a senha do Mac a
//! cada atualização do app (sem assinatura com ID de equipe da Apple).
//!
//! Cada senha fica presa ao servidor, porta e usuário para os quais foi
//! digitada: um backup ou arquivo editado que troque o servidor não faz o
//! Papeo enviá-la a outro lugar.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub secret: String,
}

/// Sem a senha: um `{:?}` esquecido num log não a mostra.
impl std::fmt::Debug for Saved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Saved")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("user", &self.user)
            .finish_non_exhaustive()
    }
}

impl Saved {
    /// A senha vale para esta conta (mesmo servidor, porta e usuário).
    pub fn matches(&self, account: &papeo_core::mail::MailAccount) -> bool {
        self.host == account.host && self.port == account.port && self.user == account.user
    }
}

pub struct SecretStore {
    #[cfg_attr(windows, allow(dead_code))]
    path: PathBuf,
    /// Uma gravação por vez no arquivo.
    #[cfg_attr(windows, allow(dead_code))]
    file: Mutex<()>,
}

/// Chamar no `setup`. `dir`: pasta local do app (fora do perfil sincronizado
/// do Windows).
pub fn init(app: &AppHandle, dir: &Path) {
    app.manage(SecretStore {
        path: dir.join("mail-secrets.json"),
        file: Mutex::new(()),
    });
}

pub fn read(app: &AppHandle, id: &str) -> Option<Saved> {
    let store = app.try_state::<SecretStore>()?;
    imp::read(&store, id)
}

pub fn write(app: &AppHandle, id: &str, saved: &Saved) -> Result<(), String> {
    let store = app
        .try_state::<SecretStore>()
        .ok_or("Cofre de senhas indisponível.")?;
    imp::write(&store, id, saved)
}

pub fn delete(app: &AppHandle, id: &str) {
    if let Some(store) = app.try_state::<SecretStore>() {
        imp::delete(&store, id);
    }
}

/// Grava um arquivo que só o usuário lê (0600 no macOS/Linux), trocando o
/// antigo de uma vez.
pub fn write_private(path: &Path, body: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("tmp-papeo");
    // Sobra antiga pode ter permissão mais aberta: começa do zero.
    let _ = std::fs::remove_file(&tmp);
    {
        use std::io::Write;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut file = opts.open(&tmp).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
        }
        file.write_all(body).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

#[cfg(not(windows))]
mod imp {
    use super::{write_private, Saved, SecretStore};
    use std::collections::BTreeMap;

    fn load(store: &SecretStore) -> BTreeMap<String, Saved> {
        std::fs::read(&store.path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn save(store: &SecretStore, all: &BTreeMap<String, Saved>) -> Result<(), String> {
        if all.is_empty() {
            return match std::fs::remove_file(&store.path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            };
        }
        let body = serde_json::to_vec(all).map_err(|e| e.to_string())?;
        write_private(&store.path, &body)
    }

    pub fn read(store: &SecretStore, id: &str) -> Option<Saved> {
        let _guard = store.file.lock().unwrap_or_else(|e| e.into_inner());
        load(store).remove(id)
    }

    pub fn write(store: &SecretStore, id: &str, saved: &Saved) -> Result<(), String> {
        let _guard = store.file.lock().unwrap_or_else(|e| e.into_inner());
        let mut all = load(store);
        all.insert(id.to_owned(), saved.clone());
        save(store, &all)
    }

    pub fn delete(store: &SecretStore, id: &str) {
        let _guard = store.file.lock().unwrap_or_else(|e| e.into_inner());
        let mut all = load(store);
        if all.remove(id).is_some() {
            let _ = save(store, &all);
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::{Saved, SecretStore};
    use windows::core::{HSTRING, PWSTR};
    use windows::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
        CRED_TYPE_GENERIC,
    };

    fn target(id: &str) -> HSTRING {
        HSTRING::from(format!("Papeo/mail/{id}"))
    }

    pub fn read(_store: &SecretStore, id: &str) -> Option<Saved> {
        let mut cred: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: `cred` recebe memória do sistema, liberada com CredFree.
        unsafe {
            CredReadW(&target(id), CRED_TYPE_GENERIC, None, &mut cred).ok()?;
            let (ptr, size) = ((*cred).CredentialBlob, (*cred).CredentialBlobSize);
            let blob = if ptr.is_null() || size == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(ptr, size as usize).to_vec()
            };
            CredFree(cred as *const _);
            serde_json::from_slice(&blob).ok()
        }
    }

    pub fn write(_store: &SecretStore, id: &str, saved: &Saved) -> Result<(), String> {
        let mut blob = serde_json::to_vec(saved).map_err(|e| e.to_string())?;
        let mut name: Vec<u16> = format!("Papeo/mail/{id}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut user: Vec<u16> = saved.user.encode_utf16().chain(Some(0)).collect();
        let cred = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(name.as_mut_ptr()),
            UserName: PWSTR(user.as_mut_ptr()),
            CredentialBlobSize: u32::try_from(blob.len()).map_err(|e| e.to_string())?,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..Default::default()
        };
        // SAFETY: os ponteiros apontam para buffers vivos até o fim da chamada.
        unsafe { CredWriteW(&cred, 0) }.map_err(|e| format!("Não deu para guardar a senha: {e}"))
    }

    pub fn delete(_store: &SecretStore, id: &str) {
        // SAFETY: só lê o nome passado.
        let _ = unsafe { CredDeleteW(&target(id), CRED_TYPE_GENERIC, None) };
    }
}
