//! Atualizações do app pelo `tauri-plugin-updater`.
//!
//! A verificação consulta o `latest.json` do último release no GitHub e só
//! roda sozinha depois que a pessoa viu o aviso (`UpdatePrefs::auto_check`).
//! O pacote baixado só é instalado se a assinatura bater com a chave pública
//! em `tauri.conf.json`; a instalação sempre parte de um clique.
//!
//! Nunca segura o lock do estado durante a rede nem ao publicar.

use crate::state::{now_secs, publish, AppState, UpdateStatus};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;

/// Primeira verificação automática após abrir o app.
const FIRST_CHECK_AFTER: Duration = Duration::from_secs(60);
/// Intervalo entre verificações automáticas (relógio de parede: conta o
/// tempo com o computador dormindo, então ao acordar já confere). Cada
/// consulta baixa só o `latest.json` (uns 2 KB).
const CHECK_EVERY_SECS: u64 = 2 * 60 * 60;
/// Nova tentativa depois de uma falha (ex.: rede ainda fora ao abrir).
const RETRY_AFTER_SECS: u64 = 60 * 60;
/// Limite da consulta ao `latest.json`.
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
/// Limite do download inteiro (o timeout vale para a requisição toda).
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);

fn set(app: &AppHandle, status: UpdateStatus) {
    app.state::<AppState>().lock().update = status;
    publish(app);
}

/// Marca o início de uma operação; `false` se outra já está em andamento.
fn begin(app: &AppHandle, status: UpdateStatus) -> bool {
    {
        let state = app.state::<AppState>();
        let mut inner = state.lock();
        if inner.update.busy() {
            return false;
        }
        inner.update = status;
        inner.update_checked = Some(now_secs());
    }
    publish(app);
    true
}

async fn find(app: &AppHandle) -> Result<Option<tauri_plugin_updater::Update>, String> {
    app.updater_builder()
        .timeout(CHECK_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| format!("Não foi possível verificar atualizações: {e}"))
}

/// Consulta se há versão nova e publica o resultado.
pub async fn check(app: &AppHandle) -> Result<(), String> {
    if !begin(app, UpdateStatus::Checking) {
        return Ok(());
    }
    match find(app).await {
        Ok(Some(update)) => set(
            app,
            UpdateStatus::Available {
                version: update.version.clone(),
                notes: update.body.clone(),
            },
        ),
        Ok(None) => set(app, UpdateStatus::UpToDate),
        Err(message) => {
            set(
                app,
                UpdateStatus::Failed {
                    message: message.clone(),
                },
            );
            return Err(message);
        }
    }
    Ok(())
}

/// Baixa, confere a assinatura, instala e reinicia o app.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    if !begin(app, UpdateStatus::Installing { percent: None }) {
        return Ok(());
    }
    let fail = |message: String| {
        set(
            app,
            UpdateStatus::Failed {
                message: message.clone(),
            },
        );
        Err(message)
    };
    let mut update = match find(app).await {
        Ok(Some(update)) => update,
        Ok(None) => {
            set(app, UpdateStatus::UpToDate);
            return Ok(());
        }
        Err(message) => return fail(message),
    };
    update.timeout = Some(DOWNLOAD_TIMEOUT);
    let progress_app = app.clone();
    let mut received: u64 = 0;
    let mut last = None;
    let result = update
        .download_and_install(
            move |chunk, total| {
                received += chunk as u64;
                let percent = total
                    .filter(|t| *t > 0)
                    .map(|t| (received.saturating_mul(100) / t).min(100) as u8);
                if percent != last {
                    last = percent;
                    set(&progress_app, UpdateStatus::Installing { percent });
                }
            },
            || {},
        )
        .await;
    match result {
        Ok(()) => app.restart(),
        Err(e) => fail(format!("Não foi possível instalar a atualização: {e}")),
    }
}

/// Chamado pela manutenção periódica: dispara a verificação automática
/// quando ligada e vencida.
pub fn tick(app: &AppHandle, started: Instant) {
    let due = {
        let state = app.state::<AppState>();
        let inner = state.lock();
        // Com versão nova já avisada, não consulta de novo: uma falha de rede
        // (comum ao acordar o computador) trocaria o aviso por um erro e
        // esconderia o botão de atualizar.
        inner.config.settings.updates.auto_check()
            && !inner.update.busy()
            && !matches!(inner.update, UpdateStatus::Available { .. })
            && match inner.update_checked {
                Some(at) => {
                    let wait = if matches!(inner.update, UpdateStatus::Failed { .. }) {
                        RETRY_AFTER_SECS
                    } else {
                        CHECK_EVERY_SECS
                    };
                    now_secs().saturating_sub(at) >= wait
                }
                None => started.elapsed() >= FIRST_CHECK_AFTER,
            }
    };
    if due {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let _ = check(&app).await;
        });
    }
}
