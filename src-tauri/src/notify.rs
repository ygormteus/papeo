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
    // Lido antes do lock: `is_focused` espera a thread principal, que também
    // pega o lock (callback de título) — segurá-lo aqui causaria deadlock.
    let focused = app
        .get_window(MAIN_WINDOW)
        .and_then(|w| w.is_focused().ok())
        .unwrap_or(false);
    // Também fora do lock: a URL vem da thread principal.
    let page = webview.url().map(|u| u.to_string()).unwrap_or_default();
    let (now_unix, minute) = (now_secs(), local_minute_of_day());
    let (name, preview, target) = {
        let mut inner = state.lock();
        let Ok(service) = inner.service(&id) else {
            return Ok(());
        };
        // Outra página aberta dentro do serviço não fala em nome dele.
        if !papeo_core::same_site(&service.url, &page) {
            return Ok(());
        }
        if !papeo_core::should_notify(&inner.config.settings, service, now_unix, minute) {
            return Ok(());
        }
        let name = service.name.clone();
        let chat = papeo_core::clip(&title, 80);
        // Clique abre a conversa; "Responder" só com o nome dela à mostra
        // e num mensageiro conhecido.
        let target = crate::notifications::Target {
            service_id: id.clone(),
            reply: service.kind.supports_chat()
                && !inner.locked
                && inner.config.settings.notification_preview != NotificationPreview::Hidden,
            chat: (!chat.is_empty()).then(|| chat.clone()),
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
                body: papeo_core::clip(&body, 240),
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
            publish(&app);
            return Ok(());
        }
        inner.last_notified.insert(id.clone(), now);
        if !focused {
            inner.last_notification = Some((id, now));
        }
        (name, preview, target)
    };

    publish(&app);
    let (heading, text) = papeo_core::notification_text(preview, &name, &title, &body);
    let sound = (!silent.unwrap_or(false)).then_some(DEFAULT_SOUND);
    crate::notifications::show(&app, &heading, &text, sound, target)
}

/// Sinal de vida periódico do site, com a detecção de "deslogado".
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
