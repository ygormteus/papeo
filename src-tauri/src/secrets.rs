//! Senhas das contas IMAP. Ficam fora de `services.json`, do backup e da
//! sincronização: no Windows, no Gerenciador de Credenciais; no macOS e no
//! Linux, num arquivo só do usuário (permissão 0600), como os cookies das
//! contas e o `sync.json`. As Chaves do macOS pediriam a senha do Mac a
//! cada atualização do app (sem assinatura com ID de equipe da Apple).
//!
//! Cada senha fica presa ao servidor, porta e usuário para os quais foi
//! digitada: um backup ou arquivo editado que troque o servidor não faz o
//! Papeo enviá-la a outro lugar.

use papeo_core::i18n::tr;
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
    /// Uma leitura ou gravação por vez (arquivo ou cofre do sistema).
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
    let _guard = store.file.lock().unwrap_or_else(|e| e.into_inner());
    imp::load_one(&store, id)
}

pub fn write(app: &AppHandle, id: &str, saved: &Saved) -> Result<(), String> {
    let store = app
        .try_state::<SecretStore>()
        .ok_or(tr("Cofre de senhas indisponível."))?;
    let _guard = store.file.lock().unwrap_or_else(|e| e.into_inner());
    imp::store_one(&store, id, saved)
}

/// Troca o segredo só se o guardado ainda é `previous` (token de renovação
/// que a Microsoft trocou): um vigia antigo não apaga um login mais novo.
/// `false` se o guardado já era outro.
pub fn replace(app: &AppHandle, id: &str, previous: &str, saved: &Saved) -> Result<bool, String> {
    let store = app
        .try_state::<SecretStore>()
        .ok_or(tr("Cofre de senhas indisponível."))?;
    let _guard = store.file.lock().unwrap_or_else(|e| e.into_inner());
    if imp::load_one(&store, id).is_none_or(|s| s.secret != previous) {
        return Ok(false);
    }
    imp::store_one(&store, id, saved).map(|()| true)
}

pub fn delete(app: &AppHandle, id: &str) {
    if let Some(store) = app.try_state::<SecretStore>() {
        let _guard = store.file.lock().unwrap_or_else(|e| e.into_inner());
        imp::remove_one(&store, id);
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

/// Resumo (SHA-256, 16 dígitos hex) para conferir as partes no Windows.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Lê o cabeçalho `<resumo>:<tamanho>\n` da parte principal.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn header(bytes: &[u8]) -> Option<(String, usize)> {
    let end = bytes.iter().position(|&b| b == b'\n')?;
    let text = std::str::from_utf8(&bytes[..end]).ok()?;
    let (sum, len) = text.split_once(':')?;
    (sum.len() == 16 && sum.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| Some((sum.to_owned(), len.parse().ok()?)))?
}

// Cada `imp` supõe o `SecretStore::file` seguro por quem chama.

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

    pub fn load_one(store: &SecretStore, id: &str) -> Option<Saved> {
        load(store).remove(id)
    }

    pub fn store_one(store: &SecretStore, id: &str, saved: &Saved) -> Result<(), String> {
        let mut all = load(store);
        all.insert(id.to_owned(), saved.clone());
        save(store, &all)
    }

    pub fn remove_one(store: &SecretStore, id: &str) {
        let mut all = load(store);
        if all.remove(id).is_some() {
            let _ = save(store, &all);
        }
    }
}

