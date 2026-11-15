# Rebuild contract repair (RB0–RB5)

Status: proposed · Repo: `/mnt/Sabrent/homelab/.crew/navo-weekend-s9` · Branch: `weekend/s9-recovery-20261114` · Base: `09015ef7866288fe5b76dfa413bfbd26d5e14cbc` · Fetched origin/main: `f241ee618f2192b9a4b7ca21d7df2931a698393c` · Started: 2026-11-15 Europe/Bucharest.

Owner: root orchestrator. Backend: persistent Qwen-plan DeepSeek/dsh sessions at high effort. Fresh Sol 6.1 reviews at high effort precede implementation and integration. No nested workers in RB runs.

## Goal

Close the portable rebuild-contract gaps that block S11, S12 and T8: S13 fault/restart proof, persistent `Snapshot` context, optional recovery, an injected `SaveClock` writer, dormant startup/protected-persistence handoff and captured journal high-water. All six atoms are dormant until T8; none adds product scope, a wire/index format or a different journal policy.

## Evidence and current state

- Reviews: `/tmp/weekend-review-rebuild-contracts.md`, `/tmp/weekend-rebuild-contract-closure.md`, `/tmp/weekend-review-s13.md` (host review set; summarized here).
- S13 draft `42b9ac33` is in `/mnt/Sabrent/homelab/.crew/navo-weekend-s13`, not this branch. It is still REQUEST CHANGES for portable fault proof. Actual paths: `crates/navo-index-service/src/quarantine_snapshot.rs` is 142 lines; `quarantine_snapshot_tests.rs` is 444 physical / 397 code lines (hard 400-code gate; it must not grow). The brief's `snapshot.rs` means `quarantine_snapshot.rs`.
- Current lengths: `rebuild_session.rs` 202, `rebuild_session_tests.rs` 75, `lifecycle.rs` 161, `persistence_tests.rs` 161, `quarantine_recovery.rs` 221, `quarantine_recovery_tests.rs` 296, `ntfs/windows.rs` 254, `ntfs/volume_io.rs` 381.
- RB0–RB5 are absent from `plans/` and `crates/` at this base; no ID collision.
- S11, S12 and T8 remain BLOCKED. The old `PreparedRebuild<'static>` session paragraph in the parent plan is superseded by “Post-RB contracts” below.

## Non-goals

No new snapshot/wire/index format, no parser rewrite, no worker activation in RB0–RB4, no ordinary-writer fallback for protected persistence, no new product surface. Arrival at the old contract by widening S13 or T8 is not allowed; stop if an exact shape below cannot be expressed against the named source.

## Risks and rollback

The main risk is turning a real recovery I/O error into a cold build. RB2’s explicit error classification, RB0’s real-I/O fault tests and RB3’s no-phase-inference rule are the guards. If a later atom cannot keep old source bytes intact on failure, stop and report the exact source mismatch; do not relax the contract. Each atom is dormant and revertible independently; no irreversible data migration exists in this plan.

## Fixed contracts (orchestrator decisions)

