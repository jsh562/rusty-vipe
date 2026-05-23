//! Compatibility mode resolution.
//!
//! Precedence ladder (FR-019, FR-020):
//! 1. Explicit `--strict` / `--no-strict` flag wins over everything.
//! 2. `RUSTY_VIPE_STRICT=1` env var (any truthy value).
//! 3. `argv[0]` basename equals `vipe` (after `.exe` strip on Windows).
//! 4. Default mode.

use crate::CompatibilityMode;
use std::ffi::OsStr;
use std::path::Path;

/// Resolve the compatibility mode from CLI flag, env var, and argv[0].
pub fn resolve(
    strict_flag: Option<bool>,
    env_strict: Option<&OsStr>,
    argv0: Option<&OsStr>,
) -> CompatibilityMode {
    if let Some(flag) = strict_flag {
        return if flag {
            CompatibilityMode::Strict
        } else {
            CompatibilityMode::Default
        };
    }
    if let Some(value) = env_strict {
        if env_var_is_truthy(value) {
            return CompatibilityMode::Strict;
        }
    }
    if let Some(arg0) = argv0 {
        if argv0_implies_strict(arg0) {
            return CompatibilityMode::Strict;
        }
    }
    CompatibilityMode::Default
}

fn env_var_is_truthy(value: &OsStr) -> bool {
    let Some(s) = value.to_str() else {
        return false;
    };
    matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn argv0_implies_strict(arg0: &OsStr) -> bool {
    let Some(stem) = Path::new(arg0).file_stem() else {
        return false;
    };
    stem == OsStr::new("vipe")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_strict_flag_wins() {
        assert_eq!(resolve(Some(true), None, None), CompatibilityMode::Strict);
        assert_eq!(
            resolve(Some(false), Some(OsStr::new("1")), Some(OsStr::new("vipe"))),
            CompatibilityMode::Default,
            "explicit --no-strict beats env and argv[0]"
        );
    }

    #[test]
    fn env_var_truthy_implies_strict() {
        for v in ["1", "true", "yes", "on", "TRUE", " 1 ", "On"] {
            assert_eq!(
                resolve(None, Some(OsStr::new(v)), None),
                CompatibilityMode::Strict,
                "env value {v:?} should imply strict"
            );
        }
    }

    #[test]
    fn env_var_falsy_does_not_imply_strict() {
        for v in ["0", "false", "no", "off", ""] {
            assert_eq!(
                resolve(None, Some(OsStr::new(v)), None),
                CompatibilityMode::Default,
                "env value {v:?} should NOT imply strict"
            );
        }
    }

    #[test]
    fn argv0_vipe_implies_strict() {
        assert_eq!(
            resolve(None, None, Some(OsStr::new("vipe"))),
            CompatibilityMode::Strict
        );
        assert_eq!(
            resolve(None, None, Some(OsStr::new("/usr/local/bin/vipe"))),
            CompatibilityMode::Strict
        );
        assert_eq!(
            resolve(None, None, Some(OsStr::new("vipe.exe"))),
            CompatibilityMode::Strict,
            "argv[0] = vipe.exe must imply strict (file_stem strips .exe)"
        );
    }

    #[cfg(windows)]
    #[test]
    fn argv0_windows_backslash_path() {
        assert_eq!(
            resolve(None, None, Some(OsStr::new("C:\\bin\\vipe.exe"))),
            CompatibilityMode::Strict
        );
    }

    #[test]
    fn argv0_rusty_vipe_does_not_imply_strict() {
        assert_eq!(
            resolve(None, None, Some(OsStr::new("rusty-vipe"))),
            CompatibilityMode::Default
        );
        assert_eq!(
            resolve(None, None, Some(OsStr::new("rusty-vipe.exe"))),
            CompatibilityMode::Default
        );
    }

    #[test]
    fn default_when_nothing_set() {
        assert_eq!(resolve(None, None, None), CompatibilityMode::Default);
    }

    // ──── T076: explicit precedence-ladder coverage ───────────────────────
    //
    // Ladder: `--strict` > `RUSTY_VIPE_STRICT` > argv[0]=`vipe` > Default
    //
    // Each rung must beat all lower rungs. The matrix tests below assert
    // pairwise precedence at every boundary.

    #[test]
    fn ladder_strict_flag_beats_env_var() {
        // --no-strict explicitly disables Strict even when RUSTY_VIPE_STRICT=1.
        assert_eq!(
            resolve(Some(false), Some(OsStr::new("1")), None),
            CompatibilityMode::Default,
            "ladder rung 1 (--no-strict) must beat rung 2 (env=1)"
        );
        // --strict explicitly enables Strict even when env says no.
        assert_eq!(
            resolve(Some(true), Some(OsStr::new("0")), None),
            CompatibilityMode::Strict,
            "ladder rung 1 (--strict) must beat rung 2 (env=0)"
        );
    }

    #[test]
    fn ladder_env_var_beats_argv0() {
        // env=1 overrides argv[0]=rusty-vipe (which would otherwise be Default).
        assert_eq!(
            resolve(None, Some(OsStr::new("1")), Some(OsStr::new("rusty-vipe"))),
            CompatibilityMode::Strict,
            "ladder rung 2 (env=1) must beat rung 3 (argv0=rusty-vipe → Default)"
        );
        // env=0 (falsy) does NOT engage Strict, but it also doesn't VETO
        // a lower rung — argv[0]=vipe can still engage Strict.
        assert_eq!(
            resolve(None, Some(OsStr::new("0")), Some(OsStr::new("vipe"))),
            CompatibilityMode::Strict,
            "rung 2 falsy is no-op; rung 3 (argv0=vipe) still engages Strict"
        );
    }

    #[test]
    fn ladder_argv0_beats_default() {
        // argv[0]=vipe → Strict, no other signal needed.
        assert_eq!(
            resolve(None, None, Some(OsStr::new("vipe"))),
            CompatibilityMode::Strict,
            "ladder rung 3 (argv0=vipe) beats rung 4 (Default)"
        );
        // argv[0]=rusty-vipe → Default (rung 3 does not apply).
        assert_eq!(
            resolve(None, None, Some(OsStr::new("rusty-vipe"))),
            CompatibilityMode::Default,
            "rung 3 only fires for argv0=vipe; rusty-vipe falls to rung 4"
        );
    }
}
