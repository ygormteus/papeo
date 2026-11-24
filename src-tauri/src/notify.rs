//! Comandos chamados pelos sites dos serviços (via `notification_shim.js`).
//! O serviço é sempre identificado pelo rótulo da webview que chamou.

use crate::state::{
    local_minute_of_day, now_secs, publish, status_of, AppState, RecentMessage, MAIN_WINDOW,
};
use papeo_core::NotificationPreview;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State, Webview};
use tauri_plugin_notification::NotificationExt;

/// Intervalo mínimo entre notificações do mesmo serviço (contra excesso de alertas).
const NOTIFY_MIN_GAP: Duration = Duration::from_millis(1500);
/// Voltar ao app até este tempo depois de uma notificação abre aquele serviço.
pub const FOCUS_AFTER_NOTIFICATION: Duration = Duration::from_secs(20);

/// Som do sistema: nome de som do macOS ou variante do toast do Windows.
#[cfg(target_os = "macos")]
const DEFAULT_SOUND: &str = "Glass";
#[cfg(not(target_os = "macos"))]
const DEFAULT_SOUND: &str = "Default";

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
    let (name, preview) = {
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
        inner.push_recent(RecentMessage {
            service_id: id.clone(),
            title: papeo_core::clip(&title, 80),
            body: papeo_core::clip(&body, 240),
            at: now_unix,
        });
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
        (name, preview)
    };

    publish(&app);
    let (heading, text) = papeo_core::notification_text(preview, &name, &title, &body);
    let mut notification = app.notification().builder().title(heading).body(text);
    if !silent.unwrap_or(false) {
        notification = notification.sound(DEFAULT_SOUND);
    }
    notification.show().map_err(|e| e.to_string())
}

/// Sinal de vida periódico do site, com a detecção de "deslogado".
#[tauri::command]
pub async fn service_status(
    app: AppHandle,
    webview: Webview,
    state: State<'_, AppState>,
    logged_out: bool,
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
        // A página está pronta: abre a conversa que esperava por ela.
        let chat = own_site.then(|| inner.pending_chat.remove(&id)).flatten();
        (before != status_of(&inner, &id, true), chat)
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