/// Windows: Gerenciador de Credenciais. Cada credencial guarda até 2560
/// bytes; o token de renovação da Microsoft pode passar disso, então o
/// conteúdo vai em partes: `Papeo/mail/<id>`, `Papeo/mail/<id>#1`, `#2`…
#[cfg(windows)]
mod imp {
    use super::{Saved, SecretStore};
    use papeo_core::i18n::{tr, trf};
    use windows::core::{HSTRING, PWSTR};
    use windows::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
        CRED_TYPE_GENERIC,
    };

    /// Bytes por parte (abaixo do limite de 2560 do sistema).
    const PART: usize = 2048;
    /// Partes no máximo (16 KB).
    const PARTS_MAX: usize = 8;

    fn name(id: &str, part: usize) -> String {
        if part == 0 {
            format!("Papeo/mail/{id}")
        } else {
            format!("Papeo/mail/{id}#{part}")
        }
    }

    fn read_part(target: &str) -> Option<Vec<u8>> {
        let mut cred: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: `cred` recebe memória do sistema, liberada com CredFree
        // depois de copiar o conteúdo.
        unsafe {
            CredReadW(&HSTRING::from(target), CRED_TYPE_GENERIC, None, &mut cred).ok()?;
            let (ptr, size) = ((*cred).CredentialBlob, (*cred).CredentialBlobSize);
            let blob = if ptr.is_null() || size == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(ptr, size as usize).to_vec()
            };
            CredFree(cred as *const _);
            Some(blob)
        }
    }

    fn write_part(target: &str, user: &str, bytes: &[u8]) -> Result<(), String> {
        let mut blob = bytes.to_vec();
        let mut target: Vec<u16> = target.encode_utf16().chain(Some(0)).collect();
        let mut user: Vec<u16> = user.encode_utf16().chain(Some(0)).collect();
        let cred = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(target.as_mut_ptr()),
            UserName: PWSTR(user.as_mut_ptr()),
            CredentialBlobSize: u32::try_from(blob.len()).map_err(|e| e.to_string())?,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..Default::default()
        };
        // SAFETY: os ponteiros apontam para buffers vivos até o fim da chamada.
        unsafe { CredWriteW(&cred, 0) }
            .map_err(|e| trf("Não deu para guardar a senha: {erro}", &[("erro", &e)]))
    }

    fn delete_part(target: &str) -> bool {
        // SAFETY: só lê o nome passado.
        unsafe { CredDeleteW(&HSTRING::from(target), CRED_TYPE_GENERIC, None) }.is_ok()
    }

    pub fn load_one(_store: &SecretStore, id: &str) -> Option<Saved> {
        let mut bytes = read_part(&name(id, 0))?;
        // Formato antigo (uma parte só, JSON puro).
        if bytes.first() == Some(&b'{') {
            return serde_json::from_slice(&bytes).ok();
        }
        let (want, len) = super::header(&bytes)?;
        let start = bytes.iter().position(|&b| b == b'\n')? + 1;
        bytes.drain(..start);
        // Só as partes que o cabeçalho pede: sobras antigas não entram.
        for part in 1..PARTS_MAX {
            if bytes.len() >= len {
                break;
            }
            bytes.extend(read_part(&name(id, part))?);
        }
        // Partes de gravações diferentes (gravação interrompida): descarta.
        if bytes.len() != len || super::digest(&bytes) != want {
            return None;
        }
        serde_json::from_slice(&bytes).ok()
    }

    pub fn store_one(_store: &SecretStore, id: &str, saved: &Saved) -> Result<(), String> {
        let body = serde_json::to_vec(saved).map_err(|e| e.to_string())?;
        // Cabeçalho na parte principal: resumo e tamanho do conteúdo.
        let mut bytes = format!("{}:{}\n", super::digest(&body), body.len()).into_bytes();
        bytes.extend_from_slice(&body);
        let parts: Vec<&[u8]> = bytes.chunks(PART).collect();
        if parts.len() > PARTS_MAX {
            return Err(tr("Senha ou token grande demais para guardar.").into());
        }
        // Partes extras primeiro, a principal por último: até ela ser
        // trocada, o resumo antigo recusa a mistura (e pede login de novo).
        for (part, chunk) in parts.iter().enumerate().skip(1) {
            write_part(&name(id, part), &saved.user, chunk)?;
        }
        write_part(
            &name(id, 0),
            &saved.user,
            parts.first().copied().unwrap_or(&[]),
        )?;
        // Sobras de um conteúdo maior gravado antes (ignoradas na leitura).
        for part in parts.len()..PARTS_MAX {
            delete_part(&name(id, part));
        }
        Ok(())
    }

    pub fn remove_one(_store: &SecretStore, id: &str) {
        for part in 0..PARTS_MAX {
            delete_part(&name(id, part));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_header() {
        let body = br#"{"host":"h"}"#;
        let head = format!("{}:{}\nresto", digest(body), body.len());
        assert_eq!(header(head.as_bytes()), Some((digest(body), body.len())));
        assert_eq!(header(b"zz:3\n"), None);
        assert_eq!(header(b"semfim"), None);
        assert_ne!(digest(b"a"), digest(b"b"));
    }
}
