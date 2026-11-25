//! Tarefas, lembretes e anotações por contato: estado, comandos da
//! interface e aviso na hora marcada.
//!
//! O conteúdo só vai para a interface com o app desbloqueado, por um evento
//! próprio (o `Snapshot` leva só as contagens).

use crate::notifications::Target;
use crate::state::{now_secs, publish, AppState, Inner, UI_WEBVIEW};
use papeo_core::{ContactRef, NotificationPreview, Organizer, TaskInput};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

pub const ORGANIZER_EVENT: &str = "papeo://organizer";
/// Mais que isto vencidos de uma vez (volta do repouso, app aberto depois):
/// um aviso só.
const GROUP_ABOVE: usize = 3;
/// "Adiar" na notificação.
pub const SNOOZE_SECS: u64 = 60 * 60;

type CmdResult<T = ()> = Result<T, String>;

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

/// Atrasadas e para hoje (contagens para a barra lateral e o Início).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskCounts {
    pub overdue: u32,
    pub today: u32,
}

/// Hoje na hora local: `AAAA-MM-DD`, início e fim em segundos Unix.
fn local_day() -> (String, u64, u64) {
    use chrono::{Days, Local, TimeZone};
    let now = Local::now();
    let today = now.date_naive();
    // Onde o relógio pula a meia-noite (horário de verão), o dia começa à 1h,
    // como no `new Date(a, m, d)` da interface.
    let start_of = |d: chrono::NaiveDate| {
        [0, 1]
            .into_iter()
            .filter_map(|h| d.and_hms_opt(h, 0, 0))
            .find_map(|t| Local.from_local_datetime(&t).earliest())
            .map(|t| t.timestamp().max(0) as u64)
    };
    let start = start_of(today).unwrap_or_else(now_secs);
    let end = today
        .checked_add_days(Days::new(1))
        .and_then(start_of)
        .unwrap_or(start + 86_400);
    (today.format("%Y-%m-%d").to_string(), start, end)
}

pub fn counts(inner: &Inner) -> TaskCounts {
    if inner.locked {
        return TaskCounts::default();
    }
    let (today, start, end) = local_day();
    let (overdue, today) = inner.organizer.counts(&today, start, end);
    TaskCounts { overdue, today }
}

/// Envia tarefas e anotações à interface e à mini janela (nada com o app
/// bloqueado) e atualiza as contagens.
pub fn publish_organizer(app: &AppHandle) {
    let view = {
        let state = app.state::<AppState>();
        let inner = state.lock();
        (!inner.locked).then(|| inner.organizer.visible())
    };
    if let Some(view) = view {
        let _ = app.emit_to(UI_WEBVIEW, ORGANIZER_EVENT, &view);
        let _ = app.emit_to(crate::popover::POPOVER, ORGANIZER_EVENT, &view);
    }
    publish(app);
}

/// Altera, grava `organizer.json` e publica.
fn change<T>(
    app: &AppHandle,
    f: impl FnOnce(&mut Inner, u64) -> Result<T, String>,
) -> Result<T, String> {
    let state = app.state::<AppState>();
    let out = {
        let mut inner = state.lock();
        // Arquivo não lido: nada muda (nem na memória).
        if let Some(err) = &inner.organizer_error {
            return Err(err.clone());
        }
        let out = f(&mut inner, now_ms())?;
        state.persist_organizer(&inner)?;
        out
    };
    publish_organizer(app);
    Ok(out)
}

fn ensure_unlocked(app: &AppHandle) -> CmdResult {
    if app.state::<AppState>().lock().locked {
        Err("Desbloqueie o Papeo primeiro.".into())
    } else {
        Ok(())
    }
}

/// Conversa escolhida na interface. Tipo e nome da conta vêm do app, não
/// da interface.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    pub service_id: String,
    #[serde(default)]
    pub chat: String,
}

