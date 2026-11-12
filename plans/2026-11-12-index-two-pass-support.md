# Two-pass support, durability and proof cuts

Status: drafting · Repo: `/mnt/Sabrent/homelab/.crew/navo-api-builder-rtdsk` · Branch: `crew/api-builder-rtdsk` · Base: `origin/main@4a32112c3803b17b89cafc697f6868d8f82e4b67` · Started: 2026-11-12

Not in scope: enabling rebuild conversion independently, new product features, a new wire/snapshot format, or deployment.

## Goal

Supply small, green prerequisites and independent proofs for the [two-pass plan](2026-11-12-index-two-pass.md). These 15 atoms share its pending Owner question (full forecast 12–15 working days) and integrated MQ2 prerequisite `62fb3812`; every S atom transitively follows MQ2. Helpers land dormant until T8 activates the complete lifecycle. Fixture and safety preparations are independently shippable with existing behavior preserved.

## Decisions

TP1 and TP4 remain accepted owner direction. TP2 and TP9–TP12 in the parent plan define all contracts here; the superseded TP3/TP5–TP8 entries are history. No additional decision IDs or worker choices are introduced. In particular, S4 adds the missing durable primitive rather than assuming the ordinary writer is sufficient; S14 proves it before T8 activates conversion.

## Risks and rollback

Parent risks/rollback apply. A code-size cap is a stop, never permission to grow another atom. Preserve valid backup bytes until canonical repair is durable. Windows permission, sharing and flush failures must return an error before checkpoint/conversion; Linux tests cannot prove that boundary. All implementation/VM checks below are currently UNTESTED.

## Components

`I`, `S`, `F` mean `crates/navo-index/src`, `crates/navo-index-service/src`, `crates/navo-fs/src`. Parent component names retain the same ownership/contracts.

| Component | Layer | Contract (in → out) | New / change | Test |
| --- | --- | --- | --- | --- |
| `mft-fixture` | test | cursor → replayable bounded buffers | change | two complete reads |
| `full-build` | NTFS | folder scan + size helper → two-pass inputs | new/change | graph/count/size assertions |
| `search-generation` | lib | fixture construction → compatible private mode | preparation | unchanged sparse restore |
| `durable-replace` | filesystem | synced sibling → durable replace or error | new | syscall faults + Windows proof |
| `quarantine` | persistence | validated backup → repaired canonical, protected commit | new | backup identity through faults |
| `rebuild-session` | service | real Shared + permit → prepared/committed lifecycle | new/change | cancel/retry/ownership matrix |
| `peak-proof` | test | real deep build stages → four independent ratios | new | first/rebuild at 3 M/5 M |

## Atoms

Every row is one worker run, ≤3 files and ≤150 changed lines including tests. Lengths below are physical lines at the parent base, except MQ2 files explicitly marked. New siblings start at 0. Existing crowded modules receive only removals, wiring or changes to existing assertions; new implementation/tests go in siblings. Completion SHAs, lengths and exact read ranges for new helpers are recorded by the orchestrator before dependent dispatch. No step may leave compilation broken.

Use parent N/C/R commands, cargo lock, log redirection and actual exit-status checks. Every changed/new test first fails on an assertion, never compilation or zero matches. Tidy cuts use a temporary behavior mutation and restore it before the green run. Report the exact red assertion and final green line. Every Windows edit also runs T15; S14 is the earlier S4 platform prerequisite. All `senior / Sol` rows mean `gpt-6.1-sol` at xhigh.

