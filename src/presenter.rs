use crate::commits::HeadMove;
use crate::diff::{DiffLine, DiffStats, FormattedHunk};
use crossterm::style::{Color, Stylize};
use crossterm::terminal::size;
use crossterm::tty::IsTty;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Whether the terminal is currently in raw mode (set by the watcher).
/// While set, the terminal no longer translates a bare `\n` into a
/// carriage return and line feed, so line endings must be written as
/// `\r\n` to keep output from staircasing.
static RAW_OUTPUT: AtomicBool = AtomicBool::new(false);

/// Records whether raw mode is active on the terminal (see `watcher`).
pub fn set_raw_output(enabled: bool) {
    RAW_OUTPUT.store(enabled, Ordering::Relaxed);
}

fn line_ending() -> &'static str {
    if RAW_OUTPUT.load(Ordering::Relaxed) {
        "\r\n"
    } else {
        "\n"
    }
}

/// Formats `[HH:MM:SS]` from the system clock.
fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let time_of_day = secs % 86_400;
    format!(
        "[{:02}:{:02}:{:02}]",
        time_of_day / 3600,
        (time_of_day % 3600) / 60,
        time_of_day % 60
    )
}

/// The path as shown in a diff banner: relative to the repository root so
/// banners stay short (`src/main.rs`); anything outside the root keeps its
/// full display form.
fn banner_path(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(relative) if !relative.as_os_str().is_empty() => relative.display().to_string(),
        _ => path.display().to_string(),
    }
}

/// Prints a file banner followed by colorized hunks. When `pace_ms` is set and
/// stdout is a terminal, each line is flushed and followed by a small delay so
/// the diff "rolls" instead of dumping all at once.
pub fn print_diff(
    root: &Path,
    path: &Path,
    stats: &DiffStats,
    hunks: &[FormattedHunk],
    pace_ms: u64,
    no_color: bool,
) {
    let pacing = if pace_ms > 0 && std::io::stdout().is_tty() {
        Some(Duration::from_millis(pace_ms))
    } else {
        None
    };

    let mut out = std::io::stdout().lock();
    let eol = line_ending();
    let banner = format!(
        "{} {} (+{}, -{})",
        timestamp(),
        banner_path(root, path),
        stats.additions,
        stats.deletions
    );
    let _ = if no_color {
        write!(out, "{banner}{eol}")
    } else {
        write!(out, "{}{eol}", banner.bold())
    };

    for hunk in hunks {
        let header = format!("@@ -{} +{} @@", hunk.old_start, hunk.new_start);
        let _ = if no_color {
            write!(out, "{header}{eol}")
        } else {
            write!(out, "{}{eol}", header.with(Color::Cyan))
        };
        paced(&mut out, pacing);

        for line in &hunk.lines {
            match line {
                DiffLine::Add(text) => {
                    let _ = if no_color {
                        write!(out, "+ {text}{eol}")
                    } else {
                        write!(out, "{}{eol}", format!("+ {text}").with(Color::Green))
                    };
                }
                DiffLine::Remove(text) => {
                    let _ = if no_color {
                        write!(out, "- {text}{eol}")
                    } else {
                        write!(out, "{}{eol}", format!("- {text}").with(Color::Red))
                    };
                }
                DiffLine::Context(text) => {
                    let _ = if no_color {
                        write!(out, "  {text}{eol}")
                    } else {
                        write!(out, "{}{eol}", format!("  {text}").dim())
                    };
                }
            }
            paced(&mut out, pacing);
        }
    }
    let _ = out.flush();
}

fn paced(out: &mut std::io::StdoutLock<'_>, delay: Option<Duration>) {
    if let Some(delay) = delay {
        let _ = out.flush();
        sleep(delay);
    }
}

/// Trims a commit id to the first 7 characters, the usual display form.
fn short_hash(hash: String) -> String {
    hash.chars().take(7).collect()
}

/// One-line note for a HEAD move that brought no new commits, i.e. a plain
/// checkout. `None` when there is nothing worth reporting (a reset or rewind
/// on the same branch).
fn switch_note(head_move: &HeadMove) -> Option<String> {
    if !head_move.ref_changed {
        return None;
    }
    Some(match &head_move.branch {
        Some(name) => format!("switched to branch {name}"),
        None => format!("detached HEAD at {}", short_hash(head_move.oid.clone())),
    })
}

