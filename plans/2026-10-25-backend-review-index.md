# Backend review index

Branch: `backend/alpha-validation`.
Worktree: the backend worktree.
Review range: `1d59fb75..backend/alpha-validation`.

## Completed scope

| Stage | Reference | Scope |
| --- | --- | --- |
| Windows rename logging | `e3fbd8a5` | Read/write lock handles and rename-log safety correction. |
| P35 quarantine and salvage | `a27b813b` | Preserve damaged databases and validate salvaged table groups. |
| P35 startup integration | `9db52dab` | Resume recovery phases and persist notices/manual recovery paths. |
| D65 engine | `80bf7d17` | Durable catalog, policy, same-volume capture, safe restore/purge and reconciliation. |
| D65 normal delete activation | `43a6b21e` | Opt-in ordinary delete jobs, durable owner intents, startup rollback, shared mutation coordination and private-path protection. |
| D65 overwrite activation | `24acde8e` | Independent Copy/Move opt-in, proved A8 adoption, atomic completion and pinned interrupted-write recovery. |
| D65 bounded maintenance | Current branch tip | Fresh-space checks, compact inventory, bounded job completion cleanup and durable terminal-holder retries. |

Detailed plans:

- [P35 startup recovery](2026-10-24-p35-startup-notice.md).
- [D65 engine and earlier review](2026-10-24-d65-retention-backend.md).
- [D65 delete activation and final checks](2026-10-24-d65-delete-jobs.md).
- [D65 overwrite activation and final checks](2026-10-25-d65-overwrites.md).
- [D65 bounded maintenance](2026-10-25-d65-maintenance.md).

## Contracts to review

- `navo-core::retention`: bounded records, validated policy and versioned retained-delete/write plans.
- `navo-store::{RetentionCatalog, RetentionTransaction}`: durable states, active-owner pins and narrowly authorized rollback.
- `navo-retention::Vault`: native atomic no-replace capture, identity/mount verification, collision-safe restore and purge reconciliation.
- `navo-app::jobs::{deletion, overwrite, held, profile, mutation_lock, recovery}`: root intents before capture, owner rollback before completion, cross-profile coordination and refusal of ambiguous recovery.
- `navo-fs::{mutation_location, save_location, check_mutation}`: private-path protection, alias-aware save destinations and whole-tree relocation checks.
- App/editor and UI worker adapters: journal/lease acquisition, creation and save coverage, cancellation while waiting, finish-error propagation and deferred undo state changes.
- P35 salvage/reporting: rejected typed payloads preserve evidence and readable manual source/container paths.

## Latest verification

The latest Linux gate passed all 10 steps in 183.3 seconds: 7,240 tests passed, 14 skipped, plus documentation tests. Native temporary-tree maintenance verified expiry cleanup without changing new user bytes. Sol 6.1 xhigh completed read-only review without remaining concrete maintenance findings.

The last native UI smoke was run on `24acde8e`: light/dark at 1280×720 and 640×480, with four screenshots inspected and fixture bytes preserved. Maintenance changes no UI code. All data fixtures were temporary.

The owner's Opus review of `24acde8e` is stored at `8c081606:docs/reviews/2026-10-25-alpha-validation-opus.md`. Its six major/nine minor findings are the next task. A clear maintenance review does not close those findings.

## Deliberately unfinished

- Retained deletes default to off. `[retention] retain-deletes = true` enables this backend stage; there is no user-facing restore flow yet.
- Retained overwrites default to off; `[retention] retain-overwrites = true` enables Copy/Move adoption. Interrupted writes retain both versions and their evidence as pinned restore-pending entries. General automatic Copy/Move rollback remains open.
- Bounded expiry/quota/free-space maintenance runs after retained jobs. Periodic idle scheduling and acknowledged terminal metadata pruning are not activated.
- Recently deleted and general user undo remain open. Existing rename undo stays available.
- Native Windows runtime verification remains open; Linux/FakeFs checks do not substitute for it.

All changes remain local to the feature branch. No merge, push or deployment was performed.
