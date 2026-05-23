//! Core vipe pipeline: drain → spawn → write-back.
//!
//! Three stateless functions composed by `lib.rs::run()`:
//! 1. [`drain_to_tempfile`] — read all of `reader` into a fresh
//!    `NamedTempFile` with the configured suffix.
//! 2. [`spawn_editor`] — build a `Command` with the resolved editor argv +
//!    user-supplied extras + tempfile path; reattach stdin/stdout to the
//!    controlling TTY (or fall back per `RUSTY_VIPE_TEST_BYPASS_TTY`);
//!    wait for the editor; return exit status.
//! 3. [`write_back_to_saved_stdout`] — read the tempfile's current bytes
//!    (opaquely — no encoding) and write them to the preserved original
//!    stdout sink. Distinguishes `NotFound` from other IO errors per
//!    FR-007 (`Error::TempFileDeleted`).

use std::ffi::OsString;
use std::io::Read;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};

use tempfile::NamedTempFile;

use crate::Error;
use crate::tty::{PreservedStdout, TtyHandles};

/// Env var that disables TTY reattachment for the editor child. Test-only:
/// set in CI/integration-test runs where the test process has no controlling
/// terminal. NEVER enable in production — the editor's stdio falls back to
/// `Stdio::null()` for stdin/stdout (the test's fake editor doesn't need real
/// terminal access) and inherits stderr so spawn failures stay visible.
/// Documented in `docs/DESIGN.md`.
const TEST_BYPASS_TTY_ENV: &str = "RUSTY_VIPE_TEST_BYPASS_TTY";

/// Drain `reader` into a fresh `NamedTempFile` with the given `suffix`.
/// Empty reader still produces a zero-byte tempfile (per FR-008 / HINT-003).
pub fn drain_to_tempfile<R: Read>(mut reader: R, suffix: &str) -> Result<NamedTempFile, Error> {
    let mut tempfile = tempfile::Builder::new()
        .prefix(".rusty-vipe-")
        .suffix(suffix)
        .tempfile()?;
    // Use std::io::copy to stream bytes opaquely. No transformation per FR-017.
    std::io::copy(&mut reader, tempfile.as_file_mut())?;
    tempfile.as_file_mut().sync_data().ok();
    Ok(tempfile)
}

/// Spawn the editor with the resolved argv + extras + tempfile path appended.
///
/// `tty` is the result of `tty::open_controlling_tty()` — if `None`, the
/// caller has chosen the test bypass path (RUSTY_VIPE_TEST_BYPASS_TTY=1)
/// and we'll feed the editor null stdin/stdout.
///
/// On spawn `ErrorKind::NotFound`, returns `Error::EditorNotFound(argv[0])`
/// per FR-016.
pub fn spawn_editor(
    argv: &[OsString],
    extras: &[OsString],
    tempfile_path: &Path,
    tty: Option<TtyHandles>,
) -> Result<ExitStatus, Error> {
    if argv.is_empty() {
        return Err(Error::EditorNotFound(String::from("(empty argv)")));
    }
    let program = &argv[0];
    let mut command = Command::new(program);
    command.args(&argv[1..]);
    command.args(extras);
    command.arg(tempfile_path);

    // Stdio wiring: TTY reattachment in production; null+inherit in test bypass.
    match tty {
        Some(handles) => {
            command.stdin(Stdio::from(handles.tty_in));
            // tty_out wraps a write-side handle; clone for stdout and stderr.
            let stdout_handle = handles.tty_out.try_clone()?;
            command.stdout(Stdio::from(stdout_handle));
            command.stderr(Stdio::from(handles.tty_out));
        }
        None => {
            command.stdin(Stdio::null());
            command.stdout(Stdio::null());
            command.stderr(Stdio::inherit());
        }
    }

    let status = match command.status() {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let program_str = program.to_string_lossy().to_string();
            return Err(Error::EditorNotFound(program_str));
        }
        Err(e) => return Err(Error::Io(e)),
    };

    Ok(status)
}

/// Read the tempfile bytes and write them to the preserved stdout sink.
/// Distinguishes `io::ErrorKind::NotFound` (user deleted the tempfile from
/// inside the editor) from other read errors per FR-007.
pub fn write_back_to_saved_stdout(
    tempfile_path: &Path,
    mut saved_stdout: PreservedStdout,
) -> Result<(), Error> {
    let bytes = match std::fs::read(tempfile_path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::TempFileDeleted(tempfile_path.to_path_buf()));
        }
        Err(e) => return Err(Error::Io(e)),
    };
    use std::io::Write;
    saved_stdout.as_writer().write_all(&bytes)?;
    saved_stdout.as_writer().flush().ok();
    Ok(())
}

