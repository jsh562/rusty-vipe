# rusty-vipe

Pop `$EDITOR` mid-pipe to edit buffered bytes, then resume the pipeline. Rust port of moreutils [`vipe(1)`](https://joeyh.name/code/moreutils/).

[![crates.io](https://img.shields.io/crates/v/rusty-vipe.svg)](https://crates.io/crates/rusty-vipe)
[![docs.rs](https://docs.rs/rusty-vipe/badge.svg)](https://docs.rs/rusty-vipe)
[![CI](https://github.com/jsh562/rusty-vipe/actions/workflows/ci.yml/badge.svg)](https://github.com/jsh562/rusty-vipe/actions/workflows/ci.yml)
[![MSRV](https://img.shields.io/badge/MSRV-1.85-blue.svg)](#msrv)
[![license: MIT OR Apache-2.0](https://img.shields.io/crates/l/rusty-vipe.svg)](#license)

Default mode adds `--help`, `--version`, `--editor=<cmd>`, & a `completions` subcommand. Strict mode mirrors moreutils' error layout for drop-in migration. Cross-platform: Windows console-host editors work via `shell-words` argv splitting; `vi` falls back to `notepad.exe` on Windows.

Part of the [Rusty portfolio](https://jsh562.github.io/rusty-portfolio).

## Install

```sh
cargo install rusty-vipe
# or, with prebuilt binaries:
cargo binstall rusty-vipe
# or, download directly from GitHub Releases:
# https://github.com/jsh562/rusty-vipe/releases
```

To also install a `vipe` binary alias (argv[0] auto-detect routes into Strict mode):

```sh
cargo install rusty-vipe --features vipe-alias
```

## Usage

```sh
# Edit mid-pipe before sending lines to a destructive command
grep ERROR /var/log/syslog | rusty-vipe | xargs -I {} report-bug.sh "{}"

# Tell the editor what filetype to highlight as
some-cmd | rusty-vipe --suffix=.json | jq .

# Override which editor opens (Default mode only)
some-cmd | rusty-vipe --editor='code --wait' | downstream

# Abort the pipeline by exiting the editor with an error (e.g. `:cq` in vim)
risky-producer | rusty-vipe | rm-files-listed-on-stdin   # editor non-zero exit blocks downstream

# Strict moreutils-compat mode (drop-in moreutils vipe replacement)
some-cmd | rusty-vipe --strict | downstream
RUSTY_VIPE_STRICT=1 some-cmd | rusty-vipe | downstream
some-cmd | vipe | downstream             # via vipe-alias feature or argv[0] symlink

# Shell completions
rusty-vipe completions bash               # > ~/.bash_completion.d/rusty-vipe
rusty-vipe completions zsh                # > ~/.zfunc/_rusty-vipe
rusty-vipe completions fish               # > ~/.config/fish/completions/rusty-vipe.fish
rusty-vipe completions powershell
```

## Library API

The crate exposes a `Read` -> `Write` runtime so library consumers can drive the pipeline against in-memory buffers in tests or TUI apps without hijacking process stdio.

```rust,no_run
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

For library-only consumers without CLI deps see the [Cargo Features](#cargo-features) section.

## Cargo Features

`default` enables `full`, which (for this single-capability port) resolves to the `cli` umbrella. `vipe-classic` reproduces v0.1.x bare-port behavior matching upstream moreutils `vipe` 1:1. To strip the CLI surface use `default-features = false` or `--no-default-features` & add the features you want.

rusty-vipe is a **single-capability port**: its one documented job is "pop `$EDITOR` mid-pipe & resume the pipeline with the edited output". No optional feature leaves are carved beyond the required umbrellas; see [`docs/feature-layout.md`](docs/feature-layout.md) for why.

### Feature matrix

| Feature | Description | Umbrella(s) |
|---|---|---|
| `cli` | All CLI-only dependencies (`clap`, `clap_complete`, `anyhow`, `signal-hook`) and the binary entry point, signal-handler install, mode resolver, and Strict-mode pre-scanner. Library consumers strip via `default-features = false`. | `full`, `vipe-classic`, `vipe-minimal`, `vipe-alias` |
| `vipe-alias` | Installs an additional `vipe` binary alongside `rusty-vipe`. Both share source; argv[0] auto-detect routes `vipe` invocations into Strict mode. | (standalone, implies `cli`) |
| `bench` | Pulls `criterion` and enables `benches/throughput.rs`. Dev-tooling only; outside the convention's leaf surface. Name preserved verbatim from v0.1.x. | (standalone) |
| `dev-helpers` | Gates the `fake-editor` `[[bin]]` used by integration tests. Never installed by `cargo install`; enable with `cargo test --features dev-helpers`. Dev-tooling only. | (standalone) |

### Preset bundles

| Bundle | Composition | Use case |
|---|---|---|
| `vipe-classic` | `cli` | Drop-in upstream moreutils `vipe` replacement. Strict mode is invoked via `--strict`, `RUSTY_VIPE_STRICT`, or `vipe-alias` argv[0] auto-detect. |
| `vipe-minimal` | `cli` | Explicit minimal-CLI alias for users who prefer the `<port>-minimal` naming convention seen across other portfolio ports. Identical composition to `vipe-classic`. |

### Keep-list workaround (Cargo features are union-only)

Cargo features cannot subtract from `default`. To get "everything except a specific feature," disable defaults & enumerate the features you want:

```sh
cargo install rusty-vipe --no-default-features --features "cli"
# → bare CLI with no vipe-alias binary, no bench tooling.

cargo install rusty-vipe --no-default-features --features "cli vipe-alias"
# → CLI + the vipe alias binary.
```

For the common cases the named [preset bundles](#preset-bundles) are usually sufficient.

### Library-only consumers

```toml
[dependencies]
rusty-vipe = { version = "0.2", default-features = false }
```

This strips `clap`, `clap_complete`, `anyhow`, & `signal-hook`. The resulting build pulls only `tempfile`, `thiserror`, `shell-words`, & the target-conditional always-on deps (`libc` on Unix, `windows-sys` on Windows; both required for TTY reattachment regardless of feature selection).

### Convention authority

This layout follows the portfolio-wide Cargo Features Convention. The "why" lives in [ADR-0006](https://github.com/jsh562/rustylib/blob/main/specs/adrs/0006-cargo-features-convention-for-portfolio-ports.md); the "what" lives in [`project-instructions.md` §Cargo Feature Surface](https://github.com/jsh562/rustylib/blob/main/project-instructions.md). Every Rusty port from v0.2 onward exposes the same umbrella set (`default` / `full` / `cli` / `<port>-classic`), per-port leaves named in kebab-case, & 2 to 4 preset bundles.

## Compatibility

`rusty-vipe` has two modes:

- **Default mode.** clap-styled flag parser. `--help`, `--version`, `--editor=<cmd>`, & the `completions` subcommand are all available. Editor resolution order: `--editor=` > `$VISUAL` > `$EDITOR` > `/usr/bin/editor` (Unix executable check) > `vi` (Unix) or `notepad.exe` (Windows).
- **Strict mode** (activated by `--strict`, `RUSTY_VIPE_STRICT=1`, or invoking the binary as `vipe`). Mirrors moreutils' Perl `Getopt::Long` error layout. `--help`, `--version`, `--editor=`, & `completions` MUST be rejected. Pinned upstream version: **moreutils 0.69** (same baseline as `rusty-sponge`).

### Pipeline-safety guarantee

When the editor exits non-zero, `rusty-vipe` MUST NOT forward the tempfile bytes downstream & MUST exit non-zero. The editor's exit code passes through verbatim where possible (clamped to 1 on Windows for codes > 254). This is the safety contract that lets users abort dangerous pipelines via `:cq` in vim.

### Documented intentional divergences

1. **`--help` / `--version` / `--editor=` flags + `completions` subcommand**. Default-mode additions; rejected in Strict.
2. **Unknown-flag stderr in Strict.** Emits only the first unknown-flag error (`rusty-vipe: invalid option -- 'X'`). moreutils' POSIX `Getopt::Long` iterates per-character for malformed long flags; we don't replicate that for undocumented inputs.
3. **Windows editor fallback.** `notepad.exe` (always present). moreutils Perl source assumes Unix `vi`.
4. **No controlling TTY.** Emits a precise error & exits non-zero rather than scrambling the pipeline by reading from the producer's pipe stdin.
5. **Exit-code clamping on Windows.** Editor exits 1-254 pass through verbatim; codes > 254 or negative collapse to 1. Unix passes 1-255 verbatim. moreutils on Windows isn't well-defined here.
6. **`shell-words` argv splitting.** `EDITOR` & `--editor=` are split using POSIX shell-quoting rules. Quoted paths like `EDITOR='"/Program Files/Code/bin/code" --wait'` work correctly; moreutils breaks on these.
7. **Windows console-host support.** `cmd.exe` & PowerShell are first-class. `mintty` (Git Bash, MSYS2) is a documented divergence; wrap with `winpty rusty-vipe ...` if needed.

See [`docs/COMPATIBILITY.md`](docs/COMPATIBILITY.md) for the full per-flag matrix.

### `vipe-alias` PATH-collision warning

Building with `--features vipe-alias` installs a second binary named `vipe` alongside `rusty-vipe`. If moreutils is also on the same `PATH`, whichever directory comes first wins. Invoke `rusty-vipe` (always unambiguous) or omit the `vipe-alias` feature when moreutils is also present.

## What's not shipped

- **Per-character iteration over malformed long flags** the way moreutils' Perl `Getopt::Long` does. Strict mode emits only the first unknown-flag error. Same posture as `rusty-sponge`.
- **`mintty` console-host support on Windows.** Wrap with `winpty rusty-vipe ...`. `cmd.exe` & PowerShell are first-class.
- **Source-code derivation from moreutils.** This is a clean-room reimplementation. The moreutils `vipe` Perl source is GPL'd & untouched. Behavioral interfaces (flag set, exit codes, editor invocation order) aren't copyrightable, so a clean-room reimplementation under a different license is well-established practice. Same posture as [`uutils/coreutils`](https://github.com/uutils/coreutils).

## MSRV

Rust **1.85** (edition 2024). Re-verified against the portfolio's stable-minus-two policy at each release.

## License

Dual-licensed under [MIT](LICENSE) or [Apache-2.0](LICENSE-APACHE) at your option.
