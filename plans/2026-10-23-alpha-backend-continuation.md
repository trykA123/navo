# Remaining alpha backend work

Status: implementation complete and verified on Linux — native Windows checks and Windows measurements remain · Repo: `navo-backend` · Branch: `backend/rename-undo-safety` · Started: 2026-10-23

## Goal

Finish remaining backend work in the four-feature alpha before starting later roadmap features. Keep every change isolated from main and record which work still needs UI integration or Windows verification.

## Decisions

- D1 — Continue in this worktree. The owner authorised completing the remaining alpha work and pushing `backend/rename-undo-safety` on 2026-10-23. No merge or deployment.
- D2 — Use Codex only; any worker runs must use Sol 6.1 high or xhigh. No Claude agents.
- D3 — Alpha work: single-file rename through the journaled batch engine (P41 C1), optional rename log (P41 E7), Rich ELF/MAP sibling discovery and configured module prefixes (P38 D8/D9), then remaining external drag platform integration (P36 X2/X6). Sibling discovery is independent of the rename log.
- D4 — Preserve single-rename validation, cursor behavior, filter behavior and error reporting. Refuse overlapping rename jobs. Create file and create folder retain their existing paths.
- D5 — Existing naming tests encode the removed temporary-name format and wait only for listing I/O. Update those fixtures to the batch planner and wait for jobs too; retain their behavioral assertions.
- D6 — Add no new dependencies. Use the existing planner, journal, settings ledger, file hooks and background execution.
- D7 — Beyond alpha: database quarantine/salvage, delete retention and general undo, shared job pools and folder sizes, compare/Git/merge, and beta copy options. Do not claim alpha complete while its backend items or native verification remain open.
- D8 — Module prefixes use the existing list setting control and live settings delivery. Keep the model's prefix snapshot through regrouping and selection; reject results built for an older prefix list.
- D9 — Rename logging is off by default. Capture the flag when starting a forward rename. Append one completed batch to `config/rename.log` on the job worker, collapse staging moves into original-to-final pairs, quote paths, and lock the file during append. A logging error reports a warning without changing a completed rename or its undo journal. Failed, cancelled and undo jobs do not append.
- D10 — Complete P36 X2/X3/X6 and P41 E4/E5 before pushing. Use isolated Codex Sol 6.1 high/xhigh workers for the drag bridge and rename model. Keep post-alpha work out of this continuation.
- D11 — Edited names override masks by row and retain normal validation. Reject mismatched line counts. Reordering carries edits and exclusions with each source and invalidates metadata caches and pending execution. File import runs off the UI thread with a bounded UTF-8 read.

## Components

| Component | Layer | Contract (in → out) | New / change | Test |
| --- | --- | --- | --- | --- |
| Single rename | service | old and new names → validated journaled rename job | change | naming, undo and restart regressions |
| Rename log | adapter | completed rename batch → optional profile log | new | disabled, success, failure and restart cases |
| Sibling discovery | adapter | file and folder candidates → unambiguous companion file | change | same-stem priority, ambiguity, cancellation and bounds |
| Module prefixes | settings and model | configured prefixes → live grouping | change | settings round trip and grouping regressions |

## Atoms

| Atom | Component | Role | Effort | After | Check | Status | Cost |
| --- | --- | --- | --- | --- | --- | --- | --- |
| N1 | Single rename plan and routing | self | high | — | failing journal/undo regression, naming tests | done | |
| N2 | Rename failure and cursor compatibility | self | high | N1 | naming and headless undo tests | done | |
| N3 | Docs and gate for single rename | self | high | N2 | full gate and diff review | done (Linux) | |
| L1 | Optional rename log backend | self | high | N3 | log adapter and job tests | done (Linux) | |
| R1 | Companion folder discovery | self | high | — | file hook and sibling tests | done (Linux) | |
| R2 | Configured module prefixes | self | high | R1 | store and model tests | done (Linux) | |
| M1 | Edited names and folder counters model | Codex Sol 6.1 | high | — | model regression tests | done (Linux) | |
| M2 | Name editor, import, paste and copy | self | high | M1 | headless command and import regressions | done (Linux gate) | |
| M3 | Preview ordering and sorting | self | high | M1 | headless reorder, exclusion and execution regressions | done (Linux gate) | |
| D1 | External drag bridge and right-button menu | Codex Sol 6.1 | xhigh | — | drag/drop regressions and platform review | done; native Windows pending | |
| V1 | Integration, docs and available gate | self | high | M2, M3, D1 | full Linux gate and native screenshots | done (Linux) | |
| V2 | Push and inspect Windows CI | self | high | V1 | remote SHA and CI results | pushed; Windows build/lint passed, four logging tests failed | |

