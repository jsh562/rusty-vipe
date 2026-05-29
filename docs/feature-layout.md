# rusty-vipe — v0.2.0 Feature Layout

**Status**: implementation draft for the v0.2.0 Cargo features convention
backfill (spec 00011, Phase 5 — rusty-vipe).

**Authority**:
- `specs/adrs/0006-cargo-features-convention-for-portfolio-ports.md` (why)
- `project-instructions.md` §Cargo Feature Surface (what)
- This document — the per-port carving + WHY for each leaf, per HINT-003
  + HINT-009 of spec 00011.

**Reference port**: rusty-figlet v0.2.0 — see `../../rusty-figlet/docs/feature-layout.md`
(FROZEN reference port) for the format anchor. rusty-vipe conforms to the
same shape with the minimum-convention surface dictated by its
single-capability scope. The companion sibling port rusty-sponge v0.2.0
(also a single-capability port; see `../../rusty-sponge/docs/feature-layout.md`)
established the zero-leaf precedent in E011 Phase 4.

**Iteration model**: v0.2.0 is a **purely additive** SemVer-minor release.
Every v0.1.x feature name and composition is preserved verbatim; new
umbrellas (`full`, `vipe-classic`, `vipe-minimal`) are layered on top
without renaming or narrowing the existing `cli` / `default` / `vipe-alias`
/ `bench` / `dev-helpers` features. Library and binary API surfaces are
unchanged.

## Single-capability port — spec 00011 §Scope Edge Cases

rusty-vipe is a **single-capability port**: it has exactly one documented
capability — pop `$EDITOR` mid-pipe so the user can edit the buffered
bytes interactively, then resume the pipeline with the edited output (a
Rust port of moreutils `vipe`). Spec 00011 §Scope Edge Cases dictates that
single-capability ports apply the **minimum convention**:

> ports with only one capability adopt the minimum convention:
> `full = ["cli"]` and `<port>-classic = ["cli"]` are the required
> umbrellas; ZERO leaves carved beyond those required umbrellas.

This document records the carving exercise and the explicit decision
to NOT split orthogonal sub-capabilities into leaves — every additional
behavior of `rusty-vipe` (Default-mode ergonomics, Strict moreutils
compat, cross-platform TTY reattachment, signal-driven cleanup,
editor resolution ladder, `--editor=<cmd>` override, `completions`
subcommand) is part of the single core capability surface and removing
any of them would break either the documented public CLI / library
contract or the pipeline-safety guarantee that is the entire raison
d'être of the tool.

## Source-tree walk

`src/` modules (v0.1.0, post-Phase-1 baseline):

| Module                | Always-on? | CLI-only deps                                       | Notes                                                                       |
|-----------------------|-----------:|-----------------------------------------------------|-----------------------------------------------------------------------------|
| `error.rs`            | yes        | (thiserror — always-on)                             | `Error` enum; library + binary need it.                                     |
| `pipeline.rs`         | yes        | (tempfile — always-on)                              | Drain stdin → tempfile → editor spawn → read tempfile → write output.       |
| `editor.rs`           | yes        | (shell-words — always-on)                           | Editor resolution ladder + `shell-words` argv splitting.                    |
| `tty.rs`              | yes        | (libc/windows-sys — always-on, target-conditional)  | Cross-platform TTY reattachment (`/dev/tty` Unix; `CONIN$/CONOUT$` Windows).|
| `lib.rs`              | yes        | none                                                | Public API (`VipeBuilder`, `Vipe`, `EditorSource`, `CompatibilityMode`).    |
| `cli.rs`              | no — `cli` | clap                                                | clap-derive `Cli` struct + `Subcommand::Completions`.                       |
| `mode.rs`             | no — `cli` | none (pure helper)                                  | Strict-mode precedence resolver (`--strict` > env > argv[0]).               |
| `signal.rs`           | no — `cli` | signal-hook (Unix), windows-sys (Windows)           | Signal handler install + cleanup-on-exit dispatch.                          |
| `strict.rs`           | no — `cli` | (clap_complete + clap pulled by `cli`)              | Hand-rolled Strict-mode argv pre-scanner + byte-equal moreutils dispatcher. |
| `main.rs`             | no — `cli` | clap, clap_complete, anyhow, signal-hook            | Binary entry; gated by `required-features = ["cli"]`.                       |
| `bin/vipe.rs`         | no — `vipe-alias` | (inherits `cli`)                             | `vipe` alias binary; gated by `required-features = ["vipe-alias"]`.         |

