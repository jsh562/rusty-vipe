//! US3 (Cross-Platform Editor Resolution, P2) integration tests.
//!
//! Verifies the precedence ladder end-to-end (binary-level), complementing
//! the unit tests in `src/editor.rs`. Each test uses the fake-editor helper
//! plus `--transform=report-argv` so we can inspect WHICH editor binary the
//! resolver picked (argv[0] in the report).

mod common;

use assert_cmd::Command;
use std::fs;

/// Normalize a fake-editor path for embedding into shell-words-parsed env vars.
/// On Windows, backslashes get escape-mangled by shell-words; convert to
/// forward slashes.
fn editor_str(transform: &str) -> String {
    let fake = common::fake_editor_path();
    let fake_str = fake.to_string_lossy().replace('\\', "/");
    format!("{fake_str} '--transform={transform}'")
}

/// Run rusty-vipe with the supplied VISUAL/EDITOR/--editor-flag configuration
/// and the `report-argv` transform. Returns the resolved editor's argv from
/// the side-channel report file.
fn run_with_resolution(
    explicit_editor: Option<&str>,
    visual: Option<&str>,
    editor: Option<&str>,
) -> Vec<String> {
    let tmpdir = common::with_tempdir();
    let report_path = tmpdir.path().join("argv-report.txt");
    let mut cmd = Command::cargo_bin("rusty-vipe").expect("binary built");
    cmd.env("RUSTY_VIPE_TEST_BYPASS_TTY", "1");
    cmd.env("RUSTY_VIPE_FAKE_EDITOR_REPORT", &report_path);
    cmd.env_remove("VISUAL");
    cmd.env_remove("EDITOR");
    if let Some(v) = visual {
        cmd.env("VISUAL", v);
    }
    if let Some(e) = editor {
        cmd.env("EDITOR", e);
    }
    if let Some(e) = explicit_editor {
        cmd.arg(format!("--editor={e}"));
    }
    cmd.write_stdin("").assert().success();
    fs::read_to_string(&report_path)
        .expect("report file should be written")
        .lines()
        .map(|s| s.to_string())
        .collect()
}

#[test]
fn editor_flag_wins_over_visual_and_editor() {
    // FR-009 rung 1: --editor=<cmd> in Default mode wins.
    let argv = run_with_resolution(
        Some(&editor_str("report-argv")),
        Some(&editor_str("delete-line:99")),
        Some(&editor_str("delete-line:99")),
    );
    let argv0 = argv.first().expect("argv[0] in report");
    assert!(
        argv0.contains("fake-editor"),
        "FR-009 rung 1: --editor= must win; got argv[0] = {argv0:?}"
    );
    // The transform arg should be present in the resolved argv.
    assert!(
        argv.iter().any(|a| a == "--transform=report-argv"),
        "FR-009 rung 1: --editor's transform arg must propagate through; \
         got argv = {argv:?}"
    );
}

#[test]
fn visual_wins_over_editor() {
    // FR-009 rung 2: $VISUAL beats $EDITOR.
    let visual_value = editor_str("report-argv");
    let editor_value = editor_str("delete-line:99");
    let argv = run_with_resolution(None, Some(&visual_value), Some(&editor_value));
    // Confirm we got the report-argv transform (proving VISUAL was used).
    assert!(
        argv.iter().any(|a| a == "--transform=report-argv"),
        "FR-009 rung 2: VISUAL must win over EDITOR; got argv = {argv:?}"
    );
}

#[test]
fn editor_used_when_visual_unset() {
    // FR-009 rung 3: $EDITOR used alone.
    let editor_value = editor_str("report-argv");
    let argv = run_with_resolution(None, None, Some(&editor_value));
    assert!(
        argv.iter().any(|a| a == "--transform=report-argv"),
        "FR-009 rung 3: EDITOR must be used when VISUAL is unset; got argv = {argv:?}"
    );
}