/// Returns true iff the `RUSTY_VIPE_TEST_BYPASS_TTY` env var is set to a
/// truthy value. Test-only escape hatch; documented in DESIGN.md.
pub fn test_bypass_tty_enabled() -> bool {
    match std::env::var_os(TEST_BYPASS_TTY_ENV) {
        Some(v) => {
            let Some(s) = v.to_str() else { return false };
            matches!(
                s.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        }
        None => false,
    }
}

/// Compute the exit code to return per FR-006's clamping rule.
/// Unix: pass through verbatim in the range 1–255; 0 only when editor exited 0.
/// Windows: pass through 1–254 verbatim; clamp >254 or negative to 1.
pub fn clamp_exit_code(status: ExitStatus) -> i32 {
    if status.success() {
        return 0;
    }
    let raw = status.code().unwrap_or(1); // signal-terminated → 1
    #[cfg(unix)]
    {
        if (1..=255).contains(&raw) { raw } else { 1 }
    }
    #[cfg(windows)]
    {
        if (1..=254).contains(&raw) { raw } else { 1 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn drain_empty_input_produces_zero_byte_tempfile() {
        let tempfile = drain_to_tempfile(Cursor::new(&[][..]), ".txt").unwrap();
        let meta = std::fs::metadata(tempfile.path()).unwrap();
        assert_eq!(meta.len(), 0, "FR-008: empty stdin → zero-byte tempfile");
    }

    #[test]
    fn drain_small_input_roundtrips() {
        let tempfile = drain_to_tempfile(Cursor::new(b"hello\nworld\n"), ".txt").unwrap();
        let bytes = std::fs::read(tempfile.path()).unwrap();
        assert_eq!(bytes, b"hello\nworld\n");
    }

    #[test]
    fn drain_uses_configured_suffix() {
        let tempfile = drain_to_tempfile(Cursor::new(b"x"), ".json").unwrap();
        let name = tempfile.path().file_name().unwrap().to_string_lossy();
        assert!(
            name.ends_with(".json"),
            "FR-012: --suffix=.json should produce *.json tempfile, got {name}"
        );
    }

    #[test]
    fn drain_empty_suffix_means_no_extension() {
        let tempfile = drain_to_tempfile(Cursor::new(b"x"), "").unwrap();
        let name = tempfile.path().file_name().unwrap().to_string_lossy();
        // Empty suffix → name is just the prefix + random part, no '.' at end.
        assert!(
            !name.ends_with(".txt") && !name.ends_with('.'),
            "FR-012 + Clarification Q2: empty --suffix= means no extension, got {name}"
        );
    }

    #[test]
    fn drain_binary_passthrough_unchanged() {
        let bytes: &[u8] = &[0x00, 0xfe, 0xff, 0xc3, 0x28, 0xa0, 0xa1];
        let tempfile = drain_to_tempfile(Cursor::new(bytes), ".bin").unwrap();
        let read_back = std::fs::read(tempfile.path()).unwrap();
        assert_eq!(read_back, bytes, "FR-017: bytes opaque, no transformation");
    }

    #[test]
    fn spawn_editor_returns_editor_not_found_on_missing_binary() {
        let argv = vec![OsString::from(
            "a-binary-that-definitely-does-not-exist-12345",
        )];
        let extras: Vec<OsString> = Vec::new();
        let result = spawn_editor(&argv, &extras, Path::new("/tmp/nope"), None);
        match result {
            Err(Error::EditorNotFound(name)) => {
                assert!(name.contains("does-not-exist"), "should carry the argv[0]");
            }
            other => panic!("expected EditorNotFound, got {other:?}"),
        }
    }

    #[test]
    fn spawn_editor_empty_argv_returns_editor_not_found() {
        let argv: Vec<OsString> = Vec::new();
        let extras: Vec<OsString> = Vec::new();
        let result = spawn_editor(&argv, &extras, Path::new("/tmp/nope"), None);
        assert!(matches!(result, Err(Error::EditorNotFound(_))));
    }

    #[test]
    fn write_back_returns_tempfile_deleted_when_file_missing() {
        let tmpdir = tempfile::tempdir().unwrap();
        let missing = tmpdir.path().join("does-not-exist.txt");

        let preserved = crate::tty::preserve_stdout().expect("preserve_stdout should succeed");
        let result = write_back_to_saved_stdout(&missing, preserved);
        assert!(matches!(result, Err(Error::TempFileDeleted(_))));
    }

    #[test]
    fn test_bypass_tty_recognizes_truthy_values() {
        for v in ["1", "true", "yes", "on", "TRUE", " 1 "] {
            // SAFETY: tests run sequentially via Cargo's default; setting env vars is fine.
            unsafe {
                std::env::set_var(TEST_BYPASS_TTY_ENV, v);
            }
            assert!(test_bypass_tty_enabled(), "{v:?} should be truthy");
        }
        unsafe {
            std::env::remove_var(TEST_BYPASS_TTY_ENV);
        }
        assert!(!test_bypass_tty_enabled());
    }
}
