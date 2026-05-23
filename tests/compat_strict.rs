//! US4 (Strict moreutils-Compat Mode) integration tests.
//!
//! Drives the real `rusty-vipe` and `vipe` binaries via `assert_cmd`, asserting
//! Strict-mode stderr text and exit codes match FR-018 / SC-005 expectations.
//!
//! TTY bypass: tests set `RUSTY_VIPE_TEST_BYPASS_TTY=1` because the test
//! process has no controlling terminal — see `src/pipeline.rs` for the
//! documented test-only escape hatch.

mod common;

use assert_cmd::Command;

/// Build a Strict-mode rusty-vipe command. Strict is engaged via the
/// `--strict` flag (not argv[0]) so tests can pin the binary explicitly.
fn rusty_vipe_strict() -> Command {
    let mut cmd = Command::cargo_bin("rusty-vipe").expect("binary built");
    cmd.env("RUSTY_VIPE_TEST_BYPASS_TTY", "1");
    cmd.env_remove("VISUAL");
    cmd.env_remove("EDITOR");
    cmd.env_remove("RUSTY_VIPE_STRICT");
    cmd.arg("--strict");
    cmd
}

#[test]
fn strict_unknown_short_flag_byte_equal_stderr() {
    // FR-018 + SC-005 second bullet: `-x` → first-error formatter.
    let output = rusty_vipe_strict()
        .arg("-x")
        .write_stdin("")
        .assert()
        .failure()
        .get_output()
        .clone();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.trim_end_matches(['\r', '\n']),
        "rusty-vipe: invalid option -- 'x'",
        "FR-018 short-flag formatter must match exactly"
    );
    assert_ne!(output.status.code(), Some(0), "must exit non-zero");
}

#[test]
fn strict_unknown_long_flag_byte_equal_stderr() {
    // FR-018 + SC-005 third bullet: `--foo` → first-error formatter.
    let output = rusty_vipe_strict()
        .arg("--foo")
        .write_stdin("")
        .assert()
        .failure()
        .get_output()
        .clone();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.trim_end_matches(['\r', '\n']),
        "rusty-vipe: unknown option -- 'foo'",
        "FR-018 long-flag formatter must match exactly"
    );
    assert_ne!(output.status.code(), Some(0), "must exit non-zero");
}

#[test]
fn strict_rejects_help_version_editor_completions() {
    // FR-013 + FR-018: --help, --version, --editor=foo, --editor=, completions bash
    // are all rejected via the first-error formatter.
    let cases: &[(&[&str], &str)] = &[
        (&["--help"], "rusty-vipe: unknown option -- 'help'"),
        (&["--version"], "rusty-vipe: unknown option -- 'version'"),
        (&["--editor=foo"], "rusty-vipe: unknown option -- 'editor'"),
        (&["--editor="], "rusty-vipe: unknown option -- 'editor'"),
        (
            &["completions", "bash"],
            "rusty-vipe: unknown option -- 'completions'",
        ),
    ];

    for (extra_args, expected) in cases {
        let output = rusty_vipe_strict()
            .args(*extra_args)
            .write_stdin("")
            .assert()
            .failure()
            .get_output()
            .clone();

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            stderr.trim_end_matches(['\r', '\n']),
            *expected,
            "Strict-mode rejection mismatch for {extra_args:?}"
        );
        assert_ne!(
            output.status.code(),
            Some(0),
            "case {extra_args:?} must exit non-zero"
        );
    }
}

