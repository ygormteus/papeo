//! Tarefas, lembretes e anotações por contato (`organizer.json`).
//!
//! Tudo já nasce pronto para sincronizar entre computadores: cada registro
//! tem um `id` aleatório, `updatedAt` (milissegundos Unix) e `deleted`
//! (apagar marca em vez de sumir, para que a remoção também viaje).

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

pub const ORGANIZER_VERSION: u32 = 1;
pub const TASK_TEXT_MAX: usize = 500;
pub const NOTE_MAX: usize = 20_000;
pub const CHAT_MAX: usize = 120;
pub const MAX_TASKS: usize = 5_000;
pub const MAX_CONTACTS: usize = 2_000;
/// Registros apagados somem de vez depois disto (milissegundos).
const TOMBSTONE_KEEP_MS: u64 = 90 * 24 * 60 * 60 * 1000;

/// Uma tarefa. Com `remind_at`, é um lembrete: avisa naquela hora.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub done: bool,
    /// Segundos Unix de quando foi marcada como feita.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done_at: Option<u64>,
    /// Dia em que vence (`AAAA-MM-DD`, data local).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
    /// Quando avisar (segundos Unix).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remind_at: Option<u64>,
    /// O `remind_at` que já foi avisado (adiar muda o `remind_at` e o aviso
    /// volta a valer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reminded: Option<u64>,
    /// Contato ligado (`Contact::id`); sem valor = tarefa solta.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact: Option<String>,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deleted: bool,
}

/// Uma conversa de uma conta (`chat` vazio = a conta inteira), com a
/// anotação livre sobre ela.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contact {
    pub id: String,
    pub service_id: String,
    /// Tipo e nome da conta, para religar em outro computador (lá o
    /// `service_id` é outro) e para mostrar se a conta for removida.
    #[serde(default)]
    pub service_kind: String,
    #[serde(default)]
    pub service_name: String,
    #[serde(default)]
    pub chat: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deleted: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Organizer {
    #[serde(default)]
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub contacts: Vec<Contact>,
}

/// Conversa a que uma tarefa ou anotação se liga.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactRef {
    pub service_id: String,
    #[serde(default)]
    pub service_kind: String,
    #[serde(default)]
    pub service_name: String,
    #[serde(default)]
    pub chat: String,
}

/// Campos editáveis de uma tarefa.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskInput {
    pub text: String,
    #[serde(default)]
    pub due: Option<String>,
    #[serde(default)]
    pub remind_at: Option<u64>,
    /// Liga a esta conversa (cria o contato se preciso)…
    #[serde(default)]
    pub contact: Option<ContactRef>,
    /// …ou mantém um contato que já existe (ex.: de uma conta removida).
    #[serde(default)]
    pub contact_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct OrganizerFile {
    version: u32,
    #[serde(flatten)]
    organizer: Organizer,
}

