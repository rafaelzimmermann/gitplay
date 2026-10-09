# gitplay

> 🎬 A live, rolling code-movement visualizer for git repositories.

`gitplay` watches your working tree in real time and streams an animated, rolling diff feed—letting you observe code changes, refactors, and AI coding agents as they craft code.

```bash
$ gitplay
```

## Features

- **Continuous Live Stream**: Immediately catches file writes, moves, and modifications.
- **Rolling Visual Pace**: Rather than dumping diffs instantaneously, lines roll smoothly into view, making code movement visible to the human eye.
- **Delta & Cumulative Diffs**: Toggle between seeing what changed in the *latest write tick* versus total diff against `HEAD`.
- **Gitignore Respecting**: Ignores noise (`target/`, `node_modules/`, binary files) via the `ignore` crate.
- **Zero-CPU Idle**: Uses native inotify waiting.

## Plan & Architecture

See [PLAN.md](PLAN.md) for full product specifications, milestone breakdown, and architectural design.
