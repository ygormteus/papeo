//! Comandos chamados pelos sites dos serviços (via `notification_shim.js`).
//! O serviço é sempre identificado pelo rótulo da webview que chamou.

use crate::state::{
    local_minute_of_day, now_secs, publish, status_of, AppState, ChatMessage, MAIN_WINDOW,
};
use papeo_core::NotificationPreview;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State, Webview};

/// Intervalo mínimo entre notificações do mesmo serviço (contra excesso de alertas).
const NOTIFY_MIN_GAP: Duration = Duration::from_millis(1500);
/// Voltar ao app até este tempo depois de uma notificação abre aquele serviço.
pub const FOCUS_AFTER_NOTIFICATION: Duration = Duration::from_secs(20);

/// Som do sistema: nome de som do macOS ou variante do toast do Windows.
#[cfg(target_os = "macos")]
pub const DEFAULT_SOUND: &str = "Glass";
#[cfg(not(target_os = "macos"))]
pub const DEFAULT_SOUND: &str = "Default";

#[tauri::command]
pub async fn service_notify(
    app: AppHandle,
    webview: Webview,
    state: State<'_, AppState>,
    title: String,
    body: String,
    silent: Option<bool>,
) -> Result<(), String> {
    let id = webview.label().to_owned();
    // Fora do lock: a URL vem da thread principal.
    let page = webview.url().map(|u| u.to_string()).unwrap_or_default();
    {
        let inner = state.lock();
        let Ok(service) = inner.service(&id) else {
            return Ok(());
        };
        // Outra página aberta dentro do serviço não fala em nome dele.
        if !papeo_core::same_site(&service.url, &page) {
            return Ok(());
        }
        // O IMAP já avisa deste e-mail: sem aviso em dobro.
        if inner.mail_live(&id) {
            return Ok(());
        }
    }
    deliver(&app, &id, &title, &body, None, silent.unwrap_or(false))
}

/// Mostra a notificação de uma conta (do site ou do IMAP), com as regras de
/// sempre: não perturbe, silenciada, modo, prévia escolhida, conta à vista.
/// `open`: o que buscar ao clicar (sem valor, o título).
pub fn deliver(
    app: &AppHandle,
    id: &str,
    title: &str,
    body: &str,
    open: Option<&str>,
    silent: bool,
) -> Result<(), String> {
    let id = id.to_owned();
    // Lido antes do lock: `is_focused` espera a thread principal, que também
    // pega o lock (callback de título) — segurá-lo aqui causaria deadlock.
    let focused = app
        .get_window(MAIN_WINDOW)
        .and_then(|w| w.is_focused().ok())
        .unwrap_or(false);
    let (now_unix, minute) = (now_secs(), local_minute_of_day());
    let state = app.state::<AppState>();
    let (name, preview, target) = {
        let mut inner = state.lock();
        let Ok(service) = inner.service(&id) else {
            return Ok(());
        };
        // Fora do modo, a conversa ainda entra nas recentes (o estado a
        // esconde até o modo voltar); só não notifica.
        let mode = inner.current_mode();
        let in_mode = mode.allows(service.context);
        if !papeo_core::should_notify(
            &inner.config.settings,
            service,
            now_unix,
            minute,
            papeo_core::Mode::All,
        ) {
            return Ok(());
        }
        let name = service.name.clone();
        let chat = papeo_core::clip(title, 80);
        let open = open
            .map(|o| papeo_core::clip(o, 80))
            .filter(|o| !o.is_empty())
            .unwrap_or_else(|| chat.clone());
        // Clique abre a conversa; "Responder" só com o nome dela à mostra
        // e num mensageiro conhecido.
        let shows_chat = !inner.locked
            && inner.config.settings.notification_preview != NotificationPreview::Hidden;
        let target = crate::notifications::Target {
            service_id: id.clone(),
            reply: service.kind.supports_chat() && shows_chat,
            // "Lembrar em 1 h": conversa de mensageiro ou assunto de e-mail.
            later: (service.kind.supports_chat() || service.kind.is_email()) && shows_chat,
            chat: (!open.is_empty()).then_some(open),
            task: None,
            remind_at: None,
        };
        // Bloqueado: nada do conteúdo aparece.
        let preview = if inner.locked {
            NotificationPreview::Hidden
        } else {
            inner.config.settings.notification_preview
        };
        if focused && !inner.locked && inner.active.as_deref() == Some(id.as_str()) {
            // Você já está olhando para este serviço.
            return Ok(());
        }
        // Barra de menus e busca (só na memória; a prévia é aplicada ao exibir).
        inner.push_recent(
            &id,
            &chat,
            ChatMessage {
                body: papeo_core::clip(body, 240),
                at: now_unix,
                mine: false,
            },
        );
        let now = Instant::now();
        if inner
            .last_notified
            .get(&id)
            .is_some_and(|t| now.duration_since(*t) < NOTIFY_MIN_GAP)
        {
            drop(inner);
            publish(app);
            return Ok(());
        }
        if !in_mode {
            drop(inner);
            publish(app);
            return Ok(());
        }
        inner.last_notified.insert(id.clone(), now);
        if !focused {
            inner.last_notification = Some((id, now));
        }
        (name, preview, target)
    };

    publish(app);
    let (heading, text) = papeo_core::notification_text(preview, &name, title, body);
    let sound = (!silent).then_some(DEFAULT_SOUND);
    crate::notifications::show(app, &heading, &text, sound, target)
}

