//! Chave mestra do WebCrypto no macOS, sem passar pelas Chaves (Keychain).
//!
//! O WebKit cifra as chaves criptográficas que os sites guardam (o WhatsApp
//! Web guarda as do login) com uma "chave mestra". Por padrão ela fica nas
//! Chaves do usuário, liberada só para o build exato do app que a criou: sem
//! assinatura com ID de equipe da Apple, cada versão nova do Papeo faz o macOS
//! pedir a senha das Chaves de novo.
//!
//! Aqui o Papeo fornece a chave ao WebKit pelo delegate privado do
//! `WKWebsiteDataStore` (`webCryptoMasterKey:`, macOS 15+), consultado antes
//! das Chaves. A chave fica em `webcrypto-master.key` na pasta de dados do app
//! (permissão 0600), com a mesma proteção dos dados das sessões.
//!
//! Migração: se o arquivo ainda não existe e há a chave antiga nas Chaves, ela
//! é lida uma única vez (o último pedido de senha), para os logins já salvos
//! continuarem valendo. Sem chave antiga, uma nova é gerada. Se a leitura for
//! negada, nada muda: o WebKit continua usando as Chaves, como antes.

use base64::Engine as _;
use block2::DynBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{define_class, msg_send, sel, AllocAnyThread};
use objc2_foundation::NSData;
use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::Path;
use std::sync::OnceLock;

const KEY_LEN: usize = 16;
const FILE_NAME: &str = "webcrypto-master.key";

static KEY: OnceLock<[u8; KEY_LEN]> = OnceLock::new();
/// Delegate único, criado uma vez e nunca liberado (o WebKit só guarda uma
/// referência fraca a ele).
static DELEGATE: OnceLock<usize> = OnceLock::new();

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "PapeoWebCryptoKeyDelegate"]
    struct KeyDelegate;

    impl KeyDelegate {
        #[unsafe(method(webCryptoMasterKey:))]
        fn web_crypto_master_key(&self, completion: &DynBlock<dyn Fn(*mut NSData)>) {
            match KEY.get() {
                Some(key) => {
                    let data = NSData::with_bytes(key);
                    completion.call((Retained::as_ptr(&data).cast_mut(),));
                }
                // Sem chave: o WebKit segue para o caminho padrão.
                None => completion.call((std::ptr::null_mut(),)),
            }
        }
    }
);

/// Conta das Chaves onde o WebKit guarda a chave deste app.
fn keychain_account(bundle_id: &str) -> String {
    format!("com.apple.WebKit.WebCrypto.master+{bundle_id}")
}

enum Legacy {
    Found([u8; KEY_LEN]),
    NotFound,
    /// Negada, cancelada ou ilegível: não arriscar perder os logins.
    Unavailable,
}

fn read_legacy(bundle_id: &str) -> Legacy {
    use security_framework::item::{ItemClass, ItemSearchOptions, Limit, SearchResult};
    let found = ItemSearchOptions::new()
        .class(ItemClass::generic_password())
        .account(&keychain_account(bundle_id))
        .load_data(true)
        .limit(Limit::Max(1))
        .search();
    match found {
        Ok(items) => {
            let data = items.into_iter().find_map(|item| match item {
                SearchResult::Data(data) => Some(data),
                _ => None,
            });
            let key = data
                .and_then(|b64| base64::engine::general_purpose::STANDARD.decode(b64).ok())
                .and_then(|bytes| <[u8; KEY_LEN]>::try_from(bytes).ok());
            key.map_or(Legacy::Unavailable, Legacy::Found)
        }
        // errSecItemNotFound
        Err(e) if e.code() == -25300 => Legacy::NotFound,
        Err(_) => Legacy::Unavailable,
    }
}

fn read_file(path: &Path) -> Option<[u8; KEY_LEN]> {
    let bytes = std::fs::read(path).ok()?;
    <[u8; KEY_LEN]>::try_from(bytes).ok()
}

/// Grava com permissão 0600, de forma atômica.
fn write_file(path: &Path, key: &[u8; KEY_LEN]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("key.tmp");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)?;
    file.write_all(key)?;
    file.sync_all()?;
    std::fs::rename(&tmp, path)
}

fn random_key() -> Option<[u8; KEY_LEN]> {
    let mut key = [0u8; KEY_LEN];
    security_framework::random::SecRandom::default()
        .copy_bytes(&mut key)
        .ok()?;
    Some(key)
}

/// Carrega (ou migra/gera) a chave. Roda uma vez, antes de criar webviews.
pub fn init(data_dir: &Path, bundle_id: &str) {
    let path = data_dir.join(FILE_NAME);
    let key = match read_file(&path) {
        Some(key) => Some(key),
        None => {
            let key = match read_legacy(bundle_id) {
                Legacy::Found(key) => Some(key),
                Legacy::NotFound => random_key(),
                Legacy::Unavailable => None,
            };
            // Só passa a usar a chave se ela ficou gravada; senão, da próxima
            // vez uma chave diferente invalidaria os logins.
            key.filter(|k| write_file(&path, k).is_ok())
        }
    };
    if let Some(key) = key {
        let _ = KEY.set(key);
    }
}

fn delegate() -> *mut AnyObject {
    *DELEGATE.get_or_init(|| {
        // SAFETY: `init` de NSObject numa classe sem ivars.
        let delegate: Retained<KeyDelegate> = unsafe { msg_send![KeyDelegate::alloc(), init] };
        Retained::into_raw(delegate) as usize
    }) as *mut AnyObject
}

/// Liga o delegate ao armazenamento de dados da webview do serviço. Sem chave
/// carregada (ou macOS sem suporte), não faz nada.
pub fn attach(webview: &tauri::Webview) {
    if KEY.get().is_none() {
        return;
    }
    let _ = webview.with_webview(|platform| {
        let ptr = platform.inner() as *mut AnyObject;
        // SAFETY: `inner()` é o WKWebView vivo, na thread principal; os
        // seletores existem no WebKit do macOS 14+ e `set_delegate:` é
        // conferido antes do uso.
        unsafe {
            let Some(webview) = ptr.as_ref() else {
                return;
            };
            let config: Retained<AnyObject> = msg_send![webview, configuration];
            let store: Retained<AnyObject> = msg_send![&*config, websiteDataStore];
            let supported: bool = msg_send![&*store, respondsToSelector: sel!(set_delegate:)];
            if supported {
                let _: () = msg_send![&*store, set_delegate: delegate()];
            }
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn account_matches_webkit() {
        assert_eq!(
            super::keychain_account("io.github.ygormteus.papeo"),
            "com.apple.WebKit.WebCrypto.master+io.github.ygormteus.papeo"
        );
    }
}
