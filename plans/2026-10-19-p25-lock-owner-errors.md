# P25 · Lock owner in errors

Status: done (Windows CI and the owner's check pending) · Repo: `<repo>` · Branch: `p25-l1` · Started: 2026-10-19

## Goal

When a copy, move or delete fails because a file is open in another program, the error names that program ("“report.docx” is in use by Microsoft Word.") instead of "another program". Roadmap Alpha item "lock owner in errors"; owner alpha priority. The side sheet chosen for later (prototype lock-owner, direction B + D strip) builds on this data and is not part of this plan.

## Decisions

Final. Workers never reopen them; a worker that finds one wrong stops and reports.

- **D1 — Lookup.** Windows Restart Manager: `RmStartSession`, `RmRegisterResources` (the one file), `RmGetList`, `RmEndSession` (always, also on error). windows-sys 0.61.2 with `Win32_System_RestartManager`. The code lives in navo-fs as `local/win_lock.rs`, with a module-level `#![allow(unsafe_code)]` like `local/win_watch.rs`, and an `// SAFETY:` line on each unsafe block. Other systems return no holders.
- **D2 — When.** Only in `local::io_error`, only when the kind is `SharingViolation` or `Locked`, and only for the failing file. The lookup runs on the job's thread, never on the UI thread. A failed lookup is silent: the error keeps today's text.
- **D3 — Data.** `FsError` gains `holders: Vec<LockHolder>` with `LockHolder { name: String, pid: u32 }` in navo-core, empty by default. Names come from `strAppName`; when it is empty, use the executable file name from the process (`QueryFullProcessImageNameW`, file name only); when both fail, skip that process. Duplicate names are listed once, in the order Restart Manager returns them. Navo's own process is left out.
- **D4 — Text.** With location: one holder "“{place}” is in use by {a}."; two "… by {a} and {b}."; three "… by {a}, {b} and 1 more program."; four or more "… by {a}, {b} and {n} more programs." Without location the same, starting "The item is in use by …". `Locked` uses the same text as `SharingViolation` when holders are known. Recovery actions do not change.
- **D5 — Diagnostics.** Holder names do not go into the diagnostics report (navo-diag keeps its own fields).

## Atoms

| Atom | What | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| L1 | D1–D5 in navo-core and navo-fs, every `FsError` literal updated | api-builder (Codex sol xhigh), then reviewer (Opus medium) | xhigh | — | message unit tests for D4; a Windows integration test that holds a file open from a child process and expects that program's name; gate | done |

## Done when

- On Windows, deleting a file open in another program shows that program's name; checked by the owner on the next alpha.
- Unit tests and the Windows integration test pass in CI; the gate passes.

## Log

- 2026-10-19 — Plan written. Today `SharingViolation` reads "is in use by another program." (`navo-core/src/fs_error/mod.rs`); `local::io_error` is the one place OS errors become `FsError`.
- 2026-10-19 — L1 merged: Codex sol xhigh; Opus review PASS WITH FIXES (the Windows test now waits for the child to print "locked" instead of probing the file; D4 wording fixed to "1 more program"). Listings never trigger a lookup (FindFirstFileEx opens no files). Restart Manager gets the verbatim `\\?\` path; CI proves the drive case, UNC shares stay unproven.
