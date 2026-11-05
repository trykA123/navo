# Linux watcher: rename records across reads

Status: done · Branch: `fix-linux-watch-rename` · Started: 2026-11-05

## Goal

Keep a rename intact when inotify delivers its two records in separate reads.
Appetite: one focused fix. No Windows changes, release or integration of other parts.

## Decisions

- Keep the existing translator across reads. Wait up to the watcher quiet interval for a missing destination, then report removal.
- Use rustix poll, already a dependency, for bounded waiting. No busy polling or extra threads.
- Force one record per read with a small buffer to reproduce the race deterministically.

## Risks and rollback

Unmatched moves must still report removal. Dropping a watcher must wake its reader.
Tests cover split rename, unmatched moves and shutdown. Revert the fix commit to roll back.

## Components

| Component | Contract | Change | Test |
| --- | --- | --- | --- |
| Linux watcher | inotify records → folder changes | retain pending rename across reads | real inotify, one-record buffer |

## Atoms

| Atom | Component | Check | Status |
| --- | --- | --- | --- |
| A1 | Linux watcher | regression red, fix green, 200 stress repetitions, crate gate | done |

## Done when

Regression fails before the fix, passes afterwards. The navo-fs gate and stress tests pass.

## Log

- 2026-11-05 — Reader recreates and flushes Translate on every read. Added a one-record-buffer reproduction before changing that behavior.

- 2026-11-05 — Regression failed deterministically before the fix: Removed(before) plus Created(after), instead of Renamed(before, after). The test uses real inotify with a 32-byte read buffer.
- 2026-11-05 — Reader retains Translate across reads, polling with a 100 ms timeout only while a source record is pending. An unmatched source emits removal when idle; watcher removal wakes the reader. Queue overflow clears pending state.
- 2026-11-05 — Final validation: `cargo xtask check -p navo-fs` passed all 8 steps, 1,116 tests passed, one skipped. Four watcher tests passed 200 repetitions each (800 executions, 90.39 s). Logs: `/tmp/navo-rename-red.log`, `/tmp/navo-rename-gate.log`, `/tmp/navo-rename-stress-final.log`.
- 2026-11-05 — Fix is isolated on its own branch. The three-part integration, Windows CI and release have not resumed; user requested the watcher fix first. Linux-only platform behavior was exercised on Linux.
