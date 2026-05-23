//! Editor resolution + argv parsing.
//!
//! Precedence ladder (FR-009):
//! 1. `--editor=<cmd>` flag (Default mode only); empty value falls through.
//! 2. `$VISUAL`
//! 3. `$EDITOR`
//! 4. `/usr/bin/editor` (Unix only; existence + executable check)
//! 5. `vi` (Unix) / `notepad.exe` (Windows)

use std::ffi::OsString;
use std::path::Path;

use crate::{CompatibilityMode, EditorSource, Error};

/// Result of resolving the editor command: the argv to spawn (NOT yet
/// including the tempfile path) plus the resolved source for diagnostics.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub argv: Vec<OsString>,
    pub source: EditorSource,
}

/// Walk the resolution ladder. Returns the editor argv (without tempfile)
/// and the matched source rung.
pub fn resolve(
    explicit_override: Option<&str>,
    env_visual: Option<&str>,
    env_editor: Option<&str>,
    mode: CompatibilityMode,
) -> Result<Resolved, Error> {
    // (1) Explicit override (Default mode only; empty falls through).
    if mode == CompatibilityMode::Default {
        if let Some(cmd) = explicit_override {
            if !cmd.is_empty() {
                let argv = parse_editor_value(cmd)?;
                if !argv.is_empty() {
                    return Ok(Resolved {
                        argv,
                        source: EditorSource::Override(cmd.to_string()),
                    });
                }
            }
        }
    }

    // (2) $VISUAL
    if let Some(v) = env_visual {
        if !v.is_empty() {
            let argv = parse_editor_value(v)?;
            if !argv.is_empty() {
                return Ok(Resolved {
                    argv,
                    source: EditorSource::EnvLookup,
                });
            }
        }
    }

    // (3) $EDITOR
    if let Some(v) = env_editor {
        if !v.is_empty() {
            let argv = parse_editor_value(v)?;
            if !argv.is_empty() {
                return Ok(Resolved {
                    argv,
                    source: EditorSource::EnvLookup,
                });
            }
        }
    }

    // (4) Unix-only: /usr/bin/editor if executable + resolvable.
    #[cfg(unix)]
    {
        const USR_BIN_EDITOR: &str = "/usr/bin/editor";
        if is_unix_executable(Path::new(USR_BIN_EDITOR)) {
            return Ok(Resolved {
                argv: vec![OsString::from(USR_BIN_EDITOR)],
                source: EditorSource::EnvLookup,
            });
        }
    }

    // (5) Platform default.
    #[cfg(unix)]
    let fallback = "vi";
    #[cfg(windows)]
    let fallback = "notepad.exe";

    Ok(Resolved {
        argv: vec![OsString::from(fallback)],
        source: EditorSource::EnvLookup,
    })
}

/// Parse an editor command string into argv using `shell-words` rules.
/// Returns `Error::InvalidEditorCommand(value.to_string())` on parse failure
/// (per FR-010 + STF-003).
pub fn parse_editor_value(value: &str) -> Result<Vec<OsString>, Error> {
    match shell_words::split(value) {
        Ok(parts) => Ok(parts.into_iter().map(OsString::from).collect()),
        Err(_) => Err(Error::InvalidEditorCommand(value.to_string())),
    }
}

/// Returns true if the path exists, is a regular (or symlink-resolving-to-regular)
/// file, and has at least one executable mode bit set on Unix.
#[cfg(unix)]
fn is_unix_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    // metadata() follows symlinks — if the link target is missing, this returns Err.
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    meta.is_file() && (meta.permissions().mode() & 0o111) != 0
}

#[cfg(not(unix))]
#[allow(dead_code)]
fn is_unix_executable(_path: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_command() {
        let argv = parse_editor_value("vi").unwrap();
        assert_eq!(argv, vec![OsString::from("vi")]);
    }

    #[test]
    fn parse_command_with_arg() {
        let argv = parse_editor_value("code --wait").unwrap();
        assert_eq!(argv, vec![OsString::from("code"), OsString::from("--wait")]);
    }

    #[test]
    fn parse_quoted_path_with_spaces() {
        let argv = parse_editor_value(r#""path with spaces/editor" --flag"#).unwrap();
        assert_eq!(
            argv,
            vec![
                OsString::from("path with spaces/editor"),
                OsString::from("--flag")
            ]
        );
    }

    #[test]
    fn parse_unbalanced_quote_returns_typed_error() {
        let raw = r#""unbalanced"#;
        match parse_editor_value(raw) {
            Err(Error::InvalidEditorCommand(s)) => assert_eq!(s, raw),
            other => panic!("expected InvalidEditorCommand, got {other:?}"),
        }
    }

    #[test]
    fn resolve_override_wins_in_default_mode() {
        let r = resolve(
            Some("my-editor"),
            Some("/should/be/ignored"),
            Some("/also/ignored"),
            CompatibilityMode::Default,
        )
        .unwrap();
        assert_eq!(r.argv, vec![OsString::from("my-editor")]);
        assert!(matches!(r.source, EditorSource::Override(_)));
    }

    #[test]
    fn resolve_override_ignored_in_strict_mode() {
        // Strict mode should NOT consume the --editor override; falls through
        // to env. (The builder-level rejection happens earlier in real usage;
        // this test verifies the resolver itself is correct even if a Strict
        // mode somehow had an Override.)
        let r = resolve(
            Some("override"),
            Some("from-visual"),
            None,
            CompatibilityMode::Strict,
        )
        .unwrap();
        assert_eq!(r.argv, vec![OsString::from("from-visual")]);
    }

    #[test]
    fn resolve_empty_override_falls_through() {
        let r = resolve(
            Some(""),
            Some("from-visual"),
            None,
            CompatibilityMode::Default,
        )
        .unwrap();
        assert_eq!(r.argv, vec![OsString::from("from-visual")]);
    }

    #[test]
    fn resolve_visual_wins_over_editor() {
        let r = resolve(
            None,
            Some("from-visual"),
            Some("from-editor"),
            CompatibilityMode::Default,
        )
        .unwrap();
        assert_eq!(r.argv, vec![OsString::from("from-visual")]);
    }

    #[test]
    fn resolve_editor_used_when_visual_unset() {
        let r = resolve(None, None, Some("from-editor"), CompatibilityMode::Default).unwrap();
        assert_eq!(r.argv, vec![OsString::from("from-editor")]);
    }

    #[cfg(unix)]
    #[test]
    fn resolve_unix_falls_back_to_vi_when_nothing_else() {
        // /usr/bin/editor may exist on the test host; we can't assert which
        // rung wins without overriding PATH. Instead assert that the result
        // is either /usr/bin/editor or "vi".
        let r = resolve(None, None, None, CompatibilityMode::Default).unwrap();
        let first = r.argv.first().expect("at least argv[0]");
        assert!(
            first == &OsString::from("/usr/bin/editor") || first == &OsString::from("vi"),
            "Unix fallback should be /usr/bin/editor or vi, got {first:?}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn resolve_windows_falls_back_to_notepad() {
        let r = resolve(None, None, None, CompatibilityMode::Default).unwrap();
        assert_eq!(r.argv, vec![OsString::from("notepad.exe")]);
    }
}
