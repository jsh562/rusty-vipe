# rusty-vipe

A Rust port of the moreutils `vipe` utility: pop `$EDITOR` mid-pipe so you can edit the buffered bytes interactively, then resume the pipeline with the edited output. Static binaries on Linux, macOS, and Windows; works with or without a Rust toolchain via `cargo install` or `cargo binstall`. Default mode adds quality-of-life niceties moreutils doesn't have (`--help`, `--version`, `--editor=<cmd>`, `completions`); Strict mode mirrors moreutils' error layout for drop-in migration.

Part of the [Rusty portfolio](https://jsh562.github.io/rusty-portfolio) — a collection of small Rust ports of utilities missing from the Rust ecosystem.

## Install

### With a Rust toolchain

```sh
cargo install rusty-vipe
```

To also install the `vipe` binary alias (auto-enables Strict mode on invocation):

```sh
cargo install rusty-vipe --features vipe-alias
```

### Without a Rust toolchain (prebuilt binaries via cargo-binstall)

```sh
cargo binstall rusty-vipe
```

### Direct download

Per-target archives are attached to each [GitHub Release](https://github.com/jsh562/rusty-vipe/releases). Linux x86_64/aarch64, macOS x86_64/aarch64, Windows x86_64. Each archive contains the binary plus pre-generated shell-completion scripts for bash, zsh, fish, and PowerShell.

## Usage

```sh
# Edit-in-pipe: pops $EDITOR on the buffered stdin, resumes with edited bytes.
grep ERROR /var/log/syslog | rusty-vipe | xargs -I {} report-bug.sh "{}"

# Suffix hint for syntax highlighting (default .txt).
some-cmd | rusty-vipe --suffix=.json | jq .

# Explicit editor override (Default mode only).
some-cmd | rusty-vipe --editor='code --wait' | downstream

# Strict moreutils-compat mode.
some-cmd | rusty-vipe --strict | downstream
RUSTY_VIPE_STRICT=1 some-cmd | rusty-vipe | downstream
some-cmd | vipe | downstream    # via the vipe-alias feature — argv[0] auto-detect

# Shell completions
rusty-vipe completions bash    # > ~/.bash_completion.d/rusty-vipe
rusty-vipe completions zsh     # > ~/.zfunc/_rusty-vipe
rusty-vipe completions fish    # > ~/.config/fish/completions/rusty-vipe.fish
rusty-vipe completions powershell
```

## Compatibility statement (vs moreutils vipe)

Byte-level fidelity for documented Strict-mode inputs is verified by snapshot tests against captured moreutils-`vipe` behavior; behavioral assertions use a deterministic `fake-editor` helper (committed under `tests/bin/`) rather than driving real interactive editors in CI. Pinned upstream version: **moreutils 0.69** (same baseline as `rusty-sponge`).

**Editor resolution order**: `--editor=<cmd>` (Default mode only) > `$VISUAL` > `$EDITOR` > `/usr/bin/editor` (Unix only; executable check) > `vi` (Unix) / `notepad.exe` (Windows).

**Pipeline-safety guarantee**: When the editor exits non-zero, `rusty-vipe` does NOT forward the tempfile bytes to the downstream consumer and exits non-zero (preserving the editor's exit code where possible — clamped to 1 on Windows for codes > 254). This is the safety contract that lets users abort dangerous pipelines via the editor's exit-with-error command (e.g., `:cq` in vim).

**Documented intentional divergences from moreutils vipe**:

1. **`--help` / `--version` / `--editor=` flags + `completions` subcommand**: not present in moreutils. Default-mode additions; rejected in Strict mode.
2. **Unknown-flag stderr format in Strict mode**: emits ONLY the first unknown-flag error (`rusty-vipe: invalid option -- 'X'` or `rusty-vipe: unknown option -- 'foo'`). moreutils' POSIX `Getopt::Long` iterates per-character for malformed long flags; we don't replicate that for undocumented inputs (same posture as `rusty-sponge` STF-003 option A).
3. **Windows editor fallback**: `notepad.exe` (always present). moreutils Perl source assumes Unix `vi`.
4. **No controlling TTY**: rusty-vipe emits a precise error and exits non-zero rather than scrambling the pipeline by reading from the producer's pipe stdin.
5. **Exit-code clamping on Windows (AD-012)**: editor exits 1–254 pass through verbatim; exit codes greater than 254 or negative collapse to 1. Unix passes 1–255 verbatim. moreutils on Windows is not well-defined here.
6. **`shell-words` argv splitting (HINT-004)**: `EDITOR` and `--editor=` are split using POSIX shell-quoting rules via the `shell-words` crate, not raw whitespace. Quoted paths (e.g. `EDITOR='"/Program Files/Code/bin/code" --wait'`) work correctly with rusty-vipe; with moreutils they break.
7. **Windows console-host support**: `cmd.exe` and PowerShell are first-class; `mintty` (Git Bash, MSYS2) is a documented divergence — wrap with `winpty rusty-vipe …` if needed.

**`vipe-alias` PATH-collision warning.** Building with `--features vipe-alias` installs a second binary named `vipe` alongside `rusty-vipe`. If moreutils is also installed on the same `PATH`, whichever directory comes first wins. Invoke `rusty-vipe` (always unambiguous) or omit the `vipe-alias` feature when moreutils is also present.

See [`docs/COMPATIBILITY.md`](docs/COMPATIBILITY.md) for the full per-flag matrix.

## Library API

The crate exposes a public Rust API for programmatic use. The canonical surface takes generic `Read` for input and generic `Write` for output, so library consumers can drive the pipeline against in-memory buffers (in tests, in TUI apps, etc.) without hijacking process stdio.

```rust
use rusty_vipe::{VipeBuilder, EditorSource, CompatibilityMode};
use std::io::Cursor;

let mut input = Cursor::new(b"line1\nline2\nline3\n".to_vec());
let mut output: Vec<u8> = Vec::new();

let mut vipe = VipeBuilder::new()
    .editor(EditorSource::Override("fake-editor --transform=passthrough".into()))
    .suffix(".txt".into())
    .compat(CompatibilityMode::Default)
    .build()?;

vipe.run(&mut input, &mut output)?;
# Ok::<(), rusty_vipe::Error>(())
```

To use the library without pulling in the CLI dependencies:

```toml
[dependencies]
rusty-vipe = { version = "0.1", default-features = false }
```

### Stability commitment

**Lockstep SemVer**: the library and binary share a single crate version. Within the `0.x` series, minor version bumps may introduce breaking changes per standard Cargo semantics — pin to the patch version (`= "0.1.0"`) if breakage is a concern. Once `1.0` lands, the API is frozen to additive-only changes guarded by `#[non_exhaustive]` on every public enum and struct.

## MSRV

Minimum supported Rust version: **1.85**.

Upward deviation from the Rusty portfolio's "current stable minus two" rule, forced by Rust edition 2024 (which requires 1.85+).

## Relationship to moreutils

`rusty-vipe` is a **clean-room Rust reimplementation** of the moreutils `vipe` utility. It contains **no source code from moreutils** — only a from-scratch Rust implementation that observes the documented behavior and reproduces it.

The moreutils `vipe` Perl source is © Joey Hess and the moreutils contributors, licensed under the GNU GPL. That license governs the Perl source code. Behavioral interfaces (flag set, exit codes, editor invocation order) are not copyrightable, so a clean-room reimplementation under a different license is well-established practice — the same posture as [`uutils/coreutils`](https://github.com/uutils/coreutils).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE](LICENSE))

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
