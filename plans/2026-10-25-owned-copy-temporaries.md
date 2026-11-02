# Owned retained-copy temporaries

Status: done · Repo: the backend worktree · Branch: `backend/beta-recovery-retention` · Started: 2026-10-25

## Goal

Resolve Opus M3. A failed or cancelled retained copy can be retried immediately. Startup removes only a temporary whose exclusive creation and identity were durably recorded; replacements and unproved files remain available with a pending notice.

## Decisions

- D1 — Main agent implements all atoms. Only the existing Sol 6.1 xhigh reviewer may review. No implementation workers, merge, push or deployment.
- D2 — Persist a fresh per-job temporary namespace in the typed retained-write plan before execution. Keep old V1 plan decoding; old unproved temporaries must never acquire deletion authority retroactively.
- D3 — Persist temporary path, intended target, parent identity and mount before creation. Create exclusively. Obtain identity from the still-open created handle and acknowledge its durable registration before writing bytes.
- D4 — Parallel workers ask the logging thread to register the created identity and wait for acknowledgement. Drain these requests after failure or cancellation so workers cannot deadlock.
- D5 — Remove only matching owned temporaries in the original verified parent/mount. An absent path can be retired only with that scope verified. A replacement, unknown object or changed parent stays pending.
- D6 — Retire temporary evidence after proved placement, or after safe failure/cancel cleanup. Retained completion must reject unretired evidence. Recovery and manual paths must recognize the new record type, including damaged owner kinds.
- D7 — Preserve the ordinary optimized copy path. Unix owned copies use exclusive handle creation and the buffered pump. Windows uses native CopyFile2 with exclusive creation, first-stream handle registration, STOP on interruption and per-stream flushes, preserving NTFS alternate streams. Owned Windows copies disable unbuffered retries that could recreate an unproved temporary.
- D8 — Keep callbacks outside filesystem-state mutexes and SQLite transactions. Do not infer ownership from path spelling or size.

## Components

| Component | Layer | Contract | Change | Test |
| --- | --- | --- | --- | --- |
| Core temporary proof and write plan | lib | Versioned namespace and scope/identity records | extend | JSON compatibility and invalid-proof cases |
| Journal evidence replacement | persistence | Indexed job_step.detail compare-and-swap, without losing step state or Undo evidence | extend | independent undo rows survive |
| Owned provider copy | filesystem adapter | Exclusive open, created-handle ID, durable callback, then writes | new | native and fake ordering/failure tests |
| Copy coordinator | service | Temporary intent, worker acknowledgement, proved placement | extend | sequential and parallel cleanup/cancel tests |
| App temporary lifecycle | service | Typed job evidence to safe cleanup or pending paths | new | retry, restart and replacement tests |

## Atoms

| Atom | Component | Role | Effort | After | Check | Status | Cost |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | Core proof and plan | main | high | — | core JSON compatibility and validation | done | |
| A2 | Journal evidence replacement | main | high | A1 | store atomic replacement/retirement tests | done | |
| A3 | Owned provider contract/native copy | main | high | A1 | native exclusive-open and registration ordering | done | |
| A4 | Fake owned copy | main | high | A3 | callbacks before bytes and injected faults | done | |
| A5 | Sequential copy coordination | main | high | A1,A3 | prepare/create/place/retire ordering | done | |
| A6 | Parallel acknowledgement | main | high | A5 | registration failure/cancel drains all workers | done | |
| A7 | App temporary lifecycle | main | high | A1,A2 | identity/scope-checked cleanup preserves replacements | done | |
| A8 | Finish/recovery integration | main | high | A6,A7 | pending evidence cannot be discarded | done | |
| A9 | Failure/cancel retry | main | high | A8 | Opus original probe and cancellation variant | done | |
| A10 | Crash/replacement boundaries | main | high | A8 | startup after mid-copy and pre-registration interruption | done | |
| A11 | Verification/report | main | high | A9,A10 | affected checks, Sol review and full gate | done | |

## Implementation reads

- Core: `crates/navo-core/src/retention/writing.rs`, `mod.rs`, validation and existing tests.
- Persistence: `crates/navo-store/src/journal/held.rs`, retention completion coverage and salvage journal decoding.
- Providers: `crates/navo-fs/src/provider/ops.rs`, local copy/identity and fake copy/state implementations.
- Coordination: `crates/navo-fs/src/job/{copy,run,pool,parallel,aside}.rs` and retained placement.
- App: `crates/navo-app/src/jobs/{overwrite,held,recovery,pending,mod}.rs` and retained-write test fixtures.

## Done when

- The original failed-copy retry probe is red before the fix and green after it; cancellation behaves the same.
- Native create-new collisions preserve existing files. Created-handle identity registration happens before any bytes are copied.
- Sequential and parallel failures, cancellation and lost registration acknowledgements cannot deadlock workers or delete unproved files.
- Startup cleans proved partials, preserves replacements and unknown files, and records their manual paths. Fresh namespaces permit another attempt.
- Other overwrite proofs and journal rows survive temporary retirement. Completion cannot clear pending temporary evidence.
- Old native/legacy identity domains and V1 retained plans still decode safely.
- Relevant checks, native temporary-file regressions, read-only Sol review and the full project gate pass. No live user data is used.

## Log

- 2026-10-25 — Prepared while the quarantine checkpoint is verified. Namespace, parent/mount proof and worker acknowledgement are required to cover the create-before-registration crash gap. Exact journal APIs and fake-copy locking must be inspected before A2/A4 implementation.

- 2026-10-26 — Implemented versioned namespaces, indexed step evidence and sequential/parallel ownership acknowledgement. Three original failure/cancel/restart probes and seven ownership boundaries pass. Sol found the initial generic Windows pump lost ADS; replaced it with registered native CopyFile2. Damaged owner/detail and salvage-marker regressions reproduced red; fixes in verification. Native Windows tests remain unrun on this Linux host.

- 2026-10-26 — Affected checks passed 3,372 tests / five skipped (43.6 s); first complete gate passed 7,313 / 17 skipped, all ten steps (174.4 s). Further official Windows review found no pathname-sharing guarantee during CopyFile2 callbacks. A new regression failed when callback registration reopened the destination. Registration now trusts the exclusive provider handle identity and rechecks only the parent/mount; closed-file identity is checked before placement and cleanup. Final gate follows this adjustment.

- 2026-10-26 — Final full gate passed all ten steps: 7,314 tests passed, 17 skipped (127.9 s), a local verification log. Native Linux cancellation/reopen/cleanup/retry passed. Sol 6.1 xhigh final read-only review is clear, including handle-only registration. Native Windows tests (ADS, registration, readonly cleanup) are present but unexecuted; this Linux host has only the Linux Rust target. No UI source changed for M3.