/// Identificador no formato gerado pelo app (hexadecimal).
pub fn is_item_id(id: &str) -> bool {
    (16..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Novo identificador aleatório.
pub fn new_item_id() -> String {
    crate::service::random_hex()
}

/// `AAAA-MM-DD` com mês e dia plausíveis.
pub fn is_date(value: &str) -> bool {
    let b = value.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let num = |r: std::ops::Range<usize>| -> Option<u32> {
        let s = &value[r];
        s.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| s.parse().ok())
            .flatten()
    };
    matches!(
        (num(0..4), num(5..7), num(8..10)),
        (Some(1970..=9999), Some(1..=12), Some(1..=31))
    )
}

fn clean_text(text: &str, max: usize) -> String {
    let text: String = text
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect();
    let text = text.trim();
    match text.char_indices().nth(max) {
        Some((cut, _)) => text[..cut].trim_end().to_owned(),
        None => text.to_owned(),
    }
}

fn clean_line(text: &str, max: usize) -> String {
    clean_text(&text.replace(['\n', '\t'], " "), max)
}

/// Nome de conversa como guardado nos contatos (o site manda do mesmo jeito).
pub fn clean_chat(text: &str) -> String {
    clean_line(text, CHAT_MAX)
}

impl Task {
    fn is_open(&self) -> bool {
        !self.deleted && !self.done
    }
}

impl Organizer {
    /// Valida o que veio do disco, de um backup ou de outro computador:
    /// descarta ids inválidos ou repetidos, limita textos e quantidades e
    /// esquece registros apagados há mais de 90 dias.
    pub fn sanitized(self, now_ms: u64) -> Self {
        let keep = |deleted: bool, updated: u64| {
            !deleted || now_ms.saturating_sub(updated) < TOMBSTONE_KEEP_MS
        };
        let mut seen = HashSet::new();
        let mut contacts: Vec<Contact> = self
            .contacts
            .into_iter()
            .filter(|c| is_item_id(&c.id) && keep(c.deleted, c.updated_at))
            .filter(|c| !c.service_id.is_empty() && c.service_id.len() <= 64)
            .filter(|c| seen.insert(c.id.clone()))
            .map(|mut c| {
                c.service_kind = clean_line(&c.service_kind, 20);
                c.service_name = clean_line(&c.service_name, 80);
                c.chat = clean_line(&c.chat, CHAT_MAX);
                c.note = clean_text(&c.note, NOTE_MAX);
                if c.deleted {
                    c.note.clear();
                }
                c
            })
            .collect();
        newest_first(&mut contacts, |c| c.updated_at, MAX_CONTACTS);
        let live: HashSet<&str> = contacts
            .iter()
            .filter(|c| !c.deleted)
            .map(|c| c.id.as_str())
            .collect();
        let mut seen = HashSet::new();
        let tasks: Vec<Task> = self
            .tasks
            .into_iter()
            .filter(|t| is_item_id(&t.id) && keep(t.deleted, t.updated_at))
            .filter(|t| seen.insert(t.id.clone()))
            .filter_map(|mut t| {
                t.text = clean_line(&t.text, TASK_TEXT_MAX);
                if t.deleted {
                    return Some(tombstone(t));
                }
                if t.text.is_empty() {
                    return None;
                }
                t.due = t.due.filter(|d| is_date(d));
                t.contact = t.contact.filter(|c| live.contains(c.as_str()));
                if !t.done {
                    t.done_at = None;
                }
                Some(t)
            })
            .collect();
        // Limite conta só as valendo; os apagados têm o próprio limite.
        let (mut live, mut gone): (Vec<Task>, Vec<Task>) =
            tasks.into_iter().partition(|t| !t.deleted);
        newest_first(&mut live, |t| t.updated_at, MAX_TASKS);
        newest_first(&mut gone, |t| t.updated_at, MAX_TASKS);
        live.extend(gone);
        Self {
            tasks: live,
            contacts,
        }
    }

    pub fn task(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id && !t.deleted)
    }

    fn task_mut(&mut self, id: &str) -> Result<&mut Task, String> {
        self.tasks
            .iter_mut()
            .find(|t| t.id == id && !t.deleted)
            .ok_or_else(|| "Tarefa não encontrada.".to_owned())
    }

    pub fn contact(&self, id: &str) -> Option<&Contact> {
        self.contacts.iter().find(|c| c.id == id && !c.deleted)
    }

    /// Contato da conversa (`service_id` + `chat`), criado se não existir.
    pub fn contact_for(&mut self, r: &ContactRef, now_ms: u64) -> Result<String, String> {
        if r.service_id.is_empty() || r.service_id.len() > 64 {
            return Err("Conta inválida.".into());
        }
        let chat = clean_line(&r.chat, CHAT_MAX);
        let name = clean_line(&r.service_name, 80);
        if let Some(c) = self
            .contacts
            .iter_mut()
            .find(|c| !c.deleted && c.service_id == r.service_id && c.chat == chat)
        {
            // Nome da conta mudou: atualiza para religar e exibir certo.
            if c.service_name != name && !name.is_empty() {
                c.service_name = name;
                c.updated_at = now_ms;
            }
            return Ok(c.id.clone());
        }
        if self.contacts.iter().filter(|c| !c.deleted).count() >= MAX_CONTACTS {
            return Err("Limite de contatos com anotações atingido.".into());
        }
        let id = new_item_id();
        self.contacts.push(Contact {
            id: id.clone(),
            service_id: r.service_id.clone(),
            service_kind: clean_line(&r.service_kind, 20),
            service_name: name,
            chat,
            note: String::new(),
            updated_at: now_ms,
            deleted: false,
        });
        Ok(id)
    }

    /// Contato existente da conversa, sem criar.
    pub fn find_contact(&self, service_id: &str, chat: &str) -> Option<&Contact> {
        let chat = clean_line(chat, CHAT_MAX);
        self.contacts
            .iter()
            .find(|c| !c.deleted && c.service_id == service_id && c.chat == chat)
    }

    fn apply(&mut self, input: &TaskInput, now_ms: u64) -> Result<Task, String> {
        let text = clean_line(&input.text, TASK_TEXT_MAX);
        if text.is_empty() {
            return Err("Escreva a tarefa.".into());
        }
        if let Some(due) = &input.due {
            if !is_date(due) {
                return Err("Data inválida.".into());
            }
        }
        let contact = match (&input.contact, &input.contact_id) {
            (Some(r), _) => Some(self.contact_for(r, now_ms)?),
            (None, Some(id)) => Some(
                self.contact(id)
                    .map(|c| c.id.clone())
                    .ok_or_else(|| "Contato não encontrado.".to_owned())?,
            ),
            (None, None) => None,
        };
        Ok(Task {
            id: String::new(),
            text,
            done: false,
            done_at: None,
            due: input.due.clone(),
            remind_at: input.remind_at,
            reminded: None,
            contact,
            created_at: now_ms,
            updated_at: now_ms,
            deleted: false,
        })
    }

    pub fn add_task(&mut self, input: &TaskInput, now_ms: u64) -> Result<String, String> {
        if self.tasks.iter().filter(|t| !t.deleted).count() >= MAX_TASKS {
            return Err("Limite de tarefas atingido. Apague as já feitas.".into());
        }
        let mut task = self.apply(input, now_ms)?;
        task.id = new_item_id();
        let id = task.id.clone();
        self.tasks.push(task);
        Ok(id)
    }

    /// Troca texto, data, lembrete e contato. Um lembrete com hora nova
    /// volta a avisar.
    pub fn edit_task(&mut self, id: &str, input: &TaskInput, now_ms: u64) -> Result<(), String> {
        self.task_mut(id)?;
        let next = self.apply(input, now_ms)?;
        let task = self.task_mut(id)?;
        if task.remind_at != next.remind_at {
            task.reminded = None;
        }
        task.text = next.text;
        task.due = next.due;
        task.remind_at = next.remind_at;
        task.contact = next.contact;
        task.updated_at = now_ms;
        Ok(())
    }

    pub fn set_done(&mut self, id: &str, done: bool, now_ms: u64) -> Result<(), String> {
        let task = self.task_mut(id)?;
        task.done = done;
        task.done_at = done.then_some(now_ms / 1000);
        task.updated_at = now_ms;
        Ok(())
    }

    /// Adia o aviso para `until` (segundos Unix).
    pub fn snooze(&mut self, id: &str, until: u64, now_ms: u64) -> Result<(), String> {
        let task = self.task_mut(id)?;
        task.remind_at = Some(until);
        task.reminded = None;
        task.done = false;
        task.done_at = None;
        task.updated_at = now_ms;
        Ok(())
    }

    pub fn delete_task(&mut self, id: &str, now_ms: u64) -> Result<(), String> {
        let task = self.task_mut(id)?;
        *task = tombstone(Task {
            updated_at: now_ms,
            ..task.clone()
        });
        Ok(())
    }

    /// Grava a anotação da conversa (cria o contato se preciso).
    pub fn set_note(&mut self, r: &ContactRef, note: &str, now_ms: u64) -> Result<String, String> {
        let note = clean_text(note, NOTE_MAX);
        if note.is_empty() && self.find_contact(&r.service_id, &r.chat).is_none() {
            return Ok(String::new());
        }
        let id = self.contact_for(r, now_ms)?;
        if let Some(c) = self.contacts.iter_mut().find(|c| c.id == id) {
            if c.note != note {
                c.note = note;
                c.updated_at = now_ms;
            }
        }
        Ok(id)
    }

    /// Lembretes vencidos ainda não avisados (tarefas abertas).
    pub fn due_reminders(&self, now_secs: u64) -> Vec<&Task> {
        self.tasks
            .iter()
            .filter(|t| t.is_open())
            .filter(|t| t.remind_at.is_some_and(|at| at <= now_secs) && t.reminded != t.remind_at)
            .collect()
    }

    /// Marca o lembrete como avisado (vale também nos outros computadores).
    pub fn mark_reminded(&mut self, id: &str, now_ms: u64) {
        if let Ok(task) = self.task_mut(id) {
            task.reminded = task.remind_at;
            task.updated_at = now_ms;
        }
    }

    /// Tarefas abertas atrasadas e para hoje. `today` é a data local
    /// (`AAAA-MM-DD`); `day_start`/`day_end`, os limites de hoje em
    /// segundos Unix.
    pub fn counts(&self, today: &str, day_start: u64, day_end: u64) -> (u32, u32) {
        let (mut overdue, mut due_today) = (0, 0);
        for t in self.tasks.iter().filter(|t| t.is_open()) {
            let late = t.due.as_deref().is_some_and(|d| d < today)
                || t.remind_at.is_some_and(|at| at < day_start);
            let now = t.due.as_deref() == Some(today)
                || t.remind_at
                    .is_some_and(|at| (day_start..day_end).contains(&at));
            if late {
                overdue += 1;
            } else if now {
                due_today += 1;
            }
        }
        (overdue, due_today)
    }

    /// Só o que está valendo (sem os apagados), para a interface.
    pub fn visible(&self) -> Self {
        Self {
            tasks: self.tasks.iter().filter(|t| !t.deleted).cloned().collect(),
            contacts: self
                .contacts
                .iter()
                .filter(|c| !c.deleted)
                .cloned()
                .collect(),
        }
    }

    pub fn open_count(&self) -> usize {
        self.tasks.iter().filter(|t| t.is_open()).count()
    }

    pub fn note_count(&self) -> usize {
        self.contacts
            .iter()
            .filter(|c| !c.deleted && !c.note.is_empty())
            .count()
    }
}

