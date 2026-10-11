use similar::{ChangeTag, TextDiff};
use std::collections::HashMap;
use std::path::Path;

/// Aggregate change counts for a single file diff.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiffStats {
    pub additions: usize,
    pub deletions: usize,
}

/// A single rendered line inside a hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLine {
    Add(String),
    Remove(String),
    Context(String),
}

/// A unified-diff hunk with starting line numbers on each side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormattedHunk {
    pub old_start: usize,
    pub new_start: usize,
    pub lines: Vec<DiffLine>,
}

/// Computes a unified diff (Myers, via `similar`) between two file contents.
///
/// Returns the aggregate `+/-` stats plus formatted hunks containing at most
/// `context` unchanged lines around each changed region.
pub fn compute_diff(
    old_content: &str,
    new_content: &str,
    context: usize,
) -> (DiffStats, Vec<FormattedHunk>) {
    let diff = TextDiff::from_lines(old_content, new_content);

    let mut stats = DiffStats::default();
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Insert => stats.additions += 1,
            ChangeTag::Delete => stats.deletions += 1,
            ChangeTag::Equal => {}
        }
    }

    let mut hunks = Vec::new();
    for group in diff.grouped_ops(context) {
        let mut lines = Vec::new();
        let mut first_old: Option<usize> = None;
        let mut first_new: Option<usize> = None;

        for change in group.iter().flat_map(|op| diff.iter_changes(op)) {
            let value = strip_eol(change.value());
            match change.tag() {
                ChangeTag::Equal => {
                    if let Some(i) = change.old_index() {
                        first_old.get_or_insert(i);
                    }
                    if let Some(i) = change.new_index() {
                        first_new.get_or_insert(i);
                    }
                    lines.push(DiffLine::Context(value));
                }
                ChangeTag::Delete => {
                    if let Some(i) = change.old_index() {
                        first_old.get_or_insert(i);
                    }
                    lines.push(DiffLine::Remove(value));
                }
                ChangeTag::Insert => {
                    if let Some(i) = change.new_index() {
                        first_new.get_or_insert(i);
                    }
                    lines.push(DiffLine::Add(value));
                }
            }
        }

        hunks.push(FormattedHunk {
            // 0-based indices become 1-based starts; an empty side stays 0,
            // matching unified-diff conventions for pure add/delete hunks.
            old_start: first_old.map_or(0, |i| i + 1),
            new_start: first_new.map_or(0, |i| i + 1),
            lines,
        });
    }

    (stats, hunks)
}

fn strip_eol(line: &str) -> String {
    line.trim_end_matches('\n')
        .trim_end_matches('\r')
        .to_string()
}

/// In-memory file snapshots: the last observed content per file (for delta
/// diffs) and the initial baseline content (for cumulative diffs).
#[derive(Debug, Default)]
pub struct SnapshotStore {
    last: HashMap<std::path::PathBuf, String>,
    baseline: HashMap<std::path::PathBuf, String>,
    /// Files whose current state has already been streamed; a repeat event
    /// carrying identical content must not stream it twice.
    reported: std::collections::HashSet<std::path::PathBuf>,
}

