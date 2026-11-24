//! Bloqueio do app com a autenticação do sistema: Touch ID ou senha do Mac,
//! Windows Hello (rosto, digital ou PIN). Enquanto bloqueado, nenhuma
//! webview de serviço aparece e as notificações não mostram conteúdo.
//! As funções aqui bloqueiam a thread: chame via `spawn_blocking`.

use std::time::Duration;
use tauri::AppHandle;

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
            .ok_or("Janela do Papeo indisponível.")?;
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
            _ => Err("Windows Hello indisponível ou não configurado.".into()),
        }
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod platform {
    use tauri::AppHandle;

    pub fn available() -> bool {
        false
    }

    pub fn authenticate(_app: &AppHandle, _reason: &str) -> Result<bool, String> {
        Err("Bloqueio indisponível neste sistema.".into())
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
    use tauri::Manager;
    let state = app.state::<crate::state::AppState>();
    {
        let mut inner = state.lock();
        let settings = &inner.config.settings;
        if !settings.lock.enabled || !inner.lock_available || inner.locked {
            return;
        }
        inner.locked = true;
    }
    crate::webviews::apply_layout(app);
    crate::state::publish(app);
}

/// Confere, fora da thread principal, se o sistema tem autenticação. Sem
/// ela, o app não fica preso na tela de bloqueio.
pub fn check_availability(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let ok = tauri::async_runtime::spawn_blocking(available)
            .await
            .unwrap_or(false);
        {
            use tauri::Manager;
            let state = app.state::<crate::state::AppState>();
            let mut inner = state.lock();
            inner.lock_available = ok;
            if !ok {
                inner.locked = false;
            }
        }
        crate::webviews::apply_layout(&app);
        crate::state::publish(&app);
    });
}

/// Pede a autenticação sem travar a thread do comando. Só um pedido por vez.
pub async fn confirm_identity(app: &AppHandle, reason: &'static str) -> Result<(), String> {
    use tauri::Manager;
    let state = app.state::<crate::state::AppState>();
    {
        let mut inner = state.lock();
        if inner.authenticating {
            return Err("Já há um pedido de autenticação aberto.".into());
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
        Ok(false) => Err("Autenticação cancelada.".into()),
        Err(e) => {
            // O sistema pode ter perdido a autenticação (Hello removido):
            // reavalia para não deixar a pessoa presa na tela de bloqueio.
            check_availability(app);
            Err(format!("Não foi possível autenticar: {e}"))
        }
    }
}
