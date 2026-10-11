# gitplay

Watch code changes appear as a live, rolling diff in your terminal.

`gitplay` monitors a repository's working tree and prints text file changes as
they happen. It is useful when you want to follow an active editing session,
including changes made by an AI coding agent, without repeatedly running
`git diff`.

## Features

- Streams additions, deletions, and context as files change.
- Reports commits as they happen: hash, branch, subject, and body.
- Keeps a status footer under the stream showing the repository path and
  current branch.
- Shows either each change since the last update or changes since startup.
- Respects repository `.gitignore` rules and `.git/info/exclude`.
- Skips hidden files, Git internals, editor temporary files, and files that
  cannot be read as UTF-8.
- Lets you adjust the display pace, debounce window, and diff context.
- Press `q`, `Esc`, or `Ctrl+C` in an interactive terminal to exit.

## Install

### Build and install from a checkout

Requires the Rust toolchain (`cargo` and `rustc`):

```sh
git clone https://github.com/rafaelzimmermann/gitplay.git
cd gitplay
./install.sh
```

The installer builds a release binary and chooses an install directory from
your `PATH`. To choose a directory explicitly:

```sh
./install.sh --dir "$HOME/.local/bin"
```

To remove the installed binary:

```sh
./install.sh --uninstall
```

You can also build without installing:

```sh
cargo build --release --locked
```

## Use

Run `gitplay` from inside a Git repository, or pass a path to one:

```sh
gitplay
gitplay ~/code/my-project
```

A session looks like this (file edits stream as diffs, commits are reported
with their messages, and a footer repeats the path and branch):

```text
[14:03:11] ~/code/my-project/src/main.rs (+12, -3)
@@ -18,7 +18,7 @@
     let parser = Parser::new(input);
-    let items = parser.parse_all()?;
+    let items = parser.parse(strict)?;
[14:03:40] commit 9f2c1ab (main) "fix off-by-one in parser"
  Commit bodies are printed indented under the subject.
───────────────────────────────────────────────────────────────────────────────
~/code/my-project (main)
```

### Options

| Option | Default | Description |
| --- | --- | --- |
| `[PATH]` | `.` | Repository path to watch. The nearest parent containing `.git` is used. |
| `-p, --pace <MS>` | `15` | Delay between displayed diff lines in a terminal. Set to `0` for no delay. |
| `-w, --debounce <MS>` | `150` | Time window for grouping filesystem events. |
| `-U, --context <N>` | `3` | Number of unchanged lines shown around changes. |
| `-c, --cumulative` | off | Compare each update with the working tree as it was when `gitplay` started. |
| `--no-color` | off | Disable ANSI color styling. |
| `--once` | off | Scan once and exit. This reports changes detected between the initial and follow-up scans. |

Run `gitplay --help` for the complete command reference.

## Notes

`gitplay` reads files in the selected working tree to display their diffs.
Diff contents are written to the terminal; the program does not send them to
an external service. Avoid running it in a terminal whose output is being
recorded if the repository contains sensitive code or data.

The cumulative view uses the working tree snapshot captured at startup as its
baseline. It does not compare against Git `HEAD`.

Commit reporting watches the `HEAD` file in the git directory and runs
`git log` locally when it moves; a single HEAD move prints at most 20 commits
(older ones are summarized), so rebases and branch switches stay readable.
Checkouts and detachments get a one-line note (`switched to branch …`,
`detached HEAD at …`), while resets on the same branch stay silent. Linked
worktrees and packed refs are resolved without spawning `git`.

## Contributing

Bug reports and pull requests are welcome. Please include a clear description
of the change and run the Rust formatting, test, and lint checks before
submitting:

```sh
cargo fmt --check
cargo test --locked
cargo clippy --all-targets -- -D warnings
```

See [PLAN.md](PLAN.md) for the project's design notes and roadmap.

## License

Licensed under the [MIT License](LICENSE).