| Atom | Component | Role / model | Effort | After | Check | Status | Cost |
| --- | --- | --- | --- | --- | --- | --- | --- |
| S1 | `mft-fixture` | api-builder / Sol | medium | MQ2 | N(navo-index, replay); C(navo-index) | blocked | — |
| S2 | `mft-fixture` | api-builder / Sol | medium | S1 | N(navo-index, blocked); C(navo-index) | blocked | — |
| S3 | `search-generation` | api-builder / Sol | medium | MQ2 | N(navo-index, restoring_a_sparse); C(navo-index) | blocked | — |
| S4 | `durable-replace` | senior / Sol | xhigh | MQ2 | N(navo-fs, durable_replace); C(navo-fs); S14 | blocked | — |
| S5 | `full-build` | senior / Sol | xhigh | MQ2 | N(navo-index, ntfs); C(navo-index) | blocked | — |
| S6 | `full-build` | senior / Sol | xhigh | T2, S5 | N(navo-index, directory_scan); C(navo-index) | blocked | — |
| S7 | `peak-proof` | senior / Sol | xhigh | T1, T5, S6 | N(navo-index, peak_harness); C(navo-index) | blocked | — |
| S8 | `peak-proof` | senior / Sol | xhigh | T3, T5, S7, S12 | R(two_pass_); C(navo-index) | blocked | — |
| S9 | `rebuild-session` | senior / Sol | xhigh | T7 | N(navo-index-service, persistence); C(navo-index-service) | blocked | — |
| S10 | `rebuild-session` | senior / Sol | xhigh | S9 | N(navo-index-service, batch); C(navo-index-service) | blocked | — |
| S11 | `rebuild-session` | senior / Sol | xhigh | T5, T7, S10, S13 | N(navo-index-service, rebuild_session); C(navo-index-service) | blocked | — |
| S12 | `rebuild-session` | senior / Sol | xhigh | T3, S11 | N(navo-index-service, rebuild_session); C(navo-index-service) | blocked | — |
| S13 | `quarantine` | senior / Sol | xhigh | T7 | N(navo-index-service, quarantine); C(navo-index-service) | blocked | — |
| S14 | `durable-replace` | tester / role default | xhigh | S4, T7, S13 | Windows durable_replace/quarantine tests; restart proof | blocked | — |
| S15 | `full-build` | senior / Sol | xhigh | S5, S6 | N(navo-index, journal_churn); C(navo-index) | blocked | — |

### S1 — Prepare the legacy FakeVolume for two traversals (≤100 lines)

- **Read/files:** `I/ntfs/tests.rs:64-202` (971), `I/ntfs/enumeration_replay.rs` (new, tests child). Replace the existing buffers/queries field types with sibling-owned ReplayBuffers/ReplayQueries wrappers; preserve existing .into()/queue methods. Retain original bounded buffers, reset on cursor 0, honor header cursors, and repeat the last journal metadata after queued responses are consumed. Add no implementation body or new fields to the crowded tests.rs. Delegate from the existing FakeVolume methods to the sibling; preserve its current one-pass tests. T3 alone changes production cursor/blocked expectations.
- **Red/check:** two direct complete enumerations must return identical records, cursors and EOFs. Today the second request exhausts pop_front; make the test compare an Option/result before any unwrap so red is an equality assertion, not a panic. Run replay and existing ntfs tests, C(navo-index). Stop if buffers or journal metadata must be guessed.

### S2 — Separate MQ2 queue storage from full-build expectations (≤110 lines)

- **Read/files:** MQ2 `I/ntfs/blocked_peak_tests.rs:1-184` (184), `I/ntfs/blocked_queue_tests.rs` (new, child). Make BlockedMft cursor-replayable. Move the ≤48 B/record queue assertion and its approximately 40-line helper to the sibling; retain journal ordering coverage. Remove the old full-build exact blocked-queue count/per-blocked metric, retaining entry/depth/size assertions. T3's new production assertion requires blocked_peak=0. This is preparation, not the two-pass proof.
- **Red/check:** temporarily corrupt a returned file size; quote the existing size assertion failure, restore it and perform the move. Run N(navo-index, blocked), queue/order tests and C(navo-index); explicitly require the moved storage test is matched. Stop above 110 changed lines.

### S3 — Remove the compact-restore struct-literal dependency (≤20 lines)

- **Read/files:** `I/ntfs/compact_restore_tests.rs:12-40` (40). Construct with existing `NtfsIndex::new('C', POSITION)` then assign the original table and ids; preserve the deliberately sparse table and rename defaults. Do not restore early, since restore compacts. T5 can then add its private flag in its three files without breaking this fixture.
- **Red/check:** temporarily change the expected sparse listing and record its equality assertion failure; restore, change construction only, run N(navo-index, restoring_a_sparse), C(navo-index). Stop if sparse allocation/compaction assertions change.

