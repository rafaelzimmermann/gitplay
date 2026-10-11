mod cli;
mod commits;
mod diff;
mod fsutil;
mod presenter;
mod repo;
mod watcher;

use clap::Parser;
use cli::Cli;
use commits::{git_available, CommitTracker, HeadBaseline};
use diff::SnapshotStore;
use repo::{find_git_root, GitignoreMatcher};
use std::path::Path;

fn main() {
    let cli = Cli::parse();
    let root = find_git_root(&cli.path);
    let matcher = GitignoreMatcher::new(&root);
    let mut snapshots = SnapshotStore::new();
    // HEAD is tracked so commits made while watching appear in the stream.
    let mut tracker = CommitTracker::new(&root);
    if !git_available() {
        println!(
            "[gitplay] git binary not found: commit messages will be skipped and cumulative diffs anchor on the startup snapshot"
        );
    }

    // Cumulative mode anchors on git HEAD; the store stays lazy, so nothing
    // is fetched until a file actually changes.
    let mut head_baseline = HeadBaseline::new(&root);

    // Seed the snapshot store from the current working tree state.
    scan_tree(
        &root,
        &root,
        &cli,
        &mut snapshots,
        &matcher,
        false,
        &mut head_baseline,
    );

    if cli.once {
        // The report pass anchors on git HEAD: every file that differs from
        // the committed state is streamed, then gitplay exits.
        scan_tree(
            &root,
            &root,
            &cli,
            &mut snapshots,
            &matcher,
            true,
            &mut head_baseline,
        );
        if let Some(head_move) = tracker.check(&root) {
            presenter::print_head_move(&head_move, cli.no_color);
        }
        presenter::print_footer(&root, tracker.branch(), cli.no_color);
        return;
    }

    println!(
        "[gitplay] Watching {} (pace: {}ms, debounce: {}ms) — press 'q' to quit",
        root.display(),
        cli.pace,
        cli.debounce
    );
    // The footer states where the watch is and which branch is under HEAD.
    presenter::print_footer(&root, tracker.branch(), cli.no_color);

    watcher::run_watcher(root, cli, snapshots, matcher, tracker);
}

/// Recursively walks `dir`, skipping ignored and non-UTF-8 files. When
/// `report` is set, changed files are diffed and streamed; otherwise their
/// content is recorded to seed the snapshot store.
#[allow(clippy::too_many_arguments)]
fn scan_tree(
    root: &Path,
    dir: &Path,
    cli: &Cli,
    snapshots: &mut SnapshotStore,
    matcher: &GitignoreMatcher,
    report: bool,
    head_baseline: &mut HeadBaseline,
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
            scan_tree(root, &path, cli, snapshots, matcher, report, head_baseline);
        } else if let Ok(content) = fsutil::read_to_string(&path) {
            if report {
                // `--once` always anchors on HEAD (it reports the current
                // uncommitted state); in watch mode only `--cumulative` does.
                let cumulative = cli.cumulative || cli.once;
                let head = if cumulative {
                    head_baseline.content(&path)
                } else {
                    None
                };
                if let Some((stats, hunks)) =
                    snapshots.check_diff(&path, &content, cli.context, cumulative, head.as_deref())
                {
                    presenter::print_diff(root, &path, &stats, &hunks, cli.pace, cli.no_color);
                }
            } else {
                snapshots.snapshot(&path, &content);
            }
        }
    }
}
