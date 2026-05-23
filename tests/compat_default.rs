//! US1 (Edit-in-Pipe Basic Flow) integration tests.
//!
//! Drives the real `rusty-vipe` binary via `assert_cmd`, using the deterministic
//! `fake-editor` helper (gated behind the `dev-helpers` Cargo feature). Tests
//! set `RUSTY_VIPE_TEST_BYPASS_TTY=1` because the test process doesn't have a
//! controlling terminal — this is the documented test-only escape hatch.

mod common;

use assert_cmd::Command;
use common::snapshot_tempdir;
use std::fs;

/// Build the rusty-vipe Command with EDITOR pointing at the fake-editor + the
/// requested transform, and the TTY bypass enabled for test mode.
///
/// Windows-portability note: `shell-words::split` (used by `editor::resolve`
/// to parse `$EDITOR`) treats backslash as a POSIX shell escape. We
/// normalize the fake-editor path to forward slashes — Windows accepts
/// forward slashes in `Command::new` program paths — so the EDITOR value
/// parses cleanly across platforms.
fn rusty_vipe_with_fake(transform: &str) -> Command {
    let fake = common::fake_editor_path();
    let fake_str = fake.to_string_lossy().replace('\\', "/");
    // POSIX single-quote the entire --transform arg so shell-words preserves
    // literal bytes (spaces, backslash-n, etc.) inside the transform value.
    // Caller MUST NOT include a literal single quote in `transform` — POSIX
    // single-quoted strings have no escape. None of our test transforms do.
    assert!(
        !transform.contains('\''),
        "transform must not contain a single quote (POSIX limitation)"
    );
    let editor_value = format!("{fake_str} '--transform={transform}'");
    let mut cmd = Command::cargo_bin("rusty-vipe").expect("binary built");
    cmd.env("RUSTY_VIPE_TEST_BYPASS_TTY", "1");
    cmd.env("EDITOR", &editor_value);
    cmd.env_remove("VISUAL");
    cmd
}

#[test]
fn small_text_delete_line_2_byte_equal() {
    // SC-001 small-text fixture + delete-line:2 transform.
    let output = rusty_vipe_with_fake("delete-line:2")
        .write_stdin("alpha\nbravo\ncharlie\ndelta\necho\n")
        .assert()
        .success()
        .get_output()
        .clone();

    assert_eq!(
        output.stdout, b"alpha\ncharlie\ndelta\necho\n",
        "FR-001 + FR-005 + US1 AS1: delete-line:2 removes 'bravo'"
    );
}

#[test]
fn small_text_delete_line_4_byte_equal() {
    // Cover the second deletion from US1 AS1 (delete 4th line).
    let output = rusty_vipe_with_fake("delete-line:4")
        .write_stdin("alpha\nbravo\ncharlie\ndelta\necho\n")
        .assert()
        .success()
        .get_output()
        .clone();

    assert_eq!(output.stdout, b"alpha\nbravo\ncharlie\necho\n");
}

#[test]
fn empty_stdin_replace_from_scratch_byte_equal() {
    // SC-001 case (a): empty stdin + editor writes from scratch.
    let output = rusty_vipe_with_fake("replace:from-scratch\n")
        .write_stdin("")
        .assert()
        .success()
        .get_output()
        .clone();

    assert_eq!(
        output.stdout, b"from-scratch\n",
        "FR-008 + US1 AS2: empty stdin still spawns editor; output is editor's content"
    );
}

#[test]
fn passthrough_returns_original_bytes_unchanged() {
    // US1 AS3 + FR-017: binary-input passthrough.
    let bytes: &[u8] = b"\x00\x01\x02\xff\xfe\xfd\n\xc3\x28\xa0\xa1\x80\x81\r\n\xef";
    let output = rusty_vipe_with_fake("passthrough")
        .write_stdin(bytes)
        .assert()
        .success()
        .get_output()
        .clone();

    assert_eq!(
        output.stdout, bytes,
        "FR-017: non-UTF-8 bytes pass through unchanged"
    );
}