/// Sinal de vida periódico do site, com a detecção de "deslogado".
// Cada argumento é um campo do IPC enviado pelo shim.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn service_status(
    app: AppHandle,
    webview: Webview,
    state: State<'_, AppState>,
    logged_out: bool,
    // Opcional: páginas abertas antes da atualização não enviam.
    media: Option<bool>,
    // Conversa aberta no site (nome no cabeçalho); vazio = nenhuma.
    chat: Option<String>,
    // Lista de conversas do site: (nome, não lidas) de cada linha visível.
    list: Option<Vec<(String, u32)>>,
    // Não lidas lidas na própria página (caixas de e-mail); sem valor, vale
    // o número do título.
    unread: Option<u32>,
) -> Result<(), String> {
    let id = webview.label().to_owned();
    let page = webview.url().map(|u| u.to_string()).unwrap_or_default();
    let (changed, chat) = {
        let mut inner = state.lock();
        let before = status_of(&inner, &id, true);
        // "Desconectado" só vale quando dito pelo próprio site do serviço.
        let own_site = inner
            .service(&id)
            .is_ok_and(|s| papeo_core::same_site(&s.url, &page));
        let Some(health) = inner.health.get_mut(&id) else {
            return Ok(());
        };
        health.last_beat = Some(Instant::now());
        health.logged_out = own_site && logged_out;
        // Vale mesmo fora do site do serviço: só evita recarregar.
        if media == Some(true) {
            health.media_at = health.last_beat;
        }
        // Conversa aberta (painel do contato). Só do próprio site.
        let mut chat_changed = false;
        if let Some(open) = chat.filter(|_| own_site) {
            let open = papeo_core::clean_chat(&open);
            let before = inner.current_chat.get(&id).cloned().unwrap_or_default();
            if before != open {
                chat_changed = true;
                if open.is_empty() {
                    inner.current_chat.remove(&id);
                } else {
                    inner.current_chat.insert(id.clone(), open);
                }
            }
        }
        // Conversas lidas no próprio site (ou no celular) saem das não lidas.
        if let Some(list) = list.filter(|_| own_site) {
            chat_changed |= inner.apply_chat_list(&id, &list);
        }
        // Contador da caixa de entrada informado pela página. Na tela de
        // login, o número antigo não vale mais.
        if own_site && logged_out && inner.page_unread.remove(&id) {
            chat_changed |= inner.unread.insert(id.clone(), 0) != Some(0);
        } else if let Some(count) = unread.filter(|_| own_site) {
            let count = count.min(crate::webviews::UNREAD_MAX);
            inner.page_unread.insert(id.clone());
            chat_changed |= inner.unread.insert(id.clone(), count) != Some(count);
        }
        // A página está pronta: abre a conversa que esperava por ela.
        let pending = own_site.then(|| inner.pending_chat.remove(&id)).flatten();
        (
            chat_changed || before != status_of(&inner, &id, true),
            pending,
        )
    };
    if let Some((query, exact)) = chat {
        crate::webviews::run_open_chat(&webview, &query, exact);
    }
    if changed {
        publish(&app);
    }
    Ok(())
}

/// A janela voltou ao foco: se foi logo após uma notificação, abre o serviço.
pub fn on_window_focus(app: &AppHandle, focused: bool) {
    let state = app.state::<AppState>();
    let target = {
        let mut inner = state.lock();
        inner.window_focused = focused;
        inner.unfocused_since = if focused { None } else { Some(Instant::now()) };
        if focused {
            if let Some(health) = inner
                .active
                .clone()
                .and_then(|id| inner.health.get_mut(&id))
            {
                health.grace();
            }
        }
        if !focused || !inner.config.settings.open_on_notification_focus {
            None
        } else {
            inner
                .last_notification
                .take()
                .filter(|(_, at)| at.elapsed() < FOCUS_AFTER_NOTIFICATION)
                .map(|(id, _)| id)
                .filter(|id| inner.active.as_deref() != Some(id.as_str()))
        }
    };
    let app = app.clone();
    // Fora da thread principal: ativar pode criar webview, que espera por ela.
    tauri::async_runtime::spawn(async move {
        match target {
            Some(id) => {
                let _ = crate::webviews::activate(&app, Some(id));
            }
            None => {
                publish(&app);
            }
        }
    });
}
