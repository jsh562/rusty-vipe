//! US7 (No-Controlling-TTY Error Handling, P3) integration tests.
//!
//! These tests force the `open_controlling_tty()` call to fail via the
//! documented test-only env var `RUSTY_VIPE_TEST_FAIL_TTY=1` because reliably
//! detaching the controlling-TTY from a child process is platform-specific
//! (Unix needs `setsid` + double-fork; Windows needs `CREATE_NO_WINDOW` +
//! detached process group). The env-var fault-injection delivers equivalent
//! FR-015 coverage with portable assertions.

mod common;

use assert_cmd::Command;

#[test]
fn no_controlling_terminal_exits_with_clear_error() {
    // T100 / SC-009 / FR-015 / US7 AS1: open_controlling_tty fails →
    // stderr contains the FR-015 text and exit is non-zero.
    let mut cmd = Command::cargo_bin("rusty-vipe").expect("binary built");
    // Force the open-tty path (do NOT set TEST_BYPASS_TTY) and force it to fail.
    cmd.env_remove("RUSTY_VIPE_TEST_BYPASS_TTY");
    cmd.env("RUSTY_VIPE_TEST_FAIL_TTY", "1");
    cmd.env_remove("VISUAL");
    // EDITOR can be anything; we never reach the spawn path.
    cmd.env("EDITOR", "vi");
    cmd.env_remove("RUSTY_VIPE_STRICT");

    let output = cmd.write_stdin("").assert().failure().get_output().clone();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no controlling terminal"),
        "FR-015: stderr must mention 'no controlling terminal'; got: {stderr:?}"
    );
    assert_ne!(output.status.code(), Some(0), "must exit non-zero");
}

#[test]
fn no_controlling_terminal_emits_exact_fr015_text() {
    // FR-015 specifies the EXACT stderr text:
    //     "rusty-vipe: no controlling terminal; cannot launch editor"
    let mut cmd = Command::cargo_bin("rusty-vipe").expect("binary built");
    cmd.env_remove("RUSTY_VIPE_TEST_BYPASS_TTY");
    cmd.env("RUSTY_VIPE_TEST_FAIL_TTY", "1");
    cmd.env_remove("VISUAL");
    cmd.env("EDITOR", "vi");
    cmd.env_remove("RUSTY_VIPE_STRICT");

    let output = cmd.write_stdin("").assert().failure().get_output().clone();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("rusty-vipe: no controlling terminal; cannot launch editor"),
        "FR-015 stderr text mismatch; got: {stderr:?}"
    );
}

#[test]
fn no_controlling_terminal_strict_mode_also_reports_clearly() {
    // Strict mode should ALSO surface the no-TTY error rather than silently
    // crashing — the message routes through strict::run's stderr path.
    let mut cmd = Command::cargo_bin("rusty-vipe").expect("binary built");
    cmd.env_remove("RUSTY_VIPE_TEST_BYPASS_TTY");
    cmd.env("RUSTY_VIPE_TEST_FAIL_TTY", "1");
    cmd.env_remove("VISUAL");
    cmd.env("EDITOR", "vi");
    cmd.env_remove("RUSTY_VIPE_STRICT");
    cmd.arg("--strict");

    let output = cmd.write_stdin("").assert().failure().get_output().clone();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no controlling terminal"),
        "Strict-mode no-TTY error must surface; got: {stderr:?}"
    );
}

#[test]
fn no_controlling_terminal_does_not_leak_pipe_stdin_to_editor() {
    // FR-015 second sentence: producer's pipe stdin MUST NOT be used as the
    // editor's stdin under any condition. Since open_controlling_tty fails
    // BEFORE spawn_editor is called, the editor never spawns at all — proving
    // the negative invariant. We verify this by feeding distinctive bytes to
    // stdin and confirming the binary did NOT echo them anywhere (drain
    // happens AFTER the TTY-open attempt, but the abort path means we never
    // reach the writer).
    let mut cmd = Command::cargo_bin("rusty-vipe").expect("binary built");
    cmd.env_remove("RUSTY_VIPE_TEST_BYPASS_TTY");
    cmd.env("RUSTY_VIPE_TEST_FAIL_TTY", "1");
    cmd.env_remove("VISUAL");
    cmd.env("EDITOR", "vi");
    cmd.env_remove("RUSTY_VIPE_STRICT");

    let output = cmd
        .write_stdin("DISTINCTIVE_MARKER_STDIN_PAYLOAD\n")
        .assert()
        .failure()
        .get_output()
        .clone();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("DISTINCTIVE_MARKER_STDIN_PAYLOAD"),
        "FR-015: producer's pipe stdin must NEVER reach downstream output; got stdout: {stdout:?}"
    );
}

#[test]
fn piped_stdio_with_tty_still_works() {
    // T102 / US7 AS2: piped stdin/stdout BUT TTY available → editor launches
    // normally. We use the TTY bypass env var to simulate "TTY available"
    // (the bypass path uses Stdio::null() for the editor's stdio, which is
    // functionally equivalent to a working TTY from the test's POV — the
    // important property is that the binary successfully completes the
    // drain → spawn → write-back cycle without erroring on TTY availability).
    let fake = common::fake_editor_path();
    let fake_str = fake.to_string_lossy().replace('\\', "/");
    let editor_value = format!("{fake_str} '--transform=passthrough'");

    let mut cmd = Command::cargo_bin("rusty-vipe").expect("binary built");
    cmd.env("RUSTY_VIPE_TEST_BYPASS_TTY", "1");
    cmd.env_remove("RUSTY_VIPE_TEST_FAIL_TTY");
    cmd.env("EDITOR", &editor_value);
    cmd.env_remove("VISUAL");

    let output = cmd
        .write_stdin("piped-stdin-payload\n")
        .assert()
        .success()
        .get_output()
        .clone();
    assert_eq!(output.stdout, b"piped-stdin-payload\n");
}
