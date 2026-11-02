# Opus backend review fixes

Status: running; alpha fixed, beta port verified · Repo: the backend worktree · Branch: `backend/beta-recovery-retention` · Started: 2026-10-25

## Goal

Finish the maintenance stage already in progress, then resolve the owner's Opus review of `24acde8e`. Preserve local reviewable commits and distinguish verified Linux fixes from native Windows checks this host cannot execute.

## Source

`8c081606:docs/reviews/2026-10-25-alpha-validation-opus.md`, available on local branch `review/alpha-validation-opus`. Read completely on 2026-10-25. Do not edit its worktree or rewrite that branch. Keep the review and a finding-by-finding resolution report with the fixes.

## Decisions

- D1 — User explicitly requested completion of maintenance followed by these fixes. Main agent implements everything. Only Sol 6.1 xhigh may review; no implementation workers. Do not start further feature stages.
- D2 — Preserve normal job/save/creation concurrency. Gate unfinished retention activation behind an explicit development option. Queue retained operations with cancellation; never refuse unrelated normal operations because a copy is running.
- D3 — Isolate retention-policy parsing. Unrelated invalid settings must not block filesystem operations, creation or editor saves.
- D4 — Journal identities of exclusively created copy temporaries before writes, and remove only matching owned temporaries on failure, cancellation and startup recovery. Preserve replacements and unknown files.
- D5 — Add Windows file-identity fallback to volume serial plus zero-extended 64-bit file index. Implement native test coverage/harness, but do not claim Windows runtime execution on Linux.
- D6 — Retain the existing branch for comparison. Prepare separate local alpha rename/drag and beta recovery/retention branches without rewriting pushed history, merging to main, pushing or deploying.
- D7 — Keep progress callbacks and pause waits outside SQLite write transactions. Recheck ownership in short mutation transactions.
- D8 — Fix quarantine crash boundaries, bounded startup lock waits and a private stable user lock location. Log incomplete rename results and rotate the log under a stable lock. Reset native outgoing drag state when reentering internally.

## Findings and atoms

| Id | Required outcome | Status |
| --- | --- | --- |
| M1 | Concurrent copies, editor saves and F7; cancellable structural/retention queue | complete; gate and native smoke passed |
| M2 | Retention-only parsing; unrelated invalid settings do not block operations | implemented; regression probes passed |
| M3 | Proved temporary-file journal and safe failure/cancel/recovery cleanup; successful retry | pending |
| M4 | Windows legacy file-ID fallback and clear error; exFAT native test/harness | pending |
| M5 | Development gate; indexed live-container queries; hidden Windows containers; terminal lifecycle | partly supplied by maintenance V7; pending |
| M6 | Separate local alpha and beta branches | complete: alpha b75cd273, beta a064f0b4 plus fixes |
| m1 | Pause and large verification leave SQLite writers available | complete; writer/pause and native interruption tests plus full gate passed |
| m2 | Localized retention-size limit and explicit permanent-delete choice | pending |
| m3 | Resume identical hard links and support quarantine on non-hard-link filesystems | complete in code; native linkless harness remains unexecuted |
| m4 | Atomic publication of synced quarantine records | complete; red-to-green probes and full gate passed |
| m5 | Bounded recovery/startup and save lock waits, visible startup notice | implemented via existing failure-notice wrapper |
| m6 | Stable lock name in a private per-user runtime/data directory | implemented; native user lookup independent of session environment |
| m7 | Gate incomplete Copy/Move recovery behavior with M5 | implemented development gate |
| m8 | Log completed pairs of Stuck batches; rotate under lock | complete in alpha b75cd273 |
| m9 | Internal drag reentry clears stale native/outgoing state; native check where supported | code/tests complete in alpha; real OS round trip pending |
| n1 | Document per-node check/remove race and power-loss durability limits | complete in docs/retention.md |
| n2 | Reuse private-container prefix constants | complete |
| n3 | Version header in rename log | complete in alpha b75cd273 |

## Verification requirements

Carry over the review's three failing probes. Add concurrency/save/F7, settings isolation, partial-copy retry and startup tests. Cover identity-unavailable rename, indexed guard behavior with large terminal catalogs, paused/large retention versus concurrent SQLite writes, quarantine crash injections, native capture crash boundary and replaced-member swap/cycle undo. Add Windows checks for exFAT, open child handles, reparse roots and SMB; record unexecuted native environments explicitly. Exercise any changed native UI in both themes at desktop and compact size. Run the project gate before each local commit.

## Log