1. `Snapshot` owns a `PathBuf` and `Option<(Source, Identity)>`; no borrowed path survives the session. `Snapshot::new(path: PathBuf, previous: Option<(Source, Identity)>) -> Self`. RB0 adds `#[cfg(test)] Snapshot::fixture(&Path, Option<&Recovery>) -> Self` forwarding the current constructor, migrates all eight basic call sites to it and uses it for every new fault fixture. RB1 changes `Snapshot::new` and ONLY the fixture-adapter body; no generic overloaded production constructor. The old test file stays at 397 code lines and gains no tests.
2. `SnapshotIo` is the only copy/sync/replace boundary. `replace(&mut self, &Path, &Path) -> io::Result<()>`, `copy(&mut self, &mut File, &mut File) -> io::Result<u64>`, `sync(&mut self, &File) -> io::Result<()>`. Native defaults use `navo_fs::native_replace_durable`, `io::copy` and `File::sync_all`; a blanket `FnMut(&Path, &Path) -> io::Result<()>` impl keeps existing test closures and routes `replace` through the closure.
3. Active quarantine precedes every repair/save, including a cold build with `previous=None`. The all-zero `Identity` is only a fence: never replay permission, never a fabricated `Recovery`, never a `complete` identity. `complete(id)` accepts only `Some((Source::Canonical, id))`, calls `quarantine::write_complete`, then `quarantine::clear`; a failure retains context. Existing `quarantine::read` classifies every nonmatching/noncomplete record as `Marker::SearchOnly`; the plan verifies that, it does not change the parser.
4. `SaveClock::save(now: Duration, write: impl FnOnce() -> Result<(), FileError>) -> Result<(), FileError>`. The closure is the writer; `failed(now)` on `Err`, `saved()` on `Ok`. No file-writer phase is inferred from an error. Existing ordinary-writer closures remain until T8 activation.
5. `restore_once(path: PathBuf, drive: Drive) -> Result<(Snapshot, Option<NtfsIndex>), NtfsError>` calls `load_optional` exactly once; `Recovery.index` moves into the handoff, never cloned or loaded twice; `Snapshot` retains only source/identity metadata. Protected persistence is `Snapshot::save -> complete` inside the clock writer; `clock.saved()` only after complete success, otherwise backoff.
6. `Volume::journal_position(&mut self) -> Result<JournalPosition, NtfsError>` uses `JournalState::parse(&self.query_journal()?)` through the existing `VolumeIo`. No bounded batch cursor substitute and no index-crate parser change.

## Components and atoms

| Atom | Component | Role / effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- |
| RB0 | S13 fault/restart proof | DeepSeek / high; Sol 6.1 / high review | S13 draft + plan PASS | N(navo-index-service, quarantine_snapshot); C(navo-index-service) | blocked |
| RB1 | persistent snapshot context | DeepSeek / high; Sol 6.1 / high review | RB0 | N(navo-index-service, quarantine_snapshot); C(navo-index-service) | blocked |
| RB2 | optional recovery classification | DeepSeek / high; Sol 6.1 / high review | RB1 | N(navo-index-service, quarantine_recovery); C(navo-index-service) | blocked |
| RB3 | injected SaveClock writer | DeepSeek / high; Sol 6.1 / high review | RB2 | N(navo-index-service, persistence); N(navo-index-service, rebuild_session); C(navo-index-service) | blocked |
| RB4 | dormant startup/protected persist | DeepSeek / high; Sol 6.1 / high review | RB3 | N(navo-index-service, rebuild_session); C(navo-index-service) | blocked |
| RB5 | captured journal high-water | DeepSeek / high; Sol 6.1 / high review | none (independent; before S12) | N(navo-index, journal_position); C(navo-index) | blocked |

Production caps below count additions plus deletions; test lines are separate. Every file must stay under 400 code lines, tests included. S11 is after RB4. S12 is after S11 and RB5. T8 is after S11/S12; S14 must precede T8 and Windows T15 remains the platform proof.

### RB0 — close S13 portable fault/restart proof (≤80 production changed lines)

- **Read/files (3):** `quarantine_snapshot.rs` (142), `quarantine_snapshot_tests.rs` (444/397; rename constructor calls only, add no tests), new `quarantine_snapshot_fault_tests.rs` (≤400 code).
- **Change:** add `#[cfg(test)] Snapshot::fixture(&Path, Option<&Recovery>) -> Self` in `quarantine_snapshot.rs`, forwarding the current `Snapshot::new`; migrate all eight basic `Snapshot::new` call sites to `Snapshot::fixture` and use it for every new fault fixture. Add the `SnapshotIo` trait, a native implementation and the blanket closure implementation. Route the repair copy, repair staging sync, candidate staging sync, backup copy/replacement and candidate replacement through `SnapshotIo`. Keep `repair()`/`save()` on the native path; keep `repair_with`/`save_with` accepting `&mut impl SnapshotIo` so old closures still compile. Register the new fault test as a `#[path]` child in `quarantine_snapshot.rs`; leave old basic test bodies unchanged as regressions.
- **Fault tests:** real files and real `io::Error`s. Cover partial copy, failed repair/candidate/backup staging sync, backup replacement before rename, rename-success-before-reported-flush-error, and the same boundaries during repair and candidate save. A post-rename fake must perform `fs::rename` itself and then return `Err`; it must not call `native_replace_durable` first. Start corrupt canonical plus sole validated backup. After EACH injected failure assert unchanged old backup bytes/checksum, a searchable validated generation, active quarantine and rejected replay through `quarantine_recovery::load` BEFORE retrying. After an ambiguous candidate rename, recovery may select the fully written candidate for search only; the old backup must still remain valid and unchanged. Then safe retry succeeds and a second restart recovers. Include `Snapshot` context retention. Candidate faults occur only after successful backup repair.
- **Red/check:** first reproduce the ordinary-writer corrupt-canonical/sole-backup baseline as an assertion red, then protect it green. Restore each temporary mutation that disables a copy/sync/replace error path and quote the new fault-test red, never a compile-only red. Command: `cargo nextest run -p navo-index-service --no-fail-fast quarantine_snapshot`; `cargo xtask check -p navo-index-service`. Windows physical flush stays S14/T15 UNTESTED.

