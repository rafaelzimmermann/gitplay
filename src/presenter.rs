use crate::diff::{DiffLine, DiffStats, FormattedHunk};
use crossterm::style::{Color, Stylize};
use crossterm::tty::IsTty;
use std::io::Write;
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

/// Prints a file banner followed by colorized hunks. When `pace_ms` is set and
/// stdout is a terminal, each line is flushed and followed by a small delay so
/// the diff "rolls" instead of dumping all at once.
pub fn print_diff(
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
    let banner = format!(
        "{} {} (+{}, -{})",
        timestamp(),
        path.display(),
        stats.additions,
        stats.deletions
    );
    let _ = if no_color {
        writeln!(out, "{banner}")
    } else {
        writeln!(out, "{}", banner.bold())
    };

    for hunk in hunks {
        let header = format!("@@ -{} +{} @@", hunk.old_start, hunk.new_start);
        let _ = if no_color {
            writeln!(out, "{header}")
        } else {
            writeln!(out, "{}", header.with(Color::Cyan))
        };
        paced(&mut out, pacing);

        for line in &hunk.lines {
            match line {
                DiffLine::Add(text) => {
                    let _ = if no_color {
                        writeln!(out, "+ {text}")
                    } else {
                        writeln!(out, "{}", format!("+ {text}").with(Color::Green))
                    };
                }
                DiffLine::Remove(text) => {
                    let _ = if no_color {
                        writeln!(out, "- {text}")
                    } else {
                        writeln!(out, "{}", format!("- {text}").with(Color::Red))
                    };
                }
                DiffLine::Context(text) => {
                    let _ = if no_color {
                        writeln!(out, "  {text}")
                    } else {
                        writeln!(out, "{}", format!("  {text}").dim())
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
    fn print_diff_writes_banner_and_hunk_lines() {
        // stdout is piped during tests, so pacing is disabled and this
        // completes without sleeping.
        let path = Path::new("src/demo.rs");
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
        print_diff(path, &stats, &hunks, 0, true);
    }
}
