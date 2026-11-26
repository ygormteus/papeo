//! Tempo sem uso do computador (teclado, mouse, trackpad), do sistema todo.

use std::time::Duration;

/// `None` se o sistema não informar.
pub fn system_idle() -> Option<Duration> {
    imp::idle()
}

/// Tela principal apagada. No macOS, as acordadas de manutenção do repouso
/// ("dark wake") rodam o app por alguns segundos com a tela apagada.
pub fn display_asleep() -> bool {
    imp::display_asleep()
}

#[cfg(target_os = "macos")]
mod imp {
    use std::time::Duration;

    // kCGEventSourceStateCombinedSessionState
    const COMBINED_SESSION_STATE: i32 = 0;
    // kCGAnyInputEventType
    const ANY_INPUT_EVENT: u32 = !0;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
        fn CGMainDisplayID() -> u32;
        fn CGDisplayIsAsleep(display: u32) -> u32;
    }

    pub fn display_asleep() -> bool {
        // SAFETY: funções puras do CoreGraphics, sem ponteiros.
        unsafe { CGDisplayIsAsleep(CGMainDisplayID()) != 0 }
    }

    pub fn idle() -> Option<Duration> {
        // SAFETY: função pura do CoreGraphics, sem ponteiros.
        let secs = unsafe {
            CGEventSourceSecondsSinceLastEventType(COMBINED_SESSION_STATE, ANY_INPUT_EVENT)
        };
        (secs.is_finite() && secs >= 0.0).then(|| Duration::from_secs_f64(secs))
    }
}

#[cfg(windows)]
mod imp {
    use std::time::Duration;
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    pub fn idle() -> Option<Duration> {
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        // SAFETY: `info` vive durante a chamada e tem `cbSize` preenchido.
        if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
            return None;
        }
        // Os dois contadores dão a volta a cada ~49 dias; a diferença
        // com `wrapping_sub` continua certa.
        let ms = unsafe { GetTickCount() }.wrapping_sub(info.dwTime);
        Some(Duration::from_millis(u64::from(ms)))
    }

    pub fn display_asleep() -> bool {
        false
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod imp {
    pub fn idle() -> Option<std::time::Duration> {
        None
    }

    pub fn display_asleep() -> bool {
        false
    }
}

#[cfg(all(test, any(target_os = "macos", windows)))]
mod tests {
    #[test]
    fn reads_system_idle() {
        let idle = super::system_idle().expect("o sistema informa o tempo sem uso");
        assert!(idle.as_secs() < 365 * 24 * 60 * 60);
    }
}
