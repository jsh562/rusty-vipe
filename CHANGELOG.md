# Changelog

All notable changes to `rusty-vipe` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-05-23

### Added

- CLI binary `rusty-vipe`: pop `$EDITOR` mid-pipe with cross-platform TTY reattachment (Rust port of moreutils `vipe`).
- Edit-in-pipe core flow: drain stdin → tempfile (with `--suffix=<ext>` hint, default `.txt`) → reattach stdin/stdout to the controlling terminal → spawn editor → read tempfile → write to preserved original stdout.
- Editor resolution ladder: `--editor=<cmd>` (Default mode only) > `$VISUAL` > `$EDITOR` > `/usr/bin/editor` (Unix) > `vi` (Unix) / `notepad.exe` (Windows). Editor command whitespace-aware via `shell-words` (so `EDITOR="code --wait"` works).
- Pipeline-safety contract: editor non-zero exit aborts the pipeline (no bytes forwarded), preserving the editor's exit code (clamped to 1 on Windows for codes > 254).
- Signal-driven cleanup: SIGINT/SIGTERM/SIGHUP (Unix) and `CTRL_C_EVENT`/`CTRL_BREAK_EVENT`/`CTRL_CLOSE_EVENT` (Windows). Tempfile is removed before exit.
- Strict moreutils-compatibility mode via `--strict`, `RUSTY_VIPE_STRICT=1`, or invocation as `vipe` (via the `vipe-alias` cargo feature). Mirrors moreutils' `<editor argv> exited nonzero, aborting` format. Unknown flags emit ONLY the first error per the portfolio STF-003 option A pattern.
- Optional `vipe` binary alias gated behind the `vipe-alias` cargo feature.
- `completions <shell>` subcommand emitting shell-completion scripts for bash, zsh, fish, and PowerShell.
- Public Rust library API: `VipeBuilder` (with `#[must_use]` chain methods, validation at `build()` time) → `Vipe::run<R: Read, W: Write>(reader, writer)`. Writer is NOT touched on non-zero editor exit; the error is signaled via `Err(Error::EditorNonZeroExit(code))` with the already-clamped exit code.
- Library-without-binary build: `default-features = false` drops `clap`, `clap_complete`, `anyhow`, and `signal-hook` from the dependency closure.
- Cross-platform binary distribution: Linux x86_64/aarch64, macOS x86_64/aarch64, Windows x86_64 via `cargo-binstall` metadata pointing at GitHub Release archives.

### Testing posture

Behavioral integration tests use a deterministic `fake-editor` helper binary (gated behind the `dev-helpers` Cargo feature; NOT installed by `cargo install`) instead of driving real interactive editors. The fake editor performs known transformations (`delete-line:<N>`, `replace:<bytes>`, `passthrough`, `exit-nonzero:<code>`, `noop`, `report-argv`, `report-filename`, `report-stdio`) so behavioral assertions are reproducible across CI runners.

### MSRV

Minimum supported Rust version: **1.85**.

Upward deviation from the Rusty portfolio's "current stable minus two" rule, forced by Rust edition 2024 (which requires 1.85+).

### Known limitations at v0.1.0

- **Uncatchable signals (SIGKILL on Unix, hard process termination on Windows)**: tempfile cleanup falls back to `tempfile`-crate `Drop`, which may leak in rare cases.
- **Windows TTY reattachment**: tested against cmd and PowerShell; other console hosts (Windows Terminal, mintty, Git Bash) may have divergent behavior. Documented in `docs/COMPATIBILITY.md`.
- **`shell-words` parsing vs Perl whitespace splitting**: moreutils' Perl source splits `$EDITOR` on raw whitespace (no quoting). `shell-words` is stricter (respects quotes). For documented cases (`EDITOR="code --wait"`, `EDITOR='"path with spaces/editor"'`) the difference doesn't matter, but pathological values may diverge.

### Verified

- Tests passing on Rust 1.85 (MSRV) and current stable.
- Clippy strict (`-D warnings`) clean.
- rustfmt clean.
- `cargo audit` clean.
- Library API consumable with `default-features = false`.

### Compatibility statement

A full Compatibility Matrix lives at [`docs/COMPATIBILITY.md`](docs/COMPATIBILITY.md).

[Unreleased]: https://github.com/jsh562/rusty-vipe/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/jsh562/rusty-vipe/releases/tag/v0.1.0
