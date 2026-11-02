# Rename undo identity checks

Status: done (Linux gate; Windows checks pending) · Repo: `navo-backend` · Branch: `backend/rename-undo-safety` · Started: 2026-10-23

## Goal

Undo and crash recovery must leave replacement files alone, even when their names, sizes and timestamps match a renamed file. Record each source's identity before renaming it and keep that record across restarts.

## Decisions

- D1 — Work only in this worktree. Do not merge, push or deploy.
- D2 — This completes the file identity portion of P41 E7. Rename logs, single-file rename routing and other backend features remain separate work.
- D3 — Local identity uses device/inode on Unix and volume serial plus the full 128-bit file identifier on Windows. Inspect links themselves, without following them.
- D4 — Store identities in existing journal step details before filesystem changes. No database migration or new dependency.
- D5 — Undo skips a replaced file or a legacy step without identity. Unreadable or unavailable identities stop recovery and keep the batch retryable. An older unverified recovery stays pending when the original name is missing and the new name exists. Missing destinations and occupied original names retain their existing behavior.
- D6 — Keep swaps, cycles, case-only renames, cancellation rollback and interrupted undo working. An unavailable identity must not be treated as a match.
- D7 — Refuse a new rename step when its provider cannot read an identity, and roll back earlier steps. Keep rename plans ordered even through the parallel job entry point.
- D8 — Owner update: use Codex only, Sol 6.1 high or xhigh for any worker runs. Do not use Claude agents.

## Components

| Component | Layer | Contract (in → out) | New / change | Test |
| --- | --- | --- | --- | --- |
| Provider identity | adapter | location → optional file identity | change local and fake providers | provider identity tests |
| Rename journal | adapter | step and identity → durable pending detail | change journal log and recovery log | app journal restart and failure tests |
| Rename undo | service | recorded and current identities → rename or skip | change runner and undo checks | replacement, swap and recovery regressions |

## Atoms

| Atom | Component | Role | Effort | After | Check | Status | Cost |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A1a | Provider contract and exports | self | high | — | provider identity tests | done | |
| A1b | Local provider identity | self | high | A1a | six identity regressions, including real temporary files | done (Windows runtime pending) | |
| A1c | Fake provider identity | self | high | A1a | replacement and fault regressions | done | |
| A2 | Rename journal | self | high | A1a | three journal identity regressions | done | |
| A3a | Rename runner and rollback | self | high | A1b, A1c, A2 | rename and recovery regressions | done | |
| A3b | Undo service and test journal | self | high | A3a | browser rename tests | done | |
| A3c | Window test journal and provider | self | high | A3b | headless undo tests | done | |
| A3d | Parallel entry point | self | high | A3a | parallel rename identity regression | done | |
| A4 | Documentation and gate | self | high | A3d | `cargo xtask gate`; inspect diff and worktree status | done (Linux) | |

## Done when

- A replacement file with matching size and timestamp remains at its new name during undo and recovery.
- Identity checks survive closing and reopening the journal.
- Swaps, cycles, case-only renames and interrupted undo retain their behavior.
- A failed identity journal write prevents its rename.
- Available checks pass; Windows runtime limitations are recorded.
- The changes remain on this branch, without a merge, push or deployment.

## Verification limits

Native Windows compilation and runtime checks remain pending. The identity check revalidates paths before mutation; it does not make external filesystem changes atomic with the rename. Legacy recovery that cannot verify an already-renamed file remains pending for manual attention.

## Log

- 2026-10-23 — Created the isolated worktree from `ce5af5c9`. Confirmed that undo currently checks path existence only and journal step details can hold identity records.
- 2026-10-23 — Six provider regressions failed against the default implementation, then passed. Both replacement-file regressions reproduced the bug before undo/recovery changes. Three journal identity tests also failed before persistence was implemented.
- 2026-10-23 — Targeted suite: 303 passed. Covers identity persistence, replaced files, in-place edits, legacy records, failed journal writes, failed identity reads, swaps, cancellation rollback and interrupted undo. The failed-write test initially required successful rollback logging from an entirely unwritable journal; corrected it to assert failure and zero filesystem renames.
- 2026-10-23 — Source review found the parallel public entry point could bypass identity preparation. Added a dedicated regression before fixing that dispatch. Split the provider and undo work into smaller atoms in the table.
- 2026-10-23 — Windows API reference: [FILE_ID_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_info), using the full identifier and volume serial. This Linux host has no Windows Rust standard library, so native Windows compilation and runtime checks remain pending.
- 2026-10-23 — Gate found the extended sequential runner above the 50-line limit and a test fixture above the argument limit; extracted the identity guard and grouped fixture arguments. Applied the existing test-helper unwrap convention to the temporary-path helper only.
- 2026-10-23 — Two additional regressions failed before the fix: unavailable current identity must keep undo retryable; an unverified legacy recovery must not silently become rolled back. Both now preserve a pending state without moving files.
- 2026-10-23 — Split journal identity regressions and lost-write fixtures into their own test modules to retain the repository's 400-line limit. No size exceptions added. Formatting, clippy, TOML, spelling, size, localization, dependency checks and audit passed; the full workspace test run is in progress.
- 2026-10-23 — Full Linux gate passed: all 10 steps, 6,959 tests passed and 14 skipped, plus documentation tests. Runtime 185.9 seconds. The replacement, legacy recovery, unavailable identity and parallel entry regressions passed in the workspace run. Main remains at `ce5af5c9`; no Claude agents, merge, push or deployment used.
