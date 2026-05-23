//! Deterministic editor stand-in for integration tests (AD-013, FR-026).
//!
//! Gated behind the `dev-helpers` Cargo feature — NOT installed by
//! `cargo install rusty-vipe`. Tests find this binary via the
//! `CARGO_BIN_EXE_fake-editor` env var (set by Cargo at test build time).
//!
//! Contract: invoked with `--transform=<name>` plus optional args, and the
//! LAST argv element is the tempfile path (matching how vipe spawns its
//! editor). Returns the transformed bytes by overwriting the tempfile.
//!
//! Side-channel reporting: when the `RUSTY_VIPE_FAKE_EDITOR_REPORT` env var
//! is set to a writable path, the helper appends diagnostic info there
//! (argv, tempfile name, stdio markers, etc.) so tests can assert what the
//! editor "saw" without polluting stdout/stderr.
//!
//! ## Supported transforms (per FR-026)
//!
//! | Transform | Behavior |
//! |---|---|
//! | `delete-line:<N>` | Delete the Nth line (1-indexed), write back, exit 0 |
//! | `replace:<bytes>` | Overwrite with literal bytes; `@<path>` reads bytes from file |
//! | `passthrough` | Read + write unchanged, exit 0 |
//! | `exit-nonzero:<code>` | Don't touch file; exit with the given code |
//! | `noop` | Don't touch file; exit 0 |
//! | `report-argv` | Write own argv (one per line) to report file; exit 0 |
//! | `report-filename` | Write the last argv element to report file; exit 0 |
//! | `report-stdio` | Write stdin/stdout tty-marker tags to report file; exit 0 |

use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

const REPORT_ENV: &str = "RUSTY_VIPE_FAKE_EDITOR_REPORT";

fn main() -> ExitCode {
    let argv: Vec<String> = env::args().collect();
    if argv.len() < 2 {
        eprintln!("fake-editor: expected at least one arg (transform + tempfile path)");
        return ExitCode::from(2);
    }

    // Parse --transform=<name>; expect tempfile path as last argv element.
    let mut transform: Option<String> = None;
    let mut remaining: Vec<&String> = Vec::new();
    for arg in argv.iter().skip(1) {
        if let Some(value) = arg.strip_prefix("--transform=") {
            transform = Some(value.to_string());
        } else {
            remaining.push(arg);
        }
    }
    let Some(transform) = transform else {
        eprintln!("fake-editor: missing --transform=<name>");
        return ExitCode::from(2);
    };

    let target_path = remaining.last().map(|s| PathBuf::from(s.as_str()));

    match transform.as_str() {
        "passthrough" | "noop" => ExitCode::SUCCESS,
        t if t.starts_with("delete-line:") => {
            let n: usize = match t["delete-line:".len()..].parse() {
                Ok(n) if n > 0 => n,
                _ => {
                    eprintln!("fake-editor: invalid delete-line N (must be >= 1)");
                    return ExitCode::from(2);
                }
            };
            let Some(path) = target_path else {
                eprintln!("fake-editor: delete-line needs a tempfile path");
                return ExitCode::from(2);
            };
            match delete_line(&path, n) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("fake-editor: delete-line failed: {e}");
                    ExitCode::from(1)
                }
            }
        }
        t if t.starts_with("replace:") => {
            let spec = &t["replace:".len()..];
            let Some(path) = target_path else {
                eprintln!("fake-editor: replace needs a tempfile path");
                return ExitCode::from(2);
            };
            let new_bytes = if let Some(file_ref) = spec.strip_prefix('@') {
                match fs::read(file_ref) {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("fake-editor: replace @{file_ref} read failed: {e}");
                        return ExitCode::from(1);
                    }
                }
            } else {
                spec.as_bytes().to_vec()
            };
            match fs::write(&path, &new_bytes) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("fake-editor: replace write failed: {e}");
                    ExitCode::from(1)
                }
            }
        }
        t if t.starts_with("exit-nonzero:") => {
            let code: i32 = match t["exit-nonzero:".len()..].parse() {
                Ok(c) => c,
                Err(_) => {
                    eprintln!("fake-editor: invalid exit-nonzero code");
                    return ExitCode::from(2);
                }
            };
            // Clamp to u8 (ExitCode::from accepts u8). Values outside the
            // 1..=255 range collapse to 1 so the exit is still non-zero.
            let byte = if (1..=255).contains(&code) {
                code as u8
            } else {
                1u8
            };
            ExitCode::from(byte)
        }
        "report-argv" => {
            if let Some(report_path) = env::var_os(REPORT_ENV) {
                let body = argv.join("\n");
                if let Err(e) = fs::write(&report_path, body) {
                    eprintln!("fake-editor: report-argv write failed: {e}");
                    return ExitCode::from(1);
                }
            }
            ExitCode::SUCCESS
        }
        "report-filename" => {
            if let Some(report_path) = env::var_os(REPORT_ENV) {
                let body = target_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                if let Err(e) = fs::write(&report_path, body) {
                    eprintln!("fake-editor: report-filename write failed: {e}");
                    return ExitCode::from(1);
                }
            }
            ExitCode::SUCCESS
        }
        "report-stdio" => {
            if let Some(report_path) = env::var_os(REPORT_ENV) {
                let stdin_marker = describe_stream(StreamKind::Stdin);
                let stdout_marker = describe_stream(StreamKind::Stdout);
                let body = format!("stdin: {stdin_marker}\nstdout: {stdout_marker}\n");
                if let Err(e) = fs::write(&report_path, body) {
                    eprintln!("fake-editor: report-stdio write failed: {e}");
                    return ExitCode::from(1);
                }
            }
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("fake-editor: unknown transform: {other}");
            ExitCode::from(2)
        }
    }
}

