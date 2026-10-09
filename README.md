# gitplay

> 🎬 A live, rolling code-movement and diff visualizer for git repositories.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Crates.io](https://img.shields.io/badge/crates.io-gitplay-orange.svg)](https://crates.io/crates/gitplay)

`gitplay` watches your working tree in real time and streams an animated,
rolling diff feed — letting you *watch* code changes, refactors, and AI coding
agents as they craft code.

## Why gitplay?

When an AI coding agent (or a developer in a fast iterative loop) modifies a
codebase, the human operator is usually left in the dark about what is
happening in real time. Existing tools sit at two suboptimal extremes:

- **Static checks** (`git diff`, `git status`) must be run manually and dump a
  wall of text with no sense of temporal progression.
- **Terminal logs / tool-call traces** show raw payloads without diff syntax,
  line context, or file-level structure.

`gitplay` turns silent file modifications into a transparent, visual **theater
of code movement**: every write event is rendered as a paced, colorized,
rolling diff — so the trajectory of the code is visible to the human eye
instead of arriving as an instant, unreadable blob.

```
$ gitplay
[gitplay] Watching /code/myproject (pace: 15ms, debounce: 150ms) — press 'q' to quit

────────────────────────────────────────────────────────────────
 14:32:05  src/main.rs                      +12 / −3
────────────────────────────────────────────────────────────────
@@ -41,6 +41,9 @@
     let root = find_git_root(&cli.path);
     let matcher = GitignoreMatcher::new(&root);
+    let mut snapshots = SnapshotStore::new();
+
+    scan_tree(&root, &root, &cli, &mut snapshots, &matcher, false);
     if cli.once {
```

## Features

- **Continuous live stream** — catches file writes, moves, and modifications
  the moment they hit disk.
- **Interactive exit** — press `q` (or `Esc` / `Ctrl+C`) to quit the watcher
  cleanly; raw mode is enabled only when stdin is a TTY and is always
  restored via an RAII guard, so piped and CI runs are unaffected.
- **Rolling visual pace** — instead of dumping diffs instantly, lines roll
  smoothly into view at a configurable pace, making code movement perceptible.
- **Delta & cumulative diffs** — see what changed in the latest write tick, or
  the total diff against the baseline.
- **Gitignore respecting** — skips noise (`target/`, `node_modules/`, binary
  files) via the `ignore` crate.
- **Zero-CPU idle** — sleeps in native inotify waits when nothing changes.

## Installation

### Quick install (recommended)

From a repository checkout:

```bash
./install.sh
```

The script builds `gitplay` in release mode and installs it into a directory
on your `PATH`, auto-detecting the best target (`~/.cargo/bin`,
`~/.local/bin`, or `/usr/local/bin`). No root required.

```bash
./install.sh --dir /usr/local/bin   # custom destination
./install.sh --uninstall            # clean removal
```

### With cargo

```bash
cargo install --git https://github.com/rafaelzimmermann/gitplay
```

### From source

```bash
git clone https://github.com/rafaelzimmermann/gitplay
cd gitplay
cargo build --release --locked
install -m 755 target/release/gitplay ~/.local/bin/gitplay
```

## Usage

```bash
gitplay                          # watch the current repository (press 'q' to quit)
gitplay ~/code/myproject         # watch a specific repository
gitplay --pace 40 --cumulative   # slower roll, diff against baseline
```

### CLI options

| Option | Default | Description |
| --- | --- | --- |
| `<PATH>` | `.` | Path to the repository to watch. |
| `-p, --pace <MS>` | `15` | Rolling line delay in milliseconds. Lower is faster. |
| `-c, --cumulative` | off | Show cumulative diffs against the initial baseline instead of per-tick deltas. |
| `-w, --debounce <MS>` | `150` | Debounce window in milliseconds to consolidate multi-pass atomic writes. |
| `-U, --context <N>` | `3` | Number of context lines shown around diff hunks. |
| `--no-color` | off | Disable ANSI color styling. |
| `--once` | off | Scan the working tree once, print diffs, and exit. |

Run `gitplay --help` for the full CLI reference.

## Contributing

Contributions are welcome! To get started:

1. Fork the repository and create your branch from `main`.
2. Make your changes — see [PLAN.md](PLAN.md) for the architecture and design
   overview.
3. Ensure the quality bar passes: `cargo fmt --check`, `cargo test --locked`,
   and `cargo clippy --all-targets -- -D warnings`.
4. Open a pull request with a clear description of the change.

## License

This project is licensed under the [MIT License](LICENSE).
