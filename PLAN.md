# gitplay — Live Rolling Code Movement Visualizer

## 1. Product Vision & Problem Statement

When AI coding agents (or developers in fast iterative loops) modify a codebase, the human operator is often left in the dark about what is happening in real time. Standard terminal tools offer two suboptimal extremes:
1. **Static manual checks (`git diff`, `git status`)**: Require manual execution, interrupting workflow and showing a wall of text with no sense of temporal progression.
2. **Terminal logs / tool calls**: Show raw text or verbose tool payloads that lack visual diff syntax, line context, or clear file-level structure.

`gitplay` is a lightweight, zero-latency terminal tool written in Rust. You launch it in any git repository:

```bash
$ gitplay
```

It monitors working tree changes in real time and streams an animated, rolling diff feed—showing code additions, modifications, and deletions as they happen. It turns silent agent file modifications into a transparent, visual "theater" of code movement.

---

## 2. Key User Experiences (UX)

### 2.1 The Rolling Diff Stream (Default Mode)
- **Zero-configuration**: Automatically discovers git repository root and starts watching.
- **Rolling Feed**: Whenever a file is written or touched, `gitplay` renders a clear file banner followed by formatted diff hunks:
  - Additions rendered in bright green (`+`) with highlighted inline edits.
  - Deletions rendered in red (`-`).
  - Context lines dimmed for readability.
- **Paced Code Movement**: Rather than dumping 2,000 lines in 1ms, `gitplay` offers a configurable rolling pace (`--pace`, default 15ms), allowing the human eye to perceive the trajectory of code as an agent crafts it.
- **Interactive Exit**: Pressing `q` (or `Esc` / `Ctrl+C`) stops the watcher cleanly; terminal raw mode is guarded by RAII and restored on exit, return, or panic.
- **Delta vs. Baseline Awareness**:
  - **Delta mode (`--delta`, default)**: Shows what was modified in the *latest write event* compared to the previous tick.
  - **Cumulative mode (`--cumulative`)**: Shows the total diff of the working tree relative to `git HEAD`.
  - Both modes can be toggled interactively or via CLI flags.

### 2.2 Replay Mode (`gitplay play [commit..commit]`)
- Allows replaying the diff progression of recent commits or git stash history at human-readable speeds.

### 2.3 Noise Suppression & Gitignore Compliance
- Fully respects `.gitignore`, `.git/info/exclude`, and global gitignores.
- Automatically ignores build artifacts (`target/`, `node_modules/`, `*.lock`, binary files).
- Configurable debounce window (default 150ms) to consolidate multi-pass atomic writes.

---

## 3. CLI Interface & Flags

```bash
USAGE:
    gitplay [OPTIONS] [PATH]

ARGS:
    <PATH>                   Repository path to monitor [default: .]

OPTIONS:
    -m, --mode <MODE>        Display mode: 'stream' (default) or 'tui' [default: stream]
    -d, --delta              Show incremental changes since last tick [default: true]
    -c, --cumulative         Show cumulative diffs against git HEAD
    -p, --pace <MS>          Delay in milliseconds per line/hunk for rolling effect [default: 15]
    -w, --debounce <MS>      Debounce window in milliseconds [default: 150]
    -u, --untracked          Include untracked files [default: true]
    -U, --context <N>        Number of context lines to display [default: 3]
        --no-color           Disable ANSI color styling
        --once               Scan working tree once, print diffs, and exit
    -v, --verbose            Show verbose debug logs
    -h, --help               Print help information
    -V, --version            Print version information
```

---

## 4. Technical Architecture

### 4.1 System Modules

```
┌─────────────────────────────────────────────────────────────┐
│                       gitplay CLI                           │
└──────────────┬───────────────────────────────┬──────────────┘
               │                               │
       ┌───────▼────────┐             ┌────────▼────────┐
       │ File Watcher   │             │ Git Repository  │
       │ (notify crate) │             │ (HEAD & Index)  │
       └───────┬────────┘             └────────┬────────┘
               │ FileChangeEvents              │ Snapshots
               └───────────────┬───────────────┘
                               │
                       ┌───────▼────────┐
                       │  Diff Engine   │
                       │ (similar crate)│
                       └───────┬────────┘
                               │ Hunks & Stats
               ┌───────────────┴───────────────┐
               │                               │
       ┌───────▼────────┐             ┌────────▼────────┐
       │ Rolling Stream │             │  Ratatui TUI    │
       │   Presenter    │             │   Dashboard     │
       └────────────────┘             └─────────────────┘
```

1. **`cli`**: Argument parsing via `clap` (derive mode), terminal detection, color configuration.
2. **`repo`**: Git repository discovery, workdir discovery, HEAD commit inspection, `.gitignore` parsing via `ignore` crate.
3. **`watcher`**: Cross-platform filesystem event monitoring (`notify` + `notify-debouncer-mini`), debounced channel sender, ignore-list filtering.
4. **`diff`**: In-memory text diffing using the `similar` crate (Myers algorithm). Manages file snapshots:
   - `last_snapshot`: file state at tick Tn-1 (for delta calculation).
   - `head_snapshot`: file state at git `HEAD` (for cumulative calculation).
   - Produces formatted `DiffHunk` structures with line numbers and token spans.
