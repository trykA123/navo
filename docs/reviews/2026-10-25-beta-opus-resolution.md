# Beta response to the Opus backend review

Source: local commit `8c081606`, `docs/reviews/2026-10-25-alpha-validation-opus.md`, reviewing `24acde8e`. The original branch remains at `c8bd352e`. Reviewed alpha fixes are isolated on `backend/alpha-opus-fixes` at `b75cd273`; recovery and retention are on `backend/beta-recovery-retention`, port baseline `a064f0b4`.

## Completed or implemented

- M1: ordinary copies, editor saves and creation share access. Structural jobs and retained operations queue cancellably before tree checks. This excludes ancestor relocation while A8 holders exist. Structural jobs can still wait behind a long copy.
- M2: only experimental Copy/Move/Delete reads the retention table. Unrelated invalid values and disabled retention do not block ordinary operations. Rename, editor save and creation require no policy parsing.
- M5/m7: retention keys require the explicit `--experimental-retention` launch flag, including instance forwarding. Mismatched modes are refused. The V7 partial index supplies live container paths without decoding manifests or terminal rows. Windows hides containers using the verified handle after durable ownership registration. Maintenance already enforces expiry, quota and low-space limits; Recently deleted, periodic scheduling and terminal metadata pruning remain open behind the gate.
- M6: separate alpha and beta branches preserve the original work for comparison. Neither branch has been merged or pushed.
- m5/m6: startup/save/creation waits have a three-second deadline. The existing startup failure path displays a persistent notice and blocks unsafe database access. The stable lock lives in a private protected directory based on the native OS account/folder lookup and is independent of XDG/HOME/LOCALAPPDATA environment overrides.
- n1/n2: private prefix constants are reused. Documentation states the residual external-process check/remove race and the accepted power-loss directory-entry limit.
- M4, m8, m9 and the rename-log version header are covered by the separate alpha resolution report. Native Windows and a real OS drag round trip remain unexecuted on this Linux host.

## Remaining fixes

Native Windows verification remains pending. The live-container guard currently streams the partial path index and filters intersections in Rust; targeted indexed intersection queries remain open. Terminal metadata pruning is still an unfinished development feature.

## Evidence

The three M1/M2/save probes failed on the beta baseline, then passed after the fixes. The affected-crate check passed 1,877 tests with three skipped. A separate valid 50-million-node terminal-catalog fixture occupied 6.4 GB. One hundred index checks took 0.73 milliseconds warm and 16.8 milliseconds including new connections. This measures the database portion of mutation guards, not full UI job-start latency. Full gate and native startup results are recorded in the work plan after execution.

Final isolation checkpoint: all ten gate steps passed, 7,271 tests passed and 15 skipped (172.2 seconds). Binary build passed. Four native Linux startup cases passed in both themes at desktop and compact sizes; screenshots were inspected. Native same-mode forwarding succeeded, and a mismatched experimental launch was refused with exit code one. Windows code remains unexecuted.

Minor m1 now keeps callbacks, pause waits and full filesystem walks outside SQLite write transactions. Short transactions recheck the observed entry and root proof; purge rechecks the owner before each node removal. Five baseline writer/pause tests failed and now pass. The native capture interruption test reopens the durable Preparing entry after the real rename, reconciles it, restores it and verifies the original bytes. A concurrent repin prevents the next purge deletion. Affected-crate verification passed 1,140 tests with two skipped; final gate results follow in the plan.

The final m1 gate also revealed a pre-existing parallel pause/resume callback race. A deterministic notification regression failed before the fix, then passed; 100 repeated parallel-pause runs also passed. The final full gate passed 7,279 tests with 15 skipped, all ten steps (145.5 seconds). Sol read-only review is clear for both changes.

