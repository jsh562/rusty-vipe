//! Cross-platform controlling-terminal reattachment.
//!
//! AD-003/AD-004/AD-005/AD-016 (per HINT-001 — implement before pipeline.rs):
//! - Open the controlling terminal (`/dev/tty` on Unix, `CONIN$`/`CONOUT$`
//!   on Windows) so the spawned editor's stdin/stdout/stderr point at the
//!   real terminal — not the pipeline's pipe stdin/stdout.
//! - Preserve the process's original stdout sink via `dup(2)` (Unix) /
//!   `DuplicateHandle` (Windows) so the post-edit bytes have a route back
//!   to the downstream consumer.
//! - On open failure (no controlling terminal — CI, cron, headless Docker),
//!   return `Error::NoControllingTty` so the caller can fail fast (FR-015).

use crate::Error;

/// File handles for the controlling terminal that the spawned editor child
/// will inherit. On Unix these wrap the file descriptors for `/dev/tty`
/// opened twice (read + write); on Windows they wrap `CONIN$` and `CONOUT$`.
#[derive(Debug)]
pub struct TtyHandles {
    pub tty_in: std::fs::File,
    pub tty_out: std::fs::File,
}

/// Snapshot of the original stdout sink, preserved BEFORE the editor's
/// reattachment so post-edit bytes can route back to the pipeline.
#[derive(Debug)]
pub struct PreservedStdout {
    inner: std::fs::File,
}

impl PreservedStdout {
    /// Borrow the preserved stdout as a `Write` for the final write-back.
    pub fn as_writer(&mut self) -> &mut std::fs::File {
        &mut self.inner
    }
}

/// Open the controlling terminal for read+write. Unix uses `/dev/tty`;
/// Windows uses `CONIN$`/`CONOUT$` via `CreateFileW`.
///
/// Honors the documented test-only fault-injection env var
/// `RUSTY_VIPE_TEST_FAIL_TTY=1` which forces `NoControllingTty` even when a
/// real terminal is available. Used by `tests/no_tty.rs` integration tests
/// because reliably stripping the controlling-TTY from a child process is
/// platform-specific (Unix needs `setsid` + double-fork; Windows needs
/// `CREATE_NO_WINDOW` + detached process group). The env-var bypass keeps
/// the test surface portable without sacrificing FR-015 coverage.
pub fn open_controlling_tty() -> Result<TtyHandles, Error> {
    if test_fail_tty_enabled() {
        return Err(Error::NoControllingTty);
    }
    #[cfg(unix)]
    {
        unix::open_tty()
    }
    #[cfg(windows)]
    {
        windows_impl::open_tty()
    }
}

/// Returns true iff `RUSTY_VIPE_TEST_FAIL_TTY` is set to a truthy value. Test
/// fault-injection only — never enable in production.
fn test_fail_tty_enabled() -> bool {
    match std::env::var_os("RUSTY_VIPE_TEST_FAIL_TTY") {
        Some(v) => match v.to_str() {
            Some(s) => matches!(
                s.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            ),
            None => false,
        },
        None => false,
    }
}

/// Preserve the process's current stdout sink via platform-specific
/// duplication, so reopening stdout to the TTY does not clobber the path
/// back to the pipeline.
pub fn preserve_stdout() -> Result<PreservedStdout, Error> {
    #[cfg(unix)]
    {
        unix::preserve_stdout()
    }
    #[cfg(windows)]
    {
        windows_impl::preserve_stdout()
    }
}

#[cfg(unix)]
mod unix {
    use super::{Error, PreservedStdout, TtyHandles};
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::os::unix::io::AsRawFd;

    pub fn open_tty() -> Result<TtyHandles, Error> {
        let tty_in = std::fs::OpenOptions::new()
            .read(true)
            .open("/dev/tty")
            .map_err(|_| Error::NoControllingTty)?;
        let tty_out = std::fs::OpenOptions::new()
            .write(true)
            .open("/dev/tty")
            .map_err(|_| Error::NoControllingTty)?;
        Ok(TtyHandles { tty_in, tty_out })
    }

    pub fn preserve_stdout() -> Result<PreservedStdout, Error> {
        let stdout = std::io::stdout();
        let raw_fd = stdout.as_raw_fd();
        // dup(2) — returns the lowest available fd pointing at the same file
        // description. Per HINT-002, NOT dup2 (which targets a specific fd
        // and could clobber).
        // SAFETY: libc::dup is a thin syscall wrapper; raw_fd is valid for
        // the lifetime of the process stdout.
        let new_fd = unsafe { libc::dup(raw_fd) };
        if new_fd < 0 {
            return Err(Error::Io(std::io::Error::last_os_error()));
        }
        // SAFETY: dup returned a fresh owned fd we now own; converting to
        // OwnedFd → File transfers that ownership.
        let owned: OwnedFd = unsafe { OwnedFd::from_raw_fd(new_fd) };
        Ok(PreservedStdout {
            inner: std::fs::File::from(owned),
        })
    }
}

#[cfg(windows)]
mod windows_impl {
    use super::{Error, PreservedStdout, TtyHandles};
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::{
        DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn create_file(name: &str, access: u32) -> Result<HANDLE, Error> {
        let wide_name = wide(name);
        // SAFETY: CreateFileW is FFI; wide_name is null-terminated UTF-16,
        // and the other parameters are well-known constants.
        let handle = unsafe {
            CreateFileW(
                wide_name.as_ptr(),
                access,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE || handle.is_null() {
            return Err(Error::NoControllingTty);
        }
        Ok(handle)
    }

    pub fn open_tty() -> Result<TtyHandles, Error> {
        let conin = create_file("CONIN$", FILE_GENERIC_READ)?;
        let conout = create_file("CONOUT$", FILE_GENERIC_WRITE)?;
        // SAFETY: handles came from CreateFileW; we take ownership.
        let owned_in: OwnedHandle = unsafe { OwnedHandle::from_raw_handle(conin as _) };
        let owned_out: OwnedHandle = unsafe { OwnedHandle::from_raw_handle(conout as _) };
        Ok(TtyHandles {
            tty_in: std::fs::File::from(owned_in),
            tty_out: std::fs::File::from(owned_out),
        })
    }

    pub fn preserve_stdout() -> Result<PreservedStdout, Error> {
        let stdout = std::io::stdout();
        let raw = stdout.as_raw_handle();
        let mut duplicate: HANDLE = std::ptr::null_mut();
        // SAFETY: DuplicateHandle FFI; current-process handle is the
        // process pseudo-handle; we duplicate the source HANDLE into a new
        // owned slot.
        let ok = unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                raw as _,
                GetCurrentProcess(),
                &mut duplicate,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        };
        if ok == 0 {
            return Err(Error::Io(std::io::Error::last_os_error()));
        }
        // SAFETY: DuplicateHandle gave us a fresh owned HANDLE.
        let owned: OwnedHandle = unsafe { OwnedHandle::from_raw_handle(duplicate as _) };
        Ok(PreservedStdout {
            inner: std::fs::File::from(owned),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_controlling_tty_returns_no_tty_or_handles() {
        // In a CI environment (no PTY) this returns Err(NoControllingTty).
        // On a developer machine with an interactive shell it returns Ok.
        // Either is acceptable — we just verify the function exists and
        // doesn't panic.
        match open_controlling_tty() {
            Ok(_handles) => {}
            Err(Error::NoControllingTty) => {}
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn preserve_stdout_succeeds_in_normal_process() {
        // In any normal test process, stdout is a valid descriptor/handle —
        // duplication should succeed.
        let _preserved = preserve_stdout().expect("preserve_stdout should succeed");
    }
}
