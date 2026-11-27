//! Bloqueio do app com a autenticação do sistema: Touch ID ou senha do Mac,
//! Windows Hello (rosto, digital ou PIN). Sem ela (ou por escolha), um PIN
//! do próprio Papeo. Enquanto bloqueado, nenhuma webview de serviço aparece
//! e as notificações não mostram conteúdo.
//! `available`/`authenticate` bloqueiam a thread: chame via `spawn_blocking`.

use crate::state::{AppState, LockMethod};
use papeo_core::i18n::{tk, tr, trf};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// Tempo máximo esperando a pessoa responder ao pedido do sistema.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
const AUTH_TIMEOUT: Duration = Duration::from_secs(180);

#[cfg(target_os = "macos")]
mod platform {
    use super::AUTH_TIMEOUT;
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSError, NSString};
    use objc2_local_authentication::{LAContext, LAPolicy};
    use tauri::AppHandle;

    pub fn available() -> bool {
        let context = unsafe { LAContext::new() };
        unsafe { context.canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthentication) }.is_ok()
    }

    pub fn authenticate(_app: &AppHandle, reason: &str) -> Result<bool, String> {
        let context = unsafe { LAContext::new() };
        let (tx, rx) = std::sync::mpsc::channel::<bool>();
        let reply = RcBlock::new(move |ok: Bool, _error: *mut NSError| {
            let _ = tx.send(ok.as_bool());
        });
        unsafe {
            context.evaluatePolicy_localizedReason_reply(
                LAPolicy::DeviceOwnerAuthentication,
                &NSString::from_str(reason),
                &reply,
            );
        }
        let result = rx.recv_timeout(AUTH_TIMEOUT).unwrap_or(false);
        unsafe { context.invalidate() };
        Ok(result)
    }
}

#[cfg(windows)]
mod platform {
    use super::AUTH_TIMEOUT;
    use crate::state::MAIN_WINDOW;
    use papeo_core::i18n::tr;
    use tauri::{AppHandle, Manager};
    use windows::core::{factory, Interface, HSTRING};
    use windows::Security::Credentials::UI::{
        UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
    };
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::WinRT::IUserConsentVerifierInterop;
    use windows_future::{IAsyncInfo, IAsyncOperation};

    pub fn available() -> bool {
        UserConsentVerifier::CheckAvailabilityAsync()
            .and_then(|op| op.join())
            .is_ok_and(|a| a == UserConsentVerifierAvailability::Available)
    }

    pub fn authenticate(app: &AppHandle, reason: &str) -> Result<bool, String> {
        let hwnd = app
            .get_window(MAIN_WINDOW)
            .and_then(|w| w.hwnd().ok())
            .map(|h| HWND(h.0))
            .ok_or(tr("Janela do Papeo indisponível."))?;
        let interop = factory::<UserConsentVerifier, IUserConsentVerifierInterop>()
            .map_err(|e| e.to_string())?;
        // Versão "ForWindow": o pedido do Windows Hello aparece sobre o Papeo.
        let op: IAsyncOperation<UserConsentVerificationResult> =
            unsafe { interop.RequestVerificationForWindowAsync(hwnd, &HSTRING::from(reason)) }
                .map_err(|e| e.to_string())?;
        // Espera com prazo: sem resposta, cancela o pedido e libera o botão.
        let (tx, rx) = std::sync::mpsc::channel();
        let waiter = op.clone();
        std::thread::spawn(move || {
            let _ = tx.send(waiter.join());
        });
        let result = match rx.recv_timeout(AUTH_TIMEOUT) {
            Ok(result) => result.map_err(|e| e.to_string())?,
            Err(_) => {
                if let Ok(info) = op.cast::<IAsyncInfo>() {
                    let _ = info.Cancel();
                }
                return Ok(false);
            }
        };
        match result {
            UserConsentVerificationResult::Verified => Ok(true),
            UserConsentVerificationResult::Canceled
            | UserConsentVerificationResult::RetriesExhausted => Ok(false),
            _ => Err(tr("Windows Hello indisponível ou não configurado.").into()),
        }
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod platform {
    use papeo_core::i18n::tr;
    use tauri::AppHandle;

    pub fn available() -> bool {
        false
    }

    pub fn authenticate(_app: &AppHandle, _reason: &str) -> Result<bool, String> {
        Err(tr("Bloqueio indisponível neste sistema.").into())
    }
}

/// O sistema tem como confirmar a identidade (Touch ID/senha, Windows Hello)?
pub fn available() -> bool {
    platform::available()
}

/// Pede a autenticação do sistema. `Ok(false)` = cancelada ou recusada.
pub fn authenticate(app: &AppHandle, reason: &str) -> Result<bool, String> {
    platform::authenticate(app, reason)
}

/// Bloqueia agora (se o bloqueio estiver ligado e disponível).
pub fn lock_app(app: &AppHandle) {
    let state = app.state::<AppState>();
    {
        let mut inner = state.lock();
        let settings = &inner.config.settings;
        if !settings.lock.enabled || inner.lock_method() == LockMethod::None || inner.locked {
            return;
        }
        inner.locked = true;
    }
    // Bloqueado: nenhum conteúdo fica à vista, nem a prévia de um link.
    crate::preview::close(app);
    crate::attachment::close_for_lock(app);
    crate::webviews::apply_layout(app);
    crate::state::publish(app);
}

/// Confere, fora da thread principal, se o sistema tem autenticação. Sem
/// ela (e sem PIN do Papeo), o app não fica preso na tela de bloqueio.
pub fn check_availability(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let ok = tauri::async_runtime::spawn_blocking(available)
            .await
            .unwrap_or(false);
        {
            let state = app.state::<AppState>();
            let mut inner = state.lock();
            inner.system_auth = ok;
            if inner.lock_method() == LockMethod::None {
                inner.locked = false;
            }
        }
        crate::webviews::apply_layout(&app);
        crate::state::publish(&app);
    });
}

