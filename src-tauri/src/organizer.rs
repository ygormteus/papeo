//! Tarefas, lembretes e anotações por contato: estado, comandos da
//! interface e aviso na hora marcada.
//!
//! O conteúdo só vai para a interface com o app desbloqueado, por um evento
//! próprio (o `Snapshot` leva só as contagens).

use crate::notifications::Target;
use crate::state::{now_secs, publish, AppState, Inner, UI_WEBVIEW};
use papeo_core::i18n::{tr, trf};
use papeo_core::{ContactRef, NotificationPreview, Organizer, Repeat, TaskInput};
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

/// A data `n` passos depois de `start` (mês: mesmo dia, ou o último do mês).
fn step(start: chrono::NaiveDate, repeat: Repeat, n: u32) -> Option<chrono::NaiveDate> {
    use chrono::{Datelike, Days, Months, Weekday};
    match repeat {
        Repeat::Daily => start.checked_add_days(Days::new(u64::from(n))),
        Repeat::Weekly => start.checked_add_days(Days::new(7 * u64::from(n))),
        Repeat::Monthly => start.checked_add_months(Months::new(n)),
        Repeat::Weekdays => {
            let mut d = start;
            let mut left = n;
            while left > 0 {
                d = d.succ_opt()?;
                if !matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
                    left -= 1;
                }
            }
            Some(d)
        }
    }
}

/// Passos já "vencidos" entre o dia-base e hoje, para não andar dia a dia
/// desde um início muito antigo (fica um pouco antes; o laço completa).
fn skip_ahead(start: chrono::NaiveDate, today: chrono::NaiveDate, repeat: Repeat) -> u32 {
    use chrono::Datelike;
    let days = (today - start).num_days().max(0);
    let k = match repeat {
        Repeat::Daily => days - 2,
        Repeat::Weekly => days / 7 - 1,
        Repeat::Weekdays => days * 5 / 7 - 3,
        Repeat::Monthly => {
            i64::from(today.year() - start.year()) * 12 + i64::from(today.month0())
                - i64::from(start.month0())
                - 1
        }
    };
    u32::try_from(k.max(1)).unwrap_or(1)
}

/// Próxima vez de uma tarefa que repete: a primeira depois de agora e da
/// vez atual, contada a partir do horário-base (adiar não muda a série).
/// Devolve o novo lembrete e/ou a nova data.
fn next_occurrence(task: &papeo_core::Task, now: u64) -> Option<(Option<u64>, Option<String>)> {
    use chrono::{Local, NaiveDate, TimeZone};
    let repeat = task.repeat?;
    let to_secs = |t: chrono::NaiveDateTime| {
        Local
            .from_local_datetime(&t)
            .earliest()
            .or_else(|| {
                Local
                    .from_local_datetime(&(t + chrono::Duration::hours(1)))
                    .earliest()
            })
            .map(|t| t.timestamp().max(0) as u64)
    };
    let today = Local::now().date_naive();
    if let Some(base) = task.anchor_at.or(task.remind_at) {
        let start = Local
            .timestamp_opt(i64::try_from(base).ok()?, 0)
            .single()?
            .naive_local();
        let after = now.max(task.remind_at.unwrap_or(0));
        let first = skip_ahead(start.date(), today, repeat);
        for n in first..first + 800 {
            let date = step(start.date(), repeat, n)?;
            let next = to_secs(date.and_time(start.time()))?;
            if next > after {
                let due = task
                    .due
                    .is_some()
                    .then(|| date.format("%Y-%m-%d").to_string());
                return Some((Some(next), due));
            }
        }
        return None;
    }
    let base = task.anchor_due.as_deref().or(task.due.as_deref())?;
    let start = NaiveDate::parse_from_str(base, "%Y-%m-%d").ok()?;
    let current = task
        .due
        .as_deref()
        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .unwrap_or(start);
    let after = today.max(current);
    let first = skip_ahead(start, today, repeat);
    for n in first..first + 800 {
        let date = step(start, repeat, n)?;
        if date > after {
            return Some((None, Some(date.format("%Y-%m-%d").to_string())));
        }
    }
    None
}

