//! Strict moreutils-compat mode entry point.
//!
//! Mirrors the `rusty-sponge` `strict.rs` pattern: bypasses clap entirely
//! (clap can't produce byte-equal moreutils errors), runs a hand-rolled argv
//! scan, emits moreutils-style stderr per FR-018.
//!
//! ## STF-003 option A
//!
//! For any unknown flag, we emit ONLY the first unknown-flag error and exit
//! non-zero. moreutils' Perl `Getopt::Long` iterates per-character; we accept
//! that as documented divergence (Strict-mode "moreutils-style" rather than
//! "moreutils-byte-equal" — see FR-018 note).
//!
//! ## Recognized inputs (Strict mode)
//!
//! | input                       | behavior                                        |
//! |-----------------------------|-------------------------------------------------|
//! | `--suffix=<ext>`            | set tempfile suffix                             |
//! | `--`                        | end of options; rest are editor extras          |
//! | `--strict` / `--no-strict`  | consumed by mode resolution upstream; ignored   |
//! | `--help` / `--version`      | rejected per FR-013 via first-error formatter   |
//! | `--editor` / `--editor=*`   | rejected per FR-013 via first-error formatter   |
//! | `completions`               | rejected per FR-013 via first-error formatter   |
//! | other `-x` / `--foo`        | first-error formatter (STF-003 option A)        |

use std::ffi::OsString;
use std::io::Write;
use std::process::ExitCode;

use crate::editor;
use crate::pipeline;
use crate::tty;
use crate::{CompatibilityMode, DEFAULT_SUFFIX, Error, validate_suffix};

/// First-error-only formatter for unknown short flags per FR-018.
pub fn format_unknown_flag(flag: &UnknownFlag) -> String {
    match flag {
        UnknownFlag::Short(c) => format!("rusty-vipe: invalid option -- '{c}'"),
        UnknownFlag::Long(name) => format!("rusty-vipe: unknown option -- '{name}'"),
    }
}

/// Moreutils-style "editor died" formatter per FR-018. Output matches the
/// Perl `die` template `<argv joined> exited nonzero, aborting`.
pub fn format_editor_died(argv: &[OsString]) -> String {
    let joined = argv
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ");
    format!("{joined} exited nonzero, aborting")
}

/// Strict-mode entry point. Bypasses clap entirely.
///
/// Returns the process exit code:
/// - 2 — first unknown flag detected (STF-003 option A)
/// - clamped editor exit code (1–255 Unix, 1–254 Windows) — on non-zero editor exit
/// - 127 — editor not found / invalid env editor value
/// - 0 — success
pub fn run(argv: &[OsString]) -> ExitCode {
    let parsed = match parse_argv(argv) {
        Ok(p) => p,
        Err(ParseError::Unknown(unk)) => {
            let msg = format_unknown_flag(&unk);
            let _ = writeln!(std::io::stderr().lock(), "{msg}");
            return ExitCode::from(2);
        }
        Err(ParseError::InvalidSuffix(reason)) => {
            let _ = writeln!(std::io::stderr().lock(), "rusty-vipe: {reason}");
            return ExitCode::from(2);
        }
    };

    // Resolve editor via the same ladder as Default mode, except no
    // `--editor` override is permitted in Strict mode (FR-013).
    let env_visual = std::env::var("VISUAL").ok();
    let env_editor = std::env::var("EDITOR").ok();
    let resolved = match editor::resolve(
        None,
        env_visual.as_deref(),
        env_editor.as_deref(),
        CompatibilityMode::Strict,
    ) {
        Ok(r) => r,
        Err(Error::InvalidEditorCommand(raw)) => {
            let _ = writeln!(
                std::io::stderr().lock(),
                "rusty-vipe: invalid EDITOR/VISUAL value: {raw}"
            );
            return ExitCode::from(127);
        }
        Err(e) => {
            let _ = writeln!(std::io::stderr().lock(), "rusty-vipe: {e}");
            return ExitCode::from(127);
        }
    };

    // Drain stdin to a tempfile with the configured (or default) suffix.
    let suffix = parsed.suffix.as_deref().unwrap_or(DEFAULT_SUFFIX);
    let stdin = std::io::stdin();
    let tempfile = match pipeline::drain_to_tempfile(stdin.lock(), suffix) {
        Ok(tf) => tf,
        Err(e) => {
            let _ = writeln!(std::io::stderr().lock(), "rusty-vipe: {e}");
            return ExitCode::from(1);
        }
    };

    let preserved_stdout = match tty::preserve_stdout() {
        Ok(p) => p,
        Err(e) => {
            let _ = writeln!(
                std::io::stderr().lock(),
                "rusty-vipe: failed to preserve stdout: {e}"
            );
            return ExitCode::from(1);
        }
    };

    let tty_handles = if pipeline::test_bypass_tty_enabled() {
        None
    } else {
        match tty::open_controlling_tty() {
            Ok(handles) => Some(handles),
            Err(Error::NoControllingTty) => {
                let _ = writeln!(
                    std::io::stderr().lock(),
                    "rusty-vipe: no controlling terminal; cannot launch editor"
                );
                return ExitCode::from(1);
            }
            Err(e) => {
                let _ = writeln!(std::io::stderr().lock(), "rusty-vipe: {e}");
                return ExitCode::from(1);
            }
        }
    };

    let extras: Vec<OsString> = parsed.editor_extras;
    let status = match pipeline::spawn_editor(&resolved.argv, &extras, tempfile.path(), tty_handles)
    {
        Ok(s) => s,
        Err(Error::EditorNotFound(name)) => {
            let _ = writeln!(
                std::io::stderr().lock(),
                "rusty-vipe: editor not found: {name}"
            );
            return ExitCode::from(127);
        }
        Err(e) => {
            let _ = writeln!(std::io::stderr().lock(), "rusty-vipe: {e}");
            return ExitCode::from(1);
        }
    };

    if !status.success() {
        // FR-018: emit moreutils-style "exited nonzero, aborting" with the
        // FULL editor argv (resolved + extras + tempfile path).
        let mut full_argv: Vec<OsString> = resolved.argv.clone();
        full_argv.extend(extras.iter().cloned());
        full_argv.push(tempfile.path().to_path_buf().into_os_string());
        let _ = writeln!(
            std::io::stderr().lock(),
            "{}",
            format_editor_died(&full_argv)
        );

        let code = pipeline::clamp_exit_code(status);
        let byte = if (1..=255).contains(&code) {
            code as u8
        } else {
            1u8
        };
        return ExitCode::from(byte);
    }

    match pipeline::write_back_to_saved_stdout(tempfile.path(), preserved_stdout) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Error::TempFileDeleted(_)) => {
            let _ = writeln!(
                std::io::stderr().lock(),
                "rusty-vipe: tempfile no longer exists after editor exited"
            );
            ExitCode::from(1)
        }
        Err(e) => {
            let _ = writeln!(std::io::stderr().lock(), "rusty-vipe: {e}");
            ExitCode::from(1)
        }
    }
}

