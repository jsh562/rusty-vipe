//! Library API (US6) integration tests.
//!
//! Verifies the programmatic surface: `VipeBuilder`, `Vipe::run`, the
//! writer-untouched invariant, and the thread-safety markers.

mod common;

use rusty_vipe::{CompatibilityMode, EditorSource, Error, Vipe, VipeBuilder};
use std::io::{Cursor, Write};

/// Produce an `EditorSource::Override` pointing at the fake-editor with the
/// requested transform — the same POSIX-single-quoted shell-words form the
/// CLI tests use, so quoting works on both Unix and Windows.
fn fake_override(transform: &str) -> EditorSource {
    let fake = common::fake_editor_path();
    let fake_str = fake.to_string_lossy().replace('\\', "/");
    assert!(
        !transform.contains('\''),
        "transform must not contain a single quote (POSIX limitation)"
    );
    let cmd = format!("{fake_str} '--transform={transform}'");
    EditorSource::Override(cmd)
}

/// Build a `Vipe` configured with the fake-editor + TTY bypass.
fn build_vipe(transform: &str) -> Vipe {
    // Vipe::run reads RUSTY_VIPE_TEST_BYPASS_TTY from process env; tests set
    // it via `Command::env` for binary tests, but library tests run in-process
    // so we set the process env directly. Safe in this test crate because all
    // lib_api tests use the same value.
    // SAFETY: tests run sequentially via Cargo's default thread but env mutation
    // is unsafe in Rust 1.85; we accept the contract.
    unsafe {
        std::env::set_var("RUSTY_VIPE_TEST_BYPASS_TTY", "1");
    }
    VipeBuilder::new()
        .editor(fake_override(transform))
        .compat(CompatibilityMode::Default)
        .build()
        .expect("builder should succeed")
}

#[test]
fn builder_default_succeeds() {
    let _ = VipeBuilder::new().build().expect("default builder ok");
}

#[test]
fn builder_rejects_strict_plus_editor_override() {
    // T093 / US6 AS3 / FR-022.
    let result = VipeBuilder::new()
        .compat(CompatibilityMode::Strict)
        .editor(EditorSource::Override(String::from("vi")))
        .build();
    assert!(
        matches!(result, Err(Error::CompatibilityViolation(_))),
        "Strict + Override must fail with CompatibilityViolation; got {result:?}"
    );
}

#[test]
fn builder_rejects_empty_editor_override() {
    let result = VipeBuilder::new()
        .editor(EditorSource::Override(String::new()))
        .build();
    assert!(
        matches!(result, Err(Error::InvalidBuilderConfiguration(_))),
        "empty Override should be rejected at build(); got {result:?}"
    );
}

#[test]
fn builder_rejects_invalid_suffix() {
    let result = VipeBuilder::new().suffix("/bad").build();
    assert!(matches!(result, Err(Error::InvalidBuilderConfiguration(_))));
}

#[test]
fn vipe_run_passthrough_roundtrips_bytes() {
    // Happy path: fake-editor passthrough leaves bytes untouched.
    let mut vipe = build_vipe("passthrough");
    let input = Cursor::new(b"hello\nworld\n".to_vec());
    let mut output: Vec<u8> = Vec::new();
    vipe.run(input, &mut output).expect("run should succeed");
    assert_eq!(output, b"hello\nworld\n");
}

#[test]
fn vipe_run_delete_line_transforms_content() {
    // delete-line:2 removes the second line.
    let mut vipe = build_vipe("delete-line:2");
    let input = Cursor::new(b"alpha\nbravo\ncharlie\n".to_vec());
    let mut output: Vec<u8> = Vec::new();
    vipe.run(input, &mut output).expect("run should succeed");
    assert_eq!(output, b"alpha\ncharlie\n");
}

#[test]
fn writer_empty_on_nonzero_editor_exit() {
    // T094 / FR-029 / STF-002: writer receives ZERO bytes on non-zero exit.
    let mut vipe = build_vipe("exit-nonzero:1");
    let input = Cursor::new(b"some bytes\n".to_vec());
    let mut output: Vec<u8> = Vec::new();
    let result = vipe.run(input, &mut output);
    match result {
        Err(Error::EditorNonZeroExit(code)) => {
            assert_eq!(code, 1, "exit-nonzero:1 → clamped code 1");
        }
        other => panic!("expected EditorNonZeroExit, got {other:?}"),
    }
    assert!(
        output.is_empty(),
        "FR-029: writer MUST be empty on non-zero exit; got {} bytes",
        output.len()
    );
}

#[test]
fn writer_empty_on_nonzero_editor_exit_42() {
    // Same invariant, but exit code 42 — verifies code propagation too.
    let mut vipe = build_vipe("exit-nonzero:42");
    let input = Cursor::new(b"xyz\n".to_vec());
    let mut output: Vec<u8> = Vec::new();
    let result = vipe.run(input, &mut output);
    match result {
        Err(Error::EditorNonZeroExit(code)) => assert_eq!(code, 42),
        other => panic!("expected EditorNonZeroExit(42), got {other:?}"),
    }
    assert!(output.is_empty());
}