fn contact_ref(inner: &Inner, link: &Link) -> Result<ContactRef, String> {
    let service = inner.service(&link.service_id)?;
    Ok(ContactRef {
        service_id: service.id.clone(),
        service_kind: serde_json::to_value(service.kind)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default(),
        service_name: service.name.clone(),
        chat: link.chat.clone(),
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskFields {
    pub text: String,
    #[serde(default)]
    pub due: Option<String>,
    #[serde(default)]
    pub remind_at: Option<u64>,
    /// Ligar a esta conversa…
    #[serde(default)]
    pub link: Option<Link>,
    /// …ou manter este contato (conta removida, por exemplo).
    #[serde(default)]
    pub contact_id: Option<String>,
}

fn task_input(inner: &Inner, fields: TaskFields) -> Result<TaskInput, String> {
    let contact = match &fields.link {
        Some(link) => Some(contact_ref(inner, link)?),
        None => None,
    };
    Ok(TaskInput {
        text: fields.text,
        due: fields.due,
        remind_at: fields.remind_at,
        contact,
        contact_id: fields.contact_id,
    })
}

#[tauri::command]
pub async fn get_organizer(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Organizer> {
    ensure_unlocked(&app)?;
    let inner = state.lock();
    if let Some(err) = &inner.organizer_error {
        return Err(err.clone());
    }
    Ok(inner.organizer.visible())
}

/// O que os comandos devolvem: tudo o que vale agora (o evento também vai).
fn view(app: &AppHandle) -> Organizer {
    app.state::<AppState>().lock().organizer.visible()
}

/// Cria (`id` vazio) ou altera uma tarefa.
#[tauri::command]
pub async fn save_task(
    app: AppHandle,
    id: Option<String>,
    task: TaskFields,
) -> CmdResult<Organizer> {
    ensure_unlocked(&app)?;
    change(&app, |inner, now| {
        let input = task_input(inner, task)?;
        match id {
            Some(id) => inner.organizer.edit_task(&id, &input, now),
            None => inner.organizer.add_task(&input, now).map(|_| ()),
        }
    })?;
    Ok(view(&app))
}

#[tauri::command]
pub async fn set_task_done(app: AppHandle, id: String, done: bool) -> CmdResult<Organizer> {
    ensure_unlocked(&app)?;
    change(&app, |inner, now| inner.organizer.set_done(&id, done, now))?;
    Ok(view(&app))
}

/// Adia o aviso para `until` (segundos Unix).
#[tauri::command]
pub async fn snooze_task(app: AppHandle, id: String, until: u64) -> CmdResult<Organizer> {
    ensure_unlocked(&app)?;
    change(&app, |inner, now| inner.organizer.snooze(&id, until, now))?;
    Ok(view(&app))
}

#[tauri::command]
pub async fn delete_task(app: AppHandle, id: String) -> CmdResult<Organizer> {
    ensure_unlocked(&app)?;
    change(&app, |inner, now| inner.organizer.delete_task(&id, now))?;
    Ok(view(&app))
}

/// Grava a anotação de uma conversa (`link`) ou de um contato existente.
#[tauri::command]
pub async fn save_note(
    app: AppHandle,
    link: Option<Link>,
    contact_id: Option<String>,
    note: String,
) -> CmdResult<Organizer> {
    ensure_unlocked(&app)?;
    change(&app, |inner, now| {
        let r = match (&link, &contact_id) {
            (Some(link), _) => contact_ref(inner, link)?,
            (None, Some(id)) => {
                let c = inner
                    .organizer
                    .contact(id)
                    .ok_or_else(|| "Contato não encontrado.".to_owned())?;
                ContactRef {
                    service_id: c.service_id.clone(),
                    service_kind: c.service_kind.clone(),
                    service_name: c.service_name.clone(),
                    chat: c.chat.clone(),
                }
            }
            (None, None) => return Err("Escolha a conversa.".into()),
        };
        inner.organizer.set_note(&r, &note, now)
    })?;
    Ok(view(&app))
}

/// Avisa os lembretes que venceram. Chamado pela manutenção (a cada 15 s).
pub fn tick(app: &AppHandle) {
    let state = app.state::<AppState>();
    let notices = {
        let mut inner = state.lock();
        let now = now_ms();
        let due: Vec<_> = inner
            .organizer
            .due_reminders(now / 1000)
            .into_iter()
            .cloned()
            .collect();
        if due.is_empty() {
            return;
        }
        for task in &due {
            inner.organizer.mark_reminded(&task.id, now);
        }
        if let Err(err) = state.persist_organizer(&inner) {
            eprintln!("papeo: não foi possível gravar as tarefas ({err})");
        }
        let preview = if inner.locked {
            NotificationPreview::Hidden
        } else {
            inner.config.settings.notification_preview
        };
        if due.len() > GROUP_ABOVE {
            let target = Target {
                service_id: String::new(),
                chat: None,
                reply: false,
                task: None,
            };
            vec![(
                "Lembretes".to_owned(),
                format!("Você tem {} lembretes vencidos.", due.len()),
                target,
            )]
        } else {
            due.iter()
                .map(|task| {
                    let contact = task
                        .contact
                        .as_deref()
                        .and_then(|id| inner.organizer.contact(id));
                    let (service_id, chat, label) = match contact {
                        Some(c) => {
                            let live = inner.service(&c.service_id).is_ok();
                            let label = if c.chat.is_empty() {
                                c.service_name.clone()
                            } else {
                                format!("{} ({})", c.chat, c.service_name)
                            };
                            (
                                if live {
                                    c.service_id.clone()
                                } else {
                                    String::new()
                                },
                                (live && !c.chat.is_empty()).then(|| c.chat.clone()),
                                Some(label),
                            )
                        }
                        None => (String::new(), None, None),
                    };
                    let (title, body) = reminder_text(preview, label.as_deref(), &task.text);
                    let target = Target {
                        service_id,
                        chat,
                        reply: false,
                        task: Some(task.id.clone()),
                    };
                    (title, body, target)
                })
                .collect()
        }
    };
    publish_organizer(app);
    for (title, body, target) in notices {
        // Lembrete toca mesmo no "não perturbe": foi você quem marcou.
        let _ = crate::notifications::show(
            app,
            &title,
            &body,
            Some(crate::notify::DEFAULT_SOUND),
            target,
        );
    }
}

/// Título e texto do aviso conforme a prévia escolhida.
fn reminder_text(
    preview: NotificationPreview,
    contact: Option<&str>,
    text: &str,
) -> (String, String) {
    match preview {
        NotificationPreview::Hidden => ("Lembrete".into(), "Você tem um lembrete no Papeo.".into()),
        NotificationPreview::Sender => (
            contact.map_or("Lembrete".into(), |c| format!("Lembrete · {c}")),
            "Abra o Papeo para ver.".into(),
        ),
        NotificationPreview::Full => (
            contact.map_or("Lembrete".into(), |c| format!("Lembrete · {c}")),
            papeo_core::clip(text, 240),
        ),
    }
}

/// Botões da notificação: "Adiar 1 h" e "Concluir".
pub fn notification_action(app: &AppHandle, task: &str, action: &str) {
    let task = task.to_owned();
    let result = match action {
        "snooze" => change(app, |inner, now| {
            inner.organizer.snooze(&task, now / 1000 + SNOOZE_SECS, now)
        }),
        "done" => change(app, |inner, now| inner.organizer.set_done(&task, true, now)),
        _ => Ok(()),
    };
    if let Err(err) = result {
        eprintln!("papeo: ação do lembrete falhou ({err})");
    }
}

/// Clique num lembrete de antes de reabrir o app: abre a conversa ligada
/// ou a página Tarefas.
pub fn open_task(app: &AppHandle, task: &str) {
    let target = {
        let state = app.state::<AppState>();
        let inner = state.lock();
        inner
            .organizer
            .task(task)
            .and_then(|t| t.contact.as_deref())
            .and_then(|c| inner.organizer.contact(c))
            .filter(|c| inner.service(&c.service_id).is_ok())
            .map(|c| (c.service_id.clone(), c.chat.clone()))
    };
    crate::tray::show_main(app);
    if app.state::<AppState>().lock().locked {
        return;
    }
    match target {
        Some((service, chat)) if !chat.is_empty() => {
            let _ = crate::webviews::open_chat(app, &service, &chat, true);
        }
        Some((service, _)) => {
            let _ = crate::webviews::activate(app, Some(service));
        }
        None => show_tasks(app),
    }
}

/// Clique num lembrete sem conversa: abre o app na página Tarefas.
pub fn show_tasks(app: &AppHandle) {
    crate::tray::show_main(app);
    if app.state::<AppState>().lock().locked {
        return;
    }
    let _ = crate::webviews::activate(app, None);
    let _ = app.emit_to(UI_WEBVIEW, crate::state::COMMAND_EVENT, "tasks");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reminder_text_follows_preview() {
        let (t, b) = reminder_text(NotificationPreview::Full, Some("Ana (WhatsApp)"), "ligar");
        assert_eq!(
            (t.as_str(), b.as_str()),
            ("Lembrete · Ana (WhatsApp)", "ligar")
        );
        let (_, b) = reminder_text(NotificationPreview::Sender, Some("Ana"), "segredo");
        assert!(!b.contains("segredo"));
        let (t, b) = reminder_text(NotificationPreview::Hidden, Some("Ana"), "segredo");
        assert!(!t.contains("Ana") && !b.contains("segredo"));
        assert_eq!(
            reminder_text(NotificationPreview::Full, None, "x").0,
            "Lembrete"
        );
    }

    #[test]
    fn local_day_is_consistent() {
        let (today, start, end) = local_day();
        assert!(papeo_core::is_date(&today));
        assert!(start <= now_secs() && now_secs() < end);
        assert!((82_800..=90_000).contains(&(end - start)));
    }
}
