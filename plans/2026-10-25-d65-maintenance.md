# D65 retention maintenance

Status: done on Linux; native Windows verification pending · Repo: the backend worktree · Branch: `backend/alpha-validation` · Started: 2026-10-25

## Goal

Execute expiry, volume quota and low-space cleanup through the verified retention engine. Observe physical free space again after every purge and preserve pinned or ambiguous payloads.

## Decisions

- D1 — Continue locally after `24acde8e`. Main agent implements; Sol 6.1 xhigh reviews only. No merge, push or deployment.
- D2 — Read validated catalog records as compact inventory summaries. Bound a pass to 100,000 catalog entries and 256 purges; reject incomplete inventory before mutation. Keep existing policy ordering and quota accounting.
- D3 — Add a provider free-space query, unsupported by default. Local Linux/Windows query available bytes from the OS. Verify the observed directory identity and mount before and after each query. Never estimate physical recovery from logical sizes.
- D4 — Recompute policy selection and observe free space after every purge. Resume unpinned Purging entries first. Missing volumes, failed observations, changed contents and cancellation stop the pass. Durable purge states preserve retry evidence.
- D5 — Expose an explicit worker service, then invoke bounded maintenance after retained-job completion while its existing mutation leases remain held. Maintenance failure does not turn a completed user operation into failure. Retention remains disabled by default. Periodic idle scheduling and acknowledged terminal metadata pruning remain separate.
- D7 — Add schema V7 indexed source/container projections and a durable terminal cleanup marker. A terminal phase remains pending until its owned empty container is absent; shared occupied holders stay pending. Migrate V6 rows conservatively and preserve P35 compatibility.
- D6 — Inventory includes all live entries on the observed volume, including pinned bytes and both retention kinds. Current validated settings supply quota/free-space limits; each entry keeps its original expiry timestamp.

## Components

| Component | Layer | Contract | Check |
| --- | --- | --- | --- |
| Compact inventory | core/store | validated records → summaries → existing purge policy | policy equivalence and real SQLite paging |
| Space observation | filesystem | verified local directory → available physical bytes | missing path, unavailable provider, native temp directory |
| Maintenance runner | retention | volume, policy, time, budget → bounded purge report | expiry/quota, fresh observations, pins, failures and restart |
| Job completion hook | app | completed retained operation → best-effort worker maintenance | no default activation; preserved completion on maintenance error |

## Atoms

| Atom | Component | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| M1 | Compact inventory | self | high | — | policy and SQLite tests | done |
| M2 | Space observation | self | high | — | provider/native tests | done |
| M3 | Maintenance runner | self | high | M1, M2 | isolated filesystem and SQLite tests | done |
| M4 | Completion hook | self | high | M3 | worker lifecycle tests | done |
| M5 | Review and gate | self; Sol reviewer | xhigh | M4 | full Linux gate and resolved review | done |

## Done when

A bounded worker pass purges eligible versions in policy order, verifies every physical observation, respects pins and safely resumes interrupted purges. Completed operations retain their successful state if maintenance refuses. The full Linux gate passes; Windows runtime and periodic scheduling remain explicit.

## Log

- 2026-10-25 — Started after committed overwrite retention. The existing drive adapter already uses OS available-block counts; the filesystem provider needs the equivalent path-aware observation for maintenance.

- 2026-10-25 — Located the owner's Opus review at commit `8c081606`, file `docs/reviews/2026-10-25-alpha-validation-opus.md`. Finish this maintenance stage first, then reproduce and fix its six major/nine minor findings. Native Windows checks must remain explicitly pending if this host cannot run them. Do not start more feature stages after maintenance.

- 2026-10-25 — Reproduced and fixed stale job-policy cleanup, a committed purge whose empty-holder deletion failed, starvation behind an occupied holder, adjacent u64 volume IDs above SQLite's signed integer range, and an unknown holder with no saved identity. The latter two regressions failed before their fixes in a local verification log. Repeat the final gate after these changes; the earlier 7,238-test gate passed before them.

- 2026-10-25 — Final `cargo xtask gate` passed all 10 steps in 183.3 seconds: 7,240 tests passed, 14 skipped, plus documentation tests. The native temporary-tree maintenance test verified expired-payload removal while preserving new user bytes. Read-only Sol 6.1 xhigh review has no remaining concrete findings. Final log: a local verification log. Periodic scheduling, terminal metadata pruning, Recently deleted and native Windows runtime checks remain explicit. The next task is the owner's Opus review, not another feature stage.

- 2026-10-25 — Final binary build passed with `cargo build -p navo-app --bin navo` (29.27 seconds).