/// A vez da tarefa que a pessoa viu ao concluir (lembrete e/ou dia). Se a
/// tarefa já passou para outra vez (concluída em outro lugar, clique
/// repetido), concluir de novo não faz nada.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Occurrence {
    #[serde(default)]
    pub remind_at: Option<u64>,
    #[serde(default)]
    pub due: Option<String>,
}

/// Concluir: tarefa que repete passa para a próxima vez; as outras fecham.
fn complete(
    inner: &mut Inner,
    id: &str,
    now: u64,
    seen: Option<&Occurrence>,
) -> Result<(), String> {
    let task = inner
        .organizer
        .task(id)
        .ok_or_else(|| tr("Tarefa não encontrada.").to_owned())?;
    if task.repeat.is_some() {
        if let Some(seen) = seen {
            let same =
                seen.remind_at == task.remind_at && (seen.due.is_none() || seen.due == task.due);
            if !same {
                return Ok(());
            }
        }
    }
    match next_occurrence(task, now / 1000) {
        Some((remind_at, due)) => inner.organizer.advance(id, remind_at, due, now),
        None => inner.organizer.set_done(id, true, now),
    }
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
    let (out, staged) = {
        let mut inner = state.lock();
        // Arquivo não lido: nada muda (nem na memória).
        if let Some(err) = &inner.organizer_error {
            return Err(err.clone());
        }
        let out = f(&mut inner, now_ms())?;
        let staged = state.stage_organizer(&inner)?;
        (out, staged)
    };
    state.write(staged)?;
    crate::sync::mark_dirty(app);
    publish_organizer(app);
    Ok(out)
}

fn ensure_unlocked(app: &AppHandle) -> CmdResult {
    if app.state::<AppState>().lock().locked {
        Err(tr("Desbloqueie o Papeo primeiro.").into())
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
    #[serde(default)]
    pub repeat: Option<Repeat>,
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
        repeat: fields.repeat,
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
pub async fn set_task_done(
    app: AppHandle,
    id: String,
    done: bool,
    seen: Option<Occurrence>,
) -> CmdResult<Organizer> {
    ensure_unlocked(&app)?;
    change(&app, |inner, now| {
        if done {
            complete(inner, &id, now, seen.as_ref())
        } else {
            inner.organizer.set_done(&id, false, now)
        }
    })?;
    Ok(view(&app))
}

/// Adia o aviso para `until` (segundos Unix).
#[tauri::command]
pub async fn snooze_task(app: AppHandle, id: String, until: u64) -> CmdResult<Organizer> {
    ensure_unlocked(&app)?;
    change(&app, |inner, now| inner.organizer.snooze(&id, until, now))?;
    Ok(view(&app))
}

/// "Responder depois": lembrete ligado à conversa, para `at` (segundos Unix).
#[tauri::command]
pub async fn reply_later(
    app: AppHandle,
    service_id: String,
    chat: String,
    at: u64,
) -> CmdResult<Organizer> {
    ensure_unlocked(&app)?;
    add_reply_later(&app, &service_id, &chat, at)?;
    Ok(view(&app))
}

/// Cria a tarefa do "Responder depois" (também usada pela notificação).
pub fn add_reply_later(app: &AppHandle, service_id: &str, chat: &str, at: u64) -> CmdResult {
    let chat = chat.trim();
    if chat.is_empty() {
        return Err(tr("Conversa não encontrada.").into());
    }
    if at + 60 < now_secs() {
        return Err(tr("Escolha um horário no futuro.").into());
    }
    change(app, |inner, now| {
        let text = reply_later_text(inner, service_id, chat);
        let fields = TaskFields {
            text,
            due: None,
            remind_at: Some(at),
            link: Some(Link {
                service_id: service_id.to_owned(),
                chat: chat.to_owned(),
            }),
            contact_id: None,
            repeat: None,
        };
        let input = task_input(inner, fields)?;
        inner.organizer.add_task(&input, now).map(|_| ())
    })
}

/// "Responder: <última mensagem recebida>", ou só "Responder" + conversa.
/// O texto só entra com a prévia completa (a tarefa é gravada e sincronizada).
fn reply_later_text(inner: &Inner, service_id: &str, chat: &str) -> String {
    let full = !inner.locked
        && inner.config.settings.notification_preview == papeo_core::NotificationPreview::Full;
    let last = inner
        .recent
        .iter()
        .filter(|_| full)
        .find(|c| c.service_id == service_id && c.title == chat)
        .and_then(|c| c.messages.iter().rev().find(|m| !m.mine))
        .map(|m| m.body.trim())
        .filter(|b| !b.is_empty());
    match last {
        Some(body) => format!("Responder: {}", papeo_core::clip(body, 140)),
        None => format!("Responder {}", papeo_core::clip(chat, 80)),
    }
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
                    .ok_or_else(|| tr("Contato não encontrado.").to_owned())?;
                ContactRef {
                    service_id: c.service_id.clone(),
                    service_kind: c.service_kind.clone(),
                    service_name: c.service_name.clone(),
                    chat: c.chat.clone(),
                }
            }
            (None, None) => return Err(tr("Escolha a conversa.").into()),
        };
        inner.organizer.set_note(&r, &note, now)
    })?;
    Ok(view(&app))
}