impl SnapshotStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a file's content without producing a diff (initial walk).
    pub fn snapshot(&mut self, path: &Path, content: &str) {
        self.baseline
            .entry(path.to_path_buf())
            .or_insert_with(|| content.to_string());
        self.last.insert(path.to_path_buf(), content.to_string());
    }

    /// Diffs `current_content` against the stored snapshot for `path` —
    /// in cumulative mode against `head` (the file's committed content in
    /// git HEAD) when available and the initial baseline otherwise; in delta
    /// mode against the last observation. Updates the stored state and
    /// returns the diff if anything changed.
    #[allow(clippy::type_complexity)]
    pub fn check_diff(
        &mut self,
        path: &Path,
        current_content: &str,
        context: usize,
        cumulative: bool,
        head: Option<&str>,
    ) -> Option<(DiffStats, Vec<FormattedHunk>)> {
        let key = path.to_path_buf();
        // Seed the baseline on first observation so cumulative mode always
        // has an anchor to fall back to.
        self.baseline
            .entry(key.clone())
            .or_insert_with(|| current_content.to_string());

        // Repeats of the same content (a touch, a reopen without edits, an
        // access-time echo from some other reader) must not re-print a
        // cumulative diff that never changed — but a first observation may
        // legitimately report state seeded earlier, as `--once` does.
        let unchanged = self.last.get(&key).map(String::as_str) == Some(current_content)
            && self.reported.contains(&key);

        let previous = if cumulative {
            // The committed content is the anchor when git can serve it;
            // untracked or binary files fall back to the startup snapshot.
            match head {
                Some(head) => head.to_string(),
                None => self.baseline.get(&key).cloned().unwrap_or_default(),
            }
        } else {
            self.last.get(&key).cloned().unwrap_or_default()
        };

        self.last.insert(key.clone(), current_content.to_string());

        if unchanged || previous == current_content {
            return None;
        }

        let (stats, hunks) = compute_diff(&previous, current_content, context);
        if stats.additions == 0 && stats.deletions == 0 {
            None
        } else {
            self.reported.insert(key);
            Some((stats, hunks))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn added_file_is_fully_additions() {
        let (stats, hunks) = compute_diff("", "alpha\nbeta\n", 3);
        assert_eq!(
            stats,
            DiffStats {
                additions: 2,
                deletions: 0
            }
        );
        assert_eq!(hunks.len(), 1);
        let hunk = &hunks[0];
        assert_eq!(hunk.old_start, 0);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(
            hunk.lines,
            vec![DiffLine::Add("alpha".into()), DiffLine::Add("beta".into())]
        );
    }

    #[test]
    fn deleted_file_is_fully_deletions() {
        let (stats, hunks) = compute_diff("alpha\nbeta\n", "", 3);
        assert_eq!(
            stats,
            DiffStats {
                additions: 0,
                deletions: 2
            }
        );
        assert_eq!(hunks.len(), 1);
        let hunk = &hunks[0];
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.new_start, 0);
        assert_eq!(
            hunk.lines,
            vec![
                DiffLine::Remove("alpha".into()),
                DiffLine::Remove("beta".into())
            ]
        );
    }

    #[test]
    fn modified_line_counts_stats_and_keeps_context() {
        let old = "one\ntwo\nthree\n";
        let new = "one\nTWO\nthree\n";
        let (stats, hunks) = compute_diff(old, new, 3);
        assert_eq!(
            stats,
            DiffStats {
                additions: 1,
                deletions: 1
            }
        );
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].old_start, 1);
        assert_eq!(hunks[0].new_start, 1);
        assert_eq!(
            hunks[0].lines,
            vec![
                DiffLine::Context("one".into()),
                DiffLine::Remove("two".into()),
                DiffLine::Add("TWO".into()),
                DiffLine::Context("three".into()),
            ]
        );
    }

    #[test]
    fn context_window_bounded_by_requested_lines() {
        let old: String = (1..=10).map(|i| format!("line{i}\n")).collect();
        let new: String = (1..=10)
            .map(|i| {
                if i == 6 {
                    "CHANGED\n".to_string()
                } else {
                    format!("line{i}\n")
                }
            })
            .collect();

        let (stats, hunks) = compute_diff(&old, &new, 1);
        assert_eq!(
            stats,
            DiffStats {
                additions: 1,
                deletions: 1
            }
        );
        assert_eq!(hunks.len(), 1);
        let hunk = &hunks[0];
        assert_eq!(hunk.old_start, 5);
        assert_eq!(hunk.new_start, 5);
        // Exactly one context line on each side of the change.
        assert_eq!(
            hunk.lines,
            vec![
                DiffLine::Context("line5".into()),
                DiffLine::Remove("line6".into()),
                DiffLine::Add("CHANGED".into()),
                DiffLine::Context("line7".into()),
            ]
        );
    }

    #[test]
    fn distant_changes_split_into_separate_hunks() {
        let old = "a\nb\nc\nd\ne\nf\ng\nh\n";
        let new = "a\nB\nc\nd\ne\nf\ng\nH\n";
        let (stats, hunks) = compute_diff(old, new, 1);
        assert_eq!(
            stats,
            DiffStats {
                additions: 2,
                deletions: 2
            }
        );
        assert_eq!(hunks.len(), 2, "changes separated by >2x context split");
        assert_eq!(hunks[0].old_start, 1);
        assert_eq!(hunks[1].old_start, 7);
    }

    #[test]
    fn identical_content_yields_no_diff() {
        let (stats, hunks) = compute_diff("same\n", "same\n", 3);
        assert_eq!(
            stats,
            DiffStats {
                additions: 0,
                deletions: 0
            }
        );
        assert!(hunks.is_empty());
    }

    #[test]
    fn snapshot_store_progression_delta_and_cumulative() {
        let mut store = SnapshotStore::new();
        let path = std::path::Path::new("src/example.rs");

        // First observation: everything counts as added relative to nothing.
        let (stats, hunks) = store
            .check_diff(path, "fn main() {}\n", 3, false, None)
            .expect("first observation diff");
        assert_eq!(stats.additions, 1);
        assert!(matches!(hunks[0].lines[0], DiffLine::Add(_)));

        // Unchanged content produces nothing.
        assert!(store
            .check_diff(path, "fn main() {}\n", 3, false, None)
            .is_none());

        // Delta mode diffs against the previous observation only.
        let (stats, _) = store
            .check_diff(path, "fn main() {\n    todo!()\n}\n", 3, false, None)
            .expect("delta diff");
        assert_eq!(
            stats,
            DiffStats {
                additions: 3,
                deletions: 1
            }
        );

        // Repeated content stays quiet in cumulative mode too: the state
        // did not change, so nothing is re-printed.
        assert!(store
            .check_diff(path, "fn main() {\n    todo!()\n}\n", 3, true, None)
            .is_none());

        // A further edit reports the cumulative diff against the initial
        // baseline (3 added, 1 removed), not against the last observation
        // (which would be 1 added, 1 removed).
        let (stats, _) = store
            .check_diff(path, "fn main() {\n    todo!();\n}\n", 3, true, None)
            .expect("cumulative diff vs baseline");
        assert_eq!(
            stats,
            DiffStats {
                additions: 3,
                deletions: 1
            }
        );

        // Reverting to the baseline makes the cumulative diff empty again,
        // and the delta matches the last observation, so nothing is emitted.
        assert!(store
            .check_diff(path, "fn main() {}\n", 3, true, None)
            .is_none());
        assert!(store
            .check_diff(path, "fn main() {}\n", 3, false, None)
            .is_none());
    }

    #[test]
    fn snapshot_store_seeds_baseline_via_snapshot_without_diffing() {
        let mut store = SnapshotStore::new();
        let path = std::path::Path::new("src/seeded.rs");

        store.snapshot(path, "original\n");
        // Seeded content is identical, so no diff is emitted.
        assert!(store
            .check_diff(path, "original\n", 3, false, None)
            .is_none());

        let (stats, _) = store
            .check_diff(path, "original\nedited\n", 3, false, None)
            .expect("delta after seeding");
        assert_eq!(
            stats,
            DiffStats {
                additions: 1,
                deletions: 0
            }
        );
    }

    #[test]
    fn cumulative_prefers_head_over_startup_snapshot() {
        let mut store = SnapshotStore::new();
        let path = std::path::Path::new("src/tracked.rs");
        store.snapshot(path, "startup\n");

        // The file drifted from both anchors; the HEAD content wins.
        let (stats, hunks) = store
            .check_diff(path, "startup\nhead\n", 3, true, Some("committed\n"))
            .expect("cumulative diff against HEAD");
        assert_eq!(
            stats,
            DiffStats {
                additions: 2,
                deletions: 1
            }
        );
        assert_eq!(
            hunks[0].lines,
            vec![
                DiffLine::Remove("committed".into()),
                DiffLine::Add("startup".into()),
                DiffLine::Add("head".into()),
            ]
        );
    }

    #[test]
    fn content_matching_head_hides_the_cumulative_diff() {
        let mut store = SnapshotStore::new();
        let path = std::path::Path::new("src/committed.rs");
        store.snapshot(path, "dirty\n");

        // Equal to its HEAD blob, so nothing is emitted even though the
        // startup snapshot differs.
        assert!(store
            .check_diff(path, "clean\n", 3, true, Some("clean\n"))
            .is_none());
    }

    #[test]
    fn repeated_content_never_reprints_a_cumulative_diff() {
        let mut store = SnapshotStore::new();
        let path = std::path::Path::new("src/dirty.rs");
        store.snapshot(path, "a\n");

        // A real edit prints the cumulative diff against HEAD.
        let (stats, _) = store
            .check_diff(path, "a\nb\n", 3, true, Some("a\n"))
            .expect("cumulative diff");
        assert_eq!(stats.additions, 1);

        // The same content arriving again (touch, reopen, access-time echo)
        // stays quiet instead of repeating the diff.
        assert!(store
            .check_diff(path, "a\nb\n", 3, true, Some("a\n"))
            .is_none());

        // A further edit resumes reporting, still anchored on HEAD.
        let (stats, _) = store
            .check_diff(path, "a\nb\nc\n", 3, true, Some("a\n"))
            .expect("cumulative diff after another edit");
        assert_eq!(stats.additions, 2);
    }

    #[test]
    fn cumulative_reports_dirt_that_predates_the_first_event() {
        let mut store = SnapshotStore::new();
        let path = std::path::Path::new("src/preexisting.rs");
        // The seeding walk observed "a\nb\n" while git HEAD holds "a\n":
        // `--once`'s report pass streams the uncommitted line even though
        // nothing changed since seeding.
        let (stats, _) = store
            .check_diff(path, "a\nb\n", 3, true, Some("a\n"))
            .expect("uncommitted dirt reported");
        assert_eq!(stats.additions, 1);
    }

    #[test]
    fn snapshot_store_reports_file_deletion() {
        let mut store = SnapshotStore::new();
        let path = std::path::Path::new("src/gone.rs");
        store.snapshot(path, "a\nb\n");
        let (stats, hunks) = store
            .check_diff(path, "", 3, false, None)
            .expect("deletion diff");
        assert_eq!(
            stats,
            DiffStats {
                additions: 0,
                deletions: 2
            }
        );
        assert!(hunks[0]
            .lines
            .iter()
            .all(|l| matches!(l, DiffLine::Remove(_))));
    }
}
