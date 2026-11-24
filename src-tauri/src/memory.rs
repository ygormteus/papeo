//! Memória usada por serviço.
//!
//! - macOS: cada WKWebView tem um processo "WebContent"; o pid vem de
//!   `_webProcessIdentifier` (API privada estável do WebKit).
//! - Windows: cada serviço tem pasta de dados própria, então um processo de
//!   navegador WebView2 próprio; soma-se a árvore de processos dele.
//!
//! Os números são aproximados (memória residente) e servem para comparar
//! serviços, não para medição exata.

use crate::state::AppState;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::mpsc;
use std::time::Duration;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tauri::{AppHandle, Manager};

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MemoryReport {
    /// Bytes por serviço carregado.
    pub services: HashMap<String, u64>,
    /// Processo principal do Papeo (interface e Rust).
    pub app: u64,
}

/// Pid do processo da webview, obtido na thread principal.
fn webview_pid(webview: &tauri::Webview) -> Option<u32> {
    let (tx, rx) = mpsc::channel();
    webview
        .with_webview(move |platform| {
            let _ = tx.send(platform_pid(&platform));
        })
        .ok()?;
    rx.recv_timeout(Duration::from_secs(2)).ok().flatten()
}

#[cfg(target_os = "macos")]
fn platform_pid(platform: &tauri::webview::PlatformWebview) -> Option<u32> {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    let ptr = platform.inner() as *mut AnyObject;
    // SAFETY: `inner()` é o WKWebView vivo desta webview; o seletor existe no
    // WebKit do macOS 10.13+ e devolve um pid_t.
    let pid: i32 = unsafe {
        let webview = ptr.as_ref()?;
        msg_send![webview, _webProcessIdentifier]
    };
    u32::try_from(pid).ok().filter(|p| *p > 0)
}

#[cfg(windows)]
fn platform_pid(platform: &tauri::webview::PlatformWebview) -> Option<u32> {
    let mut pid = 0u32;
    // SAFETY: chamadas COM no controlador vivo, na thread principal.
    unsafe {
        let core = platform.controller().CoreWebView2().ok()?;
        core.BrowserProcessId(&mut pid).ok()?;
    }
    (pid > 0).then_some(pid)
}

#[cfg(not(any(target_os = "macos", windows)))]
fn platform_pid(_platform: &tauri::webview::PlatformWebview) -> Option<u32> {
    None
}

/// Soma a memória do processo e de todos os descendentes.
fn tree_memory(system: &System, root: Pid) -> u64 {
    let mut children: HashMap<Pid, Vec<Pid>> = HashMap::new();
    for (pid, process) in system.processes() {
        if let Some(parent) = process.parent() {
            children.entry(parent).or_default().push(*pid);
        }
    }
    let mut total = 0;
    let mut stack = vec![root];
    while let Some(pid) = stack.pop() {
        if let Some(process) = system.process(pid) {
            total += process.memory();
        }
        if let Some(kids) = children.get(&pid) {
            stack.extend(kids);
        }
    }
    total
}

/// Deve rodar fora da thread principal (espera respostas dela).
pub fn report(app: &AppHandle) -> MemoryReport {
    let ids: Vec<String> = app
        .state::<AppState>()
        .lock()
        .config
        .services
        .iter()
        .map(|s| s.id.clone())
        .collect();
    let pids: Vec<(String, u32)> = ids
        .into_iter()
        .filter_map(|id| {
            let webview = app.get_webview(&id)?;
            Some((id, webview_pid(&webview)?))
        })
        .collect();

    let Ok(mut system) = System::new() else {
        return MemoryReport::default();
    };
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_memory(),
    );
    let services = pids
        .into_iter()
        .map(|(id, pid)| (id, tree_memory(&system, Pid::from_u32(pid))))
        .collect();
    let app_memory = system
        .process(Pid::from_u32(std::process::id()))
        .map(|p| p.memory())
        .unwrap_or_default();
    MemoryReport {
        services,
        app: app_memory,
    }
}
