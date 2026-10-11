use crate::repo::head_state;
use std::path::Path;
use std::process::Command;

/// Upper bound on how many commits a single HEAD move may print. A rebase or
/// a branch switch onto an old branch can move HEAD by hundreds of commits;
/// the stream stays readable instead of dumping the whole history.
const MAX_COMMITS_PER_EVENT: usize = 20;

/// Field separator used inside a formatted `git log` record (unit separator)
/// and the separator between records (record separator). Both are control
/// characters that never appear in commit metadata.
const FIELD_SEPARATOR: char = '\x1f';
const RECORD_SEPARATOR: char = '\x1e';

/// `git log` pretty format: hash, author, subject, body, record terminator.
const LOG_FORMAT: &str = "%H\x1f%an\x1f%s\x1f%b\x1e";

/// One commit as reported to the stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub hash: String,
    pub author: String,
    pub subject: String,
    pub body: Vec<String>,
}

/// A HEAD move: the commits it brought in, in the order they happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadMove {
    pub commits: Vec<Commit>,
    /// Branch HEAD now points at (`None` when detached), e.g. `main`.
    pub branch: Option<String>,
    /// Set when more commits existed than were printed.
    pub truncated: bool,
    /// The commit id HEAD now resolves to.
    pub oid: String,
    /// Whether the ref HEAD points at changed as well, i.e. this was a
    /// checkout rather than a commit (or reset) on the same branch.
    pub ref_changed: bool,
}

/// Parses `git log` output produced by `LOG_FORMAT` into commits. Pure
/// function so it can be unit-tested without spawning `git`.
pub fn parse_log(raw: String) -> Vec<Commit> {
    let mut commits = Vec::new();
    for record in raw.split(RECORD_SEPARATOR) {
        // `git log` joins records with a newline after the terminator, so
        // every record but the first starts with one; trim it away and drop
        // the empty tail the final terminator leaves behind.
        let record = record.trim();
        if record.is_empty() {
            continue;
        }
        let fields: Vec<&str> = record.split(FIELD_SEPARATOR).collect();
        if fields.len() < 4 {
            continue;
        }
        commits.push(Commit {
            hash: fields[0].to_string(),
            author: fields[1].to_string(),
            subject: fields[2].to_string(),
            body: fields[3]
                .trim()
                .lines()
                .map(|line| line.to_string())
                .collect(),
        });
    }
    commits
}

/// Runs `git` inside `root` and returns its stdout. `None` covers both a
/// missing binary and a failed invocation, so the stream stays quiet rather
/// than aborting the watcher.
fn run_git(root: &Path, args: Vec<String>) -> Option<String> {
    let mut full_args: Vec<String> = vec![
        "-C".to_string(),
        root.display().to_string(),
        "--no-pager".to_string(),
    ];
    full_args.extend(args);
    let result = match Command::new("git").args(full_args).output() {
        Ok(result) => result,
        Err(err) => {
            eprintln!("[gitplay] failed to run git: {err}");
            return None;
        }
    };
    if !result.status.success() {
        return None;
    }
    String::from_utf8(result.stdout).ok()
}

/// True when the `git` binary can be executed. Commit reporting needs it to
/// read commit messages; the diff stream works without it.
pub fn git_available() -> bool {
    run_git(Path::new("."), vec!["--version".to_string()]).is_some()
}

/// Lists commits reachable from `to` but not from `from` (`from` empty means
/// "everything new"), oldest first, capped at `MAX_COMMITS_PER_EVENT`.
pub fn list_commits(root: &Path, from: String, to: String) -> (Vec<Commit>, bool) {
    if to.is_empty() {
        return (Vec::new(), false);
    }
    let range = if from.is_empty() {
        to.clone()
    } else {
        format!("{from}..{to}")
    };
    let raw = match run_log(root, range) {
        Some(raw) => raw,
        // A rebase or garbage collection can make `from` unresolvable, which
        // fails the whole range; fall back to the new tip alone so the move
        // still gets reported. Empty ranges succeed and stay empty.
        None => run_log(root, to).unwrap_or_default(),
    };
    let mut commits = parse_log(raw);
    let truncated = commits.len() > MAX_COMMITS_PER_EVENT;
    if truncated {
        commits.truncate(MAX_COMMITS_PER_EVENT);
    }
    // `git log` is newest-first; the stream reads in the order they landed.
    commits.reverse();
    (commits, truncated)
}

/// Runs `git log` for one revision, capped and formatted per `LOG_FORMAT`.
fn run_log(root: &Path, rev: String) -> Option<String> {
    let args: Vec<String> = vec![
        "log".to_string(),
        format!("--pretty=format:{0}", LOG_FORMAT),
        format!("--max-count={}", MAX_COMMITS_PER_EVENT + 1),
        rev,
    ];
    run_git(root, args)
}

/// Tracks the last observed HEAD so every commit, merge, or branch switch
/// made while watching surfaces exactly once in the stream.
#[derive(Debug, Default)]
pub struct CommitTracker {
    pub last_oid: String,
    pub last_ref: Option<String>,
}

