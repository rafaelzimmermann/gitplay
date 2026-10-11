use crate::fsutil;
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

/// Resolves the git directory backing `root`: a `.git` directory, or the
/// `gitdir:` target recorded in a pointer file (worktrees, submodules).
pub fn git_dir(root: &Path) -> Option<PathBuf> {
    let pointer = root.join(".git");
    if pointer.is_dir() {
        return Some(pointer);
    }
    let content = match fsutil::read_to_string(&pointer) {
        Ok(content) => content.trim().to_string(),
        Err(_) => return None,
    };
    let target = match content.strip_prefix("gitdir:") {
        Some(target) => target.trim(),
        None => return None,
    };
    let path = Path::new(target);
    Some(if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    })
}

/// HEAD as the watcher sees it: the ref it points at (`None` when detached)
/// and the commit id it resolves to (empty when it does not resolve, e.g. an
/// unborn branch).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadState {
    pub ref_name: Option<String>,
    pub oid: String,
}

/// Reads `HEAD` (and the ref it points at) straight from the git directory.
/// A handful of small file reads, no `git` subprocess, so it is cheap enough
/// to run after every filesystem event.
pub fn head_state(root: &Path) -> HeadState {
    let gitdir = match git_dir(root) {
        Some(dir) => dir,
        None => {
            return HeadState {
                ref_name: None,
                oid: String::new(),
            }
        }
    };
    let content = match fsutil::read_to_string(gitdir.join("HEAD")) {
        Ok(content) => content.trim().to_string(),
        Err(_) => {
            return HeadState {
                ref_name: None,
                oid: String::new(),
            }
        }
    };
    match content.strip_prefix("ref:") {
        Some(rest) => {
            let ref_name = rest.trim();
            HeadState {
                ref_name: Some(ref_name.to_string()),
                oid: resolve_ref(&gitdir, ref_name),
            }
        }
        None => HeadState {
            ref_name: None,
            oid: content.to_string(),
        },
    }
}

/// The ref store shared by all worktrees: the directory named by
/// `commondir`, or the git directory itself in a plain repository.
fn common_git_dir(gitdir: &Path) -> PathBuf {
    match fsutil::read_to_string(gitdir.join("commondir")) {
        Ok(content) => {
            let path = Path::new(content.trim());
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                gitdir.join(path)
            }
        }
        Err(_) => gitdir.to_path_buf(),
    }
}

/// Resolves the commit id a ref points at. Worktrees keep `HEAD` privately
/// but share branch refs with the main `.git`, and refs may be packed rather
/// than loose, so three locations are tried in turn.
fn resolve_ref(gitdir: &Path, ref_name: &str) -> String {
    if let Ok(oid) = fsutil::read_to_string(gitdir.join(Path::new(ref_name))) {
        return oid.trim().to_string();
    }
    let common = common_git_dir(gitdir);
    if let Ok(oid) = fsutil::read_to_string(common.join(Path::new(ref_name))) {
        return oid.trim().to_string();
    }
    if let Ok(packed) = fsutil::read_to_string(common.join("packed-refs")) {
        for line in packed.lines() {
            let line = line.trim();
            // `#` starts comments, `^` annotates peeled tags.
            if line.starts_with('#') || line.starts_with('^') {
                continue;
            }
            let mut parts = line.splitn(2, ' ');
            if let (Some(oid), Some(name)) = (parts.next(), parts.next()) {
                if name.trim() == ref_name {
                    return oid.to_string();
                }
            }
        }
    }
    String::new()
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

    #[test]
    fn head_resolves_via_shared_ref_store_and_packed_refs() {
        // Worktree layout: private gitdir with HEAD + commondir pointer, and
        // the branch ref living in the shared store.
        let root = temp_root("worktree");
        let main_git = root.join("main.git");
        let wt_git = root.join(".git");
        std::fs::create_dir_all(wt_git.join("refs/heads")).unwrap();
        std::fs::create_dir_all(main_git.join("refs/heads")).unwrap();
        std::fs::write(wt_git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(wt_git.join("commondir"), "../main.git\n").unwrap();
        let state = head_state(&root);
        // No ref written anywhere yet: unborn branch resolves to nothing.
        assert_eq!(state.ref_name, Some("refs/heads/main".to_string()));
        assert_eq!(state.oid, "");

        // Shared loose ref wins for the worktree.
        std::fs::write(main_git.join("refs/heads/main"), "looseoid\n").unwrap();
        assert_eq!(head_state(&root).oid, "looseoid");

        // Packed refs are consulted when no loose ref exists.
        let _ = std::fs::remove_file(main_git.join("refs/heads/main"));
        std::fs::write(
            main_git.join("packed-refs"),
            concat!(
                "# pack-refs with: peeled fully-peeled sorted \n",
                "packedoid refs/heads/main\n",
                "otheroid refs/heads/other\n"
            ),
        )
        .unwrap();
        assert_eq!(head_state(&root).oid, "packedoid");
        let _ = std::fs::remove_dir_all(&root);
    }
}
