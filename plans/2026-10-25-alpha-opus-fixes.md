# Alpha fixes from Opus review

Status: done on Linux; native Windows checks pending · Repo: `navo-alpha-opus-fixes` · Branch: `backend/alpha-opus-fixes` · Base: `e3fbd8a5` · Started: 2026-10-25

## Goal

Prepare the rename/drag alpha independently from P35 recovery and D65 retention, as requested by the owner's Opus review in `8c081606`.

## Decisions

- D1 — Main agent implements. Sol 6.1 xhigh reviews only; Luna 6 high may research official API behavior. Keep all branches local. Preserve `backend/alpha-validation` at completed maintenance `c8bd352e` for comparison.
- D2 — M4: fallback to legacy Windows file information on the same non-following handle for unsupported/zero 128-bit IDs. Zero-extend the 64-bit index and preserve a distinct identity domain in the journal. Refuse unconfirmed identities with a clear drive-specific message. Native exFAT and SMB execution remains pending on this Linux host.
- D3 — m8/n3: log the applied part of Stuck batches under an incomplete header. Keep a format-version header and rotate at a size cap under a separate stable lock, preserving unknown files on collision.
- D4 — m9: an internal FileDrag reentering the app clears native/outgoing restrictions. Real external drags continue using platform modifiers and effects.
- D5 — After alpha verification, create a separate beta branch by replaying the P35/D65 commits onto the fixed alpha. Resolve remaining Opus findings there. Do not merge to main, push or deploy.

## Atoms

| Atom | Component | Check | Status |
| --- | --- | --- | --- |
| A1 | Internal drag reentry | Copy-only native platform must permit internal Move on return | done |
| A2 | Windows identity fallback | domain/serialization tests, unavailable-ID message, native test harness | implemented; Windows runtime pending |
| A3 | Rename audit log | incomplete batches, rotation, version header and concurrency | done |
| A4 | Gate and review | Linux gate; native checks with limits recorded | done on Linux |

## Log

- 2026-10-25 — Created isolated alpha worktree from the commit preceding P35/D65. Microsoft documentation confirms FAT IDs may change on rename and exFAT guarantees require native testing. Keep the fallback domain distinct and avoid universal Windows claims.

- 2026-10-25 — Drag and incomplete-log probes failed before fixes. Targeted checks passed: 48 app rename tests and 163 UI rename tests (one ignored). Sol review requested proof of an uncertain applied rename, strict backup placement, and protection against unstable FAT indices. Added red probes for uncertain rename, unconfirmed rollback and replaced-member swap undo; their results are pending.

- 2026-10-25 — All three new boundary probes reproduced failures: missing unsure step in audit, false PutBack after identity loss, and replaced-member swap Undo marked Undone. Fixed with target identity confirmation for unsure audit, source/target proof for rollback completion, and dependency-aware Undo collisions. Windows legacy fallback now refuses known unstable FAT families. Log backup placement uses the provider's no-replace rename and propagates metadata errors.

- 2026-10-25 — Rotation now uses a backported strict atomic rename primitive (native Linux NOREPLACE or Windows non-replacing MoveFileEx), refusing unsupported backends. It never enters the legacy Unix check/rename fallback. The backup collision and raw-profile tests cover the native path; an unsupported-result regression covers refusal.

- 2026-10-25 — The first full gate passed 7,073 tests and failed one old assertion that a replaced rename target closes recovery. Updated the regression to require RestorePending, retained identity proof and idempotent restart, matching the stronger recovery contract. Repeat the full gate before commit.

- 2026-10-25 — Final full gate passed all 10 steps in 133.3 seconds: 7,074 tests passed, 14 skipped, plus documentation tests. Final binary built in 31.14 seconds. Four isolated native light/dark startup runs passed at 1280×720 and 640×480; screenshots inspected and fixture bytes preserved. Evidence: a local verification log, a local verification log, the local evidence folder. Sol 6.1 xhigh read-only review has no remaining concrete alpha correctness findings. Windows exFAT/SMB and a real OS drag round trip remain unexecuted; toolkit event regressions are verified.