5. **`commits`**: HEAD tracking. Reads `HEAD` (and the ref it points at) from
   the git directory after every debounced event batch, detects when it moves,
   and lists the new commits (capped at 20) via `git log`, oldest first.
6. **`presenter`**:
   - `stream`: Terminal streaming engine using `crossterm`. Formats banners, stats, and colorized diff lines. Implements smooth pacing timer.
   - `stats`: Live counter of files touched, lines added (`+`), and lines removed (`-`).

---

## 5. Delivery Milestones

### Phase M0 — Foundations & Core Diff Engine (Completed)
- [x] Initialize Cargo workspace and crate structure.
- [x] Add dependencies (`clap`, `similar`, `notify`, `notify-debouncer-mini`, `ignore`, `crossterm`).
- [x] Implement `diff` engine: compute unified diffs from two in-memory string slices with line numbering.
- [x] Comprehensive unit tests for diff formatting, added files, removed files, modified lines.

### Phase M1 — Watcher & Git State Tracking (Completed)
- [x] Implement git repo root discovery and branch/HEAD tracking.
- [x] Integrate `.gitignore` filtering using `ignore` crate.
- [x] Set up debounced file watcher listening on working tree.
- [x] Cache file states in memory; compute delta diffs upon modification.

### Phase M2 — Rolling Stream Engine & Pacing (Completed)
- [x] Terminal banner formatting: timestamp, file path, `+additions / -deletions`.
- [x] Colorized unified diff output with configurable context lines.
- [x] Configurable rolling pacing loop (`--pace <ms>`).
- [x] Signal handling (graceful Ctrl+C exit without terminal corruption).
- [x] Interactive quit: pressing 'q' (or Esc / Ctrl+C) exits via a raw-mode keyboard listener; stdin TTY detection, RAII raw-mode guard, and panic-hook terminal restoration.

### Phase M3 — Verification, Packaging & Installation (In Progress)
- [x] Synthetic end-to-end test verifying multi-file write stream.
- [x] Strict Clippy audit (`--deny warnings`).
- [x] Zero-latency deterministic unit tests passing in `< 0.05s`.
- [ ] Relative repo-root path formatting in output banners.
- [ ] Git HEAD baseline tracking for `--cumulative` and `--once` modes.
- [x] Commit reporting: HEAD-move detection (worktrees and packed refs
      included), `git log` message listing, checkout/detach notes, footer with
      repository path and current branch.
- [x] Automated installation script (`install.sh`) supporting local build and PATH detection.
- [ ] Shell completion and PATH discovery verification.

---

## 6. Installation & Distribution Script (`install.sh`)

### 6.1 Requirements & Goals
A zero-friction, robust POSIX shell installation script (`install.sh` / `scripts/install.sh`) allowing users to install, update, or uninstall `gitplay` with a single command.

### 6.2 Key Features
1. **Target Prefix Resolution**:
   - Explicit override: respects `$INSTALL_DIR` or `--dir <path>`.
   - PATH-aware autodetection:
     - Priority 1: `~/.cargo/bin` (if present in `$PATH`).
     - Priority 2: `~/.local/bin` (if present in `$PATH`).
     - Priority 3: `/usr/local/bin` (if running with write permissions or root).
     - Fallback: `~/.local/bin` (with automatic shell profile export advice).
2. **Build & Install Pipeline**:
   - Checks for `cargo` and `rustc`. If missing, prompts with the official rustup command.
   - Compiles release binary with locked dependencies: `cargo build --release --locked`.
   - Atomically installs the binary with executable permissions (`install -m 755` or `cp` + `chmod +x`).
3. **Verification**:
   - Executes `gitplay --version` to verify functional installation.
   - Outputs a success confirmation with quick-start usage tips.
4. **Lifecycle Operations**:
   - Supports `--uninstall` to cleanly remove the installed binary and symlinks.
   - Supports `--help` for clear command usage.

### 6.3 Usage Patterns
```bash
# Install from within repository checkout:
./install.sh

# Custom destination:
INSTALL_DIR=/usr/local/bin ./install.sh
# or
./install.sh --dir /usr/local/bin

# Clean uninstall:
./install.sh --uninstall
```

---

## 7. Engineering Constraints & Invariants

1. **Local Development Rule**: All production code edits must go through a reviewed local development workflow. Design, planning, review, and documentation are maintained alongside the code.
2. **Deterministic Tests**: All core logic (diffing, snapshot tracking, formatting, filtering) must be covered by fast, deterministic unit tests running in `< 0.1s`.
3. **Strict Quality Bar**: `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` must pass on every commit.
4. **Zero Overhead Idle**: When no files are changing, `gitplay` must sleep in `inotify` wait with 0.0% CPU usage.
