# D65 overwrite retention

Status: done on Linux; native Windows verification pending · Repo: the backend worktree · Branch: `backend/alpha-validation` · Started: 2026-10-25

## Goal

Preserve previous file/link versions after opt-in Copy/Move overwrites. Commit their retention catalog entries and owner completion atomically, retaining evidence on persistence errors or interruption.

## Decisions

- D1 — Implement in the existing worktree after `43a6b21e`. Main agent implements all changes; Sol 6.1 xhigh reviews only. Keep changes local for tomorrow's review.
- D2 — Adopt existing A8 payloads in place. V1 records keep fixed `payload` paths; validated V2 records identify a single original filename in a sibling `.navo-held-*` container. Shared containers remain until empty.
- D3 — Persist parent/container/mount identities, old node identity/stamp and replacement identity before moving the old target. Opt-in overwrite jobs use atomic no-replace placement and refuse unsupported providers. No permanent-overwrite fallback.
- D4 — Store a validated policy snapshot with a distinct versioned Copy/Move job plan. A new `retain-overwrites` setting defaults to false; legacy jobs keep their current behavior.
- D5 — Validate adopted payloads, insert Ready entries, finish the owner and remove terminal A8 cleanup records in one SQLite transaction. No filesystem relocation occurs during adoption. Failed persistence leaves the job recoverable and its payloads untouched.
- D6 — Failed/cancelled jobs also retain previous versions when still held. Interrupted retained-write jobs preserve proved versions as pinned restore-pending entries and retain operation evidence. Automatic general Copy/Move undo is a following stage; do not run legacy stamp-only rollback or cleanup on these jobs.
- D7 — Keep the shared mutation lease through adoption. Runtime and startup refuse malformed kinds, unproved/replaced backups, changed parents/containers and missing volumes. Preserve manual paths.

## Components

| Component | Layer | Contract | Check |
| --- | --- | --- | --- |
| Adopted records and write plans | core | proved A8 paths + policy → bounded V2 records and versioned plans | invalid paths, owner requirements, V1 compatibility |
| Atomic catalog completion | store | active owner + records + state → entries and committed completion | rollback injection, duplicate/retry, policy/plan validation |
| Proved A8 holding | filesystem | target + replacement → durable identity proof before atomic placement | collisions, links, changed containers, failed logging |
| Adoption service | retention | proved held files → verified atomic adoption | exact native bytes, independent restore/purge of shared holders |
| App integration | app | settings + job lifecycle → retained overwrites and pending recovery | defaults, finish failure, cancellation, startup, Browser workers |

## Atoms

| Atom | Component | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| O1 | V2 records and write plans | self | high | — | core boundary regressions | done |
| O2 | Atomic catalog completion | self | high | O1 | real SQLite rollback/retry | done |
| O3 | Proved A8 holding | self | high | O1 | filesystem safety regressions | done |
| O4 | Adoption service | self | high | O2, O3 | native bytes and shared holders | done |
| O5 | Settings and app activation | self | high | O4 | Copy/Move and startup tests | done |
| O6 | Review, gate and documentation | self; Sol reviewer | xhigh | O5 | resolved review and full Linux gate | done |

## Done when

Opt-in successful and partial Copy/Move jobs preserve proved old versions. Catalog adoption and owner completion are atomic. Interrupted writes preserve both user data and evidence without legacy destructive recovery. Existing delete retention and legacy jobs remain verified. Linux gate and review pass; native Windows and remaining general undo/maintenance work stay explicit.

## Log

- 2026-10-25 — Confirmed A8 completion currently removes held old versions. Chose metadata-only adoption to avoid a second filesystem move and its crash window. Copy workers must defer strict placement to the logging thread; interrupted retained writes remain pending until verified general undo is implemented.

- 2026-10-25 — Implemented O1–O5. Eleven app integration tests pass, including native shared-holder restoration, partial/cancelled jobs, failed logging/completion, callback replacement, omitted durable proofs, repeated completion and damaged kinds. Store regression reproduced shifted completed step sequences before the fix. Read-only Sol 6.1 xhigh review is clear after fixes for complete proof coverage, post-callback revalidation, returned-holder cleanup, mount-query cleanup, terminal retries and legacy recovery guards. Full workspace verification is running.

- 2026-10-25 — Final `cargo xtask gate` passed all 10 steps in 178.5 seconds: 7,219 tests passed, 14 skipped, plus documentation tests. Final `cargo build -p navo-app --bin navo` passed. Native startup with both retention switches enabled passed light/dark at 1280×720 and 640×480; all four screenshots were inspected and fixture bytes remained unchanged. Logs: a local verification log, a local verification log; native evidence: the local evidence folder. No merge, push or deployment.
