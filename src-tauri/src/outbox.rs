//! Envios de e-mail em espera, para dar tempo de desfazer (spec 16).
//!
//! Cada envio em espera fica na memória (agendado) e num arquivo só do
//! usuário. Quem tirar a entrada do mapa primeiro vence: a tarefa da espera
//! (envia) ou o "Desfazer". O arquivo só é apagado quando o envio dá certo
//! ou quando o e-mail já está no editor (`taken`): desfeito, falha ou sobra
//! de uma sessão anterior continuam no disco até lá. Sobra nunca é enviada
//! sozinha.
//!
//! Locks: o mapa da fila tem `Mutex` próprio e nunca é pego junto com o do
//! estado.

use crate::mailbox::{self, Draft};
use crate::state::{AppState, UI_WEBVIEW};
use papeo_core::i18n::{tr, trf};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

/// Resultado de um envio que saiu da espera.
pub const EVENT: &str = "papeo://mail-send";
/// Sobra de envio mais antiga que isto é apagada ao abrir o app.
const KEEP_SECS: u64 = 30 * 24 * 60 * 60;

/// Como fica no arquivo.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Held {
    account: String,
    /// Usuário, servidor e porta da conta ao pedir o envio: se mudarem na
    /// espera, o e-mail volta ao editor em vez de sair por outra conta.
    #[serde(default)]
    from: String,
    draft: Draft,
    created: u64,
    /// Marcado logo antes do SMTP: se sobrar, pode ter sido enviado.
    #[serde(default)]
    sending: bool,
    /// O envio falhou (o e-mail espera para voltar ao editor).
    #[serde(default)]
    error: Option<String>,
}

struct Waiting {
    account: String,
    from: String,
    draft: Draft,
}

#[derive(Default)]
struct Queue {
    /// Agendados, ainda dentro da espera.
    waiting: HashMap<String, Waiting>,
    /// Saíram da espera e estão sendo enviados agora.
    sending: HashSet<String>,
}

