//! `rusty-vipe` binary entry point. Thin wrapper around [`rusty_vipe::run`].

fn main() -> std::process::ExitCode {
    rusty_vipe::run()
}
