//! CLI input-validation error tests.
//!
//! Verifies invalid `--suffix` values are rejected at parse time with a clear
//! error message and a non-zero exit code (T084 / FR-012 Edge Cases).

mod common;

use assert_cmd::Command;

/// Run rusty-vipe with the given args. Returns (exit code, stderr).
fn run(args: &[&str]) -> (Option<i32>, String) {
    let mut cmd = Command::cargo_bin("rusty-vipe").expect("binary built");
    cmd.env("RUSTY_VIPE_TEST_BYPASS_TTY", "1");
    cmd.env_remove("VISUAL");
    cmd.env_remove("EDITOR");
    for a in args {
        cmd.arg(a);
    }
    let output = cmd.write_stdin("").assert().failure().get_output().clone();
    let code = output.status.code();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (code, stderr)
}

#[test]
fn invalid_suffix_value_rejected_path_separator_unix() {
    // path separator `/` must be rejected on all platforms.
    let (code, stderr) = run(&["--suffix=/foo"]);
    assert!(
        code.is_some_and(|c| c != 0),
        "must exit non-zero, got {code:?}"
    );
    assert!(
        stderr.contains("--suffix") || stderr.contains("path separators"),
        "stderr should mention suffix rejection; got: {stderr:?}"
    );
}

#[test]
fn invalid_suffix_value_rejected_path_separator_windows() {
    // path separator `\` must also be rejected.
    let (code, stderr) = run(&["--suffix=\\foo"]);
    assert!(code.is_some_and(|c| c != 0));
    assert!(
        stderr.contains("--suffix") || stderr.contains("path separators"),
        "stderr should mention suffix rejection; got: {stderr:?}"
    );
}

#[test]
fn invalid_suffix_value_rejected_excessive_length() {
    // suffix > MAX_SUFFIX_LEN (255) bytes should be rejected.
    let long = "x".repeat(300);
    let arg = format!("--suffix=.{long}");
    let (code, stderr) = run(&[&arg]);
    assert!(code.is_some_and(|c| c != 0));
    assert!(
        stderr.contains("--suffix") || stderr.contains("too long"),
        "stderr should mention length; got: {stderr:?}"
    );
}

#[test]
fn valid_suffix_accepted() {
    // Smoke test: a reasonable suffix must NOT be rejected at parse time.
    let mut cmd = Command::cargo_bin("rusty-vipe").expect("binary built");
    cmd.env("RUSTY_VIPE_TEST_BYPASS_TTY", "1");
    cmd.env_remove("VISUAL");
    let fake = common::fake_editor_path();
    let fake_str = fake.to_string_lossy().replace('\\', "/");
    cmd.env("EDITOR", format!("{fake_str} '--transform=passthrough'"));
    cmd.arg("--suffix=.md");
    cmd.write_stdin("").assert().success();
}