## Leaf-carving criteria (HINT-009)

A capability becomes a leaf when ALL of the following hold:

1. It is **self-containable** — gated cleanly via `#[cfg(feature = "<leaf>")]`
   at the module or top-level item boundary (HINT-004).
2. Either (a) it has a **sole optional dependency** that no other leaf needs
   (HINT-005), OR (b) it is a pure-cfg-gate of an internal module worth
   exposing as a knob.
3. Disabling it does NOT break any always-on library/CLI surface.

A capability does NOT become a leaf when:

- It is foundational (TTY reattachment, signal cleanup, tempfile spill,
  editor resolution ladder) — disabling it would break the headline
  pipeline-safety guarantee or the cross-platform TTY contract.
- It is part of the single documented capability surface (Default mode,
  Strict mode, `--editor=` override, completions subcommand).
- It would create more than ~6 leaves (FR-007 + HINT-003 envelope).

## v0.2.0 Carved Leaves

**ZERO new leaves carved at v0.2.0**. Every capability inside rusty-vipe
is either:

1. Foundational always-on library code (TTY reattachment, editor
   resolution ladder, pipeline drain + edit + resume, tempfile cleanup)
   — cannot be stripped without breaking the public surface or the
   pipeline-safety guarantee.
2. Already gated by the v0.1.x `cli` umbrella (clap-derived argument
   parsing, completions subcommand, signal handler install, Strict-mode
   pre-scanner).
3. Already gated by the v0.1.x `vipe-alias` feature (the second `vipe`
   binary entry).
4. A dev-tooling feature (`bench` → criterion benches; `dev-helpers` →
   `fake-editor` test bin) outside the convention's runtime-capability
   purview.

### Leaves intentionally NOT carved

The following candidate leaves were considered + rejected:

- **`signal`**: signal handler install + cleanup-on-exit dispatch lives
  in `src/signal.rs` behind `dep:signal-hook` (Unix) and `windows-sys`
  (Windows, target-conditional always-on). It is part of the
  pipeline-safety contract — without it, a Ctrl-C mid-edit could leak
  the tempfile and leave the downstream consumer hanging on closed
  stdio. Stripping this would silently break the headline promise.
  Rejected per HINT-009 criterion 3.
- **`tty-reattach`**: cross-platform TTY reattachment (`/dev/tty` on
  Unix, `CONIN$/CONOUT$` on Windows) is the entire reason `vipe` is
  hard — without it the editor cannot read keystrokes mid-pipeline.
  This is foundational always-on library code in `src/tty.rs`. No
  carving signal.
- **`completions`**: Could be carved as `["dep:clap_complete"]`, but
  per spec 00011 §Scope Edge Cases minimum-convention single-capability
  ports declare ZERO new leaves. `clap_complete` is bundled into the
  v0.1.x `cli` umbrella verbatim. Carving it would either rename `cli`
  (breaking SemVer additivity) or duplicate the surface.
- **`editor-override`**: The `--editor=<cmd>` Default-mode flag is
  ~15 lines in `cli.rs` + `editor.rs`. Splitting it into its own leaf
  would orphan the `shell-words` dep (already always-on) and require
  separate compat-flag plumbing in `mode.rs`. No carving signal.