Minor m3/m4 are implemented: same-identity hardlink interruption resumes, strict native publication supports linkless filesystems, and quarantine records are synced before no-clobber publication. Original crash probes were red, then green. Full replacement promotion uses the same native path and preserves identity domains. Two further regressions caught stage replacement deletion before cleanup; the corrected code skips consumed paths and rechecks remaining stage ownership. Native linkless-filesystem harnesses are present but unexecuted; stable file identities remain necessary for full recovery. Final gate results follow in the plan.

Quarantine/promotion checkpoint: the affected-crate check passed 1,894 tests with five skipped; the full gate passed all ten steps, 7,288 tests passed and 17 skipped (185.5 seconds). Sol recheck confirmed the replacement-cleanup fix. Native linkless filesystem and Windows harnesses remain unexecuted.

M3 is implemented: new retained copies journal a versioned per-job namespace and parent/mount intent, then register the exclusive created-handle identity before any transfer. Indexed step evidence uses compare-and-swap and survives step-state updates. Sequential and parallel failure/cancel cleanup and startup remove only matching proved temporaries; unknown files, replacements and changed parents remain pending. Fresh namespaces allow retries while preserving unproved legacy files. Completion refuses outstanding evidence; damaged owner kinds and unreadable details cannot enter legacy cleanup. Windows retains native CopyFile2/ADS, registers the first opened stream handle, flushes every stream and uses STOP on interruption. Registration does not assume the open pathname can be reopened.

M3 validation: the original failure/cancel/restart probes, damaged-owner/detail, salvage marker and open-handle registration probes reproduced red before their fixes. Affected checks passed 3,372 tests / five skipped. Final full gate passed all ten steps, 7,314 tests / 17 skipped (127.9 seconds). Native Linux cancellation, reopen, proved cleanup and retry passed. Sol 6.1 xhigh read-only review is clear. Windows ADS and owned-cleanup regressions exist but were not executed.

Minor m2 is implemented: oversized Delete plans use the existing required permanent-delete confirmation with a localized recovery-limit warning. Confirmed plans select the ordinary permanent-delete backend only on the typed size-limit error. Kind, conflicts, step type, absolute/clean paths and duplicates are validated before that error, so malformed plans cannot enable fallback. Other retention settings/errors keep their refusal. Protected-location typed confirmation and Cancel remain active.

m2 validation: the original factory probe and the reviewer’s dot/dot-dot masking probe failed before their fixes, then passed. The affected check passed 4,531 tests / three skipped; all six retained-delete core tests passed after the final validation fix. Final full gate exited zero, all ten steps passed, 7,319 tests / 17 skipped (121.3 seconds); binary build passed. Four native Linux dialog/cancel cases passed in both themes at 1280×720 and 640×480, with all screenshots inspected. Every cancellation preserved 100,001 isolated fixture files and created zero jobs. Artifacts: the local evidence folder. Windows execution remains pending.


## Owner-approved Opus 5.5 follow-up — 2026-10-26

Baseline: `9d77c450`. This section supersedes the earlier M1 coordination claim above.

V8 is parked on `backend/beta-v8-intersections`: implementation `a3934677`, parked-plan commit `155e2ef7`. The recovery branch returned clean to `9d77c450` before B1–B5. Schema V7 remains unchanged here. No alpha/FAT branch changes, push, merge or rebase are part of this follow-up.

- B1: the experimental launch flag now controls all global mutation lease acquisition: jobs, editor saves, F7/new files, rename Undo and startup recovery. Default mode acquires no lease. Experimental mode keeps shared ordinary writes and exclusive structural/retained operations and recovery.
- B2: default guards retain reserved-path protection but skip profile-directory canonicalization and retention database access. Experimental guards use `SELECT EXISTS(... LIMIT 1)` on the existing V7 live partial index before path filtering. No migration or new index is introduced.
- B3: oversized-delete retention wording requires both experimental mode and the current `retain-deletes` setting. A worker reads the setting before confirmation; rendering performs no settings I/O. Ordinary mode and disabled delete retention keep the permanent-delete wording and protected typed confirmation.
- B4: `docs/retention.md` explains that dropping the flag leaves retained folders and stops maintenance. `reserved()` continues to protect private paths; default mode does not coordinate catalog-based ancestor moves with experimental profiles.
- B5: test lock acquisition uses a process-scoped injected temporary directory. Threads in one test process share it; separate test binaries and nextest processes cannot touch the production per-user mutation lock. Native account-anchor lookup tests only read the anchor path.

