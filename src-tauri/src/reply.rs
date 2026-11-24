//! Resposta rápida (mini janela da barra e notificações): escreve e envia uma
//! mensagem numa conversa do WhatsApp/Telegram sem trazer o app para frente.
//!
//! O envio é feito pelo próprio site, no script injetado
//! (`__papeoReply`/`__papeoReplyStatus` em `notification_shim.js`), que só
//! envia depois de conferir o nome da conversa no cabeçalho. O Papeo acompanha
//! o resultado lendo a página (`eval_with_callback`): os sites continuam sem
//! nenhum comando novo.

use crate::state::{now_secs, publish, AppState, ChatMessage, Snapshot};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, Webview};

/// Limite de texto de uma resposta rápida.
pub const MAX_REPLY_CHARS: usize = 4096;
/// Espera o site carregar (conta que estava dormindo).
const LOAD_TIMEOUT: Duration = Duration::from_secs(60);
/// Prazo da página para abrir a conversa e enviar, contado desde que o
/// pedido entra na fila dela; depois disso ela não escreve nem envia.
const PAGE_DEADLINE_MS: u64 = 30_000;
/// Espera do Papeo: maior que o prazo da página, para ler o resultado final.
const SEND_TIMEOUT: Duration = Duration::from_secs(40);
const POLL: Duration = Duration::from_millis(400);

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn clean_chat(chat: &str) -> String {
    let chat: String = chat.chars().filter(|c| !c.is_control()).collect();
    papeo_core::clip(chat.trim(), 100)
}

/// Normaliza o texto: quebras de linha `\n`, sem outros caracteres de
/// controle, sem espaços nas pontas.
fn clean_text(text: &str) -> Result<String, String> {
    let text: String = text
        .replace("\r\n", "\n")
        .chars()
        .filter(|c| *c == '\n' || !c.is_control())
        .collect();
    let text = text.trim();
    if text.is_empty() {
        return Err("Escreva a mensagem.".into());
    }
    if text.chars().count() > MAX_REPLY_CHARS {
        return Err(format!(
            "Mensagem longa demais (máximo {MAX_REPLY_CHARS} caracteres)."
        ));
    }
    Ok(text.to_owned())
}

fn failure(code: &str) -> String {
    match code {
        "chat" => "Não encontrei essa conversa. Abra a conversa para responder.",
        "ambiguous" => "Há mais de uma conversa com esse nome. Abra a conversa para responder.",
        "draft" => "Há um rascunho nessa conversa. Abra a conversa para terminar.",
        "unsupported" => "Esse serviço não aceita resposta rápida.",
        "timeout" => "O site demorou e a mensagem não foi enviada. Tente de novo.",
        "send" => "O site não aceitou a mensagem; ela não foi enviada.",
        // Não dá para saber se saiu: reenviar poderia duplicar.
        "unsure" => "Não consegui confirmar o envio. Confira a conversa antes de mandar de novo.",
        _ => "A mensagem não foi enviada. Abra a conversa para conferir.",
    }
    .to_owned()
}

/// Lê o andamento do envio na página.
fn status(webview: &Webview, id: u64) -> Option<String> {
    let (tx, rx) = mpsc::channel();
    webview
        .eval_with_callback(
            format!("window.__papeoReplyStatus ? window.__papeoReplyStatus({id}) : 'unknown'"),
            move |result| {
                let _ = tx.send(result);
            },
        )
        .ok()?;
    let raw = rx.recv_timeout(Duration::from_secs(5)).ok()?;
    serde_json::from_str::<String>(&raw).ok()
}

/// Desiste: pede à página para não escrever nem enviar e lê o resultado
/// final. Se ainda não há resultado, o envio é incerto.
fn give_up(webview: &Webview, id: u64, started: bool) -> String {
    let _ = webview.eval(format!(
        "window.__papeoReplyCancel && window.__papeoReplyCancel({id})"
    ));
    std::thread::sleep(Duration::from_millis(1500));
    match status(webview, id).as_deref() {
        Some("pending") => "unsure".to_owned(),
        // A página recarregou: se o pedido já estava andando, pode ter saído.
        Some("unknown") if started => "unsure".to_owned(),
        Some("unknown") => "error".to_owned(),
        Some(other) => other.to_owned(),
        None => "unsure".to_owned(),
    }
}