### RB1 — persistent snapshot context and quarantine fence (≤145 production; tests extra)

- **Read/files (2):** `quarantine_snapshot.rs` (after RB0), new `context_tests.rs` (≤400 code, registered as a `#[path]` child). RB0/basic tests stay unchanged.
- **Change:** make `Snapshot` own `PathBuf` and `Option<(Source, Identity)>`; add `new(path: PathBuf, previous: Option<(Source, Identity)>) -> Self`. Change ONLY the body of the existing `#[cfg(test)] Snapshot::fixture` to clone the path into `PathBuf` and map `Option<&Recovery>` to its source/identity; do not migrate or rename the eight basic call sites again. Add `complete(id)` as in contract 3. In `repair`/`save`, call `write_active` before any checkpoint or replacement; use `previous` identity or `Identity` all-zero when `previous=None`. Never use the zero fence for `complete`; only a writer-produced `Canonical` identity qualifies. Read the staged writer identity before any canonical mutation; never reread metadata to discover success. Verify the existing `read` parser returns `SearchOnly` for active/torn/nonmatching records.
- **Tests:** context ownership, zero-fence active marker is not complete, `complete` rejects zero/non-Canonical/mismatched identity, write/clear failure retains context, repair/save activate quarantine for `None`, cold no-proof path, and a successful complete clears only the matching marker.
- **Red/check:** temporarily let `complete` accept a zero or non-Canonical identity; quote that context assertion failure, restore, then green. `cargo nextest run -p navo-index-service --no-fail-fast quarantine_snapshot`; `cargo xtask check -p navo-index-service`.

### RB2 — optional recovery without turning errors into cold builds (≤145 production; tests extra)

- **Read/files:** `quarantine_recovery.rs` (221), `quarantine_recovery_tests.rs` (296).
- **Change:** normalize format truncation at the source-validation boundary before validation. `fn prefix(path: &Path) -> Result<[u8; 70], FileError>` at `quarantine_recovery.rs:192-195` maps only `FileError::Io(error)` with `io::ErrorKind::UnexpectedEof` to the existing `FileError::BadLength`; other I/O, allocation and every marker/ack error keep their original `Err`. No index-parser change. Then extract one selection helper used by both `load` and new `load_optional(&Path, Drive) -> Result<Option<Recovery>, NtfsError>`. `None` only when both canonical and backup are absent/invalid/stale; preserve existing validation, source preference and backup active-marker acknowledgement. `load` keeps its signature and returns a `FileError` when both sources are gaps; no caller changes. Classify only `FileError::Io(NotFound)`, `BadMagic`, `UnsupportedVersion`, `BadLength`, `BadChecksum`, `InvalidData` and `NtfsError::Reenumerate` as source gaps. If either source attempt is any other I/O, allocation, parse/table error, return that `Err`; never fall through to the other source or `None`. Every quarantine write/read/ack failure is `Err`, never `None`; marker/ack errors are never normalized into a source rejection.
- **Tests:** a short canonical (<70-byte prefix) plus valid backup returns `Some(Recovery { source: Backup, .. })` and leaves backup bytes unchanged; both short returns `None`; canonical/backup corrupt or missing returns `Some`/`None` as expected; a marker path that is a directory (or unreadable) makes `load_optional` return `Err`; allocation/parse errors are not swallowed; load compatibility still rejects no recovery.
- **Red/check:** temporarily map every selection error to `None`; quote the marker-error test’s `left: Err(_) right: Ok(None)` style assertion; restore and green. `cargo nextest run -p navo-index-service --no-fail-fast quarantine_recovery`; `cargo xtask check -p navo-index-service`.

