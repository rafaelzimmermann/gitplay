use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::path::{Path, PathBuf};

/// Walk up from `start` looking for the closest ancestor containing a `.git`
/// directory (or `.git` file, as produced by git worktrees). If none is found,
/// `start` itself is returned.
pub fn find_git_root(start: &Path) -> PathBuf {
    let canonical = std::fs::canonicalize(start).unwrap_or_else(|_| start.to_path_buf());
    for dir in canonical.ancestors() {
        if dir.join(".git").exists() {
            return dir.to_path_buf();
        }
    }
    start.to_path_buf()
}

/// Filters files using the repository's `.gitignore` and `.git/info/exclude`,
/// plus a small set of built-in rules for VCS internals and editor droppings.
pub struct GitignoreMatcher {
    matcher: Gitignore,
}

impl GitignoreMatcher {
    pub fn new(root: &Path) -> Self {
        let mut builder = GitignoreBuilder::new(root);
        for candidate in [
            root.join(".gitignore"),
            root.join(".git").join("info").join("exclude"),
        ] {
            if candidate.is_file() {
                let _ = builder.add(&candidate);
            }
        }
        // An empty builder always builds successfully; use it as the fallback
        // so a malformed ignore file never takes the process down.
        let matcher = builder
            .build()
            .unwrap_or_else(|_| GitignoreBuilder::new(root).build().expect("empty builder"));
        Self { matcher }
    }

    pub fn is_ignored(&self, root: &Path, path: &Path, is_dir: bool) -> bool {
        // Never inspect anything inside `.git`.
        if path.components().any(|c| c.as_os_str() == ".git") {
            return true;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if (name.starts_with('.') && name != ".gitignore")
            || name.ends_with('~')
            || name.ends_with(".swp")
            || name.ends_with(".tmp")
            || name.ends_with(".rs.bk")
        {
            return true;
        }
        match path.strip_prefix(root) {
            Ok(rel) => self
                .matcher
                .matched_path_or_any_parents(rel, is_dir)
                .is_ignore(),
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gitplay-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn finds_git_root_from_nested_path() {
        let root = temp_root("find-root");
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir_all(root.join("a/b/c")).unwrap();
        assert_eq!(find_git_root(&root.join("a/b/c")), root);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn returns_start_when_no_git_root() {
        let root = temp_root("no-root");
        assert_eq!(find_git_root(&root), root);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn respects_gitignore_patterns() {
        let root = temp_root("ignore");
        std::fs::write(root.join(".gitignore"), "target/\n*.log\nbuild/\n").unwrap();
        let matcher = GitignoreMatcher::new(&root);

        assert!(matcher.is_ignored(&root, &root.join("debug.log"), false));
        assert!(!matcher.is_ignored(&root, &root.join("src/main.rs"), false));
        // Ignored via a matched parent directory (`target/`).
        assert!(matcher.is_ignored(&root, &root.join("target/debug/gitplay"), false));
        // Ignored via a matched parent directory (`build/`).
        assert!(
            matcher.is_ignored(&root, &root.join("build/sub/output.txt"), false),
            "parent directory ignore rules must apply"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn builtin_rules_ignore_git_and_editor_artifacts() {
        let root = temp_root("builtin");
        let matcher = GitignoreMatcher::new(&root);

        assert!(matcher.is_ignored(&root, &root.join(".git/config"), false));
        assert!(matcher.is_ignored(&root, &root.join(".env"), false));
        assert!(matcher.is_ignored(&root, &root.join("notes.md~"), false));
        assert!(matcher.is_ignored(&root, &root.join("swap.swp"), false));
        assert!(matcher.is_ignored(&root, &root.join("temp.tmp"), false));
        assert!(matcher.is_ignored(&root, &root.join("code.rs.bk"), false));
        // `.gitignore` itself is tracked by git and stays visible.
        assert!(!matcher.is_ignored(&root, &root.join(".gitignore"), false));
        let _ = std::fs::remove_dir_all(&root);
    }
}