#[test]
fn small_text_passthrough_byte_equal() {
    let output = rusty_vipe_with_fake("passthrough")
        .write_stdin("hello\nworld\n")
        .assert()
        .success()
        .get_output()
        .clone();

    assert_eq!(output.stdout, b"hello\nworld\n");
}

#[test]
fn editor_replace_overwrites_content() {
    let output = rusty_vipe_with_fake("replace:NEW CONTENT\n")
        .write_stdin("original\n")
        .assert()
        .success()
        .get_output()
        .clone();

    assert_eq!(output.stdout, b"NEW CONTENT\n");
}

#[test]
fn extra_positional_args_forwarded_before_tempfile() {
    // FR-011: extra positionals from vipe's argv are forwarded to the editor
    // BEFORE the tempfile path.
    let tmpdir = common::with_tempdir();
    let report_path = tmpdir.path().join("report.txt");

    rusty_vipe_with_fake("report-argv")
        .env("RUSTY_VIPE_FAKE_EDITOR_REPORT", &report_path)
        .arg("extra1")
        .arg("extra2")
        .write_stdin("")
        .assert()
        .success();

    let report = fs::read_to_string(&report_path).expect("report-argv should have written");
    let lines: Vec<&str> = report.lines().collect();
    assert!(
        lines.len() >= 4,
        "expected at least argv[0] + --transform + extras + tempfile path; got {lines:?}"
    );
    // The fake-editor's own argv: [argv0, "--transform=report-argv", "extra1", "extra2", "<tempfile-path>"]
    // (the EDITOR env value is space-split by shell-words → argv[0] = fake-editor path,
    //  argv[1] = "--transform=report-argv", then extras, then tempfile)
    let last = lines.last().unwrap();
    assert!(
        last.contains(".rusty-vipe-"),
        "last argv element should be the tempfile path (with `.rusty-vipe-` prefix), got {last:?}"
    );
    // Extras should appear immediately before the tempfile path.
    let n = lines.len();
    assert_eq!(
        lines[n - 3],
        "extra1",
        "extra1 must be third-from-last in argv"
    );
    assert_eq!(
        lines[n - 2],
        "extra2",
        "extra2 must be second-from-last (right before tempfile path)"
    );
}

#[test]
fn suffix_flag_propagates_to_tempfile_extension() {
    // FR-012: --suffix=.json produces tempfile ending in .json.
    let tmpdir = common::with_tempdir();
    let report_path = tmpdir.path().join("report.txt");

    rusty_vipe_with_fake("report-filename")
        .env("RUSTY_VIPE_FAKE_EDITOR_REPORT", &report_path)
        .arg("--suffix=.json")
        .write_stdin("")
        .assert()
        .success();

    let report = fs::read_to_string(&report_path).expect("report-filename should have written");
    assert!(
        report.trim().ends_with(".json"),
        "FR-012: --suffix=.json → tempfile ends in .json, got {report:?}"
    );
}

#[test]
fn default_suffix_is_dot_txt() {
    let tmpdir = common::with_tempdir();
    let report_path = tmpdir.path().join("report.txt");

    rusty_vipe_with_fake("report-filename")
        .env("RUSTY_VIPE_FAKE_EDITOR_REPORT", &report_path)
        .write_stdin("")
        .assert()
        .success();

    let report = fs::read_to_string(&report_path).unwrap();
    assert!(
        report.trim().ends_with(".txt"),
        "default suffix is .txt per FR-012, got {report:?}"
    );
}

#[test]
fn version_flag_succeeds() {
    let mut cmd = Command::cargo_bin("rusty-vipe").expect("binary built");
    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::contains("rusty-vipe"));
}

#[test]
fn help_flag_succeeds() {
    let mut cmd = Command::cargo_bin("rusty-vipe").expect("binary built");
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("Usage"));
}

