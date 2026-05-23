//! # rusty-vipe
//!
//! A Rust port of the moreutils `vipe` utility: pop `$EDITOR` mid-pipe so the
//! user can edit the buffered bytes interactively, then resume the pipeline
//! with the edited output.
//!
//! ## Quick start
//!
//! ```no_run
//! use rusty_vipe::{VipeBuilder, EditorSource, CompatibilityMode};
//! use std::io::Cursor;
//!
//! let mut input = Cursor::new(b"line1\nline2\nline3\n".to_vec());
//! let mut output: Vec<u8> = Vec::new();
//!
//! let mut vipe = VipeBuilder::new()
//!     .editor(EditorSource::Override("fake-editor --transform=passthrough".into()))
//!     .suffix(".txt")
//!     .compat(CompatibilityMode::Default)
//!     .build()?;
//!
//! vipe.run(&mut input, &mut output)?;
//! # Ok::<(), rusty_vipe::Error>(())
//! ```
//!
//! ## Stability (lockstep SemVer)
//!
//! Library and binary share a single crate version. Within `0.x`, minor
//! version bumps may introduce breaking changes per standard Cargo
//! semantics. Every public enum and struct is `#[non_exhaustive]` so
//! variant additions are not breaking changes once `1.0` lands.
//!
//! ## Pipeline-safety contract
//!
//! When the editor exits non-zero, [`Vipe::run`] does NOT touch the
//! caller-supplied writer and returns `Err(Error::EditorNonZeroExit(code))`.
//! This matches the CLI invariant — no bytes downstream on abort.

pub mod error;

pub use error::Error;

/// Where the editor command comes from.
///
/// # Examples
///
/// ```
/// use rusty_vipe::EditorSource;
///
/// // Use an explicit editor command (whitespace-aware splitting).
/// let _ = EditorSource::Override(String::from("code --wait"));
///
/// // Or follow the standard env precedence ladder.
/// let _ = EditorSource::EnvLookup;
/// ```
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum EditorSource {
    /// Explicit override (`--editor=<cmd>` flag, Default mode only). Carries
    /// the raw command string; whitespace-aware splitting happens at run time.
    Override(String),
    /// Follow the precedence-laddered env lookup: `$VISUAL` > `$EDITOR` >
    /// `/usr/bin/editor` (Unix) > `vi` (Unix) / `notepad.exe` (Windows).
    EnvLookup,
}

/// Whether to apply Default-mode ergonomic extensions or Strict moreutils parity.
///
/// # Examples
///
/// ```
/// use rusty_vipe::CompatibilityMode;
///
/// assert_eq!(CompatibilityMode::default(), CompatibilityMode::Default);
/// // Strict mode rejects `--editor`, `--help`, `--version`, and completions.
/// let _ = CompatibilityMode::Strict;
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompatibilityMode {
    /// Default mode: `--help`, `--version`, `--editor=<cmd>`, `completions`
    /// subcommand all honored.
    #[default]
    Default,
    /// Strict mode: byte-equal moreutils stderr for documented inputs;
    /// rejects every Default-mode addition.
    Strict,
}

/// Default tempfile suffix (matches moreutils 0.69 `--suffix` default).
pub const DEFAULT_SUFFIX: &str = ".txt";

/// Maximum permitted length (in bytes) for a `--suffix` value. Most POSIX and
/// Windows filesystems cap a single filename component at 255 bytes; we reject
/// suffixes that would push the tempfile name past that limit.
pub const MAX_SUFFIX_LEN: usize = 255;

/// Validate a `--suffix=<ext>` value at parse time. Rejects path separators
/// (`/`, `\`), NUL bytes (which terminate C strings on every supported OS),
/// and lengths past `MAX_SUFFIX_LEN`. Empty suffix is allowed (means literally
/// no extension, per FR-012 Clarification Q2).
pub fn validate_suffix(value: &str) -> Result<(), &'static str> {
    if value.len() > MAX_SUFFIX_LEN {
        return Err("--suffix value too long (max 255 bytes)");
    }
    if value.contains('\0') {
        return Err("--suffix must not contain a NUL byte");
    }
    if value.contains('/') || value.contains('\\') {
        return Err("--suffix must not contain path separators ('/' or '\\\\')");
    }
    Ok(())
}