/// Prints the commits a single HEAD move brought in: one bold header line per
/// commit (`[HH:MM:SS] commit <hash> (branch) "subject"`), followed by the
/// message body dimmed and indented. Commits land in the order they happened.
/// A checkout that adds no commits gets a single "switched to branch" note.
pub fn print_head_move(head_move: &HeadMove, no_color: bool) {
    if head_move.commits.is_empty() && !head_move.truncated {
        let note = match switch_note(head_move) {
            Some(note) => format!("{} {note}", timestamp()),
            None => return,
        };
        let mut out = std::io::stdout().lock();
        let eol = line_ending();
        let _ = if no_color {
            write!(out, "{note}{eol}")
        } else {
            write!(out, "{0}{eol}", note.bold())
        };
        let _ = out.flush();
        return;
    }
    let mut out = std::io::stdout().lock();
    let eol = line_ending();
    let branch = match &head_move.branch {
        Some(name) => format!(" ({name})"),
        None => String::new(),
    };
    for commit in &head_move.commits {
        let header = format!(
            "{} commit {}{} \"{}\"",
            timestamp(),
            short_hash(commit.hash.clone()),
            branch,
            commit.subject,
        );
        let _ = if no_color {
            write!(out, "{header}{eol}")
        } else {
            write!(out, "{}{eol}", header.bold())
        };
        for line in &commit.body {
            let _ = if no_color {
                write!(out, "  {line}{eol}")
            } else {
                write!(out, "{}{eol}", format!("  {line}").dim())
            };
        }
    }
    if head_move.truncated {
        let note = format!("{} ... more commits not shown", timestamp());
        let _ = if no_color {
            write!(out, "{note}{eol}")
        } else {
            write!(out, "{}{eol}", note.dim())
        };
    }
    let _ = out.flush();
}

/// Width of the terminal, or a sane fallback when stdout is not one (piped
/// output, CI) so the footer rule still has a predictable length.
fn footer_width() -> usize {
    match size() {
        Ok((width, _)) => usize::from(width),
        Err(_) => 80,
    }
}

/// Shortens a path with the home directory spelled as `~` so the footer stays
/// readable: `/home/you/code/app` becomes `~/code/app`.
fn home_short(path: &Path) -> String {
    let display = path.display().to_string();
    let home = std::env::var("HOME").unwrap_or_default();
    if !home.is_empty() && display.starts_with(&home) {
        let tail: &str = &display[home.len()..];
        format!("~{tail}")
    } else {
        display
    }
}

/// Prints the footer: a full-width rule followed by the watched repository
/// path and the branch under HEAD. Redrawn after every update so the bottom
/// of the stream always states where the watch is and which branch it is on.
pub fn print_footer(root: &Path, branch: Option<String>, no_color: bool) {
    let mut out = std::io::stdout().lock();
    let eol = line_ending();
    let rule = "─".repeat(footer_width());
    let label = match branch {
        Some(name) => format!(" ({name})"),
        None => String::new(),
    };
    let status = format!("{0}{1}", home_short(root), label);
    let _ = if no_color {
        write!(out, "{rule}{eol}{status}{eol}")
    } else {
        write!(out, "{0}{1}{2}{3}", rule.dim(), eol, status.bold(), eol)
    };
    let _ = out.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_has_hour_minute_second_shape() {
        let ts = timestamp();
        assert!(ts.starts_with('[') && ts.ends_with(']'));
        let inner = &ts[1..ts.len() - 1];
        let parts: Vec<&str> = inner.split(':').collect();
        assert_eq!(parts.len(), 3);
        for part in parts {
            assert_eq!(part.len(), 2, "each component is zero-padded");
            assert!(part.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn line_ending_tracks_raw_mode() {
        set_raw_output(true);
        assert_eq!(line_ending(), "\r\n");
        set_raw_output(false);
        assert_eq!(line_ending(), "\n");
    }

    #[test]
    fn banner_path_strips_the_repository_root() {
        let root = Path::new("/home/you/app");
        assert_eq!(banner_path(root, &root.join("src/main.rs")), "src/main.rs");
        assert_eq!(banner_path(root, &root.join("README.md")), "README.md");
        // Paths outside the root keep their full form.
        assert_eq!(
            banner_path(root, Path::new("/elsewhere/other.txt")),
            "/elsewhere/other.txt"
        );
    }

    #[test]
    fn print_diff_writes_banner_and_hunk_lines() {
        // stdout is piped during tests, so pacing is disabled and this
        // completes without sleeping.
        let root = Path::new("/repo");
        let path = Path::new("/repo/src/demo.rs");
        let stats = DiffStats {
            additions: 1,
            deletions: 1,
        };
        let hunks = vec![FormattedHunk {
            old_start: 3,
            new_start: 3,
            lines: vec![
                DiffLine::Context("ctx".into()),
                DiffLine::Remove("old".into()),
                DiffLine::Add("new".into()),
            ],
        }];
        print_diff(root, path, &stats, &hunks, 0, true);
    }

    #[test]
    fn switch_note_reports_checkouts_and_detached_heads() {
        let switch = HeadMove {
            commits: Vec::new(),
            branch: Some("main".into()),
            truncated: false,
            oid: "0123456789abcdef".into(),
            ref_changed: true,
        };
        assert_eq!(
            switch_note(&switch),
            Some("switched to branch main".to_string())
        );
        let detached = HeadMove {
            commits: Vec::new(),
            branch: None,
            truncated: false,
            oid: "0123456789abcdef".into(),
            ref_changed: true,
        };
        assert_eq!(
            switch_note(&detached),
            Some("detached HEAD at 0123456".to_string())
        );
        let reset = HeadMove {
            commits: Vec::new(),
            branch: Some("main".into()),
            truncated: false,
            oid: "0123456789abcdef".into(),
            ref_changed: false,
        };
        assert_eq!(switch_note(&reset), None);
    }
}