/// Registro apagado: só o que a sincronização precisa (sem o conteúdo).
fn tombstone(task: Task) -> Task {
    Task {
        id: task.id,
        text: String::new(),
        done: false,
        done_at: None,
        due: None,
        remind_at: None,
        reminded: None,
        contact: None,
        created_at: task.created_at,
        updated_at: task.updated_at,
        deleted: true,
    }
}

/// Mantém no máximo `max` registros, os mais recentes.
fn newest_first<T>(items: &mut Vec<T>, updated: impl Fn(&T) -> u64, max: usize) {
    if items.len() > max {
        items.sort_by_key(|t| std::cmp::Reverse(updated(t)));
        items.truncate(max);
    }
}

/// Lê `organizer.json`. Sem arquivo: vazio. Ilegível (JSON quebrado):
/// guarda como `.corrompido` e começa vazio. `Err`: não deu para ler
/// (permissão, arquivo em uso) ou é de uma versão mais nova; o app não
/// deve gravar por cima.
pub fn load_organizer(path: &Path, now_ms: u64) -> Result<Organizer, String> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Organizer::default()),
        Err(err) => {
            return Err(format!(
                "Não foi possível ler as tarefas ({err}). Feche e abra o Papeo."
            ))
        }
    };
    // A versão primeiro: um formato mais novo pode nem caber no atual.
    #[derive(Deserialize)]
    struct VersionOnly {
        #[serde(default)]
        version: u32,
    }
    if let Ok(v) = serde_json::from_slice::<VersionOnly>(&bytes) {
        if v.version > ORGANIZER_VERSION {
            return Err(format!(
                "As tarefas foram gravadas por uma versão mais nova do Papeo (formato {}). Atualize o app.",
                v.version
            ));
        }
    }
    match serde_json::from_slice::<OrganizerFile>(&bytes) {
        Ok(file) => Ok(file.organizer.sanitized(now_ms)),
        Err(err) => {
            eprintln!("papeo: organizer.json inválido ({err}); começando vazio");
            let stamp = now_ms / 1000;
            let _ = fs::rename(
                path,
                path.with_extension(format!("json.corrompido-{stamp}")),
            );
            Ok(Organizer::default())
        }
    }
}

