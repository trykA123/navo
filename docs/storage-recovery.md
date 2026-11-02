# Storage recovery

The quarantine, rebuild and promotion workflow is enabled by `--experimental-retention`. Without the flag, startup keeps legacy journal recovery and leaves a bad database in place; ordinary operations fall back to unlogged execution if the profile or journal is unavailable.

P35's storage APIs and startup workflow are implemented on `backend/beta-recovery-retention`. Startup preserves damaged databases, salvages verified data into a replacement, and keeps a recovery notice until the user dismisses it. Failures block normal database opens for that run and retain degraded mode. Native Windows verification remains pending.

## Startup ownership

Startup acquires `navo.startup-lock` before the existing `navo.lock` database-session lock. Only a sole session may check, quarantine or recover. Another live session permits ordinary database use only when no pending workflow or failed-check sentinel exists and the database is present.

The sole session checks integrity after an unclean exit or failed-check sentinel. SQLite `CORRUPT` and `NOTADB` errors trigger quarantine. Busy, I/O and future-schema errors stop startup recovery without quarantining files. Every application database connection retains the session lease until SQLite closes, including background settings connections.

Startup downgrades exclusive session ownership while still holding the startup lock. A failed downgrade retains startup ownership through the connection leases. A per-profile access gate blocks later opens after a failed preflight, even when writing `navo.db.check-failed` also fails.

## Quarantine

`quarantine_database(path)` preserves a closed database as `navo.db.corrupt-N`, followed by its `-wal` and `-shm` files. The caller must own the profile exclusively and close all SQLite connections first. The recovery lock serializes recovery callers; it does not lock ordinary SQLite connections.

The lowest completely unused slot is chosen. Strict native no-replace rename is preferred; hard-link publication remains the fallback when native rename is unavailable. Existing destinations are never replaced. This removes the hard-link requirement on supported native filesystems. Symbolic links and directories are refused.

The `.quarantine-pending` record stores the destination before the first move. A later call resumes that recorded slot. If both names exist and their non-following native identities match, recovery removes the duplicate original name and resumes. Different or unavailable identities preserve both names and return `AmbiguousMove` for manual attention; equal bytes alone do not prove ownership. An incomplete or malformed record also stops recovery. A completed move writes `.quarantine-receipt` before removing the pending record. Until `acknowledge_quarantine(path, slot)` clears that receipt, repeated calls return the preserved set and leave any replacement database untouched.

Orphan WAL/SHM files without a pending record receive their own unused slot. They are never attached to an older quarantined database. The returned `QuarantineReport` identifies which files actually exist in the preserved set.

Records are written and synced as sibling temporaries before no-clobber publication. An empty or interrupted unpublished temporary does not block another recovery attempt. Previously malformed final records still require inspection. The record is durable before moving files. This handles process interruption; directory entries are not explicitly synced for power-loss durability.

## Salvage

`salvage_database(quarantined_path, &fresh_db)` reads the preserved database through a read-only SQLite connection with checkpoint-on-close disabled. Its destination must be an empty current `Db`. Missing sources, newer database versions and access/I/O failures return errors.

Every table receives a per-table integrity check. The `job`, `job_step` and `undo` tables form one transaction: all three are copied or all three remain empty. Pending and restore-pending plans must deserialize as `OperationPlan`; journal states, step numbers and record types must also be readable. Destination foreign keys remain enabled during the copy.

Other tables recover independently: tags, module data, settings defaults, history, frecency, search history, viewer memory, command use and rename presets. Columns shared with older schemas are copied; current defaults fill newer columns. Each field must match the destination's expected storage type and nullability; primary keys must be non-null even where SQLite's non-STRICT schema permits nulls. Integer values are also accepted for real-valued fields. A failed read, type check or constraint rolls back that entire table group. Destination write failures stop salvage.

Schema V6 also adds the [retention catalog](retention.md). Salvage validates its versioned payloads, relative node paths, identities and state/target consistency. Owner foreign keys remain enabled: a rejected job group cannot authorize retention purge. Rejected catalog rows retain readable paths and raw records in `SalvageReport.retained`; the notice identifies manual restoration. Pre-V6 sources without the table recover an empty catalog without a reset warning.

