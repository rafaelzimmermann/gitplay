use crate::cli::Cli;
use crate::diff::SnapshotStore;
use crate::presenter;
use crate::repo::GitignoreMatcher;
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebounceEventResult};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

/// Runs the debounced filesystem watcher loop until the process is stopped.
/// On every event, changed files are diffed against their snapshots and any
/// resulting diff is streamed to the terminal.
pub fn run_watcher(
    root: PathBuf,
    cli: Cli,
    mut snapshots: SnapshotStore,
    matcher: GitignoreMatcher,
) {
    let (tx, rx) = mpsc::channel::<DebounceEventResult>();

    let mut debouncer = match new_debouncer(Duration::from_millis(cli.debounce), move |res| {
        let _ = tx.send(res);
    }) {
        Ok(debouncer) => debouncer,
        Err(err) => {
            eprintln!("[gitplay] failed to start file watcher: {err}");
            return;
        }
    };

    if let Err(err) = debouncer.watcher().watch(&root, RecursiveMode::Recursive) {
        eprintln!("[gitplay] failed to watch {}: {err}", root.display());
        return;
    }

    loop {
        match rx.recv() {
            Ok(Ok(events)) => {
                for event in events {
                    handle_event(&root, &event.path, &cli, &mut snapshots, &matcher);
                }
            }
            Ok(Err(err)) => {
                eprintln!("[gitplay] watcher error: {err}");
            }
            // Channel closed: the watcher backend went away.
            Err(_) => break,
        }
    }
}

fn handle_event(
    root: &Path,
    path: &Path,
    cli: &Cli,
    snapshots: &mut SnapshotStore,
    matcher: &GitignoreMatcher,
) {
    let is_dir = path.is_dir();
    if matcher.is_ignored(root, path, is_dir) {
        return;
    }
    if is_dir {
        // Directories themselves carry no diffable content; their children
        // arrive as separate events.
        return;
    }

    // A vanished file is treated as emptied; files that exist but cannot be
    // read as UTF-8 (e.g. binaries) are skipped to keep the stream readable.
    let content = if path.exists() {
        match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(_) => return,
        }
    } else {
        String::new()
    };

    if let Some((stats, hunks)) = snapshots.check_diff(path, &content, cli.context, cli.cumulative)
    {
        presenter::print_diff(path, &stats, &hunks, cli.pace, cli.no_color);
    }
}
