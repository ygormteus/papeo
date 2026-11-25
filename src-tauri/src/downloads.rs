//! Downloads feitos pelos sites: pasta escolhida, nome seguro, aviso ao terminar.

use crate::state::{
    now_secs, publish, AppState, DownloadEntry, DownloadState, DOWNLOAD_EVENT, UI_WEBVIEW,
};
use serde::Serialize;
use std::path::PathBuf;
use tauri::webview::DownloadEvent;
use tauri::{AppHandle, Emitter, Manager, Webview};
use tauri_plugin_notification::NotificationExt;

/// Pasta configurada ou, na falta dela, a pasta Downloads do sistema.
pub fn downloads_dir(app: &AppHandle) -> PathBuf {
    let configured = app
        .state::<AppState>()
        .lock()
        .config
        .settings
        .downloads_dir
        .clone();
    configured
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .or_else(|| app.path().download_dir().ok())
        .unwrap_or_else(std::env::temp_dir)
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DownloadNotice {
    id: u64,
    file_name: String,
    success: bool,
}

/// Callback `on_download` das webviews dos serviços (roda na thread principal).
pub fn handle(webview: &Webview, event: DownloadEvent<'_>) -> bool {
    let app = webview.app_handle().clone();
    match event {
        DownloadEvent::Requested { url, destination } => {
            let suggested = destination
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .or_else(|| url.path_segments()?.next_back().map(str::to_owned))
                .unwrap_or_default();
            let file_name = papeo_core::safe_file_name(&suggested);
            // Anexo para visualizar: vai para a pasta temporária, fora da lista.
            if let Some(path) =
                crate::attachment::intercept(&app, webview.label(), url.as_str(), &file_name)
            {
                *destination = path;
                return true;
            }
            let dir = downloads_dir(&app);
            let state = app.state::<AppState>();
            let mut inner = state.lock();
            let pending: Vec<_> = inner
                .downloads
                .iter()
                .filter(|d| d.state == DownloadState::Downloading)
                .map(|d| d.path.clone())
                .collect();
            let path = papeo_core::unique_path(&dir, &file_name, &pending);
            *destination = path.clone();
            inner.push_download(DownloadEntry {
                id: 0,
                service_id: webview.label().to_owned(),
                file_name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or(file_name),
                url: url.to_string(),
                path,
                state: DownloadState::Downloading,
                at: now_secs(),
            });
            drop(inner);
            publish(&app);
            true
        }
        DownloadEvent::Finished { url, path, success } => {
            if crate::attachment::finished(
                &app,
                webview.label(),
                url.as_str(),
                path.as_deref(),
                success,
            ) {
                return true;
            }
            let state = app.state::<AppState>();
            let finished = {
                let mut inner = state.lock();
                let url = url.to_string();
                let service_id = webview.label();
                inner
                    .downloads
                    .iter_mut()
                    .find(|d| {
                        d.state == DownloadState::Downloading
                            && d.service_id == service_id
                            && (d.url == url || path.as_ref() == Some(&d.path))
                    })
                    .map(|entry| {
                        entry.state = if success {
                            DownloadState::Done
                        } else {
                            DownloadState::Failed
                        };
                        if let Some(p) = &path {
                            entry.path = p.clone();
                        }
                        (entry.id, entry.file_name.clone())
                    })
            };
            if let Some((id, file_name)) = finished {
                let _ = app.emit_to(
                    UI_WEBVIEW,
                    DOWNLOAD_EVENT,
                    DownloadNotice {
                        id,
                        file_name: file_name.clone(),
                        success,
                    },
                );
                // O nome do arquivo só aparece com a prévia completa e desbloqueado.
                let show_name = {
                    let inner = state.lock();
                    !inner.locked
                        && inner.config.settings.notification_preview
                            == papeo_core::NotificationPreview::Full
                };
                let body = if show_name {
                    file_name
                } else if success {
                    "Arquivo salvo na pasta de downloads.".to_owned()
                } else {
                    "Não foi possível baixar o arquivo.".to_owned()
                };
                let title = if success {
                    "Download concluído"
                } else {
                    "Download falhou"
                };
                let _ = app.notification().builder().title(title).body(body).show();
            }
            publish(&app);
            true
        }
        _ => true,
    }
}

/// Caminho de um download conhecido (a interface nunca envia caminhos).
pub fn path_of(app: &AppHandle, id: u64) -> Result<PathBuf, String> {
    app.state::<AppState>()
        .lock()
        .downloads
        .iter()
        .find(|d| d.id == id)
        .map(|d| d.path.clone())
        .ok_or_else(|| "Download não encontrado.".to_owned())
}