pub fn save_organizer(path: &Path, organizer: &Organizer) -> Result<(), String> {
    let body = serde_json::to_vec(&OrganizerFile {
        version: ORGANIZER_VERSION,
        organizer: organizer.clone(),
    })
    .map_err(|e| e.to_string())?;
    crate::store::write_atomic(path, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000_000;

    fn chat(name: &str) -> ContactRef {
        ContactRef {
            service_id: "svc-0123456789abcdef".into(),
            service_kind: "whatsapp".into(),
            service_name: "Pessoal".into(),
            chat: name.into(),
        }
    }

    fn input(text: &str) -> TaskInput {
        TaskInput {
            text: text.into(),
            ..Default::default()
        }
    }

    #[test]
    fn tasks_with_and_without_contact() {
        let mut org = Organizer::default();
        let loose = org.add_task(&input("  pagar o contador "), NOW).unwrap();
        let linked = org
            .add_task(
                &TaskInput {
                    contact: Some(chat("Ana")),
                    ..input("retornar orçamento")
                },
                NOW,
            )
            .unwrap();
        let again = org
            .add_task(
                &TaskInput {
                    contact: Some(chat("Ana")),
                    ..input("mandar contrato")
                },
                NOW,
            )
            .unwrap();
        assert_eq!(org.task(&loose).unwrap().text, "pagar o contador");
        assert!(org.task(&loose).unwrap().contact.is_none());
        // A mesma conversa vira um contato só.
        assert_eq!(
            org.task(&linked).unwrap().contact,
            org.task(&again).unwrap().contact
        );
        assert_eq!(org.contacts.len(), 1);
        assert!(org.add_task(&input("   "), NOW).is_err());
        let bad_date = TaskInput {
            due: Some("2026-13-01".into()),
            ..input("x")
        };
        assert!(org.add_task(&bad_date, NOW).is_err());
    }

    #[test]
    fn reminders_fire_once_and_snooze_rearms() {
        let mut org = Organizer::default();
        let id = org
            .add_task(
                &TaskInput {
                    remind_at: Some(1000),
                    ..input("ligar")
                },
                NOW,
            )
            .unwrap();
        assert!(org.due_reminders(999).is_empty());
        assert_eq!(org.due_reminders(1000).len(), 1);
        org.mark_reminded(&id, NOW);
        assert!(org.due_reminders(5000).is_empty());
        org.snooze(&id, 4000, NOW).unwrap();
        assert!(org.due_reminders(3999).is_empty());
        assert_eq!(org.due_reminders(4000).len(), 1);
        org.set_done(&id, true, NOW).unwrap();
        assert!(org.due_reminders(9000).is_empty());
        // Editar a hora também rearma.
        org.set_done(&id, false, NOW).unwrap();
        org.mark_reminded(&id, NOW);
        org.edit_task(
            &id,
            &TaskInput {
                remind_at: Some(8000),
                ..input("ligar")
            },
            NOW,
        )
        .unwrap();
        assert_eq!(org.due_reminders(8000).len(), 1);
    }

    #[test]
    fn delete_keeps_a_tombstone_without_content() {
        let mut org = Organizer::default();
        let id = org.add_task(&input("segredo"), NOW).unwrap();
        org.delete_task(&id, NOW + 1).unwrap();
        assert!(org.task(&id).is_none());
        let t = &org.tasks[0];
        assert!(t.deleted && t.text.is_empty());
        assert!(org.visible().tasks.is_empty());
        // Some de vez depois de 90 dias.
        let later = org.clone().sanitized(NOW + TOMBSTONE_KEEP_MS + 2);
        assert!(later.tasks.is_empty());
        assert_eq!(org.clone().sanitized(NOW + 10).tasks.len(), 1);
    }

    #[test]
    fn notes_create_contact_only_with_text() {
        let mut org = Organizer::default();
        assert_eq!(org.set_note(&chat("Bia"), "  ", NOW).unwrap(), "");
        assert!(org.contacts.is_empty());
        let id = org.set_note(&chat("Bia"), "prefere à tarde", NOW).unwrap();
        assert_eq!(org.contact(&id).unwrap().note, "prefere à tarde");
        let long = "x".repeat(NOTE_MAX + 50);
        org.set_note(&chat("Bia"), &long, NOW).unwrap();
        assert_eq!(org.contact(&id).unwrap().note.chars().count(), NOTE_MAX);
        assert_eq!(org.note_count(), 1);
    }

    #[test]
    fn counts_overdue_and_today() {
        let mut org = Organizer::default();
        let day = |d: &str| TaskInput {
            due: Some(d.into()),
            ..input("t")
        };
        org.add_task(&day("2026-11-24"), NOW).unwrap();
        org.add_task(&day("2026-11-25"), NOW).unwrap();
        org.add_task(&day("2026-11-26"), NOW).unwrap();
        org.add_task(
            &TaskInput {
                remind_at: Some(150),
                ..input("r")
            },
            NOW,
        )
        .unwrap();
        org.add_task(
            &TaskInput {
                remind_at: Some(50),
                ..input("r")
            },
            NOW,
        )
        .unwrap();
        org.add_task(&input("sem data"), NOW).unwrap();
        assert_eq!(org.counts("2026-11-25", 100, 200), (2, 2));
    }

    #[test]
    fn sanitize_drops_invalid_and_dangling() {
        let mut org = Organizer::default();
        let id = org
            .add_task(
                &TaskInput {
                    contact: Some(chat("Ana")),
                    ..input("ok")
                },
                NOW,
            )
            .unwrap();
        let mut bad = org.tasks[0].clone();
        bad.id = "../x".into();
        org.tasks.push(bad);
        let mut dup = org.tasks[0].clone();
        dup.text = "repetida".into();
        org.tasks.push(dup);
        org.contacts.clear();
        let clean = org.sanitized(NOW);
        assert_eq!(clean.tasks.len(), 1);
        assert_eq!(clean.tasks[0].id, id);
        // Contato que não existe mais: a tarefa fica solta.
        assert!(clean.tasks[0].contact.is_none());
    }

    #[test]
    fn file_roundtrip_and_corruption() {
        let dir = std::env::temp_dir().join(format!("papeo-org-{}", new_item_id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("organizer.json");
        assert_eq!(load_organizer(&path, NOW).unwrap(), Organizer::default());
        let mut org = Organizer::default();
        org.add_task(&input("a"), NOW).unwrap();
        org.set_note(&chat("Ana"), "nota", NOW).unwrap();
        save_organizer(&path, &org).unwrap();
        assert_eq!(load_organizer(&path, NOW).unwrap(), org);
        fs::write(&path, b"{oops").unwrap();
        assert!(load_organizer(&path, NOW).unwrap().tasks.is_empty());
        assert!(path
            .with_extension(format!("json.corrompido-{}", NOW / 1000))
            .exists());
        // Versão mais nova: erro (o app não grava por cima) e o arquivo fica.
        fs::write(&path, br#"{"version":99,"tasks":"outro formato"}"#).unwrap();
        assert!(load_organizer(&path, NOW).is_err());
        assert!(path.exists());
        // Não é arquivo legível (uma pasta no lugar): erro, não vazio.
        let folder = dir.join("pasta.json");
        fs::create_dir_all(&folder).unwrap();
        assert!(load_organizer(&folder, NOW).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn dates() {
        assert!(is_date("2026-11-25"));
        assert!(!is_date("2026-1-25"));
        assert!(!is_date("2026-00-10"));
        assert!(!is_date("abcd-11-25"));
        assert!(!is_date("2026-11-25T"));
    }
}