`SalvageReport` contains copied row counts and reset reasons. For a rejected journal, it also carries readable job metadata, original plan bytes, held-file record bytes and any summary-read failures. Callers can decode these records for a manual-restoration notice. Recovery never executes a job or changes user files; the normal job engine operates only on the resulting trusted database.

SQLite's partial integrity check does not validate foreign keys or cross-table page ownership. It is combined here with journal decoding and destination constraints. The preserved files remain available for inspection. See the [SQLite integrity-check documentation](https://sqlite.org/pragma.html#pragma_integrity_check).

## Resumable replacement

The versioned `store-recovery.json` records Preserve, Build, Promote and Recover phases. Records have a 1 MiB limit and validated numeric identifiers. Malformed, unsupported or conflicting records stop recovery. File mutations use fixed filenames; symbolic links and nonregular files are refused.

Build creates a new `navo-rebuild-CASE-ATTEMPT.db` for every attempt. Interrupted candidates remain available. The candidate uses DELETE journaling, receives verified salvage, and is closed and synced before promotion. `navo-recovery-CASE-ATTEMPT.jsonl` stores the full salvage report and readable manual-recovery paths before promotion.

Promotion prefers strict native no-replace rename, with hard-link publication as fallback. Neither path replaces an existing file. The workflow records the candidate's volume, file identity and native/legacy identity domain. Old native records retain their representation. A retry recognizes its own interrupted promotion by identity; a different replacement file, even with identical bytes, stops recovery. Unexpected sidecars also prevent promotion. Successful native rename consumes the stage, so a replacement appearing at its former name is never cleaned up. Fallback and resumed cleanup recheck the remaining stage identity before removal.

Only the trusted replacement journal reaches normal job recovery. Persisted job identifiers retain outcome counts if interruption occurs after a SQLite commit but before notice persistence. Retry notices also compare a stable creation/kind/plan fingerprint, so a reused job identifier cannot resolve an older warning. Missing or pruned records remain marked for manual attention. Failed rollback or cleanup remains manual; later successful retries update the notice. Completed jobs are pruned only after recovery outcomes are durable.

The workflow commits the notice before acknowledging quarantine and clears the failed-check sentinel only after success. Completed receipts can therefore be cleaned up safely after an interrupted acknowledgement. These records support process interruption; directory entries are not explicitly synced for power-loss durability.

## Persistent notice

The existing crash card shows the preserved filename, recovered/reset data groups, verified operation outcomes and manual-recovery path hints. Unverified journal contents explicitly require inspection. Full details remain in the recovery folder. The newest three cases appear in the card; earlier cases remain recorded.

Open folder reveals the profile data directory without dismissing the notice. Ctrl+Shift+R focuses the card, Alt+O opens the folder, Enter saves a diagnostic report, and Escape dismisses it. Saving a report keeps the recovery notice visible. Dismissal atomically removes only the displayed cases, preserving newer notices and pending workflow state. A failed dismissal keeps the card and reports the error.

The notice survives clean restarts until explicitly dismissed. Damaged database files, interrupted candidates and detail reports remain available after dismissal. Recovery details are not automatically included in diagnostic ZIPs because they contain original paths and journal records.

## Verification boundary

Linux tests exercise real corrupted SQLite files, lease lifetime, interrupted phases, source/replacement identity checks, untrusted journals, failed rollback and persistent notices. Native Linux screenshots cover both themes at normal and minimum window widths. Native Windows sharing, interruption and notice behavior still require a Windows runner; no release sign-off is implied.

The ignored linkless-filesystem tests require an explicit isolated `NAVO_QUARANTINE_TEST_ROOT`. They verify that the selected filesystem rejects hard links before exercising quarantine or full recovery. Run the store `quarantine_succeeds_on_a_native_filesystem_without_hard_links` test and app `complete_recovery_supports_a_native_filesystem_without_hard_links` test with `-- --ignored`. Full recovery also requires a stable file identity. These manual filesystem cases have not run on this Linux host; Windows FAT identities remain conservatively unsupported as documented in the alpha identity report.