#[test]
fn editor_flag_empty_string_falls_through_to_env() {
    // Clarification Q3: --editor='' falls through to env resolution in Default mode.
    let editor_value = editor_str("report-argv");
    let tmpdir = common::with_tempdir();
    let report_path = tmpdir.path().join("argv-report.txt");

    let mut cmd = Command::cargo_bin("rusty-vipe").expect("binary built");
    cmd.env("RUSTY_VIPE_TEST_BYPASS_TTY", "1");
    cmd.env("RUSTY_VIPE_FAKE_EDITOR_REPORT", &report_path);
    cmd.env_remove("VISUAL");
    cmd.env("EDITOR", &editor_value);
    cmd.arg("--editor=");
    cmd.write_stdin("").assert().success();

    let argv: Vec<String> = fs::read_to_string(&report_path)
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .collect();
    assert!(
        argv.iter().any(|a| a == "--transform=report-argv"),
        "Clarification Q3: --editor='' must fall through to env (EDITOR), \
         which has report-argv transform; got argv = {argv:?}"
    );
}

#[test]
fn editor_value_split_via_shell_words() {
    // FR-010: shell-words splits `code --wait` → ["code", "--wait", "<tempfile>"].
    // Use fake-editor + explicit space-separated transform args to verify splitting.
    let fake = common::fake_editor_path()
        .to_string_lossy()
        .replace('\\', "/");
    let editor_value = format!("{fake} '--transform=report-argv'");
    let tmpdir = common::with_tempdir();
    let report_path = tmpdir.path().join("argv-report.txt");

    let mut cmd = Command::cargo_bin("rusty-vipe").expect("binary built");
    cmd.env("RUSTY_VIPE_TEST_BYPASS_TTY", "1");
    cmd.env("RUSTY_VIPE_FAKE_EDITOR_REPORT", &report_path);
    cmd.env_remove("VISUAL");
    cmd.env("EDITOR", &editor_value);
    cmd.write_stdin("").assert().success();

    let argv: Vec<String> = fs::read_to_string(&report_path)
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .collect();
    // We expect at least: [fake-editor-path, --transform=report-argv, <tempfile>]
    assert!(
        argv.len() >= 3,
        "shell-words split should produce ≥3 argv elements; got {argv:?}"
    );
    let argv0 = &argv[0];
    assert!(
        argv0.contains("fake-editor"),
        "argv[0] should be the fake-editor path after shell-words split; got {argv0:?}"
    );
    assert_eq!(argv[1], "--transform=report-argv");
}

#[test]
fn malformed_editor_env_value_exits_127() {
    // FR-010 + FR-016 Clarification: unbalanced quotes in $EDITOR → exit 127.
    let mut cmd = Command::cargo_bin("rusty-vipe").expect("binary built");
    cmd.env("RUSTY_VIPE_TEST_BYPASS_TTY", "1");
    cmd.env_remove("VISUAL");
    cmd.env("EDITOR", "\"unbalanced-quote");
    let output = cmd.write_stdin("").assert().failure().get_output().clone();

    assert_eq!(
        output.status.code(),
        Some(127),
        "FR-010: shell-words parse failure → exit 127"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid EDITOR/VISUAL value") || stderr.contains("invalid editor command"),
        "stderr should mention the EDITOR parse failure; got: {stderr:?}"
    );
}

#[test]
fn editor_binary_not_found_exits_127() {
    // FR-016: editor binary not found at spawn → exit 127.
    let mut cmd = Command::cargo_bin("rusty-vipe").expect("binary built");
    cmd.env("RUSTY_VIPE_TEST_BYPASS_TTY", "1");
    cmd.env_remove("VISUAL");
    cmd.env(
        "EDITOR",
        "an-editor-that-definitely-does-not-exist-12345-abcdef",
    );
    let output = cmd.write_stdin("").assert().failure().get_output().clone();

    assert_eq!(
        output.status.code(),
        Some(127),
        "FR-016: editor not found → exit 127"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("editor not found"),
        "stderr should mention 'editor not found'; got: {stderr:?}"
    );
}
