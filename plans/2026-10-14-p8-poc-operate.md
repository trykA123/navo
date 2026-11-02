# P8 · PoC 2: Operate

Status: running (2026-10-15) · Branch: `p8-operate` · After: G7 (passed)

## Goal

**PoC 2.** Navo changes files safely: F5 copy, F6 move, F8 to the Recycle Bin and Shift+Del with a warning, each planned first, run in the background with progress and cancel, and recorded in a journal that survives a crash.

## Decisions

- D7 (Recycle Bin, Shift+Del warning), D22 (copy engine; this plan ships Standard and Auto), architecture "Safety" and "Copy engine".
- Plans are pure data built in `navo-core`; the job engine in `navo-fs` executes them through providers.
- Copies go to a temporary name, are verified by size, then renamed. A same-volume move is a rename.
- The journal lives in `navo.db` tables `job` and `job_step`, one committed row per step.

## Components

| Component | Layer | Contract | Test |
| --- | --- | --- | --- |
| `op_plan` | lib (core) | sources, target, conflict policy → `OperationPlan` (steps, totals, conflicts, risk flags) | unit + proptest: totals match steps; protected locations flagged |
| `job_journal` | adapter (store) | append step, mark done, list unfinished | unit + crash fixture |
| `job_engine` | service (fs) | run a plan on the I/O queue; progress events; cancel; resume from the journal | integration on `fake_fs` with faults |
| `copy_standard` | adapter (fs) | one file: `CopyFile2` on Windows, `std::fs::copy` with temp name on Linux | integration; Windows job |
| `copy_auto` | service (fs) | parallel by drive type, unbuffered above the threshold | integration; benchmark |
| `recycle` | adapter (shell) | `IFileOperation` with recycle flags; Linux `trash` crate for development | integration on Windows CI |
| `confirm_dialog`, `job_row` | ui | permanent-delete warning (count, size, first paths, Enter not default); progress row | headless |
| `op_wiring` | app | F5, F6, F8, Shift+Del → commands → plan → confirm → job | end-to-end on `fake_fs` |

## Atoms

| Atom | Component | Tier | Role | Effort | After | Check |
| --- | --- | --- | --- | --- | --- | --- |
| O1 | `op_plan` Red | T2 | checker | high | G7 | tests compile, fail |
| O2 | `op_plan` Green | T4 | orchestrator | high | O1 | `cargo nextest run -p navo-core op_plan` |
| O3 | `job_journal` Red | T2 | checker | high | G7 | tests compile, fail |
| O4 | `job_journal` Green | T4 | orchestrator | high | O3 | `… journal` |
| O5 | `job_engine` Red | T2 | checker | high | O2, O4 | tests compile, fail (faults: disk full, locked, network lost, crash mid-copy) |
| O6 | `job_engine` Green | T4 | orchestrator | high | O5 | `… job_engine`; no partial target after any fault |
| O7 | `copy_standard` | T4 | orchestrator | high | O6 | integration on Linux and Windows CI |
| O8 | `copy_auto` | T3 | api-builder | xhigh | O7 | integration + first benchmark numbers |
| O9 | `recycle` | T3 | api-builder | xhigh | O6 | Windows CI: file lands in the Recycle Bin |
| O10 | `confirm_dialog`, `job_row` | T2 | ui-builder | high | G7 | headless tests pass |
| O11 | `op_wiring` | T3 | api-builder | xhigh | O8, O9, O10 | end-to-end tests pass |
| O12 | PoC check on Windows | — | orchestrator | — | O11 | owner copies, moves and recycles on a Windows PC |
| G8 | gate | — | orchestrator + reviewer | high | O1–O12 | gate, CI on both OS, review, **PoC 2 demo** |

O1–O2, O3–O4 and O10 run in parallel.

## Done when

- A copy killed halfway leaves no partial file at the target and resumes or rolls back on the next start.
- Shift+Del shows the warning and nothing is deleted when it is dismissed.
- The owner copies, moves and recycles files on a Windows PC with progress and cancel.

## Log

- 2026-10-15 — P8 starts after G7 and the owner's PoC 1 check. `Location` gains serde through its text form (plans are stored as postcard bytes). O1 (`op_plan` Red), O3 (`job_journal` Red) and O10 (`confirm_dialog`, `job_row`) run in parallel on Opus medium. Decisions for the PoC:
  - `copy_standard` uses `std::fs::copy`, which is `CopyFileExW` on Windows (attributes and alternate streams kept), so `navo-fs` stays free of `unsafe`; `CopyFile2` with per-file progress comes with Auto mode.
  - `recycle` uses the `trash` crate (`IFileOperation` on Windows) instead of hand-written COM.
- 2026-10-15 — O5–O7, O9, O10 and O11 are done on `p8-operate` (O8 `copy_auto` deferred). O11 wires F5/F6/F8/Del/Shift+Del to `Browser::prepare` → `ConfirmDialog` → a job thread with progress and cancel; the app journals every step in `navo.db` and rolls back unfinished jobs at start.
- 2026-10-15 — G8 review round 1: **FAIL**. Fixed with tests:
  - Critical: a cross-volume move onto an existing name (or a target that appears after planning) deleted the source. `Ask` now emits no step, and the engine never removes a source whose own copy did not finish in this job.
  - A second Navo process rolled back the first one's running job. Only the holder of `navo.lock` runs recovery.
  - A same-volume move with a conflict failed the job instead of skipping the item.
  - Closing the window mid-job killed it and the next start rolled it back silently. Close now cancels and waits (10 s) so the job ends `Cancelled`.
  - A copy into a protected folder asked for the typed word; only Recycle/Delete do now. A panic in the job thread now ends the job `Failed`.
  - Deferred, blockers before overwrite is offered in the UI: (5) atomic replace in `ProviderOps` (`MoveFileEx` replace) and rollback skipping overwrite steps; (6) a per-job token in the temp name, failing instead of deleting an existing `.navo-part`. Check in O12: F8 on a drive without a Recycle Bin (network share, USB) must not delete permanently.
- 2026-10-15 — G8 round 2: PASS WITH FIXES, both fixed with tests. Every instance now holds `navo.lock` shared for its whole life and takes it exclusively only to recover, so a later instance never rolls back a live one's job. Only a protected copy/move *target* is exempt from the typed word; a protected source still needs it. Owner request: a left double-click opens a row, and Enter or double-click opens a file with the system handler. Deferred: closing mid-job blocks the UI thread up to 10 s with no "Stopping jobs…" feedback.