fn delete_line(path: &std::path::Path, n: usize) -> std::io::Result<()> {
    let original = fs::read(path)?;
    let mut out = Vec::with_capacity(original.len());
    let mut line_index: usize = 1;
    let mut start: usize = 0;
    for (i, byte) in original.iter().enumerate() {
        if *byte == b'\n' {
            if line_index != n {
                out.extend_from_slice(&original[start..=i]);
            }
            start = i + 1;
            line_index += 1;
        }
    }
    // Trailing partial line (no final newline).
    if start < original.len() && line_index != n {
        out.extend_from_slice(&original[start..]);
    }
    // Truncate the file and write the new content.
    let mut f = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)?;
    f.write_all(&out)?;
    Ok(())
}

#[derive(Copy, Clone)]
enum StreamKind {
    Stdin,
    Stdout,
}

#[cfg(unix)]
fn describe_stream(kind: StreamKind) -> String {
    use std::os::fd::AsRawFd;
    let fd = match kind {
        StreamKind::Stdin => std::io::stdin().as_raw_fd(),
        StreamKind::Stdout => std::io::stdout().as_raw_fd(),
    };
    // SAFETY: isatty is a thin syscall wrapper.
    let is_tty = unsafe { libc::isatty(fd) } == 1;
    if is_tty {
        "tty".to_string()
    } else {
        "not-tty".to_string()
    }
}

#[cfg(windows)]
fn describe_stream(kind: StreamKind) -> String {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Console::GetConsoleMode;
    let handle = match kind {
        StreamKind::Stdin => std::io::stdin().as_raw_handle(),
        StreamKind::Stdout => std::io::stdout().as_raw_handle(),
    };
    let mut mode: u32 = 0;
    // SAFETY: GetConsoleMode FFI; handle is owned by the std stream.
    let ok = unsafe { GetConsoleMode(handle as _, &mut mode) };
    if ok != 0 {
        "console".to_string()
    } else {
        "not-console".to_string()
    }
}