/// Pre-clap scan for `--strict` / `--no-strict` flags. Returns the resolved
/// strict_flag value for [`crate::mode::resolve`] (last occurrence wins).
pub fn pre_scan_strict_flag(argv: &[OsString]) -> Option<bool> {
    let mut chosen: Option<bool> = None;
    for arg in argv.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s == "--strict" {
            chosen = Some(true);
        } else if s == "--no-strict" {
            chosen = Some(false);
        } else if s == "--" {
            break;
        }
    }
    chosen
}

/// The first unknown flag encountered by the Strict-mode parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnknownFlag {
    /// Single-character short flag, e.g. `-x` → `Short('x')`.
    Short(char),
    /// Long flag with `--` prefix stripped, e.g. `--foo` → `Long("foo")`.
    Long(String),
}

/// Errors returned by [`parse_argv`] when the Strict-mode argv is malformed.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ParseError {
    /// First unknown flag encountered (STF-003 option A).
    Unknown(UnknownFlag),
    /// `--suffix=<bad>` failed validation.
    InvalidSuffix(&'static str),
}

/// Result of scanning the strict-mode argv.
#[derive(Debug, Default)]
struct StrictArgs {
    suffix: Option<String>,
    editor_extras: Vec<OsString>,
}

/// Parse argv per Strict-mode rules. On the FIRST unknown flag, return
/// `Err(UnknownFlag)` — the caller emits one error line and exits non-zero.
///
/// Per FR-013, the following are rejected as unknown flags:
/// - `--help`, `--version`
/// - `--editor` (with or without `=value`)
/// - `completions` (subcommand-style positional)
fn parse_argv(argv: &[OsString]) -> Result<StrictArgs, ParseError> {
    let mut out = StrictArgs::default();
    let mut iter = argv.iter().skip(1);

    while let Some(arg) = iter.next() {
        let s = arg.to_string_lossy();

        // Upstream mode-resolution flags — already consumed.
        if s == "--strict" || s == "--no-strict" {
            continue;
        }

        // End-of-options sentinel: rest are editor extras.
        if s == "--" {
            for rest in iter.by_ref() {
                out.editor_extras.push(rest.clone());
            }
            break;
        }

        // `completions` subcommand — rejected per FR-013.
        if s == "completions" {
            return Err(ParseError::Unknown(UnknownFlag::Long(String::from(
                "completions",
            ))));
        }

        // Long flags.
        if let Some(rest) = s.strip_prefix("--") {
            // `--suffix=<ext>` is the only accepted long flag.
            if let Some(value) = rest.strip_prefix("suffix=") {
                validate_suffix(value).map_err(ParseError::InvalidSuffix)?;
                out.suffix = Some(value.to_string());
                continue;
            }
            // `--suffix` (no value, value follows in next arg) — also accept.
            if rest == "suffix" {
                let value = iter
                    .next()
                    .map(|v| v.to_string_lossy().into_owned())
                    .unwrap_or_default();
                validate_suffix(&value).map_err(ParseError::InvalidSuffix)?;
                out.suffix = Some(value);
                continue;
            }
            // Anything else (--help, --version, --editor, --editor=foo, --foo) is unknown.
            // For `--editor=foo` form, the flag name is `editor` (strip the value part).
            let flag_name = rest.split('=').next().unwrap_or(rest).to_string();
            return Err(ParseError::Unknown(UnknownFlag::Long(flag_name)));
        }

        // Short flags (`-x`, `-xyz`).
        if let Some(rest) = s.strip_prefix('-') {
            if !rest.is_empty() {
                // First unknown short letter wins.
                let first = rest.chars().next().expect("non-empty after strip_prefix");
                return Err(ParseError::Unknown(UnknownFlag::Short(first)));
            }
        }

        // Positional: editor extra (forwarded before the tempfile path).
        out.editor_extras.push(arg.clone());
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(parts: &[&str]) -> Vec<OsString> {
        parts.iter().map(|s| OsString::from(*s)).collect()
    }

    #[test]
    fn pre_scan_detects_strict() {
        assert_eq!(
            pre_scan_strict_flag(&argv(&["rusty-vipe", "--strict"])),
            Some(true)
        );
    }

    #[test]
    fn pre_scan_detects_no_strict() {
        assert_eq!(
            pre_scan_strict_flag(&argv(&["rusty-vipe", "--no-strict"])),
            Some(false)
        );
    }

    #[test]
    fn pre_scan_returns_none_when_neither() {
        assert_eq!(pre_scan_strict_flag(&argv(&["rusty-vipe"])), None);
    }

    #[test]
    fn pre_scan_last_occurrence_wins() {
        assert_eq!(
            pre_scan_strict_flag(&argv(&["rusty-vipe", "--strict", "--no-strict"])),
            Some(false)
        );
    }

    #[test]
    fn pre_scan_stops_at_double_dash() {
        assert_eq!(
            pre_scan_strict_flag(&argv(&["rusty-vipe", "--", "--strict"])),
            None,
            "--strict after -- is a positional, not the strict flag"
        );
    }

    #[test]
    fn parse_no_flags_yields_defaults() {
        let r = parse_argv(&argv(&["vipe"])).unwrap();
        assert_eq!(r.suffix, None);
        assert!(r.editor_extras.is_empty());
    }

    #[test]
    fn parse_suffix_equals_value() {
        let r = parse_argv(&argv(&["vipe", "--suffix=.md"])).unwrap();
        assert_eq!(r.suffix.as_deref(), Some(".md"));
    }

    #[test]
    fn parse_suffix_separate_value() {
        let r = parse_argv(&argv(&["vipe", "--suffix", ".json"])).unwrap();
        assert_eq!(r.suffix.as_deref(), Some(".json"));
    }

    fn unwrap_unknown(err: ParseError) -> UnknownFlag {
        match err {
            ParseError::Unknown(u) => u,
            other => panic!("expected ParseError::Unknown, got {other:?}"),
        }
    }

    #[test]
    fn parse_rejects_help() {
        let err = parse_argv(&argv(&["vipe", "--help"])).unwrap_err();
        assert_eq!(unwrap_unknown(err), UnknownFlag::Long(String::from("help")));
    }

    #[test]
    fn parse_rejects_version() {
        let err = parse_argv(&argv(&["vipe", "--version"])).unwrap_err();
        assert_eq!(
            unwrap_unknown(err),
            UnknownFlag::Long(String::from("version"))
        );
    }

    #[test]
    fn parse_rejects_editor_flag() {
        let err = parse_argv(&argv(&["vipe", "--editor=foo"])).unwrap_err();
        assert_eq!(
            unwrap_unknown(err),
            UnknownFlag::Long(String::from("editor"))
        );
    }

    #[test]
    fn parse_rejects_editor_flag_empty() {
        let err = parse_argv(&argv(&["vipe", "--editor="])).unwrap_err();
        assert_eq!(
            unwrap_unknown(err),
            UnknownFlag::Long(String::from("editor"))
        );
    }

    #[test]
    fn parse_rejects_completions_subcommand() {
        let err = parse_argv(&argv(&["vipe", "completions", "bash"])).unwrap_err();
        assert_eq!(
            unwrap_unknown(err),
            UnknownFlag::Long(String::from("completions"))
        );
    }

    #[test]
    fn parse_rejects_unknown_long_flag() {
        let err = parse_argv(&argv(&["vipe", "--foo"])).unwrap_err();
        assert_eq!(unwrap_unknown(err), UnknownFlag::Long(String::from("foo")));
    }

    #[test]
    fn parse_rejects_unknown_short_flag() {
        let err = parse_argv(&argv(&["vipe", "-x"])).unwrap_err();
        assert_eq!(unwrap_unknown(err), UnknownFlag::Short('x'));
    }

    #[test]
    fn parse_first_unknown_wins_when_short_and_long_both_present() {
        // STF-003 option A: only first unknown is reported. Order matters.
        let err = parse_argv(&argv(&["vipe", "-x", "--foo"])).unwrap_err();
        assert_eq!(unwrap_unknown(err), UnknownFlag::Short('x'));

        let err = parse_argv(&argv(&["vipe", "--foo", "-x"])).unwrap_err();
        assert_eq!(unwrap_unknown(err), UnknownFlag::Long(String::from("foo")));
    }

    #[test]
    fn parse_grouped_short_unknown_reports_first_char() {
        // `-xyz` → first char `x` is the reported unknown.
        let err = parse_argv(&argv(&["vipe", "-xyz"])).unwrap_err();
        assert_eq!(unwrap_unknown(err), UnknownFlag::Short('x'));
    }

    #[test]
    fn parse_positional_becomes_editor_extra() {
        let r = parse_argv(&argv(&["vipe", "--wait", "extra"])).unwrap_err();
        // `--wait` is unknown long → error wins before positional is collected.
        assert_eq!(unwrap_unknown(r), UnknownFlag::Long(String::from("wait")));

        // Pure positionals are collected as editor extras.
        let r = parse_argv(&argv(&["vipe", "extra-arg"])).unwrap();
        assert_eq!(r.editor_extras, vec![OsString::from("extra-arg")]);
    }

    #[test]
    fn parse_rejects_invalid_suffix_path_separator() {
        let err = parse_argv(&argv(&["vipe", "--suffix=/foo"])).unwrap_err();
        assert!(
            matches!(err, ParseError::InvalidSuffix(_)),
            "/foo should fail suffix validation, got {err:?}"
        );
    }

    #[test]
    fn parse_rejects_invalid_suffix_too_long() {
        let long = "a".repeat(300);
        let arg = format!("--suffix={long}");
        let argv_vec: Vec<OsString> = vec![OsString::from("vipe"), OsString::from(arg)];
        let err = parse_argv(&argv_vec).unwrap_err();
        assert!(matches!(err, ParseError::InvalidSuffix(_)));
    }

    #[test]
    fn parse_double_dash_treats_rest_as_extras() {
        let r = parse_argv(&argv(&["vipe", "--", "--help", "-x"])).unwrap();
        assert_eq!(
            r.editor_extras,
            vec![OsString::from("--help"), OsString::from("-x")],
            "after `--` everything is an editor extra"
        );
    }

    #[test]
    fn parse_strict_and_no_strict_are_silently_consumed() {
        let r = parse_argv(&argv(&["vipe", "--strict", "--suffix=.md", "--no-strict"])).unwrap();
        assert_eq!(r.suffix.as_deref(), Some(".md"));
    }

    #[test]
    fn format_unknown_short_flag_matches_spec_text() {
        let msg = format_unknown_flag(&UnknownFlag::Short('x'));
        assert_eq!(msg, "rusty-vipe: invalid option -- 'x'");
    }

    #[test]
    fn format_unknown_long_flag_matches_spec_text() {
        let msg = format_unknown_flag(&UnknownFlag::Long(String::from("foo")));
        assert_eq!(msg, "rusty-vipe: unknown option -- 'foo'");
    }

    #[test]
    fn format_editor_died_joins_argv_with_spaces() {
        let argv = vec![
            OsString::from("vi"),
            OsString::from("--wait"),
            OsString::from("/tmp/foo.txt"),
        ];
        assert_eq!(
            format_editor_died(&argv),
            "vi --wait /tmp/foo.txt exited nonzero, aborting"
        );
    }

    #[test]
    fn format_editor_died_single_arg() {
        let argv = vec![OsString::from("vi")];
        assert_eq!(format_editor_died(&argv), "vi exited nonzero, aborting");
    }
}
