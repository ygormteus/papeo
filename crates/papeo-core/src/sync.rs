//! Sincronização de tarefas e anotações entre computadores (pela pasta do
//! app no Google Drive): junção dos dados e criptografia com a frase-senha.
//!
//! Cada computador escreve só o próprio arquivo (estado completo) e lê os
//! dos outros; nada é sobrescrito por outro computador. A junção é registro
//! a registro: vale o `updatedAt` mais novo; anotação mudada nos dois lados
//! desde a última sincronização guarda as duas versões.
//!
//! Tudo o que vai para a nuvem é cifrado com XChaCha20-Poly1305; a chave sai
//! da frase-senha por Argon2id, com um sal comum à conta (`meta`).

use crate::organizer::{Contact, Organizer, Task};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Frase-senha mínima (caracteres).
pub const PASSPHRASE_MIN: usize = 10;
const KDF_MEMORY_KIB: u32 = 64 * 1024;
const KDF_PASSES: u32 = 3;
const KDF_LANES: u32 = 1;
const CHECK_TEXT: &[u8] = b"papeo-sync-v1";
/// Separador entre as duas versões de uma anotação em conflito.
pub const CONFLICT_MARK: &str = "——— versão de outro computador ———";
/// Registros apagados somem de vez em 90 dias. Arquivo de computador sem
/// gravar há mais que isto entra na junção uma última vez e sai da nuvem; e
/// este computador, parado há mais que isto, esquece o que os outros já
/// apagaram de vez (`forget_purged`).
pub const STALE_DEVICE_MS: u64 = 80 * 24 * 60 * 60 * 1000;
/// Arquivo mais velho que isto não entra na junção (traria de volta o que
/// foi apagado): só sai da nuvem.
pub const MERGE_DEVICE_MAX_MS: u64 = 88 * 24 * 60 * 60 * 1000;

/// Chave de 32 bytes derivada da frase-senha.
#[derive(Clone, PartialEq, Eq)]
pub struct SyncKey([u8; 32]);

impl std::fmt::Debug for SyncKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SyncKey(…)")
    }
}

impl SyncKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Arquivo `meta.json` da conta: sal e parâmetros comuns, mais uma prova
/// de que a frase-senha está certa (texto conhecido cifrado).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncMeta {
    pub version: u32,
    pub salt: String,
    pub memory_kib: u32,
    pub passes: u32,
    pub lanes: u32,
    pub check: Sealed,
}

/// Conteúdo cifrado (base64).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sealed {
    pub nonce: String,
    pub data: String,
}

/// O que cada computador grava (antes de cifrar).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceFile {
    pub version: u32,
    pub device: String,
    /// Nome para mostrar ("MacBook de …").
    #[serde(default)]
    pub device_name: String,
    /// Milissegundos Unix da gravação.
    pub written_at: u64,
    pub organizer: Organizer,
}

fn random<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    getrandom::fill(&mut out).expect("gerador aleatório do sistema");
    out
}

fn derive(
    passphrase: &str,
    salt: &[u8],
    memory: u32,
    passes: u32,
    lanes: u32,
) -> Result<SyncKey, String> {
    // Limites para não travar o app com parâmetros absurdos vindos da nuvem.
    if !(8 * 1024..=256 * 1024).contains(&memory)
        || !(1..=4).contains(&passes)
        || !(1..=4).contains(&lanes)
    {
        return Err("Parâmetros de criptografia inválidos.".into());
    }
    let params = Params::new(memory, passes, lanes, Some(32)).map_err(|e| e.to_string())?;
    let mut key = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| e.to_string())?;
    Ok(SyncKey(key))
}

/// Cifra com dados associados (`aad`, ex.: o nome do arquivo) para que um
/// arquivo não possa ser trocado por outro.
pub fn seal(key: &SyncKey, aad: &str, plain: &[u8]) -> Result<Sealed, String> {
    let nonce: [u8; 24] = random();
    let cipher = XChaCha20Poly1305::new(key.0.as_slice().into());
    let data = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plain,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| "Falha ao cifrar.".to_owned())?;
    Ok(Sealed {
        nonce: B64.encode(nonce),
        data: B64.encode(data),
    })
}