#[test]
fn tempfile_cleaned_up_after_normal_exit() {
    // FR-032 path (1): point vipe's tempfile to a private per-test dir via
    // TMPDIR/TEMP so we can assert it's empty after the run without racing
    // against other tests that share std::env::temp_dir().
    let temp_root = common::with_tempdir();
    rusty_vipe_with_fake("passthrough")
        .env("TMPDIR", temp_root.path())
        .env("TEMP", temp_root.path())
        .env("TMP", temp_root.path())
        .write_stdin("data\n")
        .assert()
        .success();

    let leaked: Vec<_> = snapshot_tempdir(temp_root.path())
        .into_iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(".rusty-vipe-"))
        })
        .collect();
    assert!(
        leaked.is_empty(),
        "FR-032 path (1): tempfile must be cleaned up after normal exit; leaked: {leaked:?}"
    );
}

// =========================================================================
// US2 (Editor Non-Zero Exit Aborts Pipeline, P1) integration tests
// =========================================================================

#[test]
fn editor_exit_1_aborts_pipeline_zero_bytes_downstream() {
    // FR-006 + SC-002 + US2 AS1: editor exits 1 → wrapper exits 1, stdout empty.
    let output = rusty_vipe_with_fake("exit-nonzero:1")
        .write_stdin("would-be-output\n")
        .assert()
        .failure()
        .get_output()
        .clone();

    assert!(
        output.stdout.is_empty(),
        "FR-006: writer (preserved stdout) MUST NOT be touched on non-zero exit; \
         got {} bytes downstream",
        output.stdout.len()
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "SC-002: wrapper exit code equals editor exit code (1)"
    );
}

#[test]
fn editor_exit_42_propagates_exit_code() {
    // SC-002: editor exits N → wrapper exits N (Unix 1–255, Windows 1–254).
    let output = rusty_vipe_with_fake("exit-nonzero:42")
        .write_stdin("data\n")
        .assert()
        .failure()
        .get_output()
        .clone();

    assert_eq!(output.status.code(), Some(42));
    assert!(output.stdout.is_empty());
}

#[cfg(unix)]
#[test]
fn editor_exit_code_clamping_unix_passes_through_1_to_255() {
    // AD-012 Unix branch: 1–255 passes verbatim.
    for code in [1, 2, 42, 130, 254, 255] {
        let transform = format!("exit-nonzero:{code}");
        let output = rusty_vipe_with_fake(&transform)
            .write_stdin("data\n")
            .assert()
            .failure()
            .get_output()
            .clone();

        assert_eq!(
            output.status.code(),
            Some(code),
            "Unix: editor exit {code} must pass through verbatim"
        );
        assert!(output.stdout.is_empty());
    }
}

#[cfg(windows)]
#[test]
fn editor_exit_code_clamping_windows_passes_through_1_to_254() {
    // AD-012 Windows branch: 1–254 verbatim.
    for code in [1, 2, 42, 130, 254] {
        let transform = format!("exit-nonzero:{code}");
        let output = rusty_vipe_with_fake(&transform)
            .write_stdin("data\n")
            .assert()
            .failure()
            .get_output()
            .clone();

        assert_eq!(
            output.status.code(),
            Some(code),
            "Windows: editor exit {code} (in 1..=254) must pass through verbatim"
        );
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn editor_nonzero_exit_writer_receives_zero_bytes() {
    // FR-006: writer-untouched invariant across multiple stdin payload sizes
    // and exit codes (regression guard against accidental partial writes).
    for (stdin_payload, code) in [
        ("", 1),
        ("short\n", 2),
        (
            "longer stdin that we want to verify is NOT forwarded on abort\n",
            3,
        ),
    ] {
        let transform = format!("exit-nonzero:{code}");
        let output = rusty_vipe_with_fake(&transform)
            .write_stdin(stdin_payload)
            .assert()
            .failure()
            .get_output()
            .clone();

        assert!(
            output.stdout.is_empty(),
            "stdin={stdin_payload:?} exit-code={code}: stdout must be zero bytes; got {} bytes",
            output.stdout.len()
        );
    }
}

#[test]
fn tempfile_cleaned_up_after_nonzero_exit() {
    // FR-032 path (2): tempfile cleanup on the editor-non-zero-exit branch.
    // Use a private TMPDIR to avoid races with other parallel tests.
    let temp_root = common::with_tempdir();
    rusty_vipe_with_fake("exit-nonzero:7")
        .env("TMPDIR", temp_root.path())
        .env("TEMP", temp_root.path())
        .env("TMP", temp_root.path())
        .write_stdin("data that should NOT survive in tempdir\n")
        .assert()
        .failure();

    let leaked: Vec<_> = snapshot_tempdir(temp_root.path())
        .into_iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(".rusty-vipe-"))
        })
        .collect();
    assert!(
        leaked.is_empty(),
        "FR-032 path (2): tempfile must be cleaned up after non-zero exit; leaked: {leaked:?}"
    );
}