- 2026-10-25 — Found and read the complete Opus review. Maintenance is still in progress; its own reviewer found a stale policy, terminal cleanup retry, u64 SQLite projection precision and unknown-container cleanup acknowledgement. Resolve these first, then begin this plan.

- 2026-10-25 — Maintenance finished and committed as `c8bd352e`; final gate 7,240 passed/14 skipped, 183.3 seconds, binary build passed. Active alpha worktree: the backend worktree, branch `backend/alpha-opus-fixes`, base `e3fbd8a5`. M4/m8/m9 changes remain uncommitted there. After alpha gate/review/native smoke, create beta from fixed alpha and replay `a27b813b`, `9db52dab`, `80bf7d17`, `43a6b21e`, `24acde8e`, `c8bd352e`, preserving this original branch. Resolve identity-type move conflicts by carrying the new FileIdentityKind into navo-core, with serde default Native and omitted Native marker; keep old persisted native records compatible. Preserve the renamed-log callback status and partial applied-step logic when replaying the later UI finish-result changes.

## M3 implementation notes for the next beta phase

Use an owned-copy provider operation that creates the temporary with create_new, obtains its identity from the still-open handle, and waits for durable registration before writing. Windows preserves CopyFile2 and registers its first destination stream handle; Unix derives identity from the exclusively created File metadata. Parallel workers send a Created event plus acknowledgement to the logging thread and wait; sequential copy registers directly. Keep the legacy optimized copy path unchanged. Record typed temporary proofs separately from held-version proofs, including path, parent identity and mount. At failure/cancel/finish/startup remove only a matching owned temporary; an absent path is retired only after parent/mount verification. Preserve a replacement and its proof as pending. Overwrite completion must reject unretired temporary evidence. Implement the API with request/control structs to respect argument limits. Add failed/cancelled copy retry, crash mid-copy and replaced-temp regressions.

- 2026-10-25 — Alpha boundary tests are red→green: uncertain applied rename with lost acknowledgement enters the incomplete log; unconfirmed rollback stays Stuck; UI swap Undo with a replaced member stays RestorePending. Do not lose these fixes when rebasing beta. Full alpha verification is still pending. Rename-log audit must remain available even if later beta finish persistence returns an error; preserve the physical outcome before propagating finish errors.

For M3, persist a fresh per-job temporary namespace before execution. A crash between create_new and durable identity registration cannot authorize deleting an unproved object; fresh names prevent that gap from poisoning retries. Keep unproved collision paths for manual recovery. Compatibility with existing retained-write plans and their old deterministic suffix must remain explicit.

- 2026-10-25 — Alpha fixes committed as `b75cd273` after gate7074passed14skipped (133.3 seconds), final build and four native startup cases with screenshots inspected. Separate beta worktree created at the backend worktree. Replayed all six post-alpha stages without committing, preserving original branch `backend/alpha-validation` at `c8bd352e`. Resolved identity-type and worker audit/finish conflicts. Beta native identity JSON remains compatible; audit runs even when finish persistence fails. Baseline beta gate is required before the aggregate port commit and before the remaining findings are implemented.

- 2026-10-25 — Beta integration gate passed all 10 steps: 7,255 tests passed, 14 skipped, 198.1 seconds. Sol 6.1 xhigh read-only integration review found no concrete errors. Remaining retention findings are intentionally unchanged in this port baseline. Remote Control was verified enabled on the current managed Codex daemon.

- 2026-10-25 — Beta baseline committed as `a064f0b4`. M1/M2 regression probes failed on the baseline, then passed after shared ordinary leases and retention-only parsing. Development activation now requires `--experimental-retention`; forwarding preserves that mode and refuses mismatch. Structural jobs (Rename/Recycle/RemoveFolder) queue exclusively before scanning, while ordinary copies, saves and F7 share access. This preserves A8 holder safety when another job would relocate an ancestor.
- 2026-10-25 — Reviewer caught unlinkable runtime lock names, environment-dependent lock anchors, A8 ancestor relocation and ignored forwarded flag. Fixed reserved private paths, native OS account/folder lookup, structural leases and explicit mode matching. Native identity-checked Windows hidden-container implementation added; Windows execution is still unavailable here. Final regression/gate and large-terminal-catalog benchmark are pending before this checkpoint commit.

- 2026-10-25 — Isolated four-crate check passed all seven steps: 1,877 tests passed, three skipped (38.1 seconds). Sol reviewer confirmed all four coordination/forwarding findings closed. Large terminal-catalog test passed: 1,000 rows × 50,000 valid nodes, 6,396,039,168-byte temporary SQLite file; 100 warm guard checks 728.443 microseconds, 100 fresh-connection checks 16.802816 milliseconds. Fixture creation/run took 136.54 seconds and its temporary database was removed by TempDir. Report: the local evidence folder. Full gate and native startup verification follow.