pub fn open(key: &SyncKey, aad: &str, sealed: &Sealed) -> Result<Vec<u8>, String> {
    let bad = || {
        "Não foi possível abrir os dados: frase-senha diferente ou arquivo danificado.".to_owned()
    };
    let nonce = B64.decode(&sealed.nonce).map_err(|_| bad())?;
    if nonce.len() != 24 {
        return Err(bad());
    }
    let data = B64.decode(&sealed.data).map_err(|_| bad())?;
    XChaCha20Poly1305::new(key.0.as_slice().into())
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &data,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| bad())
}

/// Primeira conexão da conta: sal novo e prova da frase-senha.
pub fn new_meta(passphrase: &str) -> Result<(SyncMeta, SyncKey), String> {
    if passphrase.chars().count() < PASSPHRASE_MIN {
        return Err(format!(
            "Use uma frase-senha com pelo menos {PASSPHRASE_MIN} caracteres."
        ));
    }
    let salt: [u8; 16] = random();
    let key = derive(passphrase, &salt, KDF_MEMORY_KIB, KDF_PASSES, KDF_LANES)?;
    let meta = SyncMeta {
        version: 1,
        salt: B64.encode(salt),
        memory_kib: KDF_MEMORY_KIB,
        passes: KDF_PASSES,
        lanes: KDF_LANES,
        check: seal(&key, "meta", CHECK_TEXT)?,
    };
    Ok((meta, key))
}

/// Outro computador: deriva a chave e confere a frase-senha.
pub fn unlock_meta(meta: &SyncMeta, passphrase: &str) -> Result<SyncKey, String> {
    if meta.version != 1 {
        return Err("Sincronização criada por uma versão mais nova do Papeo.".into());
    }
    let salt = B64
        .decode(&meta.salt)
        .map_err(|_| "Dados de sincronização inválidos.".to_owned())?;
    if !(16..=64).contains(&salt.len()) {
        return Err("Dados de sincronização inválidos.".into());
    }
    let key = derive(passphrase, &salt, meta.memory_kib, meta.passes, meta.lanes)?;
    match open(&key, "meta", &meta.check) {
        Ok(text) if text == CHECK_TEXT => Ok(key),
        _ => Err("Frase-senha incorreta.".into()),
    }
}

pub fn seal_device(key: &SyncKey, file: &DeviceFile) -> Result<Sealed, String> {
    let plain = serde_json::to_vec(file).map_err(|e| e.to_string())?;
    seal(key, &format!("device:{}", file.device), &plain)
}

/// Abre o arquivo de um computador e confere que é mesmo dele.
pub fn open_device(
    key: &SyncKey,
    device: &str,
    sealed: &Sealed,
    now_ms: u64,
) -> Result<DeviceFile, String> {
    let plain = open(key, &format!("device:{device}"), sealed)?;
    let mut file: DeviceFile = serde_json::from_slice(&plain)
        .map_err(|_| "Arquivo de outro computador inválido.".to_owned())?;
    if file.device != device {
        return Err("Arquivo de outro computador inválido.".into());
    }
    file.organizer = file.organizer.sanitized(now_ms);
    Ok(file)
}

/// Identificador deste computador (no nome do arquivo na nuvem).
pub fn new_device_id() -> String {
    crate::organizer::new_item_id()
}

pub fn is_device_id(id: &str) -> bool {
    crate::organizer::is_item_id(id)
}

/// Mais novo vence; empate: apagado vence, depois o maior conteúdo (para
/// todos os computadores chegarem ao mesmo resultado).
fn newer<T: Serialize>(a: &T, a_at: u64, a_del: bool, b: &T, b_at: u64, b_del: bool) -> bool {
    if a_at != b_at {
        return a_at > b_at;
    }
    if a_del != b_del {
        return a_del;
    }
    serde_json::to_string(a).unwrap_or_default() >= serde_json::to_string(b).unwrap_or_default()
}