// T038 (tempfile_deleted_by_editor_emits_clear_error) deferred — needs a new
// fake-editor `delete-tempfile` transform; unit-test coverage exists via
// `pipeline::tests::write_back_returns_tempfile_deleted_when_file_missing`.
//
// T043 (editor_inherits_reattached_tty_handles via --transform=report-stdio)
// deferred — RUSTY_VIPE_TEST_BYPASS_TTY=1 gives the editor Stdio::null(), so
// tty-marker assertions need real PTY allocation in CI.

// ─── Phase 7 (US5) `--suffix` integration tests ───────────────────────────
// Note: T081 (suffix=.json) and T082 (default .txt) are already covered above
// by `suffix_flag_propagates_to_tempfile_extension` and `default_suffix_is_dot_txt`.
// The tests below cover T083 (empty suffix → no extension) and T085 (`--`
// separator forwarding with custom suffix).

#[test]
fn empty_suffix_produces_no_extension() {
    // T083 / FR-012 Clarification Q2 / US5 AS3.
    let tmpdir = common::with_tempdir();
    let report_path = tmpdir.path().join("report.txt");

    rusty_vipe_with_fake("report-filename")
        .env("RUSTY_VIPE_FAKE_EDITOR_REPORT", &report_path)
        .arg("--suffix=")
        .write_stdin("")
        .assert()
        .success();

    let report = fs::read_to_string(&report_path).expect("report-filename written");
    let basename = report.trim();
    assert!(
        !basename.ends_with(".txt"),
        "empty --suffix should NOT produce .txt extension; got {basename:?}"
    );
    assert!(
        !basename.ends_with('.'),
        "empty --suffix should not leave a trailing dot; got {basename:?}"
    );
}

#[test]
fn double_dash_separator_forwards_positional_to_editor() {
    // T085 / FR-024 / FR-030: `rusty-vipe --suffix=.json -- --foo bar` →
    // editor argv ends in [..., "--foo", "bar", "<tempfile-.json>"].
    let tmpdir = common::with_tempdir();
    let report_path = tmpdir.path().join("argv-report.txt");
    let fake = common::fake_editor_path();
    let fake_str = fake.to_string_lossy().replace('\\', "/");
    let editor_value = format!("{fake_str} '--transform=report-argv'");

    let mut cmd = Command::cargo_bin("rusty-vipe").expect("binary built");
    cmd.env("RUSTY_VIPE_TEST_BYPASS_TTY", "1");
    cmd.env("RUSTY_VIPE_FAKE_EDITOR_REPORT", &report_path);
    cmd.env("EDITOR", &editor_value);
    cmd.env_remove("VISUAL");
    cmd.arg("--suffix=.json").arg("--").arg("--foo").arg("bar");
    cmd.write_stdin("").assert().success();

    let argv: Vec<String> = fs::read_to_string(&report_path)
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .collect();
    // Last argv element is the tempfile path.
    let tempfile_arg = argv.last().expect("argv has tempfile element");
    assert!(
        tempfile_arg.ends_with(".json"),
        "FR-012: tempfile path should end in .json; got {tempfile_arg:?}"
    );
    // Two args before the tempfile should be `--foo` then `bar`.
    let n = argv.len();
    assert!(n >= 3, "argv too short: {argv:?}");
    assert_eq!(argv[n - 3], "--foo", "FR-024/FR-030 extras forwarding");
    assert_eq!(argv[n - 2], "bar", "FR-024/FR-030 extras forwarding");
}
