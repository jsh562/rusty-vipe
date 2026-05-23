//! Signal-driven cleanup, mirroring the `rusty-sponge` architecture.
//!
//! Per the SAD-promoted signal-driven tempfile cleanup pattern:
//! - Async-signal-safe handler stores `true` into a process-wide AtomicBool.
//! - Main thread polls via `is_cancelled()` at safe checkpoints.
//! - On flag set, return `io::ErrorKind::Interrupted`; tempfile drops via
//!   normal scope-exit `Drop`, leaving the target file untouched.

use std::sync::atomic::{AtomicBool, Ordering};

static CANCEL: AtomicBool = AtomicBool::new(false);

/// Reset the process-wide cancel flag (test-only helper).
#[cfg(test)]
pub fn reset_cancel() {
    CANCEL.store(false, Ordering::SeqCst);
}

/// Returns true if a registered signal has been delivered.
pub fn is_cancelled() -> bool {
    CANCEL.load(Ordering::SeqCst)
}

#[cfg(unix)]
pub fn install_handlers() -> std::io::Result<()> {
    use signal_hook::consts::signal::{SIGHUP, SIGINT, SIGTERM};
    use signal_hook::flag::register;
    use std::sync::{Arc, OnceLock};

    static BRIDGE_FLAG: OnceLock<Arc<AtomicBool>> = OnceLock::new();
    let bridge = BRIDGE_FLAG.get_or_init(|| Arc::new(AtomicBool::new(false)));

    register(SIGINT, Arc::clone(bridge))?;
    register(SIGTERM, Arc::clone(bridge))?;
    register(SIGHUP, Arc::clone(bridge))?;

    let bridge_clone = Arc::clone(bridge);
    std::thread::Builder::new()
        .name("rusty-vipe-signal-watcher".into())
        .spawn(move || {
            loop {
                if bridge_clone.load(Ordering::SeqCst) {
                    CANCEL.store(true, Ordering::SeqCst);
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        })?;

    Ok(())
}

#[cfg(windows)]
pub fn install_handlers() -> std::io::Result<()> {
    use windows_sys::Win32::Foundation::BOOL;
    use windows_sys::Win32::System::Console::{
        CTRL_BREAK_EVENT, CTRL_C_EVENT, CTRL_CLOSE_EVENT, SetConsoleCtrlHandler,
    };

    unsafe extern "system" fn handler(ctrl_type: u32) -> BOOL {
        if matches!(
            ctrl_type,
            CTRL_C_EVENT | CTRL_BREAK_EVENT | CTRL_CLOSE_EVENT
        ) {
            CANCEL.store(true, Ordering::SeqCst);
            1
        } else {
            0
        }
    }

    // SAFETY: SetConsoleCtrlHandler FFI; safe to call from any thread.
    let ok = unsafe { SetConsoleCtrlHandler(Some(handler), 1) };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_cancel_flag_starts_unset() {
        reset_cancel();
        assert!(!is_cancelled());
    }

    #[test]
    fn install_handlers_does_not_panic() {
        let _ = install_handlers();
        reset_cancel();
        assert!(!is_cancelled(), "no signal raised → flag stays clear");
    }
}