/// Runtime engine for one vipe invocation. Constructed via [`VipeBuilder`].
#[non_exhaustive]
#[derive(Debug)]
pub struct Vipe {
    editor: EditorSource,
    suffix: String,
    compat: CompatibilityMode,
}

/// Builder for [`Vipe`]. All chain methods are `#[must_use]`.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct VipeBuilder {
    editor: EditorSource,
    suffix: String,
    compat: CompatibilityMode,
}

impl Default for VipeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl VipeBuilder {
    /// Construct a new builder defaulting to `EditorSource::EnvLookup`,
    /// `.txt` suffix, Default mode.
    #[must_use]
    pub fn new() -> Self {
        Self {
            editor: EditorSource::EnvLookup,
            suffix: DEFAULT_SUFFIX.to_string(),
            compat: CompatibilityMode::Default,
        }
    }

    /// Set the editor source.
    #[must_use]
    pub fn editor(mut self, editor: EditorSource) -> Self {
        self.editor = editor;
        self
    }

    /// Set the tempfile suffix. Empty string means literally no extension.
    #[must_use]
    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = suffix.into();
        self
    }

    /// Set the compatibility mode.
    #[must_use]
    pub fn compat(mut self, compat: CompatibilityMode) -> Self {
        self.compat = compat;
        self
    }

    /// Validate and build a [`Vipe`].
    pub fn build(self) -> Result<Vipe, Error> {
        // Strict mode rejects explicit editor overrides per FR-013.
        if self.compat == CompatibilityMode::Strict
            && matches!(self.editor, EditorSource::Override(_))
        {
            return Err(Error::CompatibilityViolation(
                "--editor not honored in Strict mode",
            ));
        }
        // Empty Override is rejected — empty strings on the CLI fall through
        // via the binary's argv-parsing path, but a programmatic empty Override
        // signals user error.
        if let EditorSource::Override(ref s) = self.editor {
            if s.is_empty() {
                return Err(Error::InvalidBuilderConfiguration("empty editor override"));
            }
        }
        // Suffix validation mirrors the CLI parser (FR-012 Edge Cases).
        validate_suffix(&self.suffix).map_err(Error::InvalidBuilderConfiguration)?;
        Ok(Vipe {
            editor: self.editor,
            suffix: self.suffix,
            compat: self.compat,
        })
    }
}