#[test]
fn writer_untouched_on_invalid_editor_command_error() {
    // T095 partial: writer untouched on InvalidEditorCommand (build() succeeded
    // because the Override is syntactically a string, but parse_editor_value
    // fails on unbalanced quotes at run() time).
    let mut vipe = VipeBuilder::new()
        .editor(EditorSource::Override(String::from("\"unbalanced")))
        .build()
        .expect("builder succeeds on raw string");
    let input = Cursor::new(b"data\n".to_vec());
    let mut output: Vec<u8> = Vec::new();
    let result = vipe.run(input, &mut output);
    assert!(
        matches!(result, Err(Error::InvalidEditorCommand(_))),
        "expected InvalidEditorCommand; got {result:?}"
    );
    assert!(output.is_empty(), "writer must be untouched on this error");
}

#[test]
fn writer_untouched_on_editor_not_found_error() {
    // T095 partial: writer untouched on EditorNotFound.
    unsafe {
        std::env::set_var("RUSTY_VIPE_TEST_BYPASS_TTY", "1");
    }
    let mut vipe = VipeBuilder::new()
        .editor(EditorSource::Override(String::from(
            "an-editor-that-definitely-does-not-exist-987654",
        )))
        .build()
        .expect("builder succeeds");
    let input = Cursor::new(b"data\n".to_vec());
    let mut output: Vec<u8> = Vec::new();
    let result = vipe.run(input, &mut output);
    assert!(
        matches!(result, Err(Error::EditorNotFound(_))),
        "expected EditorNotFound; got {result:?}"
    );
    assert!(output.is_empty());
}

/// Custom writer that counts every `write` and `flush` call.
struct CountingWriter {
    bytes: Vec<u8>,
    writes: usize,
    flushes: usize,
}
impl CountingWriter {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            writes: 0,
            flushes: 0,
        }
    }
}
impl Write for CountingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.writes += 1;
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.flushes += 1;
        Ok(())
    }
}

#[test]
fn writer_zero_writes_and_zero_flushes_on_error_path() {
    // FR-029: not just "zero bytes" — zero write calls AND zero flush calls.
    let mut vipe = build_vipe("exit-nonzero:1");
    let input = Cursor::new(b"contents\n".to_vec());
    let mut writer = CountingWriter::new();
    let result = vipe.run(input, &mut writer);
    assert!(matches!(result, Err(Error::EditorNonZeroExit(_))));
    assert_eq!(writer.writes, 0, "FR-029: zero write() calls on error path");
    assert_eq!(
        writer.flushes, 0,
        "FR-029: zero flush() calls on error path"
    );
    assert!(writer.bytes.is_empty());
}

#[test]
fn send_sync_compile_time_assertion() {
    // T096: thread-safety contract.
    // Vipe and VipeBuilder are both Send + Sync — all fields (`EditorSource`,
    // `String`, `CompatibilityMode`) are Send + Sync, and `run` takes
    // `&mut self` which already prevents concurrent invocation.
    use static_assertions::assert_impl_all;
    assert_impl_all!(Vipe: Send, Sync);
    assert_impl_all!(VipeBuilder: Send, Sync);
}

#[test]
fn default_features_off_excludes_cli_deps() {
    // T092 / SC-007: `cargo tree --no-default-features` MUST NOT pull in
    // clap, clap_complete, anyhow, or signal-hook — these are gated behind
    // the `cli` feature.
    let output = std::process::Command::new("cargo")
        .args(["tree", "--no-default-features", "--prefix", "none"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("`cargo tree` should run");
    assert!(output.status.success(), "cargo tree must succeed");
    let tree = String::from_utf8_lossy(&output.stdout);

    for forbidden in ["clap ", "clap_complete", "anyhow ", "signal-hook"] {
        assert!(
            !tree.contains(forbidden),
            "FR-021 / SC-007: `cargo tree --no-default-features` must not contain {forbidden:?}\n\
             full tree:\n{tree}"
        );
    }
}

#[test]
fn library_output_equals_binary_output() {
    // T097: parity test. Same input + same fake-editor → same output bytes
    // whether driven via Vipe::run (library) or via the rusty-vipe binary.
    let input_bytes = b"alpha\nbravo\ncharlie\ndelta\n".to_vec();

    // Library path.
    let mut vipe = build_vipe("delete-line:3");
    let mut lib_out: Vec<u8> = Vec::new();
    vipe.run(Cursor::new(input_bytes.clone()), &mut lib_out)
        .expect("library run succeeds");

    // Binary path — same fake-editor + transform.
    let fake = common::fake_editor_path();
    let fake_str = fake.to_string_lossy().replace('\\', "/");
    let editor_value = format!("{fake_str} '--transform=delete-line:3'");
    let mut cmd = assert_cmd::Command::cargo_bin("rusty-vipe").expect("binary built");
    cmd.env("RUSTY_VIPE_TEST_BYPASS_TTY", "1");
    cmd.env("EDITOR", &editor_value);
    cmd.env_remove("VISUAL");
    let output = cmd
        .write_stdin(input_bytes)
        .assert()
        .success()
        .get_output()
        .clone();

    assert_eq!(
        lib_out, output.stdout,
        "library and binary must produce byte-identical output"
    );
}
