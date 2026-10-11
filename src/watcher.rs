use crate::cli::Cli;
use crate::commits::CommitTracker;
use crate::diff::SnapshotStore;
use crate::presenter;
use crate::repo::GitignoreMatcher;
use crossterm::event::{poll, read, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::tty::IsTty;
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebounceEventResult};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// How often the idle watcher loop and the key listener wake up to poll for
/// a quit request. Small enough that quitting feels instant, large enough
/// that idle wake-ups stay negligible.
const QUIT_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// RAII guard that keeps the terminal in raw mode while alive.
///
/// Raw mode is enabled only when stdin is a TTY, so piped and CI runs keep
/// their canonical terminal untouched. Dropping the guard — on normal
/// return, early return, or unwind — restores canonical mode and resets the
/// presenter's line-ending mode.
struct RawModeGuard {
    active: bool,
}

impl RawModeGuard {
    fn new() -> Self {
        if !std::io::stdin().is_tty() {
            return Self { active: false };
        }
        install_panic_hook();
        match enable_raw_mode() {
            Ok(()) => {
                presenter::set_raw_output(true);
                Self { active: true }
            }
            Err(_) => Self { active: false },
        }
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = disable_raw_mode();
            presenter::set_raw_output(false);
        }
    }
}

/// Installs a panic hook that restores the terminal before the default hook
/// runs, so a panic never leaves the session stuck in raw mode.
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        default_hook(info);
    }));
}

/// Spawns a lightweight background thread that watches the keyboard for a
/// quit request ('q'/'Q', Esc, or Ctrl+C) and flips the returned flag when
/// one arrives. Returns `None` when stdin is not a TTY (piped, CI), leaving
/// pure file watching intact.
fn spawn_key_listener(raw: &RawModeGuard) -> Option<Arc<AtomicBool>> {
    if !raw.active {
        return None;
    }
    let quit = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&quit);
    thread::Builder::new()
        .name("gitplay-keys".to_owned())
        .spawn(move || key_listener_loop(flag))
        .ok()?;
    Some(quit)
}

fn key_listener_loop(quit: Arc<AtomicBool>) {
    loop {
        match poll(QUIT_POLL_INTERVAL) {
            // Nothing pressed within the window: keep listening.
            Ok(false) => continue,
            // stdin became unreadable: give up quietly, watching still works.
            Err(_) => return,
            Ok(true) => {}
        }
        match read() {
            Ok(event) if is_quit_event(&event) => {
                quit.store(true, Ordering::Relaxed);
                return;
            }
            Ok(_) => {}
            Err(_) => return,
        }
    }
}

/// 'q'/'Q', Escape, or Ctrl+C all request a clean shutdown.
fn is_quit_event(event: &Event) -> bool {
    let Event::Key(key) = event else {
        return false;
    };
    let ctrl_c = key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
    key.kind == KeyEventKind::Press
        && (matches!(
            key.code,
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc
        ) || ctrl_c)
}

/// Runs the debounced filesystem watcher loop until the process is stopped.
/// On every event, changed files are diffed against their snapshots and any
/// resulting diff is streamed to the terminal. HEAD moves (commits, merges,
/// branch switches) are reported alongside them. In interactive sessions the
/// user can press 'q' (or Esc / Ctrl+C) to stop cleanly.
pub fn run_watcher(
    root: PathBuf,
    cli: Cli,
    mut snapshots: SnapshotStore,
    matcher: GitignoreMatcher,
    mut tracker: CommitTracker,
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

    // While alive this keeps the terminal in raw mode (TTY only) so the key
    // listener below can see individual keypresses.
    let raw_guard = RawModeGuard::new();
    let quit = spawn_key_listener(&raw_guard);
    let mut quit_requested = false;
    loop {
        // Interactive sessions poll with a short timeout so the loop can
        // notice the quit flag; piped stdin blocks exactly like before.
        let received = if quit.is_some() {
            rx.recv_timeout(QUIT_POLL_INTERVAL)
        } else {
            rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
        };
        match received {
            Ok(Ok(events)) => {
                let mut printed = false;
                for event in events {
                    if handle_event(&root, &event.path, &cli, &mut snapshots, &matcher) {
                        printed = true;
                    }
                }
                // HEAD moves are written under `.git`, which the diff stream
                // ignores entirely; checking after each batch catches a commit
                // (or branch switch) without spending any time idle.
                if let Some(head_move) = tracker.check(&root) {
                    presenter::print_head_move(&head_move, cli.no_color);
                    printed = true;
                }
                if printed {
                    presenter::print_footer(&root, tracker.branch(), cli.no_color);
                }
            }
            Ok(Err(err)) => {
                eprintln!("[gitplay] watcher error: {err}");
            }
            Err(RecvTimeoutError::Timeout) => {}
            // Channel closed: the watcher backend went away.
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if quit
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
        {
            quit_requested = true;
            break;
        }
    }

    // Restore the terminal before any farewell message so it prints on a
    // fresh line in canonical mode.
    drop(raw_guard);
    if quit_requested {
        // The leading `\r` covers a cursor left mid-line by raw-mode output.
        println!("\r[gitplay] Stopped.");
    }
}

/// Applies one filesystem event: the file is re-read and diffed against its
/// snapshot. Returns whether anything reached the terminal.
fn handle_event(
    root: &Path,
    path: &Path,
    cli: &Cli,
    snapshots: &mut SnapshotStore,
    matcher: &GitignoreMatcher,
) -> bool {
    let is_dir = path.is_dir();
    if matcher.is_ignored(root, path, is_dir) {
        return false;
    }
    if is_dir {
        // Directories themselves carry no diffable content; their children
        // arrive as separate events.
        return false;
    }

    // A vanished file is treated as emptied; files that exist but cannot be
    // read as UTF-8 (e.g. binaries) are skipped to keep the stream readable.
    let content = if path.exists() {
        match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(_) => return false,
        }
    } else {
        String::new()
    };

    if let Some((stats, hunks)) = snapshots.check_diff(path, &content, cli.context, cli.cumulative)
    {
        presenter::print_diff(path, &stats, &hunks, cli.pace, cli.no_color);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent::new(code, modifiers))
    }

    #[test]
    fn quit_keys_are_recognized() {
        assert!(is_quit_event(&key(KeyCode::Char('q'), KeyModifiers::NONE)));
        assert!(is_quit_event(&key(KeyCode::Char('Q'), KeyModifiers::NONE)));
        assert!(is_quit_event(&key(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(is_quit_event(&key(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL
        )));
    }

    #[test]
    fn other_keys_and_events_are_ignored() {
        assert!(!is_quit_event(&key(KeyCode::Char('x'), KeyModifiers::NONE)));
        assert!(!is_quit_event(&key(KeyCode::Char('c'), KeyModifiers::NONE)));
        assert!(!is_quit_event(&key(KeyCode::Enter, KeyModifiers::NONE)));
        assert!(!is_quit_event(&Event::Resize(80, 24)));
        // Key-release events must not trigger a quit.
        let release = Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ));
        assert!(!is_quit_event(&release));
    }
}