pub struct Outbox {
    dir: PathBuf,
    queue: Mutex<Queue>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hold {
    pub hold_id: String,
    pub secs: u8,
}

/// E-mail guardado que ainda não voltou ao editor.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Unsent {
    pub hold_id: String,
    pub draft: Draft,
    /// O app fechou durante o envio: pode ter saído.
    pub maybe_sent: bool,
    pub error: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Outcome {
    hold_id: String,
    id: String,
    /// Sem erro, enviado.
    error: Option<String>,
    /// Com erro, o e-mail volta ao editor.
    draft: Option<Draft>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(crate) fn new_id() -> String {
    let mut buf = [0u8; 12];
    getrandom::fill(&mut buf).expect("gerador aleatório do sistema");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// Id da fila: só o formato gerado aqui (vira nome de arquivo).
pub(crate) fn valid_id(id: &str) -> bool {
    id.len() == 24
        && id
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn path_of(dir: &Path, hold: &str) -> PathBuf {
    dir.join(format!("{hold}.json"))
}

fn write(dir: &Path, hold: &str, held: &Held) -> Result<(), String> {
    let body = serde_json::to_vec(held).map_err(|e| e.to_string())?;
    crate::secrets::write_private(&path_of(dir, hold), &body)
}

fn read(path: &Path) -> Option<Held> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Arquivos da fila: (id, caminho).
fn entries(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(list) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    list.flatten()
        .filter_map(|e| {
            let path = e.path();
            let stem = path.file_stem()?.to_str()?.to_owned();
            (path.extension()? == "json" && valid_id(&stem)).then_some((stem, path))
        })
        .collect()
}

/// Apaga sobras ilegíveis, antigas ou de contas que não existem mais (ou
/// que não usam mais o leitor, onde nunca voltariam ao editor).
fn prune(dir: &Path, alive: &HashSet<String>, now: u64) {
    for (_, path) in entries(dir) {
        let keep = read(&path).is_some_and(|h| {
            alive.contains(&h.account) && now.saturating_sub(h.created) < KEEP_SECS
        });
        if !keep {
            let _ = std::fs::remove_file(&path);
        }
    }
    // Gravação interrompida pelo meio (`write_private`).
    if let Ok(list) = std::fs::read_dir(dir) {
        for e in list.flatten() {
            if e.path().extension().is_some_and(|x| x == "tmp-papeo") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

pub fn init(app: &AppHandle, local_data: &Path) {
    let dir = local_data.join("outbox");
    let alive: HashSet<String> = app
        .state::<AppState>()
        .lock()
        .config
        .services
        .iter()
        .filter(|s| s.mail.is_some())
        .map(|s| s.id.clone())
        .collect();
    prune(&dir, &alive, now());
    app.manage(Outbox {
        dir,
        queue: Mutex::new(Queue::default()),
    });
}

fn outbox(app: &AppHandle) -> Result<tauri::State<'_, Outbox>, String> {
    app.try_state::<Outbox>()
        .ok_or_else(|| tr("Fila de envio indisponível.").to_owned())
}

/// Conta de envio como está agora (`None`: sem a conta ou sem o IMAP).
fn fingerprint(app: &AppHandle, id: &str) -> Option<String> {
    let inner = app.state::<AppState>();
    let inner = inner.lock();
    let mail = inner.service(id).ok()?.mail.as_ref()?;
    Some(format!("{}@{}:{}", mail.user, mail.host, mail.port))
}

/// Confere o e-mail, guarda e agenda o envio para o fim da espera.
pub fn hold(app: &AppHandle, id: String, draft: Draft) -> Result<Hold, String> {
    mailbox::validate(app, &id, &draft)?;
    let secs = app
        .state::<AppState>()
        .lock()
        .config
        .settings
        .mail_undo_secs;
    let from =
        fingerprint(app, &id).ok_or(tr("Esta conta não tem o e-mail por IMAP configurado."))?;
    let outbox = outbox(app)?;
    let hold_id = new_id();
    let held = Held {
        account: id.clone(),
        from: from.clone(),
        draft,
        created: now(),
        sending: false,
        error: None,
    };
    // Na fila antes do arquivo: `unsent` nunca vê o arquivo como sobra.
    outbox
        .queue
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .waiting
        .insert(
            hold_id.clone(),
            Waiting {
                account: id,
                from,
                draft: held.draft.clone(),
            },
        );
    if let Err(err) = write(&outbox.dir, &hold_id, &held) {
        outbox
            .queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .waiting
            .remove(&hold_id);
        return Err(err);
    }
    let app = app.clone();
    let fire_id = hold_id.clone();
    // Um thread por envio em espera (são poucos e curtos).
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(secs.into()));
        tauri::async_runtime::block_on(fire(app, fire_id));
    });
    Ok(Hold { hold_id, secs })
}

/// Fim da espera: envia, se ninguém desfez antes.
async fn fire(app: AppHandle, hold_id: String) {
    let Ok(outbox) = outbox(&app) else {
        return;
    };
    let taken = {
        let mut queue = outbox.queue.lock().unwrap_or_else(|e| e.into_inner());
        let taken = queue.waiting.remove(&hold_id);
        if taken.is_some() {
            queue.sending.insert(hold_id.clone());
        }
        taken
    };
    let Some(Waiting {
        account: id,
        from,
        draft,
    }) = taken
    else {
        return;
    };
    let dir = outbox.dir.clone();
    let held = |sending: bool, error: Option<String>| Held {
        account: id.clone(),
        from: from.clone(),
        draft: draft.clone(),
        created: now(),
        sending,
        error,
    };
    let result = if fingerprint(&app, &id).as_deref() != Some(from.as_str()) {
        Err(tr("A conta de e-mail mudou durante a espera. Confira antes de enviar.").to_owned())
    } else if let Err(err) = write(&dir, &hold_id, &held(true, None)) {
        // Sem a marca, um fechamento no meio do envio viraria "não enviado".
        Err(trf(
            "Não deu para preparar o envio: {erro}",
            &[("erro", &err)],
        ))
    } else {
        mailbox::send(app.clone(), id.clone(), draft.clone()).await
    };
    let outcome = match result {
        Ok(()) => {
            let _ = std::fs::remove_file(path_of(&dir, &hold_id));
            Outcome {
                hold_id: hold_id.clone(),
                id,
                error: None,
                draft: None,
            }
        }
        Err(error) => {
            // Fica no disco até voltar ao editor (`taken`).
            let _ = write(&dir, &hold_id, &held(false, Some(error.clone())));
            Outcome {
                hold_id: hold_id.clone(),
                id,
                error: Some(error),
                draft: Some(draft),
            }
        }
    };
    outbox
        .queue
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .sending
        .remove(&hold_id);
    let _ = app.emit_to(UI_WEBVIEW, EVENT, outcome);
}

/// Desfaz: devolve o e-mail se ele ainda estava na espera. O arquivo fica
/// até o e-mail chegar ao editor (`taken`).
pub fn cancel(app: &AppHandle, hold_id: &str) -> Result<Option<Draft>, String> {
    let outbox = outbox(app)?;
    let taken = outbox
        .queue
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .waiting
        .remove(hold_id);
    Ok(taken.map(|w| w.draft))
}

/// Ids na espera ou sendo enviados agora.
fn busy(outbox: &Outbox) -> HashSet<String> {
    let queue = outbox.queue.lock().unwrap_or_else(|e| e.into_inner());
    queue
        .waiting
        .keys()
        .chain(queue.sending.iter())
        .cloned()
        .collect()
}

/// E-mails guardados da conta que ainda não voltaram ao editor (desfeitos,
/// com falha ou de uma sessão anterior). Continuam no disco até `taken`.
pub fn unsent(app: &AppHandle, id: &str) -> Result<Vec<Unsent>, String> {
    let outbox = outbox(app)?;
    let busy = busy(&outbox);
    let mut found: Vec<(u64, Unsent)> = Vec::new();
    for (hold, path) in entries(&outbox.dir) {
        if busy.contains(&hold) {
            continue;
        }
        let Some(held) = read(&path) else {
            continue;
        };
        if held.account != id {
            continue;
        }
        found.push((
            held.created,
            Unsent {
                hold_id: hold,
                draft: held.draft,
                maybe_sent: held.sending,
                error: held.error,
            },
        ));
    }
    found.sort_by_key(|(created, _)| *created);
    Ok(found.into_iter().map(|(_, u)| u).collect())
}

/// O e-mail voltou ao editor: apaga o arquivo guardado.
pub fn taken(app: &AppHandle, hold_id: &str) -> Result<(), String> {
    let outbox = outbox(app)?;
    if valid_id(hold_id) && !busy(&outbox).contains(hold_id) {
        let _ = std::fs::remove_file(path_of(&outbox.dir, hold_id));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(subject: &str) -> Draft {
        serde_json::from_value(serde_json::json!({
            "to": ["a@x.com"],
            "subject": subject,
            "body": "oi",
            "attachments": [{"name": "a.txt", "mime": "text/plain", "data": "b2k="}],
        }))
        .unwrap()
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("papeo-outbox-{name}-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn ids_are_hex_only() {
        let id = new_id();
        assert!(valid_id(&id));
        assert!(!valid_id("../../etc/passwd"));
        assert!(!valid_id(&id.to_uppercase()));
        assert!(!valid_id(&id[..10]));
    }

    #[test]
    fn file_round_trip_keeps_the_draft() {
        let dir = temp_dir("rt");
        let id = new_id();
        let held = Held {
            account: "svc-1".into(),
            draft: draft("Olá"),
            created: 10,
            sending: true,
            error: Some("falhou".into()),
            from: "a@x.com@imap.x.com:993".into(),
        };
        write(&dir, &id, &held).unwrap();
        let back = read(&path_of(&dir, &id)).unwrap();
        assert_eq!(back.account, "svc-1");
        assert_eq!(back.draft.subject, "Olá");
        assert_eq!(back.draft.attachments[0].data, "b2k=");
        assert!(back.sending);
        assert_eq!(back.error.as_deref(), Some("falhou"));
        assert_eq!(back.from, "a@x.com@imap.x.com:993");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(path_of(&dir, &id))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_drops_old_orphan_and_broken_files() {
        let dir = temp_dir("prune");
        let now = 100 * 24 * 60 * 60;
        let put = |account: &str, created: u64| {
            let id = new_id();
            let held = Held {
                account: account.into(),
                draft: draft("x"),
                created,
                sending: false,
                error: None,
                from: String::new(),
            };
            write(&dir, &id, &held).unwrap();
            id
        };
        let fresh = put("svc-1", now - 60);
        let old = put("svc-1", now - KEEP_SECS - 1);
        let orphan = put("svc-gone", now - 60);
        let broken = new_id();
        std::fs::write(path_of(&dir, &broken), b"{").unwrap();
        std::fs::write(dir.join("x.tmp-papeo"), b"").unwrap();
        let alive: HashSet<String> = ["svc-1".to_owned()].into();
        prune(&dir, &alive, now);
        let left: HashSet<String> = entries(&dir).into_iter().map(|(id, _)| id).collect();
        assert!(left.contains(&fresh));
        assert!(!left.contains(&old));
        assert!(!left.contains(&orphan));
        assert!(!left.contains(&broken));
        assert!(!dir.join("x.tmp-papeo").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