impl CommitTracker {
    /// Seeds the tracker with the repository's current HEAD.
    pub fn new(root: &Path) -> Self {
        let head = head_state(root);
        Self {
            last_oid: head.oid,
            last_ref: head.ref_name,
        }
    }

    /// Returns the commits HEAD picked up since the last observation, or
    /// `None` while neither the commit id nor the ref under HEAD moved (a
    /// detach at the current commit still counts as a move).
    pub fn check(&mut self, root: &Path) -> Option<HeadMove> {
        let head = head_state(root);
        if head.oid == self.last_oid && head.ref_name == self.last_ref {
            return None;
        }
        let ref_changed = head.ref_name != self.last_ref;
        let (commits, truncated) = list_commits(root, self.last_oid.clone(), head.oid.clone());
        let label = branch_label(head.ref_name.clone());
        self.last_oid = head.oid;
        self.last_ref = head.ref_name;
        Some(HeadMove {
            commits,
            branch: label,
            truncated,
            oid: self.last_oid.clone(),
            ref_changed,
        })
    }

    /// Branch label currently under HEAD (`None` when detached).
    pub fn branch(&self) -> Option<String> {
        branch_label(self.last_ref.clone())
    }
}

/// Turns a ref name such as `refs/heads/main` into what a human reads (`main`);
/// a detached HEAD has no label.
pub fn branch_label(ref_name: Option<String>) -> Option<String> {
    ref_name.map(|name| match name.strip_prefix("refs/heads/") {
        Some(short) => short.to_string(),
        None => name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parse_log_reads_records_and_bodies() {
        let raw = "aaa\x1fAda\x1fFirst subject\x1ffirst body\nmore body\x1ebbb\x1fBob\x1fSecond subject\x1f\x1e".to_string();
        let commits = parse_log(raw);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].hash, "aaa");
        assert_eq!(commits[0].author, "Ada");
        assert_eq!(commits[0].subject, "First subject");
        assert_eq!(
            commits[0].body,
            vec!["first body".to_string(), "more body".to_string()]
        );
        assert!(commits[1].body.is_empty());
    }

    #[test]
    fn parse_log_tolerates_record_separating_newlines() {
        // `git log` puts a newline after each record terminator.
        let raw = "aaa\x1fAda\x1fOne\x1f\x1e\nbbb\x1fBob\x1fTwo\x1f\x1e\n".to_string();
        let commits = parse_log(raw);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[1].hash, "bbb");
        assert_eq!(commits[1].subject, "Two");
    }

    #[test]
    fn parse_log_skips_malformed_records() {
        assert!(parse_log(String::new()).is_empty());
        assert!(parse_log("only\x1ftwo".to_string()).is_empty());
    }

    #[test]
    fn branch_label_shortens_head_refs() {
        assert_eq!(
            branch_label(Some("refs/heads/main".to_string())),
            Some("main".to_string())
        );
        assert_eq!(
            branch_label(Some("refs/heads/feature/x".to_string())),
            Some("feature/x".to_string())
        );
        assert_eq!(branch_label(None), None);
    }

    #[test]
    fn tracker_stays_quiet_while_head_is_still() {
        let root = temp_repo("quiet");
        let mut tracker = CommitTracker::new(&root);
        assert_eq!(tracker.branch(), Some("main".to_string()));
        assert!(tracker.check(&root).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn tracker_flags_a_branch_switch_without_new_commits() {
        // No valid git objects here, so `git log` fails and the move reports
        // zero commits — exactly the checkout shape the switch note covers.
        let root = temp_repo("switch");
        let mut tracker = CommitTracker::new(&root);
        std::fs::write(root.join(".git/HEAD"), "ref: refs/heads/feature\n").unwrap();
        std::fs::write(root.join(".git/refs/heads/feature"), "feedface\n").unwrap();
        let head_move = tracker.check(&root).expect("HEAD moved");
        assert!(head_move.commits.is_empty());
        assert!(head_move.ref_changed);
        assert_eq!(head_move.branch, Some("feature".to_string()));
        assert_eq!(tracker.branch(), Some("feature".to_string()));
        assert!(tracker.check(&root).is_none());
        // Detaching at the same commit keeps the oid but changes the ref.
        std::fs::write(root.join(".git/HEAD"), "feedface\n").unwrap();
        let head_move = tracker.check(&root).expect("ref moved");
        assert!(head_move.commits.is_empty());
        assert!(head_move.ref_changed);
        assert_eq!(head_move.branch, None);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Builds a throwaway repository layout (HEAD plus a ref file) so HEAD
    /// parsing can be exercised without spawning `git`.
    fn temp_repo(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("gitplay-test-{}-{tag}", std::process::id()))
            .to_path_buf();
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".git/refs/heads")).unwrap();
        std::fs::write(dir.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(dir.join(".git/refs/heads/main"), "deadbeef\n").unwrap();
        dir
    }
}
