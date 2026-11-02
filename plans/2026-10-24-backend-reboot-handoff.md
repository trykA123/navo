# Backend reboot handoff

Saved: 2026-10-24 18:57 Europe/Bucharest.

Historical reboot checkpoint. Resumed work is completed and documented in [2026-10-24-d65-delete-jobs.md](2026-10-24-d65-delete-jobs.md); use [the review index](2026-10-25-backend-review-index.md) for the current branch scope.
Status: stopped at the user's reboot checkpoint. No build, test or shell session was started in the current continuation. No source changes were made after `80bf7d17`; this handoff is the only new file.

## Resume here

- Worktree: the backend worktree.
- Branch: `backend/alpha-validation`.
- HEAD: `80bf7d17` — Implement durable retention backend and safe restore.
- Check `git status` and `git worktree list` before resuming. Do not edit the main checkout the main worktree, the original backend checkout the backend worktree, or sibling worktrees.
- User wants continued backend implementation on this branch. They will ask Claude to review the accumulated work on Monday. Do not start a Claude review now.
- User requires the main agent to implement everything. A separate `gpt-6.1-sol` with `xhigh` is allowed for review only; `gpt-6-luna` with `high` is allowed for research. No implementation delegation.
- No merge, push or deployment was performed or is needed for this checkpoint.

## Completed and committed

| Commit | Result |
| --- | --- |
| `e3fbd8a5` | Windows rename-log locking correction. |
| `a27b813b` | Database quarantine and verified salvage backend. |
| `9db52dab` | Resumable P35 startup recovery and persistent recovery notices. |
| `80bf7d17` | D65 retention records, policy, persistent catalog and capture/restore/purge engine; safe interruption reconciliation and P35 compatibility. |

The detailed D65 implementation and review log is in [2026-10-24-d65-retention-backend.md](2026-10-24-d65-retention-backend.md). P35 startup work is documented in [2026-10-24-p35-startup-notice.md](2026-10-24-p35-startup-notice.md).

The last completed Linux gate belongs to `80bf7d17`: all 10 steps passed in 147.4 seconds, with 7,170 tests passed and 14 skipped, plus documentation tests. Sol 6.1 xhigh review findings were resolved. Native temporary-profile recovery was exercised in both themes and screenshots inspected. Native Windows runtime verification remains outstanding. These checks are historical; no new checks ran during the current design-only continuation. Logs and screenshots under the local evidence folder may not survive reboot; the tracked plan records their results.

## Current continuation: researched, not implemented

The next stage is normal-job activation of retention. The user was told work is proceeding on durable per-job policy, overwrite journal ordering and profile mutation serialization. No implementation or new activation plan has been written yet. Do not report these as completed.

A bounded next implementation should connect supported delete jobs first, with persisted policy and startup rollback, then connect overwrite retention. Keep each stage complete and checked. Do not spend another continuation only expanding the design.

Open work:

1. Define validated backend retention settings and defaults. Proposed explicit opt-in until restore UI exists; exact configuration remains unimplemented.
2. Persist the policy with each job before any filesystem mutation. Choose one representation before coding: a typed versioned job-plan envelope with a distinct job kind can reuse the current journal transaction; a new table would also require schema migration and atomic P35 salvage changes.
3. Serialize mutations in a profile across ordinary jobs, rename undo, recovery and maintenance. Keep parallel file-copy workers within a single job. Lock acquisition must report failure or support cancellation; never silently proceed without the lease.
4. Retain each selected delete root as a whole tree. Existing delete plans expand into per-node remove steps, so retaining individual descendants would leave private containers inside their parent. Never fall back to permanent removal when retention fails.
5. Add narrowly owner-authorized rollback while a job is still pinned. Existing public restore correctly refuses active-job entries. Do not mark a job terminal merely to bypass pinning.
6. Restore retained delete entries before treating rollback as complete. Persist ambiguous, unavailable-volume or changed-target cases as restore-pending and include readable original/held paths.
7. Adopt A8 overwrite backups before completing their owner job; current job completion releases and deletes them. Adoption and job completion need durable ordering. Preserve legacy undo evidence if any persistence step fails.
8. Add maintenance with fresh observed free space, bounded worker execution and terminal metadata pruning. Recently deleted and general user undo UI follow separately.

## Relevant contracts and pitfalls

- `crates/navo-retention/src/lib.rs`: `Vault::new(Db, profile, ops)` owns a read/write `retention.lock` for its lifetime. Capture accepts `CaptureRequest { source, policy, created_at, job }`. Restore, purge and reconciliation already have cancellation/progress contracts.
- `crates/navo-core/src/retention/`: validated V1 records require a sibling `.navo-retained-*` container and fixed `payload` child. Do not change held paths without versioning and recovery compatibility. Policy fields are private and currently have no serde implementation or getters.
- `crates/navo-store/src/retention/transaction.rs`: live job ownership pins restore and purge. Any authorized owner-rollback extension must also enforce the owner in the store transaction, not only in the service.
- `crates/navo-store/src/recovery/salvage/journal.rs`: unfinished plans are currently decoded as `OperationPlan`. A new job envelope or policy table must also be validated here. Rejected records must keep manual recovery paths.
- `crates/navo-app/src/jobs/mod.rs`: `log_factory` creates the durable job; `JournalLog::finish` currently returns nothing and releases A8 backups. `undo_log` marks rename undo restore-pending. `Recovery` and `roll_back_job` implement legacy startup rollback.
- `crates/navo-app/src/jobs/held.rs`: A8 records preserve original/held paths and stamps, but do not yet persist stable old/new object identities for retention adoption.
- `crates/navo-app/src/jobs/recovery.rs` and `pending.rs`: recovery reports need retained-job support and readable retained paths.
- `crates/navo-ui/src/browser/operate/{types,work}.rs`: `OpLog` currently exposes only `finish`; normal execution calls `run_job_parallel`. Backend hooks can be added here without changing rendering.
- `crates/navo-app/src/gui.rs`: `operations` currently falls back to unlogged operations when the journal cannot open. An enabled retention policy must fail closed instead. Protect private backup paths even when capture is disabled.
- `crates/navo-app/src/rename_hooks.rs`: profile-aware rename undo must share the mutation lease.
- `crates/navo-app/src/crash/store/workflow.rs`: sole-session startup recovery runs before normal DB access is allowed. A retention DB connection here must use the authorized raw `Db::open` path; runtime uses guarded `crash::open_db`.
- `crates/navo-store/src/settings/`: settings completion derives keys from serialized defaults. Update defaults snapshot, property-test constructors, parsing and save/reload tests when adding a section.
- `crates/navo-fs/src/provider/cancel.rs`: cancellation already has a condition variable; use it if implementing cancellable lock waits.
- Generic filesystem `rename` may have replacing fallbacks. Retention must use `rename_atomic`; unsupported providers must refuse. Mount and stable file identities are required. Never traverse links or bind mounts.
- Existing retained containers anchor ownership to the original source parent. Moving a parent that contains live backups needs an explicit policy; current engine refuses nested retained containers. Do not relocate or delete these implicitly.

## Verification after implementation

Read the shared agent coding rules before editing. Use temporary databases and trees only. Do not relax lint or size limits. Add behavioral regressions for cancellation, restart, journal-write failure, occupied/replaced targets, unsupported providers and live-owner pins.

Run targeted tests first, then `CARGO_BUILD_JOBS=8 nice -n 10 cargo xtask gate`. Avoid competing Cargo jobs. Keep Romanian progress updates under one minute apart and report what is actually running. Review with the permitted Sol agent only after a concrete diff exists. Stage explicit paths and commit locally only after the gate passes.