impl Vipe {
    /// Drain `reader` to a tempfile, spawn the editor against it, then write
    /// the post-edit tempfile bytes to `writer`.
    ///
    /// On non-zero editor exit, `writer` is NOT touched and the call returns
    /// `Err(Error::EditorNonZeroExit(code))` with the already-clamped code
    /// (Unix 1–255 verbatim; Windows 1–254 verbatim, else clamped to 1).
    ///
    /// **Writer-untouched invariant**: `writer` receives zero bytes (and zero
    /// `flush()` calls) on every error path — `EditorNonZeroExit`,
    /// `TempFileDeleted`, `NoControllingTty`, `InvalidEditorCommand`,
    /// `EditorNotFound`, and any underlying `Io` error during the
    /// drain/spawn/read phases. Only the final successful read-and-write step
    /// touches `writer`. See FR-029 for the formal contract.
    pub fn run<R: std::io::Read, W: std::io::Write>(
        &mut self,
        reader: R,
        mut writer: W,
    ) -> Result<(), Error> {
        // 1. Resolve editor argv. EnvLookup uses process VISUAL/EDITOR; Override
        //    uses the embedded command string. Strict mode is enforced at
        //    build() time, so we don't re-check here.
        let argv = self.resolve_editor_argv()?;

        // 2. Drain `reader` into a tempfile with the configured suffix.
        let tempfile = pipeline::drain_to_tempfile(reader, &self.suffix)?;

        // 3. Open the controlling terminal for the editor's stdio.
        //    Library consumers running headless (no PTY) get NoControllingTty.
        //    The test-bypass env var is honored so embedders' own test suites
        //    can drive Vipe::run in CI.
        let tty_handles = if pipeline::test_bypass_tty_enabled() {
            None
        } else {
            Some(tty::open_controlling_tty()?)
        };

        // 4. Spawn editor + wait. Extras are empty for the library path
        //    (the binary path forwards positional args, but the library API
        //    intentionally doesn't expose that — embedders set the full argv
        //    via EditorSource::Override).
        let extras: Vec<std::ffi::OsString> = Vec::new();
        let status = pipeline::spawn_editor(&argv, &extras, tempfile.path(), tty_handles)?;

        // 5. FR-006: non-zero exit aborts; writer NOT touched.
        if !status.success() {
            let code = pipeline::clamp_exit_code(status);
            return Err(Error::EditorNonZeroExit(code));
        }

        // 6. Read tempfile bytes and write to the user's writer. Distinguish
        //    NotFound (user deleted the tempfile from within the editor) per
        //    FR-007.
        let bytes = match std::fs::read(tempfile.path()) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::TempFileDeleted(tempfile.path().to_path_buf()));
            }
            Err(e) => return Err(Error::Io(e)),
        };
        writer.write_all(&bytes)?;
        writer.flush()?;
        Ok(())
    }

    /// Resolve `self.editor` into a spawnable argv. Pure helper extracted so
    /// `run` reads top-to-bottom in pipeline order.
    fn resolve_editor_argv(&self) -> Result<Vec<std::ffi::OsString>, Error> {
        match &self.editor {
            EditorSource::Override(cmd) => {
                let argv = editor::parse_editor_value(cmd)?;
                if argv.is_empty() {
                    return Err(Error::InvalidBuilderConfiguration(
                        "editor override resolved to empty argv",
                    ));
                }
                Ok(argv)
            }
            EditorSource::EnvLookup => {
                let env_visual = std::env::var("VISUAL").ok();
                let env_editor = std::env::var("EDITOR").ok();
                let resolved = editor::resolve(
                    None,
                    env_visual.as_deref(),
                    env_editor.as_deref(),
                    self.compat,
                )?;
                Ok(resolved.argv)
            }
        }
    }
}

// Library-essential modules (always available — needed by `Vipe::run`).
// These intentionally avoid clap/anyhow/signal-hook so library consumers can
// depend on rusty-vipe with `default-features = false`.
pub mod editor;
pub mod pipeline;
pub mod tty;

// CLI-only modules: clap parsing, signal handlers, Strict-mode argv scan,
// CompatibilityMode resolver — gated behind `cli` because they pull clap,
// signal-hook, and other binary-only deps.
#[cfg(feature = "cli")]
pub mod cli;
#[cfg(feature = "cli")]
pub mod mode;
#[cfg(feature = "cli")]
pub mod signal;
#[cfg(feature = "cli")]
pub mod strict;

