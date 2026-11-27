//! Saída do não perturbe/horário silencioso: resumo do que chegou
//! (spec 17). A contagem fica em `Inner::silence` (ver `notify::deliver`).

use crate::state::{local_minute_of_day, now_secs, publish, AppState};
use papeo_core::i18n::{tr, trf};
use tauri::{AppHandle, Manager};

/// Confere se o silêncio começou ou acabou desde a última vez. Na saída,
/// guarda o resumo para o cartão e mostra a notificação.
pub fn check(app: &AppHandle) {
    let (now, minute) = (now_secs(), local_minute_of_day());
    let state = app.state::<AppState>();
    let notice = {
        let mut inner = state.lock();
        let active = inner.config.settings.dnd_active(now, minute);
        let was = std::mem::replace(&mut inner.silence_active, active);
        if active && !was {
            inner.silence.start(now);
            None
        } else if was && !active {
            // Contas que sumiram (backup restaurado) saem do resumo.
            let gone: Vec<String> = inner
                .silence
                .ids()
                .filter(|id| inner.service(id).is_err())
                .map(str::to_owned)
                .collect();
            for id in &gone {
                inner.silence.forget(id);
            }
            let summary = inner.silence.finish(now);
            match summary {
                Some(summary) if inner.config.settings.silence_summary => {
                    // A notificação segue o cartão: contas fora do modo atual
                    // ficam de fora (o resumo inteiro fica guardado).
                    let mode = inner.current_mode();
                    let shown: Vec<(String, String, u32)> = summary
                        .accounts
                        .iter()
                        .filter_map(|a| {
                            let s = inner.service(&a.service_id).ok()?;
                            mode.allows(s.context)
                                .then(|| (s.id.clone(), s.name.clone(), a.count))
                        })
                        .collect();
                    inner.silence_summary = Some(summary);
                    let total = shown.iter().fold(0u32, |n, a| n.saturating_add(a.2));
                    let names: Vec<(String, u32)> = shown
                        .iter()
                        .map(|(_, name, n)| (name.clone(), *n))
                        .collect();
                    shown
                        .first()
                        .map(|(top, _, _)| (top.clone(), text(total, &names, inner.locked)))
                }
                _ => None,
            }
        } else {
            None
        }
    };
    let Some((top, text)) = notice else {
        return;
    };
    publish(app);
    let target = crate::notifications::Target {
        service_id: top,
        reply: false,
        later: false,
        chat: None,
        task: None,
        remind_at: None,
    };
    let _ = crate::notifications::show(
        app,
        tr("Enquanto você estava em silêncio"),
        &text,
        None,
        target,
    );
}

fn plural_msgs(n: u32) -> String {
    if n == 1 {
        trf("{n} mensagem", &[("n", &n)])
    } else {
        trf("{n} mensagens", &[("n", &n)])
    }
}

fn plural_accounts(n: u32) -> String {
    if n == 1 {
        trf("{n} conta", &[("n", &n)])
    } else {
        trf("{n} contas", &[("n", &n)])
    }
}

/// "14 mensagens em 3 contas: WhatsApp (9), Gmail (3), Telegram (2)".
/// Bloqueado: só os números.
fn text(total: u32, accounts: &[(String, u32)], locked: bool) -> String {
    let head = trf(
        "{mensagens} em {contas}",
        &[
            ("mensagens", &plural_msgs(total)),
            ("contas", &plural_accounts(accounts.len() as u32)),
        ],
    );
    if locked || accounts.is_empty() {
        return head;
    }
    let list: Vec<String> = accounts
        .iter()
        .take(4)
        .map(|(name, n)| format!("{name} ({n})"))
        .collect();
    let more = accounts.len().saturating_sub(4);
    let tail = if more > 0 {
        trf(" e mais {n}", &[("n", &more)])
    } else {
        String::new()
    };
    format!("{head}: {}{tail}", list.join(", "))
}

#[cfg(test)]
mod tests {
    use super::text;

    #[test]
    fn summary_text() {
        let accounts = vec![("WhatsApp".to_owned(), 9), ("Gmail".to_owned(), 1)];
        assert_eq!(
            text(10, &accounts, false),
            "10 mensagens em 2 contas: WhatsApp (9), Gmail (1)"
        );
        assert_eq!(text(10, &accounts, true), "10 mensagens em 2 contas");
        assert_eq!(
            text(1, &accounts[1..], false),
            "1 mensagem em 1 conta: Gmail (1)"
        );
        let many: Vec<(String, u32)> = (0..6).map(|i| (format!("C{i}"), 1)).collect();
        assert!(text(6, &many, false).ends_with("C3 (1) e mais 2"));
    }
}