/// Junta o estado local com o de outro computador.
///
/// `base`: o que este computador tinha na última sincronização; serve para
/// perceber anotação editada nos dois lados (as duas versões ficam).
pub fn merge(base: &Organizer, local: &Organizer, remote: &Organizer, now_ms: u64) -> Organizer {
    let mut tasks: HashMap<&str, &Task> = HashMap::new();
    for t in local.tasks.iter().chain(&remote.tasks) {
        match tasks.get(t.id.as_str()) {
            Some(cur)
                if newer(
                    *cur,
                    cur.updated_at,
                    cur.deleted,
                    t,
                    t.updated_at,
                    t.deleted,
                ) => {}
            _ => {
                tasks.insert(&t.id, t);
            }
        }
    }
    let base_notes: HashMap<&str, &str> = base
        .contacts
        .iter()
        .map(|c| (c.id.as_str(), c.note.as_str()))
        .collect();
    let remote_by_id: HashMap<&str, &Contact> =
        remote.contacts.iter().map(|c| (c.id.as_str(), c)).collect();
    let local_by_id: HashMap<&str, &Contact> =
        local.contacts.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut contacts: Vec<Contact> = Vec::new();
    let mut ids: Vec<&str> = local
        .contacts
        .iter()
        .chain(&remote.contacts)
        .map(|c| c.id.as_str())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    for id in ids {
        let merged = match (local_by_id.get(id), remote_by_id.get(id)) {
            (Some(l), Some(r)) => {
                let base = base_notes.get(id).copied();
                let both_changed = !l.deleted
                    && !r.deleted
                    && l.note != r.note
                    && base.is_some_and(|b| b != l.note && b != r.note);
                let mut win = if newer(*l, l.updated_at, l.deleted, *r, r.updated_at, r.deleted) {
                    (*l).clone()
                } else {
                    (*r).clone()
                };
                if both_changed {
                    let (mine, other) = if win.note == l.note {
                        (&l.note, &r.note)
                    } else {
                        (&r.note, &l.note)
                    };
                    // Junção já feita (com a marca) que contém a outra versão:
                    // fica ela, sem repetir texto. Anotação apagada de um
                    // lado: fica o texto do outro. O resto guarda as duas.
                    let joined = |a: &str, b: &str| a.contains(CONFLICT_MARK) && a.contains(b);
                    win.note = if joined(mine, other) || other.is_empty() {
                        mine.clone()
                    } else if joined(other, mine) || mine.is_empty() {
                        other.clone()
                    } else {
                        format!("{mine}\n\n{CONFLICT_MARK}\n{other}")
                    };
                    win.updated_at = win.updated_at.max(l.updated_at).max(r.updated_at) + 1;
                }
                win
            }
            (Some(c), None) | (None, Some(c)) => (*c).clone(),
            (None, None) => continue,
        };
        contacts.push(merged);
    }
    let mut tasks: Vec<Task> = tasks.into_values().cloned().collect();
    tasks.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    Organizer { tasks, contacts }.sanitized(now_ms)
}

/// Este computador ficou mais tempo que `STALE_DEVICE_MS` sem sincronizar:
/// o que outro computador apagou pode já ter sumido de vez (sem registro de
/// apagado). Tira daqui o que estava na última sincronização, não mudou
/// desde então e não existe em nenhum outro computador.
pub fn forget_purged(local: &mut Organizer, base: &Organizer, remotes: &[Organizer]) {
    if remotes.is_empty() {
        return;
    }
    let seen_task = |id: &str| remotes.iter().any(|r| r.tasks.iter().any(|t| t.id == id));
    let seen_contact = |id: &str| {
        remotes
            .iter()
            .any(|r| r.contacts.iter().any(|c| c.id == id))
    };
    local.tasks.retain(|t| {
        let unchanged = base.tasks.iter().any(|b| b == t);
        !(unchanged && !seen_task(&t.id))
    });
    local.contacts.retain(|c| {
        let unchanged = base.contacts.iter().any(|b| b == c);
        !(unchanged && !seen_contact(&c.id))
    });
}