/// Avisa os lembretes que venceram. Chamado pela manutenção (a cada 15 s).
pub fn tick(app: &AppHandle) {
    let state = app.state::<AppState>();
    let (notices, staged) = {
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
        let staged = state.stage_organizer(&inner);
        crate::sync::mark_dirty(app);
        let preview = if inner.locked {
            NotificationPreview::Hidden
        } else {
            inner.config.settings.notification_preview
        };
        let notices = if due.len() > GROUP_ABOVE {
            let target = Target {
                service_id: String::new(),
                chat: None,
                reply: false,
                later: false,
                task: None,
                remind_at: None,
            };
            vec![(
                tr("Lembretes").to_owned(),
                trf("Você tem {n} lembretes vencidos.", &[("n", &due.len())]),
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
                        later: false,
                        task: Some(task.id.clone()),
                        remind_at: task.remind_at,
                    };
                    (title, body, target)
                })
                .collect()
        };
        (notices, staged)
    };
    if let Err(err) = staged.and_then(|s| state.write(s)) {
        eprintln!("papeo: não foi possível gravar as tarefas ({err})");
    }
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
        NotificationPreview::Hidden => (
            tr("Lembrete").into(),
            tr("Você tem um lembrete no Papeo.").into(),
        ),
        NotificationPreview::Sender => (
            contact.map_or(tr("Lembrete").into(), |c| {
                trf("Lembrete · {contato}", &[("contato", &c)])
            }),
            tr("Abra o Papeo para ver.").into(),
        ),
        NotificationPreview::Full => (
            contact.map_or(tr("Lembrete").into(), |c| {
                trf("Lembrete · {contato}", &[("contato", &c)])
            }),
            papeo_core::clip(text, 240),
        ),
    }
}

/// Botões da notificação: "Adiar 1 h" e "Concluir". `remind_at`: a vez
/// que a notificação avisou.
pub fn notification_action(app: &AppHandle, task: &str, remind_at: Option<u64>, action: &str) {
    let task = task.to_owned();
    let result = match action {
        "snooze" => change(app, |inner, now| {
            inner.organizer.snooze(&task, now / 1000 + SNOOZE_SECS, now)
        }),
        "done" => change(app, |inner, now| {
            let seen = remind_at.map(|at| Occurrence {
                remind_at: Some(at),
                due: None,
            });
            complete(inner, &task, now, seen.as_ref())
        }),
        _ => Ok(()),
    };
    if let Err(err) = result {
        eprintln!("papeo: ação do lembrete falhou ({err})");
    }
}