- 2026-10-25 — Final isolation gate passed all ten steps: 7,271 tests passed, 15 skipped, 172.2 seconds. Binary build passed in 30.57 seconds. Four native startup runs (light/dark, 1280×720 and 640×480) exited zero; all screenshots inspected. Dark cases used the development flag. Native forwarding passed: matching mode exited zero; mismatched experimental mode exited one with an explicit refusal. Artifacts: the local evidence folder, logs a local verification log and a local verification log. No Windows runtime tests ran.

- 2026-10-25 — Isolation fixes committed as `9d67252f`. Implemented m1 with full snapshots and callbacks outside write transactions, checked short state transitions and per-node purge pin checks. Capture remains Preparing across the native rename and post-move verify. Five writer/pause probes failed on the checkpoint, then passed. Added native capture interruption after rename with reopen/reconcile/restore, plus owner repinning before purge deletion. Sol 6.1 xhigh read-only review found no concrete correctness defects. Affected-crate check passed all seven steps, 1,140 tests passed/two skipped (28.3 seconds). Full gate is required before commit.

- 2026-10-25 — First full m1 gate exposed a pre-existing parallel-pause notification race: 7,277 passed, one timeout, 15 skipped. Added a deterministic callback-resume regression that failed with only `[false]` notification instead of `[false, true]`. The coordinator now rechecks pause state after reporting, before blocking for worker events. The regression passed and 100 consecutive parallel-pause stress iterations passed. Sol 6.1 xhigh read-only review clear. Final gate passed all ten steps: 7,279 tests passed, 15 skipped, 145.5 seconds (a local verification log). Native capture interruption and concurrent owner repin regressions passed in the affected-crate check. No UI code changed in this checkpoint.

- 2026-10-25 — m1 and the parallel-pause fix committed as `b842dfdd`. Implemented m3/m4: native no-replace quarantine with safe hardlink fallback; matching-identity duplicate names resume; synced temporary records use no-clobber publication. Both baseline crash probes failed, then passed. The store reuses the existing filesystem adapter without adding unsafe code. Added publication collision and orphan-empty-temp tests plus ignored native linkless harnesses.
- 2026-10-25 — Extended the same native publication path to replacement promotion. A new red regression proved that P35 dropped the native/legacy identity domain; persisted and compared the domain while preserving old Native JSON. Sol found that the renamed stage path could reappear and be deleted by cleanup. Two red regressions reproduced replacement deletion; promotion now skips cleanup after a consuming rename and verifies fallback/resume stage identity before unlink. Sol read-only recheck clear. Final affected-crate check and full gate pending.

- 2026-10-25 — Quarantine/promotion final check passed all seven steps: 1,894 tests passed, five skipped, 15.4 seconds. Final full gate passed all ten steps: 7,288 tests passed, 17 skipped, 185.5 seconds (a local verification log). Sol recheck confirms the stage replacement cleanup finding is closed. Native linkless/Windows harnesses were not executed. Next implementation is M3 under `plans/2026-10-25-owned-copy-temporaries.md`; long destination names require generated temporary basenames from namespace plus step sequence, not appending a namespace to the original filename.

- 2026-10-26 — M3 implemented under the owned-temporary plan. Failure/cancel/restart retries, unknown creation gaps, changed parents/replacements, legacy temporaries, two-worker registration refusal, completion barrier and damaged owner/detail boundaries are covered. Initial Windows generic buffering lost ADS; Sol caught it, and native CopyFile2 now retains streams with STOP-based interruption. Callback registration uses the created handle without reopening its pathname. All final ten gate steps passed, 7,314 tests / 17 skipped (127.9 s). Native Linux cancel/reopen/retry passed; Windows runtime remains unverified. Next: m2 localized oversized-delete confirmation.

- 2026-10-26 — M3 committed as `97d956a1`. Implemented m2: existing mandatory permanent-delete confirmation now shows a localized recovery-limit warning for oversized plans. Only typed DeleteTooLarge selects ordinary permanent deletion, after full kind/step/path/duplicate validation. Settings and other invalid plans still fail. The original large-delete probe and Sol dot-component finding reproduced red, then green. Final gate exited zero with all ten steps passed, 7,319 tests / 17 skipped (121.3 s). Binary build passed. Four native dialog/cancel cases passed with all 100,001 fixture files preserved and zero jobs; screenshots inspected. Remaining: native Windows/manual platform checks, indexed intersection filtering for live guards, terminal metadata pruning and future retention UI/scheduling behind the development flag.