/// Pede a autenticação sem travar a thread do comando. Só um pedido por vez.
pub async fn confirm_identity(app: &AppHandle, reason: &'static str) -> Result<(), String> {
    let state = app.state::<AppState>();
    {
        let mut inner = state.lock();
        if inner.authenticating {
            return Err(tr("Já há um pedido de autenticação aberto.").into());
        }
        inner.authenticating = true;
    }
    let handle = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || authenticate(&handle, reason))
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
    state.lock().authenticating = false;
    match result {
        Ok(true) => Ok(()),
        Ok(false) => Err(tr("Autenticação cancelada.").into()),
        Err(e) => {
            // O sistema pode ter perdido a autenticação (Hello removido):
            // reavalia para não deixar a pessoa presa na tela de bloqueio.
            check_availability(app);
            Err(trf("Não foi possível autenticar: {erro}", &[("erro", &e)]))
        }
    }
}

/// Confirma a identidade pelo método em uso: PIN do Papeo (`pin`) ou
/// autenticação do sistema (`reason` aparece no pedido do sistema).
pub async fn confirm(
    app: &AppHandle,
    reason: &'static str,
    pin: Option<String>,
) -> Result<(), String> {
    let method = app.state::<AppState>().lock().lock_method();
    match method {
        LockMethod::Pin => verify_pin(app, pin).await,
        LockMethod::System => confirm_identity(app, reason).await,
        LockMethod::None => Err(tr(NO_METHOD).into()),
    }
}

pub const NO_METHOD: &str = tk(
    "Este computador não tem Touch ID, senha do sistema nem Windows Hello. Crie um PIN do Papeo.",
);

fn wait_text(secs: u64) -> String {
    if secs < 60 {
        trf("{n} s", &[("n", &secs)])
    } else {
        trf("{n} min", &[("n", &secs.div_ceil(60))])
    }
}