### S4 — Add a strict durable replacement primitive (≤150 lines; prerequisite)

- **Read/files:** `F/lib.rs:1-70` (70), `F/durable_replace.rs` (new), `F/durable_replace_tests.rs` (new). Read-only: `F/local/win_replace.rs:45-78` (78), `F/local/win_volume.rs:18-64` (94). Add/export `native_replace_durable(&Path, &Path) -> io::Result<()>`; ordinary native_replace keeps its current semantics. Input is a synced create-new sibling on the same local filesystem, with no symlinks/reparse redirection. Reject unsupported/cross-volume destinations; errors never report durability.
- **Change:** Windows resolves the target's actual containing volume GUID using GetVolumePathNameW/GetVolumeNameForVolumeMountPointW, opens its volume handle with write access and sharing, then performs MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH) without fs::rename fallback and FlushFileBuffers on that volume. Acquire the flush-capable handle before replacement; close through RAII. Unix uses rename plus parent sync. Volume/namespace checks and failed flush return errors, including ambiguous post-replace errors. Do not infer durability from a directory no-op or the fallback. This uses existing windows-sys features; no dependency change.
- **Red/check:** injected operation tests first use ordinary fallback/no-op sync and fail assertions for replacement-error propagation, zero fallback calls, and completed volume flush before Ok. Cover denied handle, sharing violation, wrong volume and post-replace flush failure. Run N(navo-fs, durable_replace), C(navo-fs); S14 must run the actual Windows path before activation. Stop if the primitive/tests exceed the cap or require a new platform assumption.

### S5 — Extract the existing file-size rule (≤90 lines)

- **Read/files:** `I/ntfs/volume_io.rs:154-196` (407), `I/ntfs/two_pass_size.rs` (new). Move only the current file_size/current_file_size/FileGone/unknown logic into a helper returning skipped or known/unknown bytes. Existing add_record delegates unchanged; no pass behavior changes. Register the sibling under volume_io, reducing that crowded file.
- **Red/check:** temporarily return a wrong known size and capture an existing ntfs size assertion failure; restore, extract, run N(navo-index, ntfs), C(navo-index). Stop if size I/O frequency or fallback behavior changes.

### S6 — Add the dormant folder-only scan adapter (≤145 lines)

- **Read/files:** `I/ntfs/volume_io.rs:20-65` (407 before S5), `I/ntfs/directory_scan.rs` (new), `I/ntfs/directory_scan_tests.rs` (new child). Read T2's graph at its completion SHA. Consume a genuine cursor-zero traversal; retain only directories and scalar file/name counts. Return resolved directories plus capacity totals and original journal cursor. Validate limits, cancellation and continuity; never retain file records. Keep it disconnected until T3.
- **Red/check:** a no-op scan fails folder order and file/name count assertions. Cover deep/reverse order, sequenced root parents, missing/cyclic/new parents, logical limit and cancellation; assert file-size calls remain zero. Run N(navo-index, directory_scan), C(navo-index). Stop before 145 lines.

### S7 — Prepare an allocation-accounted rebuild harness (≤145 lines)

- **Read/files:** `I/ntfs/peak_tests.rs:50-79` (379), `I/ntfs/two_pass_peak.rs` (new child). Read `I/heap_probe.rs:80-114` (114), T1/S6 helpers and the preserved `plans/evidence/2026-11-12-index-peak-probe/checker_peak_probe.rs` cited in the parent. Provide reusable functions only: real deep old NtfsIndex, Probe before old allocation, mark after old allocation but before checkpoint work, streamed old checkpoint and conversion, candidate through the existing production enumerate_volume_sized adapter (two reads after T3), name queries, replay, streamed v3 save, old drop and trim. Do not reset peak at conversion, since that would exclude checkpoint work. No fabricated old byte vector or excluded lifecycle stage.
- **Red/check:** force a retained allocation through the harness and first fail the exact accounted-byte assertion; restore it and prove measured delta matches a drop of the real old index. Run N(navo-index, peak_harness), C(navo-index). Keep scale cases for S8; stop above 145 lines.