/// Binary entry-point helper used by both `src/main.rs` and `src/bin/vipe.rs`.
///
/// Per FR-006 / AD-012: editor non-zero exit is propagated as the process
/// exit code (with Windows clamping); writer (the preserved stdout sink) is
/// NOT touched on non-zero exit.
#[cfg(feature = "cli")]
pub fn run() -> std::process::ExitCode {
    use clap::Parser;
    use std::ffi::OsString;
    use std::process::ExitCode;

    // Install signal handlers as early as possible (FR-014).
    if let Err(e) = signal::install_handlers() {
        eprintln!("warning: could not install signal handlers: {e}");
    }

    // Pre-clap detection of `--strict` / `--no-strict` + env + argv[0] for
    // Strict-mode dispatch. Strict mode bypasses clap entirely (clap can't
    // produce byte-equal moreutils errors).
    let raw_argv: Vec<OsString> = std::env::args_os().collect();
    let pre_strict = strict::pre_scan_strict_flag(&raw_argv);
    let env_strict = std::env::var_os("RUSTY_VIPE_STRICT");
    let argv0 = raw_argv.first().cloned();
    let resolved_mode = mode::resolve(pre_strict, env_strict.as_deref(), argv0.as_deref());
    if resolved_mode == CompatibilityMode::Strict {
        return strict::run(&raw_argv);
    }

    let cli_args = match cli::Cli::try_parse() {
        Ok(args) => args,
        Err(e) => {
            e.print().ok();
            return match e.kind() {
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion => {
                    ExitCode::SUCCESS
                }
                _ => ExitCode::from(2),
            };
        }
    };

    // Subcommands (completions). Same pattern as rusty-sponge.
    if let Some(cli::Subcommand::Completions { shell }) = cli_args.command {
        use clap::CommandFactory;
        let mut cmd = cli::Cli::command();
        let name = cmd.get_name().to_string();
        clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
        return ExitCode::SUCCESS;
    }

    // Resolve editor argv.
    let env_visual = std::env::var("VISUAL").ok();
    let env_editor = std::env::var("EDITOR").ok();
    let editor_resolved = match editor::resolve(
        cli_args.editor.as_deref(),
        env_visual.as_deref(),
        env_editor.as_deref(),
        CompatibilityMode::Default,
    ) {
        Ok(r) => r,
        Err(Error::InvalidEditorCommand(raw)) => {
            eprintln!("rusty-vipe: invalid EDITOR/VISUAL value: {raw}");
            return ExitCode::from(127);
        }
        Err(e) => {
            eprintln!("rusty-vipe: {e}");
            return ExitCode::from(127);
        }
    };

    // Drain stdin to a tempfile with the configured suffix.
    let suffix = cli_args.suffix.as_deref().unwrap_or(DEFAULT_SUFFIX);
    let stdin = std::io::stdin();
    let tempfile = match pipeline::drain_to_tempfile(stdin.lock(), suffix) {
        Ok(tf) => tf,
        Err(e) => {
            eprintln!("rusty-vipe: {e}");
            return ExitCode::from(1);
        }
    };

    // Preserve the original stdout sink BEFORE TTY reattachment (HINT-002).
    let preserved_stdout = match tty::preserve_stdout() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("rusty-vipe: failed to preserve stdout: {e}");
            return ExitCode::from(1);
        }
    };

    // Open the controlling TTY (or fall back to test-bypass mode).
    let tty_handles = if pipeline::test_bypass_tty_enabled() {
        None
    } else {
        match tty::open_controlling_tty() {
            Ok(handles) => Some(handles),
            Err(Error::NoControllingTty) => {
                eprintln!("rusty-vipe: no controlling terminal; cannot launch editor");
                return ExitCode::from(1);
            }
            Err(e) => {
                eprintln!("rusty-vipe: {e}");
                return ExitCode::from(1);
            }
        }
    };

    // Spawn editor and wait.
    let extras: Vec<OsString> = cli_args.editor_extras.iter().map(OsString::from).collect();
    let status = match pipeline::spawn_editor(
        &editor_resolved.argv,
        &extras,
        tempfile.path(),
        tty_handles,
    ) {
        Ok(s) => s,
        Err(Error::EditorNotFound(name)) => {
            eprintln!("rusty-vipe: editor not found: {name}");
            return ExitCode::from(127);
        }
        Err(e) => {
            eprintln!("rusty-vipe: {e}");
            return ExitCode::from(1);
        }
    };

    // FR-006: non-zero editor exit aborts; writer (preserved stdout) is NOT touched.
    if !status.success() {
        let code = pipeline::clamp_exit_code(status);
        // Clamp code to u8 for ExitCode::from (codes 1-255). Already clamped
        // upstream; this is just the final type conversion.
        let byte = if (1..=255).contains(&code) {
            code as u8
        } else {
            1u8
        };
        return ExitCode::from(byte);
    }

    // Read tempfile and write to preserved stdout.
    match pipeline::write_back_to_saved_stdout(tempfile.path(), preserved_stdout) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Error::TempFileDeleted(_)) => {
            eprintln!("rusty-vipe: tempfile no longer exists after editor exited");
            ExitCode::from(1)
        }
        Err(e) => {
            eprintln!("rusty-vipe: {e}");
            ExitCode::from(1)
        }
    }
    // tempfile drops here → cleanup
}