Test-first evidence: a local verification log records all three initial probes failing: default Copy held a global lease; unavailable profile directories refused default mutation; default oversized delete displayed retention wording. Affected nextest passed 3,621 tests with six skipped in 103.7 seconds. Affected Clippy (`-D warnings`) and app build passed. Sol 6.1 xhigh static review found no actionable issue.

Four native Linux default-mode runs used retention keys set to true, proving the launch flag gates the wording. Light/dark screenshots at 1280×720 and 640×480 were inspected. Each cancellation preserved all 100,001 fixture files and left zero journal jobs. Native smoke runs used default mode, avoiding the production user mutation lock. Windows runtime checks remain unexecuted. Full gate ran once: all ten repository steps passed, including 7,331 workspace tests with 17 skipped and doc-tests. Test execution took 115.7 seconds; total gate wall time was 455.3 seconds including external-build waits. The serial runner waited before every Cargo invocation; all builds used nice 10 and `CARGO_BUILD_JOBS=8`. The final GUI wiring delta also passed the same read-only reviewer.

Durable evidence: the local evidence folder. It contains red-probe output, affected checks, full gate/build logs, native screenshots and cancellation results, plus a checksum manifest. No push, merge, rebase or alpha changes were made.


## Rebase onto released alpha — 2026-10-26

Created `backend/beta-retention-on-main` from `2294facb` and fetched `origin/main` at `e077fb5a`. Replayed only the seven beta commits after `b75cd273`. The original `backend/beta-recovery-retention` ref remains at `2294facb`; V8 remains parked separately.

Resolved conflicts in `CHANGELOG.md`, `docs/roadmap.md`, `crates/navo-fs/src/provider/ops.rs` and `crates/navo-app/src/jobs/identity.rs`. The provider import conflict recurred when retained-copy ownership was replayed. Released alpha notes and Navo roadmap priority remain intact.

The shared core identity now includes main's fingerprint domain. Its constructor receives size, modification time and folder kind, preserving the same bytes, domain distinction and exact one-hour tolerance without a core-to-filesystem dependency. FAT/exFAT and unknown filesystems still refuse native-ID proof before either native-ID query; rename uses the existing fingerprint fallback. Retention continues to require proved identities. The app decoder preserves journal v1/v2 compatibility and reusable decoding for beta recovery.

Main's drag auto-scroll, hover-open, drive menus and external-drop files match fetched main. Sol 6.1 xhigh read-only review found no actionable integration issue. Added shared-identity round-trip/domain coverage and extended promotion-domain rejection to both fingerprint kinds. The initial affected compilation exposed a test that assumed only two identity domains; it now checks all four domains.

Removed machine-specific paths from sixteen review/plan documents. Historical results are retained with plain references to local logs and evidence folders. Affected all-target Clippy passed across six crates; all 744 selected integration tests passed. The actual `cargo xtask gate` ran once and passed all ten steps in 245.5 seconds: 7,361 tests passed, 17 skipped, plus documentation tests. Three orchestration probes waited for external Cargo/Nextest and then passed. A temporary command guard checked external build processes before every Cargo invocation while excluding its own process tree; all commands used nice 10 and eight build jobs.

Final public-note scanning found no machine-specific paths. Durable logs, range-diff and reviewer output remain in the local evidence folder. Native Windows FAT/exFAT runtime verification remains pending. The original beta ref is unchanged; no push or merge occurred.