/// Confere o PIN do Papeo. Depois de alguns erros seguidos, só aceita outra
/// tentativa após uma espera crescente (que sobrevive a reabrir o app).
pub async fn verify_pin(app: &AppHandle, pin: Option<String>) -> Result<(), String> {
    let pin = pin.filter(|p| !p.is_empty()).ok_or(tr("Digite o PIN."))?;
    let state = app.state::<AppState>();
    let stored = {
        let mut inner = state.lock();
        if inner.authenticating {
            return Err(tr("Já há um pedido de autenticação aberto.").into());
        }
        let wait = inner
            .pin_retry_at
            .map(|t| t.saturating_duration_since(Instant::now()))
            .unwrap_or_default();
        if !wait.is_zero() {
            return Err(trf(
                "Muitas tentativas. Tente de novo em {tempo}.",
                &[("tempo", &wait_text(wait.as_secs() + 1))],
            ));
        }
        let stored = inner
            .config
            .lock_pin
            .clone()
            .ok_or(tr("Nenhum PIN definido."))?;
        inner.authenticating = true;
        stored
    };
    let hash = stored.hash.clone();
    // Argon2id leva dezenas de ms: fora da thread do comando e sem o lock.
    let ok = tauri::async_runtime::spawn_blocking(move || stored.verify(&pin))
        .await
        .unwrap_or(false);
    let (staged, wait) = {
        let mut inner = state.lock();
        inner.authenticating = false;
        let mut wait = 0;
        let changed = match inner.config.lock_pin.as_mut().filter(|p| p.hash == hash) {
            // PIN trocado ou removido durante a conferência: não conta.
            None => return Err(tr("O PIN mudou. Tente de novo.").into()),
            Some(current) if ok => std::mem::take(&mut current.failures) > 0,
            Some(current) => {
                current.failures = current.failures.saturating_add(1);
                wait = papeo_core::pin_wait_secs(current.failures);
                true
            }
        };
        inner.pin_retry_at = (wait > 0).then(|| Instant::now() + Duration::from_secs(wait));
        let staged = if changed {
            Some(state.stage(&inner)?)
        } else {
            None
        };
        (staged, wait)
    };
    if let Some(staged) = staged {
        // Sem gravar a contagem, fechar e abrir o app zeraria a espera.
        state.write(staged)?;
    }
    if ok {
        return Ok(());
    }
    crate::state::publish(app);
    Err(if wait > 0 {
        trf(
            "PIN incorreto. Tente de novo em {tempo}.",
            &[("tempo", &wait_text(wait))],
        )
    } else {
        tr("PIN incorreto.").into()
    })
}

/// "Esqueci o PIN": tira o PIN e desbloqueia, mas antes sai de todas as
/// contas (sessões novas; as antigas entram na fila de remoção gravada, e
/// um backup restaurado depois não volta a usá-las), apaga tarefas e
/// anotações deste computador e desliga a sincronização. Assim, quem não
/// sabe o PIN não vê nada do que estava protegido. Só com o app bloqueado.
pub async fn forget_pin(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    {
        let mut inner = state.lock();
        if !inner.locked || inner.lock_method() != LockMethod::Pin {
            return Err(tr("Não há PIN do Papeo para redefinir.").into());
        }
        if inner.authenticating {
            return Err(tr("Já há um pedido de autenticação aberto.").into());
        }
        // Nenhum desbloqueio corre junto enquanto a limpeza acontece.
        inner.authenticating = true;
    }
    let result = forget_pin_locked(app).await;
    state.lock().authenticating = false;
    let old = result?;
    crate::sessions::remove_later(app, old);
    crate::organizer::publish_organizer(app);
    Ok(())
}

async fn forget_pin_locked(app: &AppHandle) -> Result<Vec<String>, String> {
    let state = app.state::<AppState>();
    // Primeiro a sincronização: senão as tarefas voltariam da nuvem.
    crate::sync::disconnect(app, false).await?;
    crate::webviews::close_all(app);
    let mut inner = state.lock();
    // Tarefas primeiro: se não der para gravar, nada mudou. Gravadas antes
    // do PIN sair, uma falha depois deixa o app bloqueado, nunca aberto
    // com as tarefas.
    let old_org = std::mem::take(&mut inner.organizer);
    let old_error = inner.organizer_error.take();
    if let Err(err) = state.stage_organizer(&inner).and_then(|s| state.write(s)) {
        inner.organizer = old_org;
        inner.organizer_error = old_error;
        return Err(err);
    }
    let before = inner.config.clone();
    let old: Vec<String> = inner
        .config
        .services
        .iter_mut()
        .map(|s| std::mem::replace(&mut s.session, papeo_core::new_session()))
        .collect();
    // Na fila gravada junto com o PIN removido: mesmo que apagar demore,
    // nenhuma conta volta a usar essas sessões.
    for s in &old {
        if !inner.config.pending_removal.contains(s) {
            inner.config.pending_removal.push(s.clone());
        }
    }
    inner.config.open.clear();
    inner.config.lock_pin = None;
    inner.config.settings.lock.enabled = false;
    if let Err(err) = state.stage(&inner).and_then(|s| state.write(s)) {
        inner.config = before;
        return Err(err);
    }
    inner.pin_retry_at = None;
    inner.recent.clear();
    inner.unread.clear();
    inner.page_unread.clear();
    inner.current_chat.clear();
    inner.pending_chat.clear();
    inner.last_used.clear();
    inner.downloads.clear();
    inner.locked = false;
    inner.unfocused_since = None;
    Ok(old)
}
