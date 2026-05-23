//! Library-level error type for `rusty_vipe`.
//!
//! Uses `thiserror` to produce typed errors per AD-009; the binary boundary
//! wraps these in `anyhow::Result` for human-readable diagnostics.

use std::path::PathBuf;

/// Errors returned by the `rusty_vipe` library API.
///
/// Marked `#[non_exhaustive]` so future variant additions are not breaking
/// changes within a major version.
///
/// # Examples
///
/// ```
/// use rusty_vipe::Error;
///
/// // Errors implement Display via thiserror.
/// let e = Error::EditorNonZeroExit(42);
/// assert_eq!(e.to_string(), "editor exited with code 42");
/// ```
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// Opening the controlling terminal failed (`/dev/tty` on Unix or
    /// `CONIN$`/`CONOUT$` on Windows). Typical cause: process is running
    /// without a controlling terminal (CI, cron, `nohup`-detached, headless
    /// Docker `RUN` step).
    #[error("no controlling terminal; cannot launch editor")]
    NoControllingTty,

    /// No editor could be resolved from the precedence ladder, OR the
    /// editor binary was not found at spawn time. Carries the editor
    /// command or env-var value that was attempted.
    #[error("editor not found: {0}")]
    EditorNotFound(String),

    /// The editor exited with a non-zero code. Per FR-006, this carries
    /// the already-clamped exit code (Unix 1–255 verbatim; Windows 1–254
    /// verbatim, else clamped to 1). Library consumers can pattern-match
    /// on a known range.
    #[error("editor exited with code {0}")]
    EditorNonZeroExit(i32),

    /// `shell-words` parsing of an editor env value (`$VISUAL`/`$EDITOR`)
    /// or `--editor=<cmd>` flag failed. Carries the original input.
    #[error("invalid editor command: {0}")]
    InvalidEditorCommand(String),

    /// The tempfile no longer exists after the editor returned. Typical
    /// cause: user deleted it from within the editor (e.g., `:!rm <file>`).
    #[error("tempfile no longer exists after editor exited: {0}")]
    TempFileDeleted(PathBuf),

    /// The builder was configured into an impossible state.
    #[error("invalid builder configuration: {0}")]
    InvalidBuilderConfiguration(&'static str),

    /// A Default-mode-only setting was passed to Strict mode (or vice versa).
    #[error("compatibility violation: {0}")]
    CompatibilityViolation(&'static str),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
