# P35 · Quarantine a damaged navo.db after a crash

Status: storage, startup and notice complete on Linux; native Windows pending · Repo: the backend worktree · Branch: `backend/alpha-validation` · Started: 2026-10-20

## Goal

When the integrity check after a crash finds `navo.db` damaged, Navo sets the file aside with its WAL, starts on a fresh database, carries over every table that is still sound, rolls back the crashed run's jobs only from a journal that passed its own check, and tells the user what happened. The former interim sentinel only blocked recovery and left the session unjournaled until the user acted.

## Decisions

From the Opus review of `store-safety` (2026-10-20). Final.

- D1 — **One check, before anything opens.** `crash::check_store(data, unclean)` replaces `note_previous_exit` in `main.rs` and runs before `gui::open`, so no SQLite connection exists yet (Windows opens without `FILE_SHARE_DELETE`; on Unix a rename under open connections splits the db from its `-wal`). It calls `Db::open(path, true)` once. Corrupt means `DbError::Corrupt`, `DbError::Sqlite` with `DatabaseCorrupt`, or `NotADatabase`; busy, I/O and TooNew degrade as today, without quarantine. A pass clears the unclean flag so the four later opens skip the check, and removes the `check-failed` sentinel.
- D2 — **Finish an interrupted quarantine first.** If `navo.db` is missing but `-wal` or `-shm` exists, move them into the newest `corrupt-N` slot whose `-wal` name is free, so SQLite never replays an orphan WAL into a new database.
- D3 — **Names.** `navo.db.corrupt-N`, `navo.db.corrupt-N-wal`, `navo.db.corrupt-N-shm`; N is the lowest number for which none of the three exists. `sqlite3 navo.db.corrupt-N` then opens the copy with its WAL.
- D4 — **Order and no-replace.** Rename the db first, then `-wal`, then `-shm`, never replacing an existing file (hard link plus remove, or `MoveFileExW` without `REPLACE_EXISTING`).
- D5 — **Salvage, read only.** Open the quarantined copy with no checkpoint on close. Per table group run `PRAGMA integrity_check(<table>)`:
  - journal (`job`, `job_step`, `undo`) as one unit: copy only if all three pass and every unfinished or restore-pending job's plan JSON parses; otherwise copy none and build a summary (job kind, time, source and target when readable, held paths from readable `undo` rows);
  - `tags`, `module_kv`, `settings_defaults`: user data, copy if they pass (a missing ledger is safe: `follow()` treats keys without a record as user-set);
  - `history`, `frecency`, `search_history`, `viewer_memory`: copy if they pass, otherwise drop.
- D6 — **Fresh database, then normal recovery.** Create the fresh db with `Db::open(path, false)`, insert the salvaged rows; normal start-up then runs `jobs::recover` on the healthy db (sole instance only). Nothing is ever rolled back from the damaged file, and never in a later run.
- D7 — **Rename failure.** If the quarantine rename fails (antivirus, backup tool): open nothing this run (all four callers degrade), never call `recover`, keep the `check-failed` sentinel so the next start retries.
- D8 — **Notice.** In the crash-notice area, stays until dismissed and is never hideable (D58), text through `.ftl` (D59): the database was damaged after the crash and set aside as `navo.db.corrupt-N` (with the folder); which interrupted jobs were rolled back or could not be read, with the held-file paths for manual restore; which data was reset (marks, history); an "Open folder" action. Also `tracing::error` with N, the failing tables and the salvaged counts.

## Components

| Component | Layer | Contract (in → out) | New / change | Test |
| --- | --- | --- | --- | --- |
| `crash::check_store` | app | `data dir, unclean → StoreState` | new | `crash/tests` |
| `store::quarantine` | store | `db path → QuarantineReport \| Error` | new | `navo-store` db tests |
| `store::salvage` | store | `quarantined copy, fresh Db → SalvageReport` | new | `navo-store` db tests |
| crash notice | ui | `SalvageReport → notice rows` | change | `crash_notice` tests |

## Atoms

| Atom | Component | Role | Effort | After | Check | Status | Cost |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Q1 | `store::quarantine`, `store::salvage`, `crash::check_store` | main agent; Sol 6.1 xhigh review | high | `store-safety` merged | tests below; gate | done (Linux gate and review) | |
| Q2 | crash notice | main agent; Sol 6.1 xhigh review | high | Q1 | notice test; gallery shot | done (Linux gate, native screenshots and review) | |

### Q1 — tests

Reuse `corrupt_middle_page` from `db/tests/guards.rs`:
- the quarantined copy keeps the original bytes and the fresh db opens;
- a journal that passes its check rolls the job back on the fake file system;
- a damaged journal copies no rows, gives a non-empty summary and rolls nothing back;
- an orphan `-wal` is moved aside and the new db does not contain its table;
- an existing `corrupt-1` leads to `corrupt-2`, never a replace;
- Windows only: with a second handle open the quarantine fails, the sentinel is written and recovery is skipped.

## Done when

- A crash plus a damaged `navo.db` leads, on the next start, to a fresh journaled db, the damaged copy kept under `corrupt-N`, sound data carried over, and a notice naming what was reset.
- The gate, Windows CI and the owner's requested Sol 6.1 xhigh review are green.

## Log

- 2026-10-20 — Plan written from the Opus review of `store-safety`; that branch carries the interim `check-failed` sentinel (no recovery while it exists).
- 2026-10-23 — Owner requested continued backend implementation. Storage work is split into [quarantine and salvage backend](2026-10-23-p35-recovery-backend.md), with startup activation and the persistent notice following separately. A durable slot record strengthens D2: an unrecorded orphan WAL is isolated in a separate empty slot rather than attached to an unrelated older database. The current schema also requires preserving `command_use` and `rename_preset`. Main-agent implementation; Sol 6.1 xhigh reviews only.
- 2026-10-23 — Storage APIs and 28 Linux regressions are complete; the full gate passed all 10 steps with 7,088 tests passed, 14 skipped, plus documentation tests (188.0 seconds). Review found a non-STRICT SQLite field-type issue; the regression failed first and the fix passed review. Startup activation and the persistent notice are not implemented yet; native Windows sharing remains pending. See [storage recovery contracts](../docs/storage-recovery.md).
- 2026-10-24 — Implemented startup activation and the persistent notice in the [startup integration plan](2026-10-24-p35-startup-notice.md). The main agent implemented all changes. The single Sol 6.1 xhigh reviewer confirmed both notice fixes and no unresolved findings. All 141 targeted tests and four inspected native light/dark screenshots pass; final Linux gate is running. Native Windows runtime verification remains pending.
- 2026-10-24 — Final integration gate passed all 10 steps in 209.0 seconds: 7,117 tests passed, 14 skipped, plus documentation tests. Two final-binary native runs also preserved the exact damaged source, opened a healthy replacement and retained the notice across a clean restart. Inspected both real app themes. No unresolved Sol 6.1 xhigh findings. Native Windows runtime checks remain the outstanding P35 verification boundary.
