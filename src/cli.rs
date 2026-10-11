use clap::{Parser, Subcommand};
use clap_complete::Shell;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "gitplay",
    about = "Live rolling code movement visualizer",
    version
)]
pub struct Cli {
    #[arg(default_value = ".", help = "Path to repository to watch")]
    pub path: PathBuf,

    #[arg(short, long, help = "Show cumulative diffs against git HEAD")]
    pub cumulative: bool,

    #[arg(short, long, default_value_t = 15, help = "Rolling line delay in ms")]
    pub pace: u64,

    #[arg(
        short = 'w',
        long,
        default_value_t = 150,
        help = "Debounce window in ms"
    )]
    pub debounce: u64,

    #[arg(
        short = 'U',
        long,
        default_value_t = 3,
        help = "Context lines around diffs"
    )]
    pub context: usize,

    #[arg(long, help = "Disable ANSI color styling")]
    pub no_color: bool,

    #[arg(
        long,
        help = "Scan once, print the uncommitted diff against git HEAD, and exit"
    )]
    pub once: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Print a completion script for the given shell to stdout
    Completions { shell: Shell },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn default_invocation_watches_the_current_directory() {
        let cli = Cli::try_parse_from(["gitplay"]).expect("no arguments is valid");
        assert_eq!(cli.path, PathBuf::from("."));
        assert!(cli.command.is_none());
        assert!(!cli.cumulative);
        assert!(!cli.once);
    }

    #[test]
    fn parses_completions_subcommand() {
        let cli = Cli::try_parse_from(["gitplay", "completions", "zsh"]).expect("parses");
        assert!(matches!(
            cli.command,
            Some(Commands::Completions { shell: Shell::Zsh })
        ));
        // A path argument never collides with the subcommand name.
        let cli = Cli::try_parse_from(["gitplay", "some/repo"]).expect("path still parses");
        assert!(cli.command.is_none());
    }

    #[test]
    fn rejects_unknown_subcommand_shells() {
        assert!(Cli::try_parse_from(["gitplay", "completions", "tcsh"]).is_err());
    }

    #[test]
    fn generates_a_script_for_every_shell() {
        for shell in [
            Shell::Bash,
            Shell::Zsh,
            Shell::Fish,
            Shell::Elvish,
            Shell::PowerShell,
        ] {
            let mut cmd = Cli::command();
            let mut buffer = Vec::new();
            clap_complete::generate(shell, &mut cmd, "gitplay".to_string(), &mut buffer);
            let script = String::from_utf8(buffer).expect("scripts are UTF-8");
            assert!(!script.is_empty(), "{shell:?} script is non-empty");
            assert!(script.contains("gitplay"), "{shell:?} names gitplay");
        }
    }
}
