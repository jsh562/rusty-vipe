# rusty-vipe Design Notes

This document is a maintainer-facing distillation of the implementation plan. The authoritative source-of-truth is `specs/00003-vipe-port/plan.md` in the umbrella repo.

## Architecture overview

`rusty-vipe` is a single Cargo crate exposing a library API (`Vipe`, `VipeBuilder`) and a CLI binary (`rusty-vipe`, plus an optional `vipe` alias gated behind the `vipe-alias` feature). A third dev-only binary (`fake-editor`, gated behind `dev-helpers`) is used by integration tests instead of driving real interactive editors in CI.

The CLI flow:

1. Pre-clap scan for `--strict` / `--no-strict`; resolve Default vs Strict mode (precedence: `--strict` > `RUSTY_VIPE_STRICT=1` > `argv[0] == "vipe"` > Default).
2. Default mode: parse args via clap. Strict mode: bypass clap, hand-rolled argv pre-parser emits moreutils-style stderr.
3. Open the controlling terminal: `/dev/tty` on Unix, `CONIN$`/`CONOUT$` via `CreateFileW` on Windows. If unavailable → `Error::NoControllingTty` and exit non-zero.
4. Preserve the original stdout sink via `dup(2)` (Unix) or `DuplicateHandle` (Windows) so post-edit bytes can route back to the pipeline.
5. Drain stdin into a `tempfile::NamedTempFile::Builder::suffix(<suffix>).tempfile()`. Default suffix `.txt`; configurable via `--suffix=<ext>`.
6. Resolve the editor command (precedence ladder, shell-words-aware splitting).
7. Spawn editor with reattached stdin/stdout/stderr + tempfile path appended.
8. On editor exit 0: read tempfile bytes → write to preserved stdout → exit 0.
9. On editor non-zero exit: do NOT touch the preserved stdout; exit with clamped exit code per AD-012.
10. Signal handler (SIGINT/SIGTERM/SIGHUP on Unix; console-control on Windows) cleans up the tempfile if it exists and the run is interrupted.

## Upstream Dependency Status

**E003 (umbrella reusable workflow) — STILL NOT AVAILABLE.** As of 2026-05-23, the umbrella's reusable CI/release workflow does not exist. This repo therefore duplicates the inline workflows from `rusty-sponge/.github/workflows/{ci,release}.yml` as a **pragmatic-path** approach (third repo to do so after `rusty-ts` and `rusty-sponge`). When E003 ships (`<umbrella>/.github/workflows/port-ci.yml@v1.0.0`), this repo's workflows will be replaced by ~20-line caller stubs.

**E004 (cargo-generate template) — STILL NOT AVAILABLE.** Scaffolded by hand mirroring the `rusty-sponge` v0.1.0 layout.

Tracked in `specs/project-plan.md` under Epics E003 and E004.

## Repo provenance

This repo was bootstrapped on 2026-05-22/23 as the **third** port in the Rusty portfolio (following `rusty-ts` v0.1.0 and `rusty-sponge` v0.1.0). It inherits all conventions established by the prior two ports:

- Dual MIT/Apache-2.0 license (LICENSE = MIT for GitHub sidebar; LICENSE-APACHE alongside)
- MSRV 1.85 (edition 2024 minimum)
- Inline CI/release workflows duplicated from rusty-sponge
- Library-vs-binary error split: `thiserror` in lib, `anyhow` at the binary boundary
- Drift-tested generated documentation
- Strict-mode option A: first-unknown-flag-only error stderr
- Signal-driven tempfile cleanup pattern (signal-hook + SetConsoleCtrlHandler)

This port adds two new project-level baseline patterns (promoted during `/sddp-specify`):

- **TTY reattachment for interactive subprocess control** — `/dev/tty` on Unix, `CONIN$`/`CONOUT$` on Windows; preserve original stdout sink via `dup`/`DuplicateHandle`.
- **Deterministic fake-process testing pattern** — ship a `fake-editor` test helper binary, never drive real interactive tools in CI.

## fake-editor contract

The `fake-editor` binary is the deterministic substitute for real editors in integration tests. It accepts the last argv element as the target file path (matching how vipe spawns its editor) and uses `--transform=<name>` to select one of eight named behaviors:

| Transform | Behavior |
|---|---|
| `delete-line:<N>` | Read target file, delete the Nth line (1-indexed), write back, exit 0 |
| `replace:<bytes>` | Overwrite target file with the given bytes (use `@<path>` for non-UTF-8 binary input); exit 0 |
| `passthrough` | Read target file, write same bytes back unchanged, exit 0 |
| `exit-nonzero:<code>` | Write nothing to target, exit with the given code |
| `noop` | Don't touch target, exit 0 |
| `report-argv` | Write own argv (one element per line) to `${RUSTY_VIPE_FAKE_EDITOR_REPORT}`, exit 0 |
| `report-filename` | Write the last argv element (the tempfile path) to the report file, exit 0 |
| `report-stdio` | Write tty-marker tags for stdin/stdout to the report file, exit 0 |

Side-channel reporting via the `RUSTY_VIPE_FAKE_EDITOR_REPORT` env var lets tests assert what the editor "saw" without polluting stdout (which is the actual editor↔terminal channel during normal operation).

Determinism guarantees: every transform produces identical bytes given identical input. No clocks, no random numbers, no environmental reads beyond the documented report-file env var.

## Key Architecture Decisions (feature-local)

See `specs/00003-vipe-port/plan.md` for the full AD-001…AD-016 table. Highlights:

- **AD-001/AD-013**: `tempfile` 3.x with `Builder::suffix(...)`; `fake-editor` as feature-gated `[[bin]]`.
- **AD-002/AD-006**: `shell-words` for editor argv splitting; dedicated `editor::resolve()` returning `Vec<OsString>` + `EditorSource` discriminant.
- **AD-003/AD-004**: TTY reattachment — `/dev/tty` on Unix, `CONIN$`/`CONOUT$` via `CreateFileW` on Windows.
- **AD-005**: Stdout sink preservation via `dup(2)` (Unix) / `DuplicateHandle` (Windows).
- **AD-007**: Hand-rolled Strict-mode argv pre-parser (bypasses clap for byte-equal stderr; mirrors `rusty-sponge` `strict.rs`).
- **AD-012**: Editor-exit-code platform-specific clamping per FR-006.
- **AD-015**: Parameterized `Vipe::run<R: Read, W: Write>(reader, writer)`.
