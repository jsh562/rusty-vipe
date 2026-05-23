//! `vipe` binary alias entry point (gated behind the `vipe-alias` Cargo feature).
//!
//! Shares the same body as [`rusty_vipe::run`]; argv[0] auto-detect inside
//! `run()` routes invocations as `vipe` into Strict mode per FR-019.

fn main() -> std::process::ExitCode {
    rusty_vipe::run()
}