- **`strict-compat`**: rusty-vipe's Strict mode is dispatched inline
  in `lib.rs::run()` (via `mode::resolve` + `strict::run`). The
  hand-rolled getopt mirror lives in `src/strict.rs`. Both are gated
  by the `cli` umbrella in v0.1.x (since they consume `clap` +
  `clap_complete`). Carving out a separate `strict-compat` leaf would
  require splitting `strict.rs` away from `cli.rs`, which is more
  refactoring than the additive v0.2.0 release allows. The capability
  survives untouched inside the existing `cli` composition.
- **`vipe-alias`**: This v0.1.x feature ships a second binary named
  `vipe`. It IS retained verbatim per the v0.2.0 SemVer additive
  contract — but it is NOT one of the 2 required preset bundles per
  FR-007 (those are `vipe-classic` and `vipe-minimal` below).
  Documented separately as an installation-time convenience knob.
- **`bench`**: The v0.1.x `bench` feature is a tooling / benchmark
  scaffold (criterion benches under `benches/throughput.rs`), not a
  runtime capability leaf. It remains a dev-tooling feature outside
  the convention's purview (the vendored `tools/feature-lint/lint.sh`
  allowlist skips `bench` from leaf-CI-matrix and phantom-leaf checks)
  and is retained verbatim from v0.1.0.
- **`dev-helpers`**: The v0.1.x `dev-helpers` feature gates the
  `fake-editor` `[[bin]]` used by integration tests. Like `bench`,
  this is dev-tooling outside the convention's purview. Retained
  verbatim; the feature-lint allowlist treats it as a dev-tooling
  name (matches the `test-util` family in the lint script's allowlist
  conceptually — explicit allowlist entry for `dev-helpers` is
  documented below).

## Preset bundles (FR-007 — 2 required for single-capability ports)

Per spec 00011 §Scope Edge Cases + FR-007, even single-capability ports
declare 2 preset bundles to give the keep-list workaround documentation
something concrete to point at.

### `vipe-classic` (REQUIRED — bare port, 1:1 with moreutils `vipe`)

```toml
vipe-classic = ["cli"]
```

- Includes `cli` so the binary exists.
- Single-capability port; the `cli` umbrella IS the bare-port surface.
- Use case: `cargo install rusty-vipe --no-default-features --features vipe-classic`
  for a moreutils-`vipe` drop-in replacement (Strict mode is invoked
  via the `--strict` flag, `RUSTY_VIPE_STRICT` env var, or `vipe-alias`
  binary name — none of these require additional features).

### `vipe-minimal`

```toml
vipe-minimal = ["cli"]
```

- Same composition as `vipe-classic` (single-capability port has no
  smaller subset to carve).
- Use case: explicit "minimal CLI install" alias for users who prefer
  the `<port>-minimal` naming convention seen across other Rusty ports
  (figlet-minimal, pwgen-minimal, ts-minimal, sponge-minimal).
  Documented as an intentional semantic alias rather than a distinct
  composition.

### `vipe-alias` (v0.1.x feature retained, NOT a convention preset)

`vipe-alias = ["cli"]` from v0.1.0 ships an additional `vipe` binary
alongside `rusty-vipe`. It is retained verbatim per the v0.2.0 SemVer
additive contract — but it is NOT one of the 2 required preset bundles
per FR-007 (those are `vipe-classic` and `vipe-minimal` above).
`vipe-alias` is documented separately as an installation-time
convenience knob, not a capability subset.

### `bench` (v0.1.x dev-tooling feature retained, NOT a convention preset)

`bench = ["dep:criterion"]` from v0.1.0 enables `benches/throughput.rs`.
It is dev-tooling, not a runtime capability — outside the convention's
purview per the vendored feature-lint allowlist.

### `dev-helpers` (v0.1.x dev-tooling feature retained, NOT a convention preset)

`dev-helpers = []` from v0.1.0 gates the `fake-editor` `[[bin]]` used
by integration tests. It is dev-tooling — never installed by
`cargo install`, used only by `cargo test --features dev-helpers`.
Outside the convention's purview; matches the `test-util` family of
dev-tooling names already in the vendored feature-lint allowlist.

## Cross-port glossary candidates

