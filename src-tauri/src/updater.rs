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

/// No Mac, app aberto de um lugar que não pode ser trocado: a cópia
/// temporária do Gatekeeper (app ainda marcado como baixado da internet) ou
/// o próprio `.dmg`.
fn blocked_location(exe: &str) -> Option<&'static str> {
    if exe.contains("/AppTranslocation/") {
        Some("O Mac está abrindo o Papeo de uma cópia temporária, que não pode ser atualizada. Feche o Papeo (ícone na barra de menus → Sair) e abra de novo: ele já se corrigiu. Se o aviso voltar, rode no Terminal: xattr -dr com.apple.quarantine /Applications/Papeo.app")
    } else if exe.starts_with("/Volumes/") {
        Some("O Papeo está aberto direto do instalador. Arraste-o para a pasta Aplicativos, abra de lá e atualize de novo.")
    } else {
        None
    }
}

/// `.app` original do executável atual. Aberto da cópia temporária do
/// Gatekeeper, pergunta ao macOS onde está o original; aberto do `.dmg`, nada.
#[cfg(target_os = "macos")]
fn app_bundle() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // Papeo.app/Contents/MacOS/papeo
    let bundle = exe.parent()?.parent()?.parent()?.to_path_buf();
    if bundle.extension()? != "app" {
        return None;
    }
    let bundle = if exe.to_string_lossy().contains("/AppTranslocation/") {
        original_of_translocated(&bundle)?
    } else {
        bundle
    };
    (bundle.extension()? == "app" && !bundle.starts_with("/Volumes/")).then_some(bundle)
}

/// Caminho original de um app na cópia temporária (`AppTranslocation`), pela
/// função do Security.framework que o próprio macOS oferece para isso
/// (`SecTranslocateCreateOriginalPathForURL`, macOS 10.12+). Procurada em
/// tempo de execução: se não existir, nada muda.
#[cfg(target_os = "macos")]
fn original_of_translocated(bundle: &std::path::Path) -> Option<std::path::PathBuf> {
    use objc2::rc::Retained;
    use objc2_foundation::{NSString, NSURL};
    use std::ffi::c_void;
    type Original = unsafe extern "C" fn(*const c_void, *mut *const c_void) -> *mut c_void;
    // SAFETY: dlopen/dlsym com textos terminados em zero; o símbolo, se
    // existir, tem a assinatura acima (CFURLRef, CFErrorRef*) -> CFURLRef, e
    // CFURL/NSURL são intercambiáveis. O retorno segue a regra "Create" (é
    // nosso), então vira um `Retained` sem retain extra.
    unsafe {
        let lib = libc::dlopen(
            c"/System/Library/Frameworks/Security.framework/Security".as_ptr(),
            libc::RTLD_LAZY,
        );
        if lib.is_null() {
            return None;
        }
        let sym = libc::dlsym(lib, c"SecTranslocateCreateOriginalPathForURL".as_ptr());
        if sym.is_null() {
            return None;
        }
        let original: Original = std::mem::transmute(sym);
        let url = NSURL::fileURLWithPath(&NSString::from_str(&bundle.to_string_lossy()));
        let out = original(Retained::as_ptr(&url).cast(), std::ptr::null_mut());
        let out: Retained<NSURL> = Retained::from_raw(out.cast())?;
        Some(std::path::PathBuf::from(out.path()?.to_string()))
    }
}

/// Tira do app a marca de "baixado da internet". O Papeo instalado com ela
/// grava a versão nova também marcada, e o Mac passa a abri-lo de uma cópia
/// temporária, só de leitura, onde a próxima atualização falha.
pub fn clear_quarantine() {
    #[cfg(target_os = "macos")]
    if let Some(bundle) = app_bundle() {
        let _ = std::process::Command::new("/usr/bin/xattr")
            .args(["-dr", "com.apple.quarantine"])
            .arg(&bundle)
            .status();
    }
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
    if let Some(message) = std::env::current_exe()
        .ok()
        .and_then(|exe| blocked_location(&exe.to_string_lossy()))
    {
        return fail(message.to_owned());
    }
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
        Ok(()) => {
            clear_quarantine();
            app.restart()
        }
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

#[cfg(test)]
mod tests {
    use super::blocked_location;

    #[test]
    fn spots_places_that_cannot_be_updated() {
        assert!(blocked_location(
            "/private/var/folders/3v/x/T/AppTranslocation/F1/d/Papeo.app/Contents/MacOS/papeo"
        )
        .is_some());
        assert!(blocked_location("/Volumes/Papeo/Papeo.app/Contents/MacOS/papeo").is_some());
        assert!(blocked_location("/Applications/Papeo.app/Contents/MacOS/papeo").is_none());
        assert!(blocked_location("C:\\Users\\a\\AppData\\Local\\Papeo\\papeo.exe").is_none());
    }
}