### RB3 — inject the SaveClock writer (≤145 production; tests extra)

- **Read/files:** `lifecycle.rs` (161), `persistence_tests.rs` (161), `rebuild_session.rs` (202).
- **Change:** replace `SaveClock::save(path, snapshot, now)` with `save(now, write: impl FnOnce() -> Result<(), FileError>)`; on `Err` call `failed(now)` and return it, on `Ok` call `saved()`. Update `save_snapshot` to pass a closure around `file::save_with_ids(input.path, &header, table, ids)`. Update the persistence test helper and direct calls to the same ordinary-writer closure. Do not infer staging/backup/canonical phases from `Err`; do not change due/backoff rules.
- **Tests:** direct `SaveClock` success clears due; a failed writer sets the 30-second retry, a second failure sets 300 seconds, and a success after retry clears it. Keep all existing S9 persistence regressions.
- **Red/check:** temporarily skip `failed(now)` on writer error; quote the direct backoff assertion failure; restore and green. `cargo nextest run -p navo-index-service --no-fail-fast persistence`; `cargo nextest run -p navo-index-service --no-fail-fast rebuild_session`; `cargo xtask check -p navo-index-service`.

### RB4 — dormant startup and protected-persistence handoff (≤145 production; tests extra)

- **Read/files:** `rebuild_session.rs` (after RB3), `rebuild_session_tests.rs` (75; split only if the 400-code limit demands another listed sibling).
- **Change:** add internal `restore_once_with(path: PathBuf, drive: Drive, mut load: impl FnMut(&Path, Drive) -> Result<Option<Recovery>, NtfsError>) -> Result<(Snapshot, Option<NtfsIndex>), NtfsError>`; production `restore_once` delegates to it with `quarantine_recovery::load_optional`. Call the loader exactly once in the correct implementation; its repeatable FnMut seam makes a second-call mutation compile and increment the same counter. On `Some(recovery)`, build `Snapshot::new(path, Some((recovery.source, recovery.identity)))` and move `recovery.index` into the returned handoff; on `None`, `Snapshot::new(path, None)` and `None`. No clone, no second load, no metadata-only “probe”. Add the protected persistence helper used by T8: inside `clock.save(now, ...)`, call `Snapshot::save`, then `Snapshot::complete(id)`, then return `Ok`; `clock.saved()` is reachable only after complete success. A failed complete leaves the updated Snapshot context and records backoff. Keep this dormant; no worker wiring.
- **Tests:** call `restore_once_with` with a counted loader that invokes real `load_optional` exactly once, increments the count, then removes only the fixture canonical and backup files INSIDE the loader before returning `Recovery`. Assert count 1, the moved index/context, and that any later second load would fail against the removed files. Also cover canonical/backup handoffs, missing sources returning `(Snapshot, None)`, complete failure keeping quarantine/backoff, safe retry and no ordinary-writer fallback.
- **Red/check:** temporarily call the same injected `load` twice in `restore_once_with`, ignoring the second result; the retained count-1 assertion must fail at runtime (count 2), not compile. Every recovery read in this helper must use that injected loader; never add an uncounted direct `load_optional` probe. Restore and green. Deleting files after `restore_once` returns is not the proof. `cargo nextest run -p navo-index-service --no-fail-fast rebuild_session`; `cargo xtask check -p navo-index-service`.

### RB5 — captured journal high-water (≤145 production; tests extra)

