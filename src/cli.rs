//! Command-line interface stub.
//!
//! **STUB** — full implementation pending Phase 3+ (US1) tasks.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "rusty-vipe",
    version,
    about = "Pop $EDITOR mid-pipe; edit the buffered bytes; resume the pipeline.",
    long_about = "A Rust port of moreutils `vipe`. Buffers stdin to a tempfile, \
                  spawns $EDITOR on it with cross-platform TTY reattachment, then \
                  writes the post-edit bytes back to the original stdout sink."
)]
pub struct Cli {
    /// Tempfile filename suffix (default `.txt`). Editors use this as a
    /// syntax-highlighting hint.
    #[arg(long = "suffix", value_parser = parse_suffix_arg)]
    pub suffix: Option<String>,

    /// Explicit editor override (Default mode only). Whitespace-aware
    /// (`code --wait` parses correctly).
    #[arg(long = "editor")]
    pub editor: Option<String>,

    /// Enable strict moreutils-compat mode.
    #[arg(long, conflicts_with = "no_strict")]
    pub strict: bool,

    /// Explicitly disable strict mode (overrides env + argv[0]).
    #[arg(long = "no-strict")]
    pub no_strict: bool,

    /// Extra positional arguments forwarded to the editor (before the
    /// tempfile path).
    #[arg(trailing_var_arg = true)]
    pub editor_extras: Vec<String>,

    /// Subcommand (currently only `completions`).
    #[command(subcommand)]
    pub command: Option<Subcommand>,
}

#[derive(clap::Subcommand, Debug)]
pub enum Subcommand {
    /// Emit shell completion scripts (Default mode only).
    Completions { shell: clap_complete::Shell },
}

/// clap value_parser for `--suffix=<ext>`. Delegates to
/// [`crate::validate_suffix`] so Default and Strict modes share validation.
fn parse_suffix_arg(value: &str) -> Result<String, String> {
    crate::validate_suffix(value).map_err(|e| e.to_string())?;
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_command_factory_compiles() {
        let cmd = Cli::command();
        assert_eq!(cmd.get_name(), "rusty-vipe");
    }

    #[test]
    fn parse_no_args() {
        let cli = Cli::try_parse_from(["rusty-vipe"]).unwrap();
        assert!(cli.suffix.is_none());
        assert!(cli.editor.is_none());
        assert!(!cli.strict);
        assert!(!cli.no_strict);
    }

    #[test]
    fn parse_suffix() {
        let cli = Cli::try_parse_from(["rusty-vipe", "--suffix=.json"]).unwrap();
        assert_eq!(cli.suffix.as_deref(), Some(".json"));
    }

    #[test]
    fn parse_editor_override() {
        let cli = Cli::try_parse_from(["rusty-vipe", "--editor=code --wait"]).unwrap();
        assert_eq!(cli.editor.as_deref(), Some("code --wait"));
    }

    #[test]
    fn parse_strict_conflicts_with_no_strict() {
        let result = Cli::try_parse_from(["rusty-vipe", "--strict", "--no-strict"]);
        assert!(result.is_err());
    }
}