## Done when

- Single-file rename offers journaled undo after a restart, with the batch engine's rollback and identity checks.
- Optional logging and Rich data gaps have explicit implementation and verification status.
- Available gates pass; native Windows and remaining UI work are reported separately.
- All work stays on this branch, with main untouched.

## Backend still open

The current alpha follows D75's four-feature scope. Work below stays unmerged on this branch.

| Phase | Remaining work |
| --- | --- |
| Alpha | P36 X2/X3/X6 implementation is complete on this branch: incoming bridge, outgoing wiring and right-button choices. Native Windows OLE checks remain pending. Unix incoming external drops permit Copy only until native modifiers can be captured. |
| Alpha parity | P41 E4/E5 implementation is complete on this branch: edited/imported/pasted names and copy export, preview ordering and sorting, and per-folder counters. The Linux gate and native light/dark screenshots passed. |
| Alpha verification | Native Windows checks for rename/recovery, drag, SCM/UAC/ACL and NTFS/USN; Ctrl+K final review, docs and Windows measurements. |
| Optional alpha tooling | P43 B1–B3: informational listing, walk and index benchmark harness. These do not block alpha. |
| After alpha | P35 quarantine/salvage and startup activation are complete on `backend/alpha-validation`, with durable recovery details and a persistent notice. The [startup integration](2026-10-24-p35-startup-notice.md) passed the Linux gate, native screenshots, recovery/restart smoke checks and review; native Windows checks remain pending. |
| After alpha | D65's [retention backend](2026-10-24-d65-retention-backend.md) is implemented on `backend/alpha-validation`: persistent catalog, explicit 1–7 day/volume policies, same-volume capture, no-replace restore, purge and interruption handling. Normal delete/overwrite activation, settings, terminal-metadata maintenance, Recently deleted and general copy/move/delete undo remain. Current operation hold-aside files still live only through a job until activation. |
| After alpha | P43: shared worker pools, cancellation races, per-tab size ownership, volume-aware queues, allocated-size totals, hard-link deduplication and opt-in automatic sizes. Existing I/O priority queues and cache soak tests remain in place. |
| Beta and before 1.0 | P42: folder/content compare and hash cache, text/large/binary diff, read-only Git engines and refs, journaled file/hunk writes, and three-way merge. Folder sync depends on D65 and general undo. |
| Beta | Explicit parallel/large-file copy modes, verification, bandwidth limits, copy profiles and destination rules; terminal providers, network refresh policies and portable-profile detection. Auto copy already supports parallel work. |
| Before 1.0 | Content/attribute search and saved collections, archive operation providers with safe extraction/packing, checksums, properties/ACLs and default-file-manager integration. Archive viewing already exists. |
| Later | Module lifecycle, WASM/Lua extensions, remote providers and automation. |

## Log

- 2026-10-23 — Rechecked code and plans. Alpha backend remains: C1, rename log, sole-companion fallback, module-prefix settings and external drag platform wiring. The previous commit completed identity verification only. Starting with C1.
- 2026-10-23 — Three new single-rename regressions failed first, including headless F2/Ctrl+Z and undo after reopening SQLite. Routing and compatibility changes passed all 52 targeted tests. Added case-insensitive provider and overlapping-job coverage for the full gate.
- 2026-10-23 — Seven companion regressions failed first: sole MDF/MAP lookup, cancellation, real folder enumeration, ambiguity and errors. Implemented a shared lazy scan, two retained candidates per extension, a 100,000-entry cap, head/body limits and provenance. The combined targeted suite passed 57 tests. The gate caught an overly nested test fixture; extracted its folder-name filter without relaxing lint rules.
- 2026-10-23 — Single rename and companion discovery passed `CARGO_BUILD_JOBS=8 nice -n 10 cargo xtask gate`: all 10 steps, 6,979 tests passed, 14 skipped, plus doc tests (167.8 seconds). Native Windows checks remain pending. Main is untouched; no merge, push, deployment or Claude use.
- 2026-10-23 — The new typed-prefix regression and headless live ELF regression failed first. Module settings now reach model builds, regrouping and selection; ELF and MAP update live and name matched configured modules. Updated settings snapshots and completion fixtures for the two new keys without weakening the comment-preservation assertions.
- 2026-10-23 — Both new log behavior regressions failed first. Added the optional profile adapter and worker callback, with nonfatal activity feedback. All 613 targeted settings, Rich model, rename and activity tests pass, including actual append/reopen/concurrent-file tests and live settings-file reload. Full gate and isolated light/dark settings screenshots are running.
- 2026-10-23 — Inspected the Files and module-prefix controls in native Linux gallery screenshots at 1280×720, light and dark, using an isolated headless Cage display. The gate required grouping rename settings, moving profile-hook assembly into its adapter, and separating general setting rows from the field contract. No lint or size limits were relaxed. Windows verification remains pending.
- 2026-10-23 — Logging and module prefixes passed `CARGO_BUILD_JOBS=8 nice -n 10 cargo xtask gate`: all 10 steps, 6,996 tests passed, 14 skipped, plus doc tests (180.1 seconds). No post-alpha feature was started because alpha integration and parity gaps remain. Main stays at ce5af5c9; no merge, push or deployment.
- 2026-10-23 — Owner authorised completing alpha gaps and pushing this branch, using only Codex Sol 6.1 high/xhigh workers. Integrated model and external-drag changes from isolated worktrees. The rename suite passed 87 tests (one timing test skipped), the core rename suite passed 162, and the drag worker passed 118 targeted tests. Full combined verification is running.
- 2026-10-23 — Review regressions reproduced stale counter validation, hidden-editor edits, edited-row reordering, mismatched editor reopening and a cursor panic after source removal. Fixed all five. Toggle events carry stable source identity; provisional row order updates immediately. Two core regressions also reproduced folder-alias collisions under insensitive folding and now pass.
- 2026-10-23 — External hover performs no metadata I/O. Captured allowed effects and target identity prevent forbidden operations or silent tab retargeting. Unix external drops are Copy-only. The gate caught a gallery dependency import, the page-table size limit and two untranslated adapter errors; corrected them without relaxing checks. Gallery page order now matches its enum ids.

