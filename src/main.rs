mod cli;
mod diff;
mod presenter;
mod repo;
mod watcher;

use clap::Parser;
use cli::Cli;
use diff::SnapshotStore;
use repo::{find_git_root, GitignoreMatcher};
use std::path::Path;

fn main() {
    let cli = Cli::parse();
    let root = find_git_root(&cli.path);
    let matcher = GitignoreMatcher::new(&root);
    let mut snapshots = SnapshotStore::new();

    // Seed the snapshot store from the current working tree state.
    scan_tree(&root, &root, &cli, &mut snapshots, &matcher, false);

    if cli.once {
        // One more pass: any file that changed since the seeding walk is
        // dirty and gets streamed before exiting.
        scan_tree(&root, &root, &cli, &mut snapshots, &matcher, true);
        return;
    }

    println!(
        "[gitplay] Watching {} (pace: {}ms, debounce: {}ms) — press 'q' to quit",
        root.display(),
        cli.pace,
        cli.debounce
    );

    watcher::run_watcher(root, cli, snapshots, matcher);
}

/// Recursively walks `dir`, skipping ignored and non-UTF-8 files. When
/// `report` is set, changed files are diffed and streamed; otherwise their
/// content is recorded to seed the snapshot store.
fn scan_tree(
    root: &Path,
    dir: &Path,
    cli: &Cli,
    snapshots: &mut SnapshotStore,
    matcher: &GitignoreMatcher,
    report: bool,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_dir = path.is_dir();
        if matcher.is_ignored(root, &path, is_dir) {
            continue;
        }
        if is_dir {
            scan_tree(root, &path, cli, snapshots, matcher, report);
        } else if let Ok(content) = std::fs::read_to_string(&path) {
            if report {
                if let Some((stats, hunks)) =
                    snapshots.check_diff(&path, &content, cli.context, cli.cumulative)
                {
                    presenter::print_diff(&path, &stats, &hunks, cli.pace, cli.no_color);
                }
            } else {
                snapshots.snapshot(&path, &content);
            }
        }
    }
}