/// Dois computadores podem criar o contato da mesma conversa com ids
/// diferentes. Junta os iguais (mesma conta e conversa) no de menor id, para
/// todos chegarem ao mesmo resultado: anotações diferentes ficam as duas,
/// as tarefas passam para o que fica e os outros viram apagados.
pub fn dedupe_contacts(org: &mut Organizer, now_ms: u64) {
    let mut keep: HashMap<(String, String), usize> = HashMap::new();
    let mut moved: HashMap<String, String> = HashMap::new();
    let mut order: Vec<usize> = (0..org.contacts.len())
        .filter(|&i| !org.contacts[i].deleted)
        .collect();
    order.sort_by(|&a, &b| org.contacts[a].id.cmp(&org.contacts[b].id));
    for i in order {
        let k = (
            org.contacts[i].service_id.clone(),
            org.contacts[i].chat.clone(),
        );
        match keep.get(&k) {
            None => {
                keep.insert(k, i);
            }
            Some(&main) => {
                let extra = std::mem::take(&mut org.contacts[i].note);
                let dup_id = org.contacts[i].id.clone();
                org.contacts[i].deleted = true;
                org.contacts[i].updated_at = now_ms;
                let target = &mut org.contacts[main];
                if !extra.is_empty() && extra != target.note {
                    target.note = if target.note.is_empty() {
                        extra
                    } else {
                        format!("{}\n\n{CONFLICT_MARK}\n{extra}", target.note)
                    };
                    target.updated_at = now_ms;
                }
                moved.insert(dup_id, target.id.clone());
            }
        }
    }
    if moved.is_empty() {
        return;
    }
    for t in org.tasks.iter_mut().filter(|t| !t.deleted) {
        if let Some(to) = t.contact.as_ref().and_then(|c| moved.get(c)) {
            t.contact = Some(to.clone());
            t.updated_at = now_ms;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organizer::{ContactRef, TaskInput};

    const NOW: u64 = 1_800_000_000_000;

    fn chat(name: &str) -> ContactRef {
        ContactRef {
            service_id: "svc-0123456789abcdef".into(),
            service_kind: "whatsapp".into(),
            service_name: "Pessoal".into(),
            chat: name.into(),
        }
    }

    fn text(t: &str) -> TaskInput {
        TaskInput {
            text: t.into(),
            ..Default::default()
        }
    }

    #[test]
    fn newest_edit_wins_and_deletes_travel() {
        let mut a = Organizer::default();
        let id = a.add_task(&text("ligar"), NOW).unwrap();
        let keep = a.add_task(&text("pagar"), NOW).unwrap();
        let mut b = a.clone();
        a.set_done(&id, true, NOW + 10).unwrap();
        b.edit_task(&id, &text("ligar pra Ana"), NOW + 5).unwrap();
        b.delete_task(&keep, NOW + 20).unwrap();
        let m = merge(&Organizer::default(), &a, &b, NOW + 30);
        assert!(m.task(&id).unwrap().done);
        assert_eq!(m.task(&id).unwrap().text, "ligar");
        assert!(m.task(&keep).is_none());
        // Mesmo resultado nos dois sentidos.
        assert_eq!(m, merge(&Organizer::default(), &b, &a, NOW + 30));
    }

    #[test]
    fn note_edited_on_both_sides_keeps_both() {
        let mut base = Organizer::default();
        base.set_note(&chat("Ana"), "cliente", NOW).unwrap();
        let mut a = base.clone();
        let mut b = base.clone();
        a.set_note(&chat("Ana"), "cliente vip", NOW + 1).unwrap();
        b.set_note(&chat("Ana"), "cliente desde 2020", NOW + 2)
            .unwrap();
        let m = merge(&base, &a, &b, NOW + 3);
        let note = &m.contacts[0].note;
        assert!(
            note.contains("cliente vip") && note.contains("cliente desde 2020"),
            "{note}"
        );
        assert!(note.contains(CONFLICT_MARK));
        // Só um lado mudou: sem conflito.
        let m = merge(&base, &base, &b, NOW + 3);
        assert_eq!(m.contacts[0].note, "cliente desde 2020");
    }

    #[test]
    fn passphrase_roundtrip() {
        let (meta, key) = new_meta("uma frase bem longa").unwrap();
        assert!(new_meta("curta").is_err());
        assert_eq!(unlock_meta(&meta, "uma frase bem longa").unwrap(), key);
        assert!(unlock_meta(&meta, "outra frase qualquer").is_err());

        let mut org = Organizer::default();
        org.add_task(&text("segredo"), NOW).unwrap();
        let file = DeviceFile {
            version: 1,
            device: new_device_id(),
            device_name: "Mac".into(),
            written_at: NOW,
            organizer: org.clone(),
        };
        let sealed = seal_device(&key, &file).unwrap();
        assert!(!sealed.data.contains("segredo"));
        assert_eq!(
            open_device(&key, &file.device, &sealed, NOW)
                .unwrap()
                .organizer,
            org
        );
        // Arquivo de um computador não passa por outro.
        assert!(open_device(&key, &new_device_id(), &sealed, NOW).is_err());
        let wrong = SyncKey::from_bytes([7; 32]);
        assert!(open_device(&wrong, &file.device, &sealed, NOW).is_err());
    }

    #[test]
    fn same_chat_from_two_computers_becomes_one_contact() {
        let mut a = Organizer::default();
        let mut b = Organizer::default();
        let ta = a
            .add_task(
                &TaskInput {
                    contact: Some(chat("Ana")),
                    ..text("ligar")
                },
                NOW,
            )
            .unwrap();
        a.set_note(&chat("Ana"), "nota A", NOW).unwrap();
        b.set_note(&chat("Ana"), "nota B", NOW).unwrap();
        let mut m = merge(&Organizer::default(), &a, &b, NOW + 1);
        assert_eq!(m.contacts.iter().filter(|c| !c.deleted).count(), 2);
        dedupe_contacts(&mut m, NOW + 2);
        let live: Vec<_> = m.contacts.iter().filter(|c| !c.deleted).collect();
        assert_eq!(live.len(), 1);
        assert!(live[0].note.contains("nota A") && live[0].note.contains("nota B"));
        assert_eq!(
            m.task(&ta).unwrap().contact.as_deref(),
            Some(live[0].id.as_str())
        );
        // De novo: nada muda.
        let again = m.clone();
        dedupe_contacts(&mut m, NOW + 3);
        assert_eq!(m, again);
    }

    #[test]
    fn later_conflicts_still_keep_both_versions() {
        let mut base = Organizer::default();
        base.set_note(&chat("Ana"), "cliente", NOW).unwrap();
        let mut a = base.clone();
        let mut b = base.clone();
        a.set_note(&chat("Ana"), "cliente vip", NOW + 1).unwrap();
        b.set_note(&chat("Ana"), "cliente novo", NOW + 2).unwrap();
        let first = merge(&base, &a, &b, NOW + 3);
        // Os dois lados chegam à mesma junção (com a marca).
        assert_eq!(first, merge(&base, &b, &a, NOW + 3));
        // Junção de novo com um lado ainda antigo: sem repetir texto.
        let again = merge(&base, &first, &b, NOW + 4);
        assert_eq!(again.contacts[0].note, first.contacts[0].note);
        // Uma versão cortou parte da outra: as duas ficam (sem perder edição).
        let mut a3 = base.clone();
        let mut b3 = base.clone();
        a3.set_note(&chat("Ana"), "cliente, ligar seg e ter", NOW + 1)
            .unwrap();
        b3.set_note(&chat("Ana"), "cliente, ligar seg", NOW + 2)
            .unwrap();
        let m3 = merge(&base, &a3, &b3, NOW + 3);
        assert!(m3.contacts[0].note.contains(CONFLICT_MARK));
        // Novo conflito depois da marca: as duas edições ficam.
        let base2 = first.clone();
        let mut a2 = base2.clone();
        let mut b2 = base2.clone();
        let note = base2.contacts[0].note.clone();
        a2.set_note(&chat("Ana"), &format!("{note}\nligou"), NOW + 10)
            .unwrap();
        b2.set_note(&chat("Ana"), "resumo novo", NOW + 11).unwrap();
        let m = merge(&base2, &a2, &b2, NOW + 12);
        let n = &m.contacts[0].note;
        assert!(n.contains("ligou") && n.contains("resumo novo"), "{n}");
    }

    #[test]
    fn long_offline_computer_drops_what_others_purged() {
        let mut base = Organizer::default();
        let gone = base.add_task(&text("apagada lá"), NOW).unwrap();
        let kept = base.add_task(&text("continua"), NOW).unwrap();
        let mut local = base.clone();
        let edited = local.add_task(&text("nova aqui"), NOW + 5).unwrap();
        let mut remote = base.clone();
        remote.tasks.retain(|t| t.id != gone);
        forget_purged(&mut local, &base, std::slice::from_ref(&remote));
        assert!(local.task(&gone).is_none());
        assert!(local.task(&kept).is_some());
        assert!(local.task(&edited).is_some());
        // Sem outro computador: nada muda.
        let mut alone = base.clone();
        forget_purged(&mut alone, &base, &[]);
        assert_eq!(alone, base);
    }

    #[test]
    fn rejects_absurd_kdf_params() {
        let (mut meta, _) = new_meta("uma frase bem longa").unwrap();
        meta.memory_kib = u32::MAX;
        assert!(unlock_meta(&meta, "uma frase bem longa").is_err());
    }
}