/// Mini janela: abre o app na conversa da tarefa (ou em Tarefas).
#[tauri::command]
pub async fn show_task(app: AppHandle, id: String) -> CmdResult {
    crate::popover::hide(&app);
    open_task(&app, &id);
    Ok(())
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
    fn repeats_move_to_the_next_time_after_now() {
        use chrono::{Local, TimeZone};
        let at = |y, m, d, h| {
            Local
                .with_ymd_and_hms(y, m, d, h, 0, 0)
                .unwrap()
                .timestamp() as u64
        };
        let task = |repeat, remind: u64| papeo_core::Task {
            id: "0123456789abcdef0123456789abcdef".into(),
            text: "t".into(),
            done: false,
            done_at: None,
            due: None,
            remind_at: Some(remind),
            reminded: None,
            contact: None,
            repeat: Some(repeat),
            anchor_at: Some(remind),
            anchor_due: None,
            created_at: 0,
            updated_at: 0,
            deleted: false,
        };
        // Quarta 25/11/2026 9h, concluída na quinta 26 às 10h.
        let now = at(2026, 11, 26, 10);
        let next = |r| {
            next_occurrence(&task(r, at(2026, 11, 25, 9)), now)
                .unwrap()
                .0
                .unwrap()
        };
        assert_eq!(next(Repeat::Daily), at(2026, 11, 27, 9));
        assert_eq!(next(Repeat::Weekly), at(2026, 12, 2, 9));
        assert_eq!(next(Repeat::Monthly), at(2026, 12, 25, 9));
        assert_eq!(next(Repeat::Weekdays), at(2026, 11, 27, 9));
        let friday = task(Repeat::Weekdays, at(2026, 11, 27, 9));
        assert_eq!(
            next_occurrence(&friday, at(2026, 11, 27, 10)).unwrap().0,
            Some(at(2026, 11, 30, 9))
        );

        // Adiado (lembrete às 10h02): a série continua às 9h.
        let mut snoozed = task(Repeat::Daily, at(2026, 11, 25, 9));
        snoozed.remind_at = Some(at(2026, 11, 25, 10) + 120);
        assert_eq!(
            next_occurrence(&snoozed, at(2026, 11, 25, 10) + 200)
                .unwrap()
                .0,
            Some(at(2026, 11, 26, 9))
        );

        // Todo dia 31: 31/01 → 28/02 → 31/03 (não fica preso no 28).
        let mut monthly = task(Repeat::Monthly, at(2027, 1, 31, 8));
        let feb = next_occurrence(&monthly, at(2027, 1, 31, 9))
            .unwrap()
            .0
            .unwrap();
        assert_eq!(feb, at(2027, 2, 28, 8));
        monthly.remind_at = Some(feb);
        assert_eq!(
            next_occurrence(&monthly, feb + 60).unwrap().0,
            Some(at(2027, 3, 31, 8))
        );

        // Começo muito antigo: ainda acha a próxima (sem fechar a tarefa).
        let old = task(Repeat::Daily, at(2015, 1, 1, 7));
        let n = next_occurrence(&old, now).unwrap().0.unwrap();
        assert!(n > now && n - now <= 86_400);
    }

    #[test]
    fn completing_a_stale_occurrence_does_nothing() {
        let mut inner = Inner::new(papeo_core::Config::default());
        let now = now_ms();
        let first = now / 1000 - 600;
        let id = inner
            .organizer
            .add_task(
                &TaskInput {
                    text: "tomar remédio".into(),
                    remind_at: Some(first),
                    repeat: Some(Repeat::Daily),
                    ..Default::default()
                },
                now,
            )
            .unwrap();
        let seen = Occurrence {
            remind_at: Some(first),
            due: None,
        };
        complete(&mut inner, &id, now, Some(&seen)).unwrap();
        let after = inner.organizer.task(&id).unwrap().remind_at;
        assert!(after > Some(first));
        // A mesma notificação (ou um segundo clique) não pula mais um dia.
        complete(&mut inner, &id, now + 1, Some(&seen)).unwrap();
        assert_eq!(inner.organizer.task(&id).unwrap().remind_at, after);
    }

    #[test]
    fn local_day_is_consistent() {
        let (today, start, end) = local_day();
        assert!(papeo_core::is_date(&today));
        assert!(start <= now_secs() && now_secs() < end);
        assert!((82_800..=90_000).contains(&(end - start)));
    }
}