- 2026-10-23 — Final combined gate passed all 10 steps: 7,060 tests passed, 14 skipped, plus doc tests (174.7 seconds). Inspected six native Linux gallery screenshots at 1280×720 in both themes for the name editor, token menu and right-button drop menu. Fixed the missing counter icon and updated the two keymap count fixtures for seven new commands. Implementation is ready for the authorised branch push. Native Windows OLE checks, Ctrl+K owner review and Windows measurements remain open; no post-alpha work or deployment.

- 2026-10-23 — Roadmap refresh: checked application commit ddda8d0b against Windows CI 39937857823. Build and Clippy passed; the Windows suite has 7,073 passes, four rename-log failures and 15 skips. Updated the HTML and Markdown roadmap to distinguish implementation, unreleased work and release validation. No application code changes.

- 2026-10-23 — Resumed from `1d59fb75` in the isolated `backend/alpha-validation` branch. Windows CI 39945045244 confirms the same four log failures. The log's append-only handle cannot be locked on Windows; added read access and strengthened history-preservation and distinct-concurrent-batch checks. Verification and review are tracked in [Windows rename log continuation](2026-10-23-windows-rename-log.md). Native Windows rerun remains pending.
- 2026-10-23 — The logging continuation passed all eight targeted tests and the full Linux gate: all 10 steps, 7,060 tests passed, 14 skipped, plus documentation tests (222.0 seconds). The single Sol 6.1 xhigh review found no actionable issues. Windows rerun, native checks and Windows measurements remain open; the original backend worktree and main are untouched.
- 2026-10-23 — Owner requested further backend implementation. Completed the [P35 storage backend](2026-10-23-p35-recovery-backend.md) on `backend/alpha-validation`: no-replace quarantine, recorded-slot resume, isolated orphan WALs, verified transactional salvage and manual-recovery details. All 10 Linux gate steps passed, 7,088 tests passed and 14 skipped, plus doc tests; Sol 6.1 xhigh review has no unresolved findings. Startup activation, the persistent notice and native Windows checks remain open.
- 2026-10-24 — Completed P35 startup activation and its persistent notice on the local continuation branch. The main agent implemented all changes; Sol 6.1 xhigh reviewed only. Full Linux gate: all 10 steps, 7,117 passed, 14 skipped, plus doc tests. Inspected four gallery screenshots and two final-app screenshots in both themes; actual damaged-profile recovery and clean restart passed. No unresolved review findings. Native Windows checks remain open; main and the original backend worktree are unchanged. No merge, push or deployment.
- 2026-10-24 — Completed the [D65 retention backend](2026-10-24-d65-retention-backend.md): persistent V6 catalog, 1–7 day/per-volume policy, atomic same-volume capture/restore, guarded purge, interrupted-phase reconciliation and P35 manual recovery compatibility. All 10 Linux gate steps passed, with 7,170 tests passed and 14 skipped, plus doc tests. Sol 6.1 xhigh confirmed the safety fixes and no remaining findings; both final-app themes passed the isolated recovery/restart check. Normal operation activation, settings/maintenance, Recently deleted, general undo and native Windows checks remain.
