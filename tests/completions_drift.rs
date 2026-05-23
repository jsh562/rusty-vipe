//! US8 (Shell Completions, P3) drift tests.
//!
//! Regenerates each shell's completion script from the current `Cli` schema
//! and asserts byte equality with the committed file in `completions/`.
//! If clap_complete changes its output OR the CLI schema changes, the drift
//! test fails and the regenerated file must be committed.
//!
//! Per SC-010: bash, zsh, fish, and PowerShell are all supported and verified.

use clap::CommandFactory;
use clap_complete::Shell;
use rusty_vipe::cli::Cli;
use std::fs;
use std::path::PathBuf;

/// Generate the completion script for `shell` into a Vec<u8>, normalized so
/// platform-specific line endings can't trigger spurious drift on Windows
/// checkouts.
fn generate(shell: Shell) -> Vec<u8> {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    let mut out: Vec<u8> = Vec::new();
    clap_complete::generate(shell, &mut cmd, name, &mut out);
    normalize_line_endings(&out)
}

fn normalize_line_endings(bytes: &[u8]) -> Vec<u8> {
    // Strip CRs so committed LF files compare cleanly against in-memory output.
    bytes.iter().copied().filter(|b| *b != b'\r').collect()
}

fn committed_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("completions")
        .join(filename)
}

fn read_committed(filename: &str) -> Vec<u8> {
    let path = committed_path(filename);
    let bytes =
        fs::read(&path).unwrap_or_else(|e| panic!("committed completion missing at {path:?}: {e}"));
    normalize_line_endings(&bytes)
}

#[test]
fn drift_bash() {
    let actual = generate(Shell::Bash);
    let committed = read_committed("rusty-vipe.bash");
    assert_eq!(
        committed, actual,
        "bash completion drift detected — regenerate with `cargo run -- completions bash > completions/rusty-vipe.bash`"
    );
}

#[test]
fn drift_zsh() {
    let actual = generate(Shell::Zsh);
    let committed = read_committed("_rusty-vipe");
    assert_eq!(
        committed, actual,
        "zsh completion drift detected — regenerate with `cargo run -- completions zsh > completions/_rusty-vipe`"
    );
}

#[test]
fn drift_fish() {
    let actual = generate(Shell::Fish);
    let committed = read_committed("rusty-vipe.fish");
    assert_eq!(
        committed, actual,
        "fish completion drift detected — regenerate with `cargo run -- completions fish > completions/rusty-vipe.fish`"
    );
}

#[test]
fn drift_powershell() {
    let actual = generate(Shell::PowerShell);
    let committed = read_committed("rusty-vipe.ps1");
    assert_eq!(
        committed, actual,
        "powershell completion drift detected — regenerate with `cargo run -- completions powershell > completions/rusty-vipe.ps1`"
    );
}

#[test]
fn strict_mode_rejects_completions_subcommand() {
    // T107 / US8 AS2 / FR-013: Strict mode rejects the `completions`
    // subcommand via the moreutils-style first-error formatter. (Note: this
    // is also covered by `compat_strict::strict_rejects_help_version_editor_completions`
    // but T107 calls it out explicitly under the completions phase.)
    let mut cmd = assert_cmd::Command::cargo_bin("rusty-vipe").expect("binary built");
    cmd.env("RUSTY_VIPE_TEST_BYPASS_TTY", "1");
    cmd.env_remove("VISUAL");
    cmd.env_remove("EDITOR");
    cmd.env_remove("RUSTY_VIPE_STRICT");
    cmd.arg("--strict").arg("completions").arg("bash");

    let output = cmd.write_stdin("").assert().failure().get_output().clone();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown option -- 'completions'"),
        "Strict mode must reject completions; got: {stderr:?}"
    );
}
