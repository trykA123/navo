# D65 retained delete jobs

Status: done on Linux; native Windows verification pending · Repo: the backend worktree · Branch: `backend/alpha-validation` · Started: 2026-10-24

## Goal

Connect the retention engine to opt-in ordinary delete jobs and recover interrupted captures through the startup journal. Preserve complete selected trees without permanent-delete fallback.

## Decisions

- D1 — Main agent implements all changes. Sol 6.1 xhigh reviews only. Keep changes local for the user's Monday review.
- D2 — Store a typed versioned retained-delete plan and validated policy in the existing job transaction. Keep ordinary OperationPlan fields readable for manual recovery reports. No database schema migration.
- D3 — Add explicit delete-retention settings, disabled by default while restore UI is pending. Read and validate settings on the job worker; invalid settings or unavailable journal refuse mutations.
- D4 — Serialize runtime mutations and rename undo with a profile lock plus a shared user mutation lease. A busy profile refuses a new operation with a retry message. Waiting for another profile supports cancellation. Creation, editor saves and sole-session startup recovery share the user lease; parallel copy workers inside one job remain available. Tests use a separate lock namespace.
- D5 — Capture topmost deletion roots as whole trees. Persist capture intent before moving, and roll back retained entries on cancellation/failure. Authorize owner rollback in the store transaction while retaining normal public pin guards.
- D6 — Protect catalog containers against ordinary writes, deletion and parent relocation, including alias ancestors, leaf symlinks used by editor saves, other profiles and inverse rename undo. Recheck profile paths after obtaining the shared lease. Unsupported providers, changed trees, occupied restore targets and unavailable volumes preserve evidence and require retry/manual recovery.
- D7 — Overwrite adoption, maintenance, terminal metadata pruning and restore UI remain separate follow-up work. This stage does not automatically purge retained payloads or enforce quota/free-space maintenance. Missing profiles or unavailable journals refuse user writes rather than using unlogged mutation.

## Components

| Component | Layer | Contract | Change | Check |
| --- | --- | --- | --- | --- |
| Retained delete plan | core | plan + policy → validated roots and serialized intent | new | invalid policy/plan and root selection |
| Owner rollback | store/service | active owner + retained entry → safe restore | extend | wrong owner, pins, restart, target collision |
| Delete settings | store | TOML → validated opt-in policy | extend | defaults, parse/save/reload |
| Runtime adapter | app/UI job boundary | journal + provider → retained deletion outcome | extend | actual temporary trees and journal failures |
| Startup recovery | app | interrupted retained job → rollback or readable pending paths | extend | restart and conflict regressions |

## Atoms

| Atom | Component | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| A1 | Retained delete plan | self | high | — | core boundary tests | done |
| A2 | Owner rollback | self | high | A1 | store and engine tests | done |
| A3 | Delete settings | self | high | A1 | settings tests | done |
| A4 | Profile lease and private-path guards | self | high | A2 | contention and mutation tests | done |
| A5 | Runtime delete adapter | self | high | A3, A4 | native temporary deletion/cancellation | done |
| A6 | Startup recovery | self | high | A5 | interrupted capture/retry tests | done |
| A7 | Gate and review | self; Sol reviewer | xhigh | A6 | full gate and resolved review | done |

## Done when

Ordinary opt-in deletes retain selected roots, cancellation and startup recovery restore only owned matching payloads, and persistence failures never trigger permanent deletion. Native temporary-file tests, the full Linux gate and read-only review pass. Record Windows runtime limits and remaining D65 stages explicitly.

## Log

- 2026-10-24 — Resumed after the user's reboot at `80bf7d17`. The saved handoff survived; no source changes or checks were pending. Chose a versioned journal plan over a separate policy table to keep policy creation atomic without a schema migration.

- 2026-10-25 — Implemented versioned retained-delete plans with validated policy snapshots, durable root identity intents and complete-root capture. Connected ordinary Browser workers and startup preflight. Cancellation/failure restores captured payloads while their owner stays pinned; missing or ambiguous data remains restore-pending with readable source/container paths.
- 2026-10-25 — Read-only Sol 6.1 xhigh review found cross-profile parent relocation, writes through aliases, creation/editor paths outside job logging, inverse-undo precheck ordering and malformed owner kinds. Added shared mutation coordination, canonical destination guards, creation/editor leases, deferred undo marking, legacy recovery guards and fail-closed kind validation. Normal waiting-job cancellation now persists Cancelled; refused undo preserves the original job state. The reviewer confirmed no remaining concrete correctness findings.
- 2026-10-25 — The kind-tamper regression failed first: 11 delete-job tests passed and the damaged-kind recovery test reported false success. Fixed owner/envelope dispatch and salvage validation; evidence a local verification log. Tests cover actual temporary trees, restart, occupied originals, cancellation, failed finish persistence, missing catalogs, private paths, two profiles, shared-lease cancellation, editor symlinks and Browser worker routing.
- 2026-10-25 — Updated all default-settings fixtures and ledgers from 53 to 57 keys. Generic parsing still preserves unknown settings; runtime mutation rejects unknown retention keys. Policy defaults are represented by validated core values, without unchecked constructor calls or lint exceptions. Moved new user-facing failures into `jobs.ftl`.
- 2026-10-25 — `CARGO_BUILD_JOBS=8 nice -n 10 cargo xtask gate` passed all 10 steps in 200.5 seconds. Nextest ran 7,198 tests: all passed, with 14 skipped; documentation tests passed. The 8 existing allowed audit warnings remain unchanged. Log: a local verification log. Built the final binary with `cargo build -p navo-app --bin navo`; log a local verification log.
- 2026-10-25 — Native headless Cage smoke passed light/dark at 1280×720 and 640×480. Inspected all four screenshots. Each instance used an isolated profile, home and cache, retained the exact fixture bytes, wrote metrics, removed its running marker and exited normally. The native render smoke exercises startup/layout; action correctness is covered separately by Browser worker tests and native temporary-file recovery tests. Artifacts: a local verification report; log a local verification log. Test-only compositor resizing code stayed outside the repository.
- 2026-10-25 — Final Sol 6.1 xhigh review found no new concrete correctness issues and confirmed the gate/native results. Implementation remained with the main agent. Main and sibling worktrees were not changed; no push, merge or deployment.

## Remaining D65 work

- Adopt A8 overwrite backups atomically with owner completion. Current overwrite behavior still releases those legacy backups.
- Activate expiry/quota/free-space maintenance using fresh space observations and add terminal metadata pruning.
- Expose Recently deleted and general user restore/undo. Retained deletes remain opt-in through `retention.retain-deletes`, false by default.
- Verify native Windows sharing failures, atomic moves and reparse-point behavior.
