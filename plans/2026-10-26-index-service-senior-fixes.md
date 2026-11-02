# Windows index service senior-review fixes

Status: done · Repo: index fixes worktree · Branch: `backend/index-service-fixes` · Base: `2ebc69ef`

## Goal

Make service removal and interrupted installation recoverable. Refresh search coverage immediately after service loss and avoid serialized candidate filtering.

## Decisions

- D1 — Owner-authorized six findings follow P40 D22–D27 and C12–C17. Work only on this branch; preserve the beta worktree. No push or merge.
- D2 — Add a failing behavioral regression before every fix. Exercise portable policies and filesystem helpers on isolated temporary data; Windows adapters stay minimal.
- D3 — Compare coverage by root, error-free state and usable index, not progress counters. Retain grace only while the pipe remains reachable; failed queries invalidate it immediately.
- D4 — Initialize an ancestor memo per parallel search worker and normalize exclusions once at construction. Apply filtering before the top-500 cut.
- D5 — Recover exactly one old executable before install cleanup. Refuse ambiguous recovery. Restore service configuration through the existing atomic writer.
- D6 — Serialize Cargo against other machine builds; nice 10 and eight build jobs. Redirect output to local logs without a pipe. Run `cargo xtask check -p navo-app -p navo-index`.
- D7 — Keep public notes free of machine-specific paths. Native Windows compilation/runtime remain unverified on this host.

## Components

| Component | Contract | Check |
| --- | --- | --- |
| removal | absent service and mapped image → retriable cleanup | missing-service/retry probes |
| coverage cache | service availability and coverage → walk roots | reset/failure/progress probes |
| parallel search | worker-local filter → bounded eligible hits | million-candidate normalization counter |
| install rollback | interrupted image/configuration → preserved previous state | sole-backup/partial-config probes |

## Atoms

| Atom | Component | After | Check | Status |
| --- | --- | --- | --- | --- |
| A1 | failing probes | — | every finding fails on baseline | done |
| A2 | removal | A1 | retry and idempotence | done |
| A3 | coverage | A1 | invalidation and coverage equality | done |
| A4 | parallel search | A1 | private memos and million candidates | done |
| A5 | rollback | A1 | image recovery and atomic restore | done |
| A6 | validation | A2–A5 | affected check, review, local commit | done |

## Done when

All six findings have red/green evidence and the requested two-crate check passes. Report file/line references, tests, commit and exact check result. No push or merge.

## Log

- 2026-10-26 — Read P40 decisions and C12–C17. Initial worktree clean at `2ebc69ef`; original beta worktree remains untouched.
- 2026-10-26 — All eight primary probes failed before fixes, covering every finding; the additional planted-image observation probe also failed first. Initial green run passed all 78 focused tests. The million-candidate probe now normalizes its path exclusion once, with filtering still before the 500-hit limit.
- 2026-10-26 — Removal retries missing/mapped image cleanup; preflight restores a unique previous image and refuses ambiguity; settings restore uses the atomic writer. Executable observation runs after trust verification and interrupted-image recovery, while directory tracking remains from before creation.
- 2026-10-26 — Final unpiped `cargo xtask check -p navo-app -p navo-index` passed: `check: all 7 steps passed (9.1 s)`. All 680 tests passed; one skipped. Output stayed in local log files. Every Cargo launch waited on other Cargo/rustc processes, with nice 10 and eight build jobs.
- 2026-10-26 — Sol 6.1 xhigh found a Windows-only function-size issue; extracted unchanged registration rollback and kept recovery/observation ordering in the portable helper. Final static review has no remaining actionable findings. Windows compilation/runtime remain unverified. No push or merge.
- 2026-10-26 — Verification round: reverting each fix made its regression fail, except the reset and failed-query probes, which passed because the real pipe connect failed on this host. Query replies now drop cached coverage without a connect, and both probes use a connected stub; they fail without the fix. `cargo xtask check -p navo-app -p navo-index`: all 7 steps passed.
