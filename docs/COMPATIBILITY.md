# Compatibility Matrix — `rusty-vipe` vs moreutils `vipe`

> Pinned baseline: moreutils 0.69 (Ubuntu 24.04 LTS, same baseline as `rusty-sponge`).

## Flag matrix

| Flag / Form | Default mode | Strict mode |
|---|---|---|
| `--suffix=<ext>` | Accepted (default `.txt`); `--suffix=` empty means no extension at all (overrides default) | Accepted with same semantics |
| `--editor=<cmd>` | Rusty extension — Override over env; empty value falls through to env | Rejected as unknown flag (first-error-only per option A) |
| `--help` | clap-rendered help (exit 0) | Rejected as unknown long-form flag |
| `--version` | clap-rendered version (exit 0) | Rejected as unknown long-form flag |
| `--strict` | Activates Strict mode (Rusty extension) | Consumed pre-parse (no-op) |
| `--no-strict` | Explicit Default override; highest precedence | Consumed pre-parse (no-op) |
| `completions <shell>` | Subcommand: emit completion script | Treated as positional (not a subcommand in Strict) |
| Extra positionals | Forwarded to editor argv before tempfile path (per FR-011) | Same |
| `--` separator | Standard clap end-of-options; args after are positional editor args | Same |

## Env var matrix

| Variable | Default mode | Strict mode |
|---|---|---|
| `RUSTY_VIPE_STRICT=1` | Activates Strict mode (precedence: `--strict` > env > argv[0]=vipe > Default) | n/a (already in Strict) |
| `$VISUAL` | Editor resolution rung 2 | Same |
| `$EDITOR` | Editor resolution rung 3 | Same |

## Exit-code matrix

| Scenario | rusty-vipe Default | rusty-vipe Strict | moreutils vipe |
|---|---|---|---|
| Successful edit + write | 0 | 0 | 0 |
| Editor exits N (1 ≤ N ≤ 255 Unix; 1 ≤ N ≤ 254 Windows) | N | N | N (Unix), clamped on Windows |
| Editor exits >254 on Windows or negative | 1 | 1 | varies |
| `--help` / `--version` | 0 | non-zero (unknown opt) | n/a |
| Unknown flag | 2 (clap parse error) | non-zero (first-error-only stderr) | non-zero (multi-error stderr) |
| Editor not found | 127 | 127 | non-zero |
| No controlling TTY | non-zero | non-zero | varies / hangs |
| Tempfile deleted in editor | non-zero | non-zero | varies |
| SIGINT/SIGTERM/SIGHUP | 130 | 130 | 130 (OS default) |

## Intentional divergences from moreutils

1. **Long-form flag rejection format (STF-003 option A)** — first error only, not per-character iteration. moreutils' Perl `Getopt::Long` emits one error line per unrecognized character of an unknown long flag; rusty-vipe emits ONE total error line and exits non-zero. Verified by `tests/compat_strict.rs::strict_first_unknown_flag_only` (and its reversed-order sibling).
2. **`--help` / `--version`** — moreutils has neither; rusty-vipe adds both in Default mode (clap-generated), rejected in Strict mode via the first-error formatter.
3. **`--editor=<cmd>`** — Rusty addition for explicit override (Default mode only). Empty `--editor=''` falls through to env per Clarification Q3.
4. **`completions <shell>` subcommand** — Rusty addition (Default mode only). Pre-generated scripts for bash/zsh/fish/PowerShell are committed under `completions/` and drift-tested in `tests/completions_drift.rs`.
5. **Windows fallback editor** — `notepad.exe` (moreutils assumes Unix `vi`).
6. **No-controlling-TTY detection** — moreutils may hang or scramble the pipeline; rusty-vipe emits the precise error `rusty-vipe: no controlling terminal; cannot launch editor` and exits non-zero per FR-015.
7. **`shell-words` argv splitting** vs moreutils' raw whitespace splitting (HINT-004) — `shell-words` honors POSIX shell quoting and backslash escaping; moreutils splits on whitespace alone. Practical impact: editor commands containing quoted paths (e.g. `EDITOR='"/Program Files/Code/bin/code" --wait'`) work correctly with rusty-vipe; with moreutils they break.
8. **Exit-code clamping (AD-012)** — Unix passes editor exit codes 1–255 verbatim; Windows passes 1–254 verbatim, clamping >254 or negative to 1. moreutils on Windows is not well-defined here.
9. **Editor-died message format (FR-018)** — Strict mode emits `<argv> exited nonzero, aborting` (matching Perl `die` text). This is byte-stable across rusty-vipe releases but is documented as "moreutils-style" rather than "moreutils-byte-equal" because upstream's exact `Getopt::Long` output differs slightly.

## Windows console-host support

| Host | Status |
|---|---|
| `cmd.exe` | Supported (primary target — `CONIN$`/`CONOUT$` open succeeds, editor receives real console handles). |
| Windows PowerShell / PowerShell 7 | Supported (same console-host model as `cmd.exe`). |
| Windows Terminal (`wt.exe`) | Supported when hosting `cmd`/PowerShell; conpty layer is transparent to the `CONIN$`/`CONOUT$` API. |
| `mintty` (Git Bash, MSYS2) | **Documented divergence** — mintty does not expose a Win32 console; `CONIN$` open succeeds but the spawned editor inherits MSYS pty semantics. Editor invocation may behave differently than under `cmd`/PowerShell. Use `winpty rusty-vipe …` as a workaround.

## `vipe-alias` feature — installed-name collision warning

The `vipe-alias` feature ships a second binary at `vipe` (`vipe.exe` on Windows) alongside `rusty-vipe`. If moreutils is also installed on the same `PATH`, whichever directory precedes the other in `PATH` wins. To pin to rusty-vipe explicitly, invoke `rusty-vipe` (always unambiguous) or use the `vipe-alias` only in environments where moreutils is not installed.

## Atomic-safety guarantee scope

vipe is fundamentally a non-atomic operation — the editor truncates and rewrites the tempfile, and the downstream pipe receives bytes only after the editor exits cleanly. There is no atomic-safety guarantee analogous to `rusty-sponge`'s regular-file rename path. Key invariants:

- **Editor non-zero exit aborts** (FR-006) — zero bytes forwarded downstream.
- **Tempfile cleanup-on-exit** (FR-014) — three exit paths covered: normal, panic, signal-driven.
- **Uncatchable signals** (SIGKILL) — tempfile may leak; documented known limitation.

## Known limitations at v0.1.0

- See [`CHANGELOG.md`](../CHANGELOG.md) §"Known limitations at v0.1.0".

---

**Generation note.** This file is currently hand-authored from the spec + plan. A future revision may regenerate it from the clap `Cli::command()` introspection per AD-006 (drift-tested via integration test, never via `build.rs`). For v0.1.0 the flag surface is small enough that hand-maintenance plus the byte-equal `compat_strict.rs` tests provide equivalent drift protection.