### S8 — Prove deep first-build and rebuild gates separately (≤140 lines)

- **Read/files:** `I/ntfs/two_pass_peak.rs` (S7 ≤140), `I/ntfs/two_pass_scale_tests.rs` (new child). Use S7 and T1's four-deep fixture: 10% folders and 30% early files. Register four ignored tests: `two_pass_first_peak_3m`, `two_pass_first_peak_5m`, `two_pass_peak_3m`, `two_pass_peak_5m`. Each asserts file/folder counts, nested paths, exactly two EOF traversals, expected pass calls and only pass-2 size reads, plus its own numerical ≤1.7 ratio. First-build denominator is final retained bytes; rebuild denominator is full-old resting bytes measured before conversion.
- **Red/check:** MEASURE the unmodified deep MQ2 baseline first at both sizes for first build and full-old rebuild; do not assume any ratio exceeds 1.7. S7/S15 land before T3: record that exact pre-T3 SHA containing MQ2 and identical fixture/harness/mode APIs. Apply only this atom's two test-file changes to an isolated copy of that SHA, run each numerical assertion BEFORE traversal assertions, and record peak, actual denominator, ratio and exit even if it passes. The rebuild baseline retains the real full old generation. MQ2's separate 1.61/1.69 fixture results cannot substitute for these measurements.
- **Controlled red if a numerical baseline passes:** for that case only, record a temporary harness mutation allocating and touching a `Vec<u8>` of `2 * denominator_bytes` while the measured build/save interval remains active; retain it through the peak read and use `std::hint::black_box` so optimization cannot remove it. Keep the real denominator unchanged (for first build use the clean baseline's final retained bytes). Re-run the SAME ≤1.7 assertion before traversal checks and quote its numerical failure, not a structural/compile error. Record the mutation patch and clean baseline separately; remove the injected allocation completely and prove green on the candidate. Never describe a passing MQ2 baseline as a production memory failure.
- Run each named R(test) separately; print raw bytes/MiB, denominator, pass times/calls/bytes and size reads. Record four clean baseline measurements, each natural or controlled numerical red, mutation removal and four candidate greens. Compare ownership against S11/S12; T15 covers actual service memory/durability. All are future UNTESTED checks; no baseline measurement is claimed by this planning rework. Stop if a stage or case is absent.

### S9 — Extract snapshot-save plumbing and test that helper (≤145 lines)

- **Read/files:** `S/worker.rs:262-305` (324), `S/lib.rs:50-95` (264), `S/rebuild_session.rs` (new, inline tests). Read-only `S/persistence_tests.rs:28-42,44-75` (161): its local save/header helper bypasses worker plumbing and is NOT evidence for this extraction. Register the sibling for Windows and tests. Budget: at most 40 moved lines (80 deletion/addition), 20 wiring/docs, 45 direct test lines = 145 changed lines.
- **Change:** extract `save_snapshot(input: SaveInput<'_>, clock: &mut SaveClock) -> Result<Option<usize>, file::FileError>`; SaveInput holds shared, Drive, path, elapsed Duration, stopping and built_at seconds since epoch: select the published index/snapshot, apply SaveClock due, build the production header and stream-save; return saved entry count or None when not due/no snapshot. Keep deferred-rebuild guard, SystemTime acquisition, logging and error conversion in worker; it delegates to this helper. No quarantine/conversion yet. S10 preserves these direct helper tests when adding ownership helpers.
- **Red/check:** new inline tests call save_snapshot DIRECTLY with real Shared, Drive, SaveClock and a temp path, then load_with_ids and assert serial, journal ID, next_usn, root, built_at and entry count. Temporarily corrupt next_usn inside the extracted helper; quote this direct test's equality failure, restore it, then run green. Check not-due/no-snapshot skips and save-error backoff through that same helper. Existing persistence tests run as regressions only. Run N(navo-index-service, rebuild_session), persistence tests and C(navo-index-service). T15 proves Windows; stop above 145 changed lines.

### S10 — Extract only hint/swap ownership plumbing (≤130 lines)

- **Read/files:** `S/worker.rs:173-183,248-260` (324 before S9), `S/rebuild_session.rs` (S9 ≤100 including inline tests), `S/rebuild_session_tests.rs` (new child). Move hint and swap/drop/trim into the existing sibling, preserving behavior and explicit Shared ownership. Do not move follow/build loops or activate conversion. Register tests below that sibling, not lib.rs.
- **Red/check:** temporarily skip the swap and fail the published new-hit assertion; restore, extract, then run rebuild_session, batch/persistence tests and C(navo-index-service). Stop if more than 35 lines must move or state ownership changes.

### S11 — Prepare permit, quarantine and conversion (≤145 lines)

- **Read/files:** `S/rebuild_session.rs` (after S10 ≤145 including inline tests), `S/rebuild_prepare.rs` (new child), `S/rebuild_prepare_tests.rs` (new child). Read `S/build_permit.rs:20-72` (167), T7 and S13 completed contracts. Implement dormant prepare: wait without shared locks, establish TP10 active marker, repair/checkpoint safely, capture hint, then Arc::get_mut conversion under write lock. Return a session owning the permit; on any error preserve the published old generation and validated disk copy. Queued/cancelled volumes never degrade.
- **Red/check:** fail old-hit, disk-copy and full-queued-capacity assertions before the guards; cover queued cancel, marker/checkpoint failure, held Arc (no cloning), repeat retry and two-volume exclusion. Run rebuild_session tests and C(navo-index-service). No worker wiring before T8/S14; stop at 145 lines.

### S12 — Prepare catch-up, commit and cancellation (≤145 lines)

- **Read/files:** `S/rebuild_session.rs` (after S11 ≤155 including inline tests), `S/rebuild_commit.rs` (new child), `S/rebuild_commit_tests.rs` (new child). Read `S/batch.rs:13-75` (75), S11/S13 contracts. Session owns permit through high-water catch-up, snapshot/no-unresolved validation, final cancel check, durable candidate save, swap/drop/trim, complete marker and release. No cancellation inside commit. Failed cleanup stays quarantined; old search or the committed generation remains published. Shutdown refuses search-only saves.
- **Red/check:** first fail old-hit/disk-copy assertions for cancellation during pass 1, pass 2, catch-up and immediately before commit; test unresolved finish despite advanced cursor, reset/wrap, save failure, restart-after-save-before-swap, two failed retries and marker cleanup failure. Require second-volume start after promotion. Run rebuild_session, persistence and build_permit tests, C(navo-index-service). Keep Windows activation in T8/T15; stop at 145 lines.

### S13 — Repair backup-only recovery and protect snapshot replacement (≤150 lines)

- **Read/files:** `S/quarantine.rs` (T7 ≤100), `S/quarantine_snapshot.rs` (new child), `S/quarantine_snapshot_tests.rs` (new child). Read `I/file.rs:49-83,115-160` (246), `crates/navo-store/src/atomic_write/mod.rs:160-188` (225). Implement TP10 repair using a create-new repair sibling and bounded byte copy of the already validated backup; sync and S4-replace canonical, leaving `.bak` untouched until durable success. Use only one loaded old table; no second full snapshot load.
- **Change:** for checkpoint/promotion, save v3 through the existing streamed writer to a fresh staging path, prepare/replace backup only from a validated canonical, then S4-replace canonical. Refuse any path that would invoke ordinary canonical backup rotation over corrupt canonical. Sync prepared copies, bind complete marker only to canonical header/checksum (never backup replay permission), and retain the last valid copy across ambiguous errors. Clean only owned staging files after inspecting them; retries do not accumulate complete snapshots indefinitely.
- **Red/check:** start corrupt canonical + sole valid `.bak`; inject failure before/after repair rename and flush, then a candidate replace failure. First fail an assertion that the backup checksum/bytes remain identical under the ordinary writer sequence; fix and restart after every boundary, asserting one matching searchable snapshot survives. Also cover promotion that rotates an unsafe checkpoint to backup, successful marker unlink, subsequent canonical corruption and checksum-valid backup fallback: assert search-only/no replay and active quarantine before repair. Neither a missing marker nor the prior canonical complete marker certifies that backup; no independent backup replay-safety proof exists in this plan. Cover successful repair before rotation and no second old allocation. Run N(navo-index-service, quarantine), C(navo-index-service); S14/T15 prove Windows. Stop above 150 lines or if protected-copy logic needs another component.

### S14 — Prove durable Windows boundaries before activation

- **Read:** parent VM setup/restore procedure and S4/T7/S13 tests. **Files:** none; orchestrator records commands, SHA, exits and final lines in the parent Log. Reserve/checkpoint the isolated VM; run Windows durable_replace/quarantine tests as the actual LocalSystem service identity. Confirm the storage volume for ProgramData separately from indexed T:/U:.
- **Red/check:** retain the injected ordinary-fallback/no-flush assertion failure from S4 and corrupt-canonical sequence from S13. Exercise real held-open target, denied volume access, strict replacement, successful actual volume flush and restart after marker-before-checkpoint; forced VM power-off after acknowledged marker must recover quarantined. Require failed marker/repair operations never advance to a subsequent checkpoint or conversion, and failed candidate durability never reaches promotion. No absent/torn marker may permit unsafe replay. Restore the checkpoint, stop test processes. Stop and mark UNTESTED if rights, sharing, volume flush or restart cannot be exercised; T8 remains blocked.

### S15 — Reject hard-link journal changes and prove cross-pass churn (≤145 lines)

- **Read/files:** `I/ntfs/volume_io.rs:183-193,203-240` (407 before S5), `I/ntfs/journal_churn_tests.rs` (new child). Read-only `I/usn/mod.rs:78-145` (308) shows the attributes-only mapping. Inspect raw parsed records for reason bit 0x10000 IMMEDIATELY after parse_batch and BEFORE refresh_sizes, cursor validation or into_changes; return Reenumerate on any occurrence, including combined reason flags. Preserve one selected enumerated name per FRN and MQ2 journal ordering. This guard covers normal follow and candidate catch-up, without a new wire/type policy.
- **Red/check:** first fail the assertion that a hard-link-only journal record requires Reenumerate. Add a combined CREATE (`0x100`) | HARD_LINK_CHANGE (`0x10000`) record with a size provider scripted to return FileGone: assert Reenumerate and zero current_file_size calls, proving rejection before refresh_sizes could overwrite its reason as DELETE. Then exercise creates/removes, paired renames, directory moves, and sequenced root parents injected during pass 1, between passes and during pass 2 through the dormant S6 scan plus existing journal adapter. Before T3 activation use a real two-read fixture driver, not two cloned result lists; T3 reuses these scripts through production enumeration. Require reconciled paths/totals or explicit Reenumerate for invalidated parents, never partial success. Run N(navo-index, journal_churn), existing journal ordering tests, C(navo-index); T15 checks Windows. Stop above 145 lines.

## Done when

Each support atom records its assertion-red and green evidence; all four S8 numerical gates pass independently. S14 proves Windows durability before T8 activation, and T15 completes service/VM/full-gate/review proof. Parent acceptance criteria remain mandatory; this companion does not waive a failed memory/time gate or a missing platform run.

## Log

- 2026-11-12 Europe/Bucharest — Created during review-1 rework. Split 15 support atoms from T3/T5/T7/T8/T15 to keep each plan at 15 runnable atoms. No implementation or VM check ran; every S atom remains blocked on MQ2 and its listed dependencies. No product edits, commits or deployment.

- 2026-11-12 18:43 Europe/Bucharest — Review 2 findings 1–4 updated in S13/T7, S15, S9 and S8 respectively; parent Log names each closure. Finding 5 remains an owner choice; no scope removed. Updated MQ2/base and S9/S10 successor read-length estimates; all senior rows remain Sol xhigh. Runtime/platform gates remain UNTESTED.