- **Read/files (3):** `crates/navo-index/src/ntfs/windows.rs` (254), `crates/navo-index/src/ntfs/volume_io.rs` (381), new `crates/navo-index/src/ntfs/journal_position.rs`.
- **Change:** in `volume_io.rs` add exactly `#[path = "journal_position.rs"] pub(super) mod journal_position;`. In the new file add `pub(crate) fn current_position(io: &mut impl VolumeIo) -> Result<JournalPosition, NtfsError>`: `Ok(JournalState::parse(&io.query_journal()?)?.position)`. In `windows.rs` add `pub fn journal_position(&mut self) -> Result<JournalPosition, NtfsError>` delegating as `super::volume_io::journal_position::current_position(self)`; the `pub(super)` module and `pub(crate)` helper are visible to the Windows sibling. Reuse `JournalState::parse`; no parser copy, no `UsnBatch.next_usn` or saved-cursor substitute.
- **Tests:** a portable `VolumeIo` fake returns `test_support::journal_metadata(7)`; assert `{journal_id: 7, next_usn: 100}`; query errors propagate; no batch is read. The real Windows call remains UNTESTED until T15.
- **Red/check:** temporarily return a fabricated journal id or a batch cursor; quote the portable equality assertion failure; restore and green. `cargo nextest run -p navo-index --no-fail-fast journal_position`; `cargo xtask check -p navo-index`.

## Post-RB S11/S12/T8 contracts

- S11 uses `PrepareInput<'s, 'w> { shared: &'w Shared, snapshot: &'s mut Snapshot, drive: Drive, cancel: &'w CancelToken, permits: &'static Permits }`; `prepare(input) -> Result<PreparedRebuild<'s>, NtfsError>` borrows ONLY `Snapshot` for `'s`, and `PreparedRebuild` owns `BuildPermit<'static>`. Portable core takes `Permits` explicitly; the Windows wrapper passes `BUILDS`.
- Quarantine acknowledgement precedes the first checkpoint or conversion, including cold/no-proof. A failed consumed commit retains the updated `Snapshot` through the borrow.
- `catch_up` consumes `Enumeration` and rejects `enumeration.unresolved` plus every `BatchApply::finish` unresolved result; it returns a privately constructed `CaughtUp` carrying the index and high-water. `commit` consumes `CaughtUp`, never a bare index; verifies journal, high-water and snapshot, performs the final cancel check, then durable save/publish/complete/release with no cancellation inside commit.
- No `navo-index` parser change. T8 retains the `Snapshot` across reopen/retry, wires startup and ALL periodic/shutdown saves to protected persistence, and removes the ordinary production writer; no ordinary-save-then-header-refresh protocol. If T8’s measured 3-file/150-line budget cannot contain that activation, recut it into small wiring atoms; do not grow it. S14 must be green before T8; Windows T15 remains mandatory.

## Order, checks and done

DAG: RB0 → RB1 → RB2 → RB3 → RB4 → S11 → S12; RB5 is independent of RB0–RB4, may run in parallel from the same base, and must complete before S12. T8 is after S11/S12. RB0 requires S13 draft `42b9ac33` plus repair-plan PASS, not accepted S13; closing RB0 closes the original S13 portable-proof REQUEST CHANGES, and RB1 follows RB0.

Every RB atom runs its named nextest filter and `cargo xtask check -p <affected crate>` with the shared cargo lock, logs under `target/`, and quotes a meaningful assertion red before green. Integration runs the full gate once. Windows compile, flush and restart proof is S14/T15 and explicitly UNTESTED until run. Done when RB0–RB5 are green, RB0 has closed S13’s portable fault review, S11/S12/T8 remain blocked on those results, and the three documentation edits pass `git diff --check`.

## Log

- 2026-11-15 Europe/Bucharest — Plan created from the orchestrator’s RB0–RB5 contracts and review evidence. No product code, tests, cargo run, commit or deployment in this turn. S11/S12/T8 remain blocked; Windows UNTESTED.

- 2026-11-15 Europe/Bucharest — Closed review findings 1–5: prefix truncation→`BadLength`, explicit `#[path]` RB5 registration, counted one-load seam, RB0 fixture migration (RB1 adapter body only), and draft+plan-PASS RB0 with independent RB5. No code, tests or cargo.

- 2026-11-15 Europe/Bucharest — Closure review found the one-load proof seam must be repeatable. RB4 now specifies counted `FnMut`, called once normally; a second-call mutation compiles and fails count 1 at runtime.