#[test]
fn strict_editor_died_byte_equal_stderr() {
    // FR-018 + SC-005 first bullet: editor non-zero exit → "<argv> exited
    // nonzero, aborting". Drive via $EDITOR=fake-editor --transform=exit-nonzero:1.
    let fake = common::fake_editor_path();
    let fake_str = fake.to_string_lossy().replace('\\', "/");
    let editor_value = format!("{fake_str} '--transform=exit-nonzero:1'");

    let mut cmd = Command::cargo_bin("rusty-vipe").expect("binary built");
    cmd.env("RUSTY_VIPE_TEST_BYPASS_TTY", "1");
    cmd.env_remove("VISUAL");
    cmd.env("EDITOR", &editor_value);
    cmd.env_remove("RUSTY_VIPE_STRICT");
    cmd.arg("--strict");

    let output = cmd.write_stdin("").assert().failure().get_output().clone();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("exited nonzero, aborting"),
        "FR-018 editor-died formatter must appear; got: {stderr:?}"
    );
    // Editor argv starts with the fake-editor path; verify it leads the message.
    assert!(
        stderr.contains("fake-editor"),
        "editor-died message must include the resolved editor argv; got: {stderr:?}"
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "exit-nonzero:1 → clamped exit code 1"
    );
}

#[test]
fn argv0_vipe_implies_strict() {
    // FR-019 + US4 Acceptance #3: `vipe` binary auto-enables Strict mode.
    // We can only test the actual built binary name here — the vipe.exe / vipe
    // built by cargo via the vipe-alias feature.
    let mut cmd = Command::cargo_bin("vipe").expect("vipe-alias binary built");
    cmd.env("RUSTY_VIPE_TEST_BYPASS_TTY", "1");
    cmd.env_remove("VISUAL");
    cmd.env_remove("EDITOR");
    cmd.env_remove("RUSTY_VIPE_STRICT");
    // --help would succeed in Default mode (clap prints help, exit 0). In
    // Strict mode it gets rejected by the first-error formatter, exit non-zero.
    cmd.arg("--help");

    let output = cmd.write_stdin("").assert().failure().get_output().clone();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.trim_end_matches(['\r', '\n']),
        "rusty-vipe: unknown option -- 'help'",
        "FR-019: vipe binary should default to Strict; --help → unknown option"
    );
}

#[test]
fn env_var_activates_strict_mode() {
    // FR-020 + US4 Acceptance #4: RUSTY_VIPE_STRICT=1 against `rusty-vipe`
    // (Default-named binary) engages Strict mode.
    let mut cmd = Command::cargo_bin("rusty-vipe").expect("binary built");
    cmd.env("RUSTY_VIPE_TEST_BYPASS_TTY", "1");
    cmd.env_remove("VISUAL");
    cmd.env_remove("EDITOR");
    cmd.env("RUSTY_VIPE_STRICT", "1");
    cmd.arg("--help");

    let output = cmd.write_stdin("").assert().failure().get_output().clone();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.trim_end_matches(['\r', '\n']),
        "rusty-vipe: unknown option -- 'help'",
        "FR-020: RUSTY_VIPE_STRICT=1 should engage Strict; --help → unknown option"
    );
}

#[test]
fn strict_first_unknown_flag_only() {
    // STF-003 option A (FR-018): when both `-x` and `--foo` are present, only
    // the FIRST unknown-flag error is emitted (left-to-right scan).
    let output = rusty_vipe_strict()
        .arg("-x")
        .arg("--foo")
        .write_stdin("")
        .assert()
        .failure()
        .get_output()
        .clone();

    let stderr = String::from_utf8_lossy(&output.stderr);
    let lines: Vec<&str> = stderr.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(
        lines.len(),
        1,
        "STF-003 option A: only ONE error line expected; got {lines:?}"
    );
    assert_eq!(
        lines[0], "rusty-vipe: invalid option -- 'x'",
        "STF-003 left-to-right: -x is encountered before --foo"
    );
}

#[test]
fn strict_first_unknown_flag_only_reversed_order() {
    // Symmetric of the above with `--foo` first → long-form error wins.
    let output = rusty_vipe_strict()
        .arg("--foo")
        .arg("-x")
        .write_stdin("")
        .assert()
        .failure()
        .get_output()
        .clone();

    let stderr = String::from_utf8_lossy(&output.stderr);
    let lines: Vec<&str> = stderr.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(
        lines.len(),
        1,
        "STF-003 option A: only ONE error line expected; got {lines:?}"
    );
    assert_eq!(lines[0], "rusty-vipe: unknown option -- 'foo'");
}