/// Carrega a conta escondida, se estiver dormindo, e espera o site ficar pronto.
fn ensure_loaded(app: &AppHandle, id: &str) -> Result<Webview, String> {
    let state = app.state::<AppState>();
    let (service, area) = {
        let mut inner = state.lock();
        let service = inner.service(id)?.clone();
        // Conta como uso: a hibernação automática espera.
        inner.last_used.insert(id.to_owned(), Instant::now());
        (service, inner.area)
    };
    if app.get_webview(id).is_none() {
        // Sem o lock: add_child espera a thread principal.
        crate::webviews::create_service_webview(app, &service, area)?;
        crate::webviews::apply_layout(app);
    }
    let start = Instant::now();
    loop {
        let ready = state
            .lock()
            .health
            .get(id)
            .is_some_and(|h| h.last_beat.is_some());
        if ready {
            break;
        }
        if start.elapsed() > LOAD_TIMEOUT {
            return Err("A conta ainda está carregando. Tente de novo em instantes.".into());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    app.get_webview(id)
        .ok_or_else(|| "A conta foi fechada.".to_owned())
}

/// Envia `text` para a conversa `chat` da conta `id`. Bloqueia até o site
/// confirmar (rode fora da thread principal).
pub fn send(app: &AppHandle, id: &str, chat: &str, text: &str) -> Result<Snapshot, String> {
    let chat = clean_chat(chat);
    if chat.is_empty() {
        return Err("Conversa inválida.".into());
    }
    let text = clean_text(text)?;
    {
        let state = app.state::<AppState>();
        let inner = state.lock();
        if inner.locked {
            return Err("Desbloqueie o Papeo primeiro.".into());
        }
        if !inner.service(id)?.kind.supports_chat() {
            return Err(failure("unsupported"));
        }
    }
    let webview = ensure_loaded(app, id)?;
    // Só no próprio site do serviço (outra página aberta nele não recebe o
    // texto), e só se o app não foi bloqueado enquanto a conta carregava.
    let page = webview.url().map(|u| u.to_string()).unwrap_or_default();
    {
        let state = app.state::<AppState>();
        let inner = state.lock();
        if inner.locked {
            return Err("Desbloqueie o Papeo primeiro.".into());
        }
        if !papeo_core::same_site(&inner.service(id)?.url, &page) {
            return Err(failure("chat"));
        }
    }

    let reply_id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let chat_js = serde_json::to_string(&chat).map_err(|e| e.to_string())?;
    let text_js = serde_json::to_string(&text).map_err(|e| e.to_string())?;
    webview
        .eval(format!(
            "window.__papeoReply && window.__papeoReply({reply_id}, {chat_js}, {text_js}, {PAGE_DEADLINE_MS})"
        ))
        .map_err(|e| e.to_string())?;

    let start = Instant::now();
    let mut unknown = 0;
    let mut started = false;
    let result = loop {
        std::thread::sleep(POLL);
        if start.elapsed() > SEND_TIMEOUT {
            break give_up(&webview, reply_id, started);
        }
        match status(&webview, reply_id).as_deref() {
            Some("pending") => started = true,
            // A página recarregou ou ainda não registrou o pedido.
            Some("unknown") | None => {
                unknown += 1;
                if unknown >= 8 {
                    break give_up(&webview, reply_id, started);
                }
            }
            Some(other) => break other.to_owned(),
        }
    };
    if result != "ok" {
        return Err(failure(&result));
    }

    {
        let state = app.state::<AppState>();
        state.lock().push_recent(
            id,
            &chat,
            ChatMessage {
                body: papeo_core::clip(&text, 240),
                at: now_secs(),
                mine: true,
            },
        );
    }
    Ok(publish(app))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_trimmed_and_limited() {
        assert_eq!(
            clean_text("  oi\r\ntudo\u{7} bem ").unwrap(),
            "oi\ntudo bem"
        );
        assert!(clean_text(" \n ").is_err());
        assert!(clean_text(&"a".repeat(MAX_REPLY_CHARS + 1)).is_err());
        assert_eq!(clean_chat(" Ana\u{0} "), "Ana");
    }
}