No leaves carved → no cross-port glossary contributions from rusty-vipe
in this iteration. If a future minor release adds an orthogonal capability
(e.g., a `tui-preview` leaf for showing a render of the about-to-be-edited
bytes), the leaf would be a candidate for promotion to
`docs/feature-vocabulary.md` per FR-053.

## CI matrix shape (FR-010..FR-014)

Per plan §Per-Port v0.2.0 CI Matrix, scaled to a zero-leaf port:

- **Tier 1 — `test-default`**: full DDR-003 cross-compile matrix
  (5 targets). Post-v0.2.0 `default = ["full"]` and `full = ["cli"]`,
  so the kitchen-sink test resolves to the same set as v0.1.0
  `default = ["cli"]` — no regression in coverage.
- **Tier 2 — `test-no-default`**: Linux x86_64 only. `cargo test
  --no-default-features --lib` + dep-tree audit (SC-001 evidence).
- **Tier 3 — `test-<bundle>`**: one job per preset bundle. Linux only.
  - `test-vipe-classic`
  - `test-vipe-minimal`
- **Tier 4 — `check-leaf-<leaf>`**: SKIPPED. Zero leaves → no
  per-leaf compile-check jobs. A placeholder comment in `ci.yml`
  documents why this tier is empty. The `bench` + `dev-helpers`
  features are in the vendored feature-lint allowlist (dev-tooling)
  and do not require a Tier-4 entry.
- **Tier 5 — `lint-convention`**: single Linux job invoking the
  vendored `tools/feature-lint/run.sh` script.

Per FR-014, bundle/lint jobs are constrained to Linux x86_64.

## Vendored feature-lint

Per spec 00011 §Phase 2 iteration 6 precedent (rusty-figlet vendored
the lint script because the umbrella `jsh562/rustylib` is private and
cross-repo `actions/checkout` cannot reach it), rusty-vipe vendors
`tools/feature-lint/{lint.sh,run.sh,README.md}` from the umbrella into
the port repo. The vendored copy is byte-equal to the umbrella source
of truth as of the freeze commit (post the dev-tooling-allowlist +
benches/tests-search + additive-CHANGELOG-support fixes from rusty-ts
v0.2.0 / E011 Phase 3 iteration 2 and the path-sanitization fixes from
rusty-sponge v0.2.0 / E011 Phase 4).

## Why no new leaves — explicit rationale

Spec 00011 §Scope Edge Cases anticipates this case verbatim:

> Some ports have only one orthogonal capability. Those ports adopt the
> minimum convention: `full = ["cli"]` and `<port>-classic = ["cli"]`
> as aliases; the convention SHAPE is consistent across the portfolio
> even when the per-port leaf carving yields zero leaves.

rusty-vipe deliberately chooses the zero-leaf path because:

1. The pipeline-safety guarantee (editor non-zero abort = no downstream
   bytes) is the entire reason this tool exists. Carving any of its
   supporting machinery (signal handlers, TTY reattachment, editor
   resolution ladder) into an opt-out leaf would silently change the
   FR-006 contract for users who turned that leaf off.
2. The cost of carving a speculative leaf (cfg-gate scaffolding,
   per-leaf CI matrix entry, README/CHANGELOG row, glossary candidacy)
   outweighs the value when no orthogonal capability exists to gate.
3. The portfolio-wide convention shape (umbrella set, README "Cargo
   Features" section, lint compliance) is preserved verbatim — a
   downstream library consumer reading the README for rusty-vipe
   gets the same one-glance feature matrix UX as one reading
   rusty-figlet, rusty-ts, or rusty-sponge.
4. v0.2.0 is **purely additive**. Every v0.1.x feature is preserved
   verbatim; no SemVer break. Future minor releases can add leaves
   without breaking the v0.2.0 contract: a hypothetical `tui-preview`
   v0.3.0 feature would slot in as `tui-preview = ["dep:ratatui"]`
   alongside the existing umbrellas with zero migration cost.
