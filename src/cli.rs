use clap::Parser;
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
}
