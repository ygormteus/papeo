//! Memória usada por serviço.
//!
//! - macOS: cada WKWebView tem um processo "WebContent"; o pid vem de
//!   `_webProcessIdentifier` (API privada estável do WebKit).
//! - Windows: cada serviço tem pasta de dados própria, então um processo de
//!   navegador WebView2 próprio; soma-se a árvore de processos dele.
//!
//! No macOS o número é o "footprint" (o mesmo do Monitor de Atividade:
//! inclui memória comprimida e de vídeo, sem contar bibliotecas
//! compartilhadas). No Windows é a memória residente da árvore, aproximada.

use crate::state::AppState;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};
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

/// Pid do processo de cada webview, lido na thread principal: todos os
/// pedidos saem juntos e a espera tem um prazo só (não 2 s por conta).
fn webview_pids(webviews: Vec<(String, tauri::Webview)>) -> Vec<(String, u32)> {
    let (tx, rx) = mpsc::channel();
    let mut asked = 0;
    for (id, webview) in webviews {
        let tx = tx.clone();
        if webview
            .with_webview(move |platform| {
                let _ = tx.send((id, platform_pid(&platform)));
            })
            .is_ok()
        {
            asked += 1;
        }
    }
    drop(tx);
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut pids = Vec::with_capacity(asked);
    for _ in 0..asked {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok((id, Some(pid))) => pids.push((id, pid)),
            Ok((_, None)) => {}
            Err(_) => break,
        }
    }
    pids
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

/// Footprint do processo (macOS), como no Monitor de Atividade.
#[cfg(target_os = "macos")]
fn footprint(pid: u32) -> Option<u64> {
    let mut info = std::mem::MaybeUninit::<libc::rusage_info_v2>::zeroed();
    // SAFETY: `info` tem o tamanho de `rusage_info_v2`, o formato pedido.
    let rc = unsafe {
        libc::proc_pid_rusage(
            i32::try_from(pid).ok()?,
            libc::RUSAGE_INFO_V2,
            info.as_mut_ptr().cast(),
        )
    };
    // SAFETY: preenchido pelo kernel quando rc == 0.
    (rc == 0).then(|| unsafe { info.assume_init() }.ri_phys_footprint)
}

/// Soma a memória do processo e de todos os descendentes.
#[cfg_attr(target_os = "macos", allow(dead_code))]
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
    let webviews = ids
        .into_iter()
        .filter_map(|id| Some((id.clone(), app.get_webview(&id)?)))
        .collect();
    let pids = webview_pids(webviews);

    #[cfg(target_os = "macos")]
    {
        let services = pids
            .into_iter()
            .filter_map(|(id, pid)| Some((id, footprint(pid)?)))
            .collect();
        let app = footprint(std::process::id()).unwrap_or_default();
        return MemoryReport { services, app };
    }

    #[allow(unreachable_code)]
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
