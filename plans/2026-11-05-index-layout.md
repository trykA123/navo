# Index layout: a smaller resting index

Status: done · Repo: `navo` · Branch: `index-layout` from `main` · Started: 2026-11-05

## Goal

The index service holds about 100 MiB per 2 million files at rest instead of about 160 MiB, back toward D11's 80–100 MB. Search stays as fast as today, every peak stays within D109, and snapshots stay format v3: older and newer builds read each other's files with the same meaning.

Sources: D11, D83, D103, D109; `plans/2026-11-04-index-ram.md` (RD1–RD12, shipped at `33a59962`); the two RAM audits of 2026-11-04. Code read at `a3ba33aa`. Plan decisions are `LD1`–`LD11`.

## What exists today (paths under `crates/navo-index/src/`)

- **Table** (`table/mod.rs:38-47`): `names: String` (NUL-terminated, append-only), `offsets`, `parents: Vec<u32>`, `flags: Vec<u16>` (5 bits used), `children` (`table/children.rs:10-15`: `first`, `next`, `previous: Vec<u32>` per slot, `waiting`), `bytes: Vec<u64>`, `counts: Vec<(u32, [u32; 2])>` (rows for entries pushed as folders, sorted by id).
- **Flag changes on a live id:** `TableChange::Add` on a known reference, `Rename` and `Attributes` overwrite `flags[id]` (`table/change.rs:55-80`); a file can become a folder and back. Today every slot has a `first` link, so any id can hold children. `valid_structure` (`table/mod.rs:149`) rejects a saved file whose live entry has a non-folder parent.
- **File-reference map** (`usn/frn_map.rs`): `HashMap<u64, u32>` plus `HashMap<u128, u32>`; 35.7 B per entry at 1 M or 2 M entries, 28.5 B at 5 M.
- **Search** (`search.rs:32-150`, rayon `into_par_iter`) reads `names`, `offsets`, `flags`, `parents`. `Term::Ext` takes `&[String]`.
- **Snapshot v3** (`file.rs:105-145`, `file/stream.rs:30-55`): header, root, arena, per slot offsets u32, parents u32, flags u16, sizes u64, counts 2 × u32, then (u128, u32) pairs in `FrnMap::iter` order (hash order today, so a file with several narrow references has no fixed byte order). L0 = `file/tests.rs:562 format_v3_bytes_are_unchanged` (one narrow and one wide reference, no shared names).
- **Measurement**: `heap_probe.rs` (`Probe`, `fail_allocations_from`), `ntfs/peak_tests.rs` (scenarios, `scale_report`), `ntfs/memory_tests.rs` (`TABLE_BUDGET = 51`, `FILE_ID_BUDGET = 40`). `Probe.peak` counts from the probe start, so a build or load probe already includes the new index.

Steady bytes per entry, **exact-fit model** (18-byte names with NUL, 10 % folders, every column exactly full, map pages fully used):

| Column | Today | Planned | How |
| --- | --- | --- | --- |
| name arena | 18 | 18 (≈ 13.5 with dedup, if gated in) | LD6 |
| offsets, parents | 8 | 8 | — |
| flags | 2 | 1 | LD4 |
| child heads | 4 | 0.8 | LD3 |
| `next` + `previous` | 8 | 8 | LD3 |
| sizes, folder counts | 9.2 | 9.2 | LD5 |
| reference map, live pages | 35.7 | 6.0 (record span 1.0), 7.5 (span 1.25) | LD2 |
| **Total** | **84.9 B ≈ 162 MiB / 2 M** | **51.0–52.5 B ≈ 97–100 MiB / 2 M** | |

Not in the model, reported apart by M1, M2 and G1: map directory (`max_record / 1024 × size_of::<Option<Page>>()`), the last partial page, record holes inside allocated pages (span 1.25 is record spread, not page fill), `overflow` and `wide` buckets, `waiting`, pending renames, growth slack, and service-side state (pipe, search scratch). The VM step measures the whole service.

## Shape

- **Appetite:** three days of agent runs, 17 atoms plus 5 gated dedup atoms. The overshoot of the 15-atom guide comes from guard atoms that pin today's behaviour first. Work not done by then stops.
- **Non-goals:** freeing the old index during a rebuild (RD1 stays); search scratch and pipe buffers; the walker's frontier; a dead-name-bytes compaction trigger (R7 measures the cost); sizes or counts in fewer bytes; a snapshot format change (no v4 in this plan, see LD7).
- **No-gos:** a v3 file that an older or newer build reads differently or refuses; search slower than LD10 allows.

## Decisions

Final. Workers never reopen them; a worker that finds one wrong stops and reports.

- LD1 — **Measure first.** Steady = heap the `NtfsIndex` holds after `load_with_ids` + `NtfsIndex::restore` of a saved snapshot, read by `held()`. Fixture `steady_index(entries, record_gap)`: root; every 10th entry a folder under the root; other entries files under the latest folder; names `entry-{n:07}.txt` (18 B with NUL); reference of the root (entry 0) = `5`, as `NtfsIndex::restore` requires; of entry `n ≥ 1` = `100 + n + n / record_gap` (gap 0 = none; gap 4 = span 1.25).
- LD2 — **Reference map: paged dense array by MFT record, exact keys.** Narrow reference `r` (fits `u64`): `record = r & 0xFFFF_FFFF_FFFF`, `seq = (r >> 48) as u16`. `DensePages` (`usn/frn_pages.rs`): `PAGE = 1024`; directory `Vec<Option<Page>>` by `record / PAGE`; `Page { ids: Box<[u32]>, seqs: Box<[u16]>, live: u16 }`; `EMPTY = u32::MAX`.
  - **Partition invariant:** every key lives in exactly one of `dense`, `overflow: HashMap<u64, u32>`, `wide: HashMap<u128, u32>`. Wide keys always go to `wide`.
  - **Narrow insert order** (existing keys first, so an update never allocates and never duplicates):
    1. `overflow` holds the key: update it there.
    2. `dense` holds `(record, seq)` and `id != EMPTY`: update the slot in place.
    3. `dense` holds `(record, seq)` and `id == EMPTY` (an `EMPTY` slot reads as absent): migrate. `overflow.try_reserve(1)`, then remove the dense slot, then insert into `overflow`.
    4. New key: `overflow` when `id == EMPTY`, `record >= 1 << 32`, the dense slot holds another `seq`, or the budget refuses; otherwise `dense`.
  - **Budget:** a new page, plus directory growth to reach it, is allocated only while `dense bytes after ≤ 24 * (dense_len + 1) + 256 KiB`.
  - **Staged fallible insert, capacity-exact:** each path has one fallible step, and nothing in the map changes before it. Migrate and new-`overflow` paths: `overflow.try_reserve(1)`. New-dense path in a missing page: build the page as a local value (`ids`, `seqs` through `try_reserve_exact`, then boxed), then `directory.try_reserve_exact` as the last fallible step; on `Err` the local page drops. After that step, `resize` within capacity and the writes cannot allocate. So any `Err` leaves `len`, `get`, `iter` and `memory_parts` exactly as before.
  - **Aborting `insert`:** calls the fallible path; on `Err` it migrates (step 3) or inserts the new key into `overflow` with std's aborting growth (today's behaviour). A key is never in two parts.
  - **Removal keeps the invariant:** removing a dense key empties its slot and frees the page at `live == 0`; an `overflow` key with the same record stays in `overflow` (never moved back), `get` still finds it, and a later new `seq` may take the empty dense slot.
  - `get`: dense hit only when the slot is not `EMPTY` and `seq` matches, else `overflow`, else `wide`. `retain` and `remap` work in place; a `remap` result of `EMPTY` on a dense entry moves that pair to `overflow` (table ids never reach `u32::MAX`, since `push` refuses `NO_PARENT`). `iter` yields dense in record order, then `overflow`, then `wide`. `try_reserve` and `with_capacity` reserve nothing (a dense map is not sized by a count; the service's `with_capacity(hint) <= old` test stays true). `shrink_to_fit` drops trailing empty directory slots and shrinks both maps. `PartialEq` compares pair sets. `memory_bytes` = directory capacity × `size_of::<Option<Page>>()` + pages × 6 KiB + both hash tables; `memory_parts()` (test-only) returns those three apart.
  - Lost: a sorted `Vec<(u64, u32)>` (12 B, O(n) inserts under the write lock); a static perfect hash (≈ 12 B with overlay); table id = record number (27 B per free record).
- LD3 — **Child heads only where needed.** `Children.first` becomes `heads: Vec<(u32, u32)>` (parent id, first child), sorted by id; `first_child`, `of`, `attach`, `detach` find the row by binary search.
  - A row is created (a) at `push` of an entry with `FOLDER` (appended: ids grow, so no shift), and (b) on `attach` to a live parent id below `slots` that has no row, whatever its flags (a file that became a folder, a child under an id pushed as a file); (b) inserts at the sorted place (O(rows) shift, rare) with the same aborting growth as every column today.
  - Flag changes (`Add` on a known id, `Rename`, `Attributes`) never add or drop rows. Rows leave only through compaction or rebuild. So `of(id)`, `remove` and totals behave as today for every flag history.
  - `waiting` holds only children of parents at or beyond `slots`, as today. `Children` keeps its own `slots: u32`, the boundary that `first.len()` gave.
  - `add(id, parents, folder: bool)`; `rebuild(parents, is_live, is_folder)` makes rows for every live folder and every live id that parents a live child, in id order, through a bit set (`slots / 8` B scratch); `try_reserve_total(entries, folders)`; `remapped` keeps its signature and maps kept rows in order. `next`, `previous` and sibling order stay.
  - Cost: 8 B per row instead of 4 B per slot. Lost: a parent-sorted child array (O(siblings) removals, move-back duplicates); singly linked lists (O(siblings) removals).
- LD4 — **Flags column in one byte.** `Flags(u16)` and `Flags::bits() -> u16` stay public; the column stores `StoredFlags` (alias in `table/flags.rs`, `u16` until F1c, then `u8`). Column code never mixes the column with `bits()`: it uses `Flags::stored()`, `Flags::DELETED: StoredFlags` and `Flags::VALID: StoredFlags` (F1a, F1b), so F1c only changes the alias and the disk edges. Disk stays u16: `stream.rs` writes `u16::from(flag)`; `file.rs` decodes `StoredFlags::try_from(u16::from_le_bytes(b)).unwrap_or(StoredFlags::MAX)`, so unknown high bits still fail `from_raw_parts`.
- LD5 — **Sizes and counts stay.** Lost: 48-bit sizes (a chosen network folder (D103) can exceed 256 TiB).
- LD6 — **Names: dedup during builds and compaction, gated.** Offsets (u32) and the NUL arena stay. N1b–N2 run only if M3 shows a saving ≥ 15 % of live name bytes on real names, and N3 keeps search within LD10. `Interner` (`table/interner.rs`) = `hashbrown::HashTable<u32>` of arena offsets with a `RandomState`; it lives between `begin_interning` and `end_interning`; compaction uses a local one. While the interner lives, `append_name` (push and rename) reuses a found offset, which is safe because arena bytes never change; outside it every name appends and the next compaction folds them. `compact` drops a live interner first (N2), since its offsets name the old arena. An allocation failure turns interning off with a warning. `live_name_bytes` counts each distinct live offset once (N1a). Split: N1a accounting, N1b the standalone `Interner`, N1c the table wiring. Lost: front coding; offset checkpoints (−3.75 B, conflicts with dedup); 24-bit offsets.
- LD7 — **Snapshot compatibility = v3 wire semantics plus cross-version reads.**
  - Wire semantics: version field 3, the v3 layout, and every value decodes to the same meaning: per slot live or deleted, name, parent, flags bits, size, folder counts; the reference set as (u128, u32) pairs. Byte identity is required only for L0's fixture (single narrow reference, unique names), which every atom keeps passing unedited. Reference order and arena layout may differ (P2 order, N2 shared offsets), since v3 readers rebuild the map and accept shared offsets.
  - Cross-version reads (L1): one rich fixture with **shared offsets** (two live entries in different folders on one arena offset; the table API always appends, so `rich()` rewrites `raw_parts` and reloads through `from_raw_parts`), several narrow references per folder, two references to one id (a root alias), two references on one record with different `seq`, records ≥ 2^32, wide references, deleted files, a deleted folder with a child, a rename (dead arena bytes), sizes, folder totals. `RICH_V3_HEX`, written once on `main`, must load on every later build with equal semantics. Two ignored tests (`compat_write`, `compat_read`) let cross-check X write a file with one build and read it with the other, both ways; the base worktree has them because S0 lands after L1.
  - **No v4 in this plan.** No atom needs a format change, so the format stays v3 and there is no promotion step. A later format change gets its own plan, which must first answer the downgrade case from plan check 2: an older build rebuilds or updates v3 while an older v4 file still exists. Its rule must name a freshness order both builds agree on before either file is deleted.
- LD8 — **Sparse trigger at 1.5×.** `pub fn is_sparse(&self, min_slots: u32) -> bool` on `NameTable` (`table/compact.rs`): `slots >= min_slots && slots as usize > len + len / 2`. `NtfsIndex::compact_if_sparse` and the host's `compact_if_sparse` (`host/scan.rs:262`) call it. Bound: per-slot columns stay ≤ 1.5× their compact size. The name arena is not bounded by slots: renames append and only compaction folds dead bytes, as today (R7). Bad: one O(slots) compaction per `len / 2` deletions instead of per `len`.
- LD9 — **Peak reading (D109, Q1 answered 2026-11-05).** Constants in `ntfs/peak_ratio_tests.rs`: `PEAK_CEILING = 1.7`, `REBUILD_CEILING = 2.2`. All peaks come from a `Probe` started before the scenario; `Probe::mark()` resets the peak to the bytes held at that moment and returns them as the base.
  - Build or load from nothing: `peak / held_after` ≤ `PEAK_CEILING` (the peak includes the new index).
  - Compaction: `peak / base`, base = the sparse index just before it starts, ≤ `PEAK_CEILING`.
  - Push after load: `peak / base`, base = the loaded index, ≤ `PEAK_CEILING`.
  - Rebuild with the old index still searchable (RD1): `peak / base`, base = the old index at rest, ≤ `REBUILD_CEILING`.
  - 2026-11-05 owner: 2.2× applies to the sized rebuild; the unsized fallback after a failed reserve may peak at 2.5×.
  - 2026-11-05 owner: the first load of a snapshot an older build left sparse may peak at 2.5× (`SPARSE_LOAD_CEILING`, measured 2.39×; once per upgrade). A sized rebuild of a volume that grew past the hint may peak at 2.6× (`GROWN_REBUILD_CEILING`, measured 2.50×).
- LD10 — **Search gate.** Baseline = a detached worktree at `S0_SHA` (the commit that lands S0, after M3 and L1, before any layout change): `git -C <repo> worktree add --detach <wt>/../navo-layout-base <S0_SHA>`, own `CARGO_TARGET_DIR`. Run `search_scale_timing` (release, `RAYON_NUM_THREADS=4`, no other cargo job: the `flock` holds it) in 5 rounds, each base then branch. Per query: ratio = branch median ÷ base median per round. Pass: the median of the 5 ratios ≤ 1.05. Noise rule: if the base medians' max ÷ min > 1.10 for any query, the gate is inconclusive; rerun 5 rounds once; still noisy: stop and report (the orchestrator moves the gate to the VM). The service `timing_tests` keep passing.
- LD11 — **Check form.** `<wt>` = the atom's worktree. `$P` is text the worker writes out in full, never a shell variable (zsh does not split an unquoted variable, and `~` inside a variable never expands): `flock "$HOME/.cache/rusty-cargo.lock" nice -n 10 env CARGO_BUILD_JOBS=8 CARGO_TARGET_DIR=<wt>/target`. Each check is: `cd <wt> && $P cargo nextest run -p <crate> --no-tests=fail -E '<filterset>' > <wt>/../<atom>.log 2>&1; echo $?`, then `cd <wt> && $P cargo xtask check -p <crate> >> <wt>/../<atom>.log 2>&1; echo $?`, then `grep -E 'Starting [1-9]|Summary' <wt>/../<atom>.log` and `tail -n 40 <wt>/../<atom>.log`. Pass = both exit 0 and `Starting` shows at least one test. Reports add `--cargo-profile release --run-ignored only --no-capture`. No atom edits Windows-only files (`worker.rs`, `ntfs/windows.rs`).
  - **Cross-check X** (after P2, F1c, N2, G1): `cd <wt> && NAVO_COMPAT_OUT=<wt>/../x-new.index $P cargo nextest run -p navo-index --no-tests=fail --run-ignored only -E 'test(=file::compat_tests::compat_write)' > <wt>/../X.log 2>&1; echo $?`; then in the base worktree `cd <wt>/../navo-layout-base && NAVO_COMPAT_IN=<wt>/../x-new.index flock "$HOME/.cache/rusty-cargo.lock" nice -n 10 env CARGO_BUILD_JOBS=8 CARGO_TARGET_DIR=<wt>/../navo-layout-base/target cargo nextest run -p navo-index --no-tests=fail --run-ignored only -E 'test(=file::compat_tests::compat_read)' >> <wt>/../X.log 2>&1; echo $?`; then the same two with the worktrees swapped (`x-old.index`). Pass = four exit codes 0.
  - **VM commands** (orchestrator): `/mnt/Sabrent/homelab/winvm/tools/ci.sh /mnt/Sabrent/homelab/navo <sha> test` puts `C:\src\navo` at `<sha>`; then in `/mnt/Sabrent/homelab/winvm/winvm.sh ssh` (admin, elevated) run the PowerShell lines the step gives.

## Owner questions

- Q1 — **Answered 2026-11-05 (owner): yes.** A rebuild that keeps the old index searchable may peak at 2.2 × the old resting size; load and cleanup peaks stay ≤ 1.7× (cleanup against the index just before it starts). D109 amended in `docs/decisions.md`. LD9 holds the constants.
- Q2 — May M3 run read-only on your PC's index snapshot? It prints counts only. **Default (kept): the VM snapshot only.**
- Q3 — Stop at ≈ 100 MiB per 2 M files, or aim for 80 MB with a second plan? **Default (kept): stop at ≈ 100 MiB; revisit after the VM numbers.**

## Risks and rollback

- R1 — The map returns a wrong id, so a journal change lands on the wrong entry. Prevented: LD2 partition invariant with `seq`, an oracle against `HashMap<u128, u32>` with allocation failures, `senior`, `reviewer-senior`.
- R2 — Dense pages grow on a sparse MFT. Prevented: the LD2 budget; a test with 100,000 references spread to record `2^31`.
- R3 — A flag change hides children or keeps stale heads, so removal or totals go wrong. Prevented: LD3 lazy rows; C0t pins today's behaviour for every transition before C0 changes the layout.
- R4 — Flags written as one byte break the format. Prevented: L0, L1 and a corrupt-flags load test.
- R5 — Dedup slows search. Prevented: N3 gate; N2 parks on failure.
- R6 — Downgrade: L1 and cross-check X prove both directions; the VM step repeats it with real snapshots.
- R7 — Rename churn grows the arena without dead slots, so no compaction runs. Measured by M2 (one full rename pass, B/entry after); G1 asserts one pass at 1 M stays ≤ D109's 170 MiB / 2 M (89 B/entry). This is today's behaviour; a dead-bytes trigger needs its own plan.
- No user file is written; the index is derived data. Rollback: `git revert <sha>`; a bad snapshot is fixed by deleting `%ProgramData%\Navo\index\<letter>.index`, then the service rebuilds.

## Components

| Component | Paths (`crates/navo-index/src/`) | Contract | Test |
| --- | --- | --- | --- |
| `steady-tests` | `table/memory.rs`, `ntfs/peak_tests.rs`, `ntfs/peak_ratio_tests.rs`, `ntfs/memory_tests.rs`, `heap_probe.rs` | fixture → bytes, peak ratios | itself |
| `snapshot-stats` | `file/stats_tests.rs` | v3 file → counts | itself (ignored) |
| `snapshot-compat` | `file/compat_tests.rs` | v3 bytes ↔ semantics | itself |
| `search-timing` | `search/timing_tests.rs` | 2 M table → medians | itself (ignored) |
| `frn-pages` | `usn/frn_pages.rs` | record, seq → id | in file |
| `frn-map` | `usn/frn_map.rs`, `usn/frn_map_oracle_tests.rs` | u128 → id (exact) | oracle |
| `table-children` | `table/children.rs`, `table/children_flag_tests.rs`, `table/mod.rs`, `table/growth.rs` | parent → ordered children | in file, flag tests, totals, compaction oracle |
| `table-flags` | `table/flags.rs`, `table/mod.rs`, `table/totals.rs`, `table/totals_tests.rs`, `table/change.rs`, `table/memory.rs`, `file.rs`, `file/stream.rs` | flags column ↔ v3 u16 | L0, L1, `file`, `table` |
| `sparse-trigger` | `table/compact.rs`, `ntfs/compact.rs`, `ntfs/compact_tests.rs`, `host/scan.rs` | slots, len → compact? | `table/compact.rs` tests |
| `table-interner` | `table/growth.rs`, `table/interner.rs`, `table/mod.rs`, `table/memory.rs`, `ntfs/volume_io.rs`, `table/compact.rs` | names → shared offsets | in file, compaction oracle |

## Atoms

`api-builder` runs on DeepSeek. Each atom shows its new test failing first, except **exempt** ones (measurements, guards that pass on `main`, tidies). Atoms in `navo-index` build one at a time. Line numbers are at `a3ba33aa`. Filtersets go into LD11's `-E`; in the table, `\|` stands for `|`.

| Atom | Component | Files | Role | Effort | After | Check (crate, filterset) | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| M0 | `steady-tests` | `table/memory.rs`, `table/children.rs` | patcher | medium | — | `navo-index`, `test(/^table::/)` | done |
| M1 | `steady-tests` | `ntfs/peak_tests.rs`, `heap_probe.rs` | api-builder | medium | M0 | `navo-index`, `test(/^ntfs::peak_tests::/)`; report | done |
| M2 | `steady-tests` | `ntfs/peak_ratio_tests.rs` (new), `ntfs/mod.rs` | api-builder | medium | M1 | `navo-index`, `test(/^ntfs::peak_ratio_tests::/)`; report | done |
| M3 | `snapshot-stats` | `file/stats_tests.rs` (new), `file.rs` | api-builder | medium | — | `navo-index`, `test(=file::stats_tests::snapshot_stats)` ignored; VM | done |
| L1 | `snapshot-compat` | `file/compat_tests.rs` (new), `file.rs` | api-builder | medium | M3 | `navo-index`, `test(/^file::/)` | done |
| S0 | `search-timing` | `search/timing_tests.rs` (new), `search.rs` | api-builder | medium | L1 | `navo-index`, `test(/^search::/)`; report | done |
| C0t | `table-children` | `table/children_flag_tests.rs` (new), `table/children.rs` | api-builder | medium | M0 | `navo-index`, `test(/^table::/)` | done |
| C0g | `table-children` | `table/growth.rs`, `table/children.rs` | patcher | medium | C0t | `navo-index`, `test(/^table::/)` | done |
| K0 | `sparse-trigger` | `ntfs/compact_tests.rs` | patcher | medium | — | `navo-index`, `test(/^ntfs::compact::/)` | done |
| P1 | `frn-pages` | `usn/frn_pages.rs` (new), `usn/mod.rs` | **senior** | high | M1 | `navo-index`, `test(/^usn::/)` | done |
| P2 | `frn-map` | `usn/frn_map.rs`, `usn/frn_map_oracle_tests.rs` (new) | **senior** | high | P1, L1 | `navo-index`, `test(/^(usn\|file\|ntfs)::/)`; `navo-index-service`, `test(/^tests::/)`; X | done |
| C0 | `table-children` | `table/children.rs`, `table/mod.rs`, `table/growth.rs` | **senior** | high | C0g | `navo-index`, `test(/^(table\|ntfs)::/)` | done |
| F1a | `table-flags` | `table/flags.rs`, `table/mod.rs`, `table/change.rs` | patcher | medium | C0 | `navo-index`, `test(/^table::/)`; grep | done |
| F1b | `table-flags` | `table/totals.rs`, `table/memory.rs`, `table/totals_tests.rs` | patcher | medium | F1a | `navo-index`, `test(/^table::/)`; grep | done |
| F1c | `table-flags` | `table/flags.rs`, `file.rs`, `file/stream.rs` | **senior** | medium | F1b, L1 | `navo-index`, `test(/^(file\|table)::/)`; X | done |
| K1 | `sparse-trigger` | `table/compact.rs`, `ntfs/compact.rs`, `host/scan.rs` | patcher | medium | K0 | `navo-index`, `test(/^(table\|ntfs\|host)::/)` | done |
| N1a | `table-interner` | `table/growth.rs` | patcher | medium | C0g, M3 gate | `navo-index`, `test(/^table::growth::/)` | parked (LD6; steady 51.04 B/entry meets the target) |
| N1b | `table-interner` | `table/interner.rs` (new), `table/mod.rs` (`mod` line only), `navo-index/Cargo.toml` | api-builder | medium | N1a, F1c | `navo-index`, `test(/^table::interner::/)` | parked (LD6; steady 51.04 B/entry meets the target) |
| N1c | `table-interner` | `table/mod.rs`, `table/memory.rs`, `table/interner.rs` | api-builder | high | N1b | `navo-index`, `test(/^table::/)` | parked (LD6; steady 51.04 B/entry meets the target) |
| N3 | `search-timing` | `search/timing_tests.rs` | patcher | medium | N1c, S0 | LD10 | parked (LD6; steady 51.04 B/entry meets the target) |
| N2 | `table-interner` | `ntfs/volume_io.rs`, `table/compact.rs`, `table/compact_oracle_tests.rs` | **senior** | medium | N3, K1 | `navo-index`, `test(/^(table\|ntfs)::/)`; X | parked (LD6; steady 51.04 B/entry meets the target) |
| G1 | `steady-tests` | `ntfs/peak_ratio_tests.rs`, `ntfs/peak_tests.rs`, `ntfs/memory_tests.rs` | patcher | medium | M2, P2, F1c, K1, N2 or parked | `navo-index`, `test(/^ntfs::(peak\|memory)/)`; reports; X | done |
| T1 | `steady-tests` | `table/growth.rs`, `table/children.rs`, `ntfs/volume_io.rs`, `ntfs/peak_ratio_tests.rs` | senior | medium | G1 attempt | `navo-index`, `test(/^(table::growth\|ntfs::(peak\|volume_io))/)`; reports | done (sized and owner-approved fallback ceilings) |

### M0 — column bytes (exempt: measurement)

- **Read:** `table/memory.rs`, `table/children.rs:1-30,130-145`.
- **Change:** `#[cfg(test)] pub(crate) fn column_bytes(&self) -> Vec<(&'static str, usize)>` on `NameTable` in `memory.rs` (capacity × element size: `names`, `offsets`, `parents`, `flags`, `bytes`, `counts`, then the children's entries), using `#[cfg(test)] pub(super) fn column_bytes(&self) -> [(&'static str, usize); 4]` in `children.rs` (`children.first`, `children.next`, `children.previous`, `children.waiting`).
- **Test** (in `memory.rs`): `column_bytes_sum_to_memory_bytes` on `table(10_000)`.
- **Stop if:** the sum differs from `memory_bytes()` (a column is missing).

### M1 — steady report (exempt: measurement)

- **Read:** `ntfs/peak_tests.rs`, `heap_probe.rs:60-90`, `ntfs/memory_tests.rs:1-40`.
- **Change:** `heap_probe.rs`: `pub fn mark(&self) -> usize` on `Probe` (stores `LIVE` into `PEAK`, returns bytes retained since start). `peak_tests.rs`: make `header`, `held`, `save`, `flat_index` `pub(super)`; add LD1's `steady_index`, saved and loaded through `file::save_with_ids` / `load_with_ids` + `NtfsIndex::restore`; `#[ignore = "steady report"] fn steady_report()` prints `STEADY <M> gap <g>: <column> <B/entry> … frn <B/entry> total <B/entry> held <KiB>` for 1, 2, 5 M and gaps 0 and 4; a passing `steady_stays_within_budget` at 1 M, gap 0: held / entries ≤ `STEADY_PER_ENTRY` (measured × 1.02, rounded up) and column sum plus `ids.memory_bytes()` within 1 % of `held`.
- **Report:** `$P cargo nextest run -p navo-index --no-tests=fail --cargo-profile release --run-ignored only --no-capture -E 'test(=ntfs::peak_tests::steady_report)'`; the `STEADY` lines go into the Log.
- **Stop if:** the column sum and `held` differ by more than 1 %.

### M2 — peak ratios and churn (exempt: measurement)

- **Read:** `ntfs/peak_tests.rs`, `ntfs/compact_tests.rs:1-75`, `ntfs/mod.rs:100-190,370-385`.
- **Change:** `ntfs/peak_ratio_tests.rs` (`#[cfg(test)] mod peak_ratio_tests;` in `ntfs/mod.rs`). `pub(super)` scenario functions returning `(peak, base_or_held)` per LD9: `build_ratio`, `load_ratio`, `compact_ratio` (sparse index from `flat_index` minus every second file, `mark()` before `compact_if_sparse(0)`), `push_ratio`, `rebuild_ratio` (old `flat_index` held; `mark()`; build `MftFixture::new(entries, 7).read()`; drop the old index), `rename_churn` (restored index; `mark()`; `table.rename` of every file to `renamed-{n:07}.txt`; returns held after ÷ entries), `pending_renames(index, pending)` (restored index; builds one `UsnBatch` of `pending` `OLD_NAME`-only records for distinct live files before `mark()`, then one direct `apply_batch` call, not the 10,000-record `apply` helper, since `MAX_CARRIED_READS = 2` expires entries after two more non-empty batches; asserts `index.pending_renames().len() == pending`; returns (peak − base) ÷ pending and held growth ÷ pending). Record builder copied from `compact_tests.rs:47-68` (14 lines). `#[ignore = "peak ratio report"] fn peak_ratio_report()` prints `RATIO <case> <M>: <ratio> (peak <KiB>, base <KiB>)` at 1, 2, 5 M and `CHURN rename <B/entry>`, `CHURN pending peak <B/pending> held <B/pending>` (pending = 100,000 at 1 M), plus `MAP dir/pages/overflow` bytes once P2 exists (prints `MAP n/a` before). One passing smoke test at 100,000 entries (pending = 10,000) asserts every ratio is finite and above 1.0 and the pending count.
- **Report:** the LD11 report form with `-E 'test(=ntfs::peak_ratio_tests::peak_ratio_report)'`.
- **Stop if:** a scenario needs a change outside these two files and `peak_tests.rs`'s visibility.

### M3 — real snapshot counts (exempt: measurement)

- **Read:** `file.rs:105-145`, `file/stream.rs:120-175`, `table/mod.rs:93-100`.
- **Change:** `file/stats_tests.rs` (`#[cfg(test)] #[path = "file/stats_tests.rs"] mod stats_tests;` in `file.rs`), one `#[ignore = "real snapshot stats"]` test `snapshot_stats`. It reads the path in `NAVO_INDEX_STATS`; when unset it saves its own `sample()` (root, two folders, six files with one name used twice, four references including a wide one; `golden()` is private to `file/tests.rs`) through `save_with_ids` to a temporary file and reads that. Prints `STATS <key> <value>` for: slots, live, folders, arena bytes, live name bytes, unique live name bytes, dedup saving %, mean name bytes, references, wide references, max record, dense pages (distinct `record / 1024` with `record < 2^32`), dense B/reference (`pages * 6144 / narrow references`), records with more than one reference. Never prints a name or path.
- **Linux check:** the LD11 form with `--run-ignored only --no-capture -E 'test(=file::stats_tests::snapshot_stats)'`; prints `STATS` lines from the fixture.
- **VM run (orchestrator):** after `ci.sh … <sha> test`, in `winvm.sh ssh`:
  ```
  Set-Location C:\src\navo
  $env:NAVO_INDEX_STATS = "$env:ProgramData\Navo\index\C.index"
  cargo nextest run -p navo-index --no-tests=fail --run-ignored only --no-capture -E 'test(=file::stats_tests::snapshot_stats)' *> C:\winvm\M3.log; $LASTEXITCODE
  Select-String -Path C:\winvm\M3.log -Pattern '^STATS '
  ```
  Log the lines. **Gate:** dedup saving ≥ 15 % unblocks N1a.
- **Stop if:** reading needs a change outside these two files; `Access is denied` on the VM (report, do not change ACLs).

### L1 — cross-version fixtures (exempt: guard, passes on `main`)

- **Read:** `file/tests.rs:510-592`, `file.rs:1-60,105-145`.
- **Change:** `file/compat_tests.rs` (`#[cfg(test)] #[path = "file/compat_tests.rs"] mod compat_tests;` in `file.rs`). `rich()` builds LD7's fixture through the table and map API (≥ 3 folders, ≥ 12 files, two names used in two folders, ≥ 8 narrow references including records `2^32 + 1` and `2^40`, two references on record 300 with `seq` 1 and 2, a second reference to the root id, two wide references, one deleted file, one deleted folder with a child, one rename, sizes on every file), then shares offsets: from `raw_parts()` it points each later live entry whose name equals an earlier live entry's to the earlier offset and reloads through `from_raw_parts` with `stored_totals()`. A helper asserts at least two live ids share an offset. `semantics(&NameTable, &FrnMap) -> Semantics` (slots; per slot `Option<(String, u32, u16, u64, Option<[u32; 2]>)>`; `BTreeSet<(u128, u32)>`). Tests: `rich_round_trips` (save, load, equal semantics, shared offset kept); `stored_rich_v3_loads` (writes `RICH_V3_HEX` to a file, loads it, equal semantics to `rich()`, version byte 3); ignored `compat_write` (saves `rich()` to `NAVO_COMPAT_OUT`) and `compat_read` (loads `NAVO_COMPAT_IN`, asserts equal semantics). `RICH_V3_HEX` is produced once by this atom on `main` (any one run; reference order may vary) and is never edited later.
- **Stop if:** `rich()` cannot be built through the table and map API plus `raw_parts`/`from_raw_parts`, or `from_raw_parts` refuses the shared offsets (that is a v3 reader limit: report it).

### S0 — search baseline at 2 M (exempt: measurement)

- **Read:** `search.rs:1-60`, `search/tests.rs:210-235`.
- **Change:** `search/timing_tests.rs` (`#[cfg(test)] #[path = "search/timing_tests.rs"] mod timing_tests;` in `search.rs`). `fn fixture() -> NameTable`, 2,000,000 entries, xorshift64 seed 7:
  - id 0: root `C:` (folder). Folder ids `f % 10 == 0`, `f > 0`: parent = root when `f <= 10_000`, else folder `((f / 10) % 1000 + 1) * 10`; name `{FOLDER_WORDS[(f / 10) % 8]}-{f:07}`, `FOLDER_WORDS = [src, docs, build, assets, tests, media, notes, tools]`.
  - File ids: parent `n - n % 10`; when `rng % 10 < 3`, name `POOL[rng % 5000]` with `POOL[i] = {FILE_WORDS[i % 16]}-{i:04}.{EXTS[i % 8]}`; else `{FILE_WORDS[n % 16]}-{n:07}.{EXTS[n % 8]}`. `FILE_WORDS = [document, report, main, readme, invoice, photo, video, song, config, notes, backup, draft, summary, plan, budget, letter]`, `EXTS = [txt, pdf, rs, md, jpg, mp4, toml, docx]`.
  - `#[ignore = "timing"] search_scale_timing`: `let pdf = vec!["pdf".to_owned()];` queries `doc` = `[Term::Name("doc")]`, `report-pdf` = `[Term::Name("report"), Term::Ext(&pdf)]`, `src-main` = `[Term::Path("src"), Term::Name("main")]`, limit 500, 2 warm-ups then 15 timed runs. Asserts per query: exactly 500 hits; every timed run returns the warm-up's hits; every `report-pdf` hit ends with `.pdf`; every `src-main` hit's path contains `src`. Prints `SEARCH <label>: median <µs> min <µs> max <µs>`.
- **Order:** S0 lands after L1, so `S0_SHA` holds `compat_write` and `compat_read` for cross-check X.
- **Run (S0's own commit, base worktree):** `cd <wt> && RAYON_NUM_THREADS=4 $P cargo nextest run -p navo-index --no-tests=fail --cargo-profile release --run-ignored only --no-capture -E 'test(=search::timing_tests::search_scale_timing)' > <wt>/../S0.log 2>&1; echo $?`, five times; log medians and max ÷ min per query.
- **Stop if:** a query returns fewer than 500 hits (fix the fixture, not the search); base max ÷ min > 1.10 (report: machine too noisy).

### C0t — child links across flag changes (exempt: guard, passes on `main`)

- **Read:** `table/change.rs`, `table/children.rs:230-330`, `table/totals_tests.rs:1-60`, `table/compact.rs:30-60`.
- **Change:** `table/children_flag_tests.rs` (`#[cfg(test)] #[path = "children_flag_tests.rs"] mod flag_tests;` in `children.rs`). Through `NameTable::apply` and the table API, assert `child_names`, `totals` and `remove` counts for: `Attributes` file → folder, then `Add` a child under it; `Add` on a known file reference with `FOLDER` (upsert path), then a child; `Attributes` folder → file on a folder with children (children still listed, `remove` deletes the subtree, ancestors' totals drop); `Rename` of a child into an id pushed as a file; reparent between two folders and back; each case after `NameTable::compact` (same `child_names` through the remap); reload through `file::save_with_ids` / `load_with_ids`: the file → folder case loads with the same children, the folder → file-with-children case is refused (today's `valid_structure`).
- **Stop if:** any case fails on `main` (report it: that is a bug today, not a plan input).

### C0g — growth tests layout-neutral (exempt: tidy)

- **Change:** `children.rs`: `#[cfg(test)] pub(super) fn spares(&self) -> [(usize, usize); 3]` (capacity, len per link column). `growth.rs` tests: `spares_within_slack` checks each children column against its own len; `try_reserve_total_reaches_the_requested_totals` asserts `next` and `previous` = 100,000 and the first column between 10,000 and 100,000.
- **Stop if:** a test outside `growth.rs` changes.

### K0 — compaction test neutral to the trigger (exempt: tidy)

- **Change:** `ntfs/compact_tests.rs:341-372`, renamed `the_batch_path_compacts_only_past_the_minimum_and_the_sparse_trigger`: first delete `FIRST_FILE..FIRST_FILE + 30_000` (110,001 slots, 80,001 live: dense under 2× and 1.5×), then `FIRST_FILE + 30_000..FIRST_FILE + 60_000` (50,001 live: sparse under both). Other asserts stay.
- **Stop if:** it fails on `main`.

### P1 — dense pages

- **Read:** `usn/frn_map.rs`, `heap_probe.rs`.
- **Change:** `DensePages` per LD2 in `usn/frn_pages.rs` (`mod frn_pages;` in `usn/mod.rs`, `pub(super)`): `get(record: u64, seq: u16) -> Option<u32>`, `try_insert(record, seq, id) -> Result<Placed, TryReserveError>` with `enum Placed { Inserted, Updated(u32), Refused }` (`Refused` = `id == EMPTY` whether or not the key is held, budget, `record >= 2^32`, or slot held by another `seq`; `Refused` changes nothing), staged per LD2, `remove(record, seq) -> Option<u32>`, `retain`, `remap(FnMut(u32) -> Option<u32>) -> Vec<(u64, u16, u32)>` (returns pairs whose new id is `EMPTY`, removed from the pages), `iter()` in record order, `len`, `clear`, `shrink_to_fit`, `memory_parts() -> (directory, pages)`.
- **Test first** (in file): 1 M consecutive records ≤ 6.1 B/record; a lone record `2^31` is `Refused`; 100,000 records spread to `2^31` ≤ 24 B each + 256 KiB; a page is freed when its last record goes; another `seq` in a held slot is `Refused`; `id == EMPTY` is `Refused`; `try_insert` of `EMPTY` on a held key is `Refused` and `get` still returns the old id; `remap` and `retain` never allocate (probe) when no id maps to `EMPTY`. Staging, both with `len`, `iter` and `memory_parts` equal before and after: (a) page fails: a record in a new page under `fail_allocations_from(2048)` returns `Err`; (b) directory fails after the page: on an empty map, record `2^20` (directory 1,025 × 40 B ≈ 41 KB, within the budget) under `fail_allocations_from(8192)` returns `Err` (the 4 KiB and 2 KiB page boxes succeed and drop).
- **Stop if:** `Page` cannot stay ≤ 6 KiB + 48 B, `size_of::<Option<Page>>()` is not 40 (fix the test sizes, report), or a test needs `frn_map.rs`.

### P2 — `FrnMap` on dense pages

- **Read:** `usn/frn_map.rs`, `usn/frn_pages.rs`, `file/stream.rs:40-55`, `file/tests.rs:540-570`, `navo-index-service/src/tests.rs:380-400`.
- **Change:** LD2's `FrnMap { dense, overflow, wide }` with the partition invariant and insert order; manual `PartialEq`; same public API; `remap` moves `EMPTY` results to `overflow`. Rewrite only the in-file tests that read `narrow`/`wide` fields (`memory_counts_allocated_buckets`, `remap_rewrites_both_widths_…`) to assert through `memory_bytes`, `len` and `get`.
- **Test first** (`frn_map_oracle_tests.rs`, `#[cfg(test)] #[path] mod`): 50 seeded runs of 2,000 operations (insert, try_insert, remove, get, retain, remap, iter, clear) against `HashMap<u128, u32>`; keys mix small records, records ≥ `2^32`, `u64::MAX`, wide ones and same-record-other-seq pairs; values include `u32::MAX`. After each operation `len`, every `get` and the sorted `iter` agree, and no key is in two parts. Fixed sequences: dense `(300, 1)` + overflow `(300, 2)`, remove `(300, 1)`, `get (300, 2)` hits, insert `(300, 3)` goes dense, re-insert `(300, 2)` updates overflow; dense `(400, 1) = 5` then `insert (400, 1) = u32::MAX`: `get` returns `u32::MAX`, the key is in `overflow` only, `len` unchanged. Failures: every oracle operation also runs as `try_insert` under `fail_allocations_from(4096)`, `(2048)` and `(1)`; each returns `Ok` with the oracle updated or `Err` with `len`, `get`, `iter` and `memory_parts` unchanged; `(1)` on the dense-to-`overflow` migration with no spare `overflow` room returns `Err` and `get` still returns the old id. Aborting fallback: with spare `overflow` room (insert 20 keys ≥ 2^32, remove 10), `insert` of a new record in a new page under `fail_allocations_from(2048)` lands in `overflow` once and `memory_parts().pages` is unchanged. Memory: 1 M references `100 + n` ≤ 6.2 B each.
- **Check also:** L0 and L1 pass unedited; `navo-index-service` `test(/^tests::/)` passes; cross-check X. **Stop if:** any test outside `frn_map.rs` needs a change.

### C0 — child heads where needed

- **Read:** `table/children.rs`, `table/children_flag_tests.rs`, `table/mod.rs:56-70,196-275,336-345`, `table/totals.rs:150-185`, `table/growth.rs:36-60`.
- **Change:** LD3. `mod.rs`: `push` calls `children.add(id, &self.parents, flags.contains(Flags::FOLDER))`; `relink` passes `is_folder` from the flags column (`& Flags::FOLDER.bits()` today; F1a converts it). `with_capacity` sizes `Children` for `entries` slots and no head rows. `growth.rs`: `try_reserve_total` calls `children.try_reserve_total(entries, folders)` (no test edits). `column_bytes` and `spares` report `children.heads`.
- **Test first** (in `children.rs`): a 1 M-file table under one folder holds ≤ 8 B per slot + 64 KiB in `Children`; `of` on a file id with no children is empty; a child attached to an id pushed as a file is listed and gets a row; `rebuild` scratch ≤ `slots / 8` + 4 KiB (probe). C0t, `totals_tests.rs`, `compact_oracle_tests.rs`, `growth.rs` tests and `ntfs/compact_*tests.rs` pass unedited.
- **Stop if:** `compact.rs` or `remapped`'s signature must change, or any other test changes.

### F1a — flags alias, part 1 (exempt: tidy)

- **Change:** `table/flags.rs`: `pub(crate) type StoredFlags = u16;`; `pub(super) const DELETED: StoredFlags`; `pub(super) const VALID: StoredFlags` (`FOLDER | HIDDEN | SYSTEM | REPARSE` bits `| DELETED`); `pub(super) const fn stored(self) -> StoredFlags`; `from_stored(StoredFlags)`. `table/mod.rs`: field, `raw_parts`, `from_raw_parts` use `StoredFlags`; `valid_flags` becomes `Flags::VALID`; `push` stores `Flags::from_stored(flags.stored()).stored()`; C0's `is_folder` closure uses `Flags::FOLDER.stored()`. `table/change.rs`: the three `flags.bits()` writes become `flags.stored()`.
- **Grep check:** `grep -nE '\.bits\(\)' crates/navo-index/src/table/mod.rs crates/navo-index/src/table/change.rs` prints nothing.
- **Stop if:** anything but type and accessor swaps is needed.

### F1b — flags alias, part 2 (exempt: tidy)

- **Change:** `totals.rs` (`:80`, `:89`, `:122`, `:129`, `:188`): every `Flags::FOLDER.bits()` becomes `Flags::FOLDER.stored()`; the `:129` closure takes `&StoredFlags`. `memory.rs`: `size_of::<StoredFlags>()` at `:21` and in the `:60` test. `totals_tests.rs:261` helper takes `&mut Vec<StoredFlags>`.
- **Grep check:** `grep -nE '\.bits\(\)|u16' crates/navo-index/src/table/totals.rs crates/navo-index/src/table/memory.rs` prints nothing.
- **Stop if:** anything but type and accessor swaps is needed.

### F1c — flags in one byte

- **Change:** LD4. `StoredFlags = u8`; `Flags::stored` narrows (`self.0 as StoredFlags`, with a `const` assert that every defined bit and `DELETED` fit in `u8`); `from_stored` widens. `stream.rs:39-41` writes `u16::from(*flag).to_le_bytes()`. `file.rs:119` decodes per LD4. After F1a and F1b no other file names the column type, so `mod.rs`, `totals.rs`, `change.rs`, `memory.rs` and `totals_tests.rs` compile unchanged.
- **Test first** (`table/flags.rs` tests): a 1 M-slot table's flags column is 1 B/slot (`column_bytes`). L0 and L1 pass unedited; the corrupt-flags case at `file/tests.rs:389` (`u16::MAX`) still rejects; cross-check X.
- **Stop if:** L0 bytes change, or a file outside these three needs an edit.

### K1 — sparse trigger at 1.5×

- **Read:** `table/compact.rs`, `ntfs/compact.rs`, `host/scan.rs:255-270`.
- **Change:** LD8. `NameTable::is_sparse` in `table/compact.rs`; `ntfs/compact.rs::compact_if_sparse` returns `false` unless `self.table.is_sparse(min_slots)`, doc "once dead slots pass a third of all slots"; `host/scan.rs:263` uses `table.is_sparse(COMPACT_MIN_SLOTS)`.
- **Test first** (`table/compact.rs` tests): 150,001 slots with 100,000 live is sparse at `min_slots = 100_000`, 150,000 slots is not, a table below `min_slots` is not.
- **Stop if:** an existing test changes (K0 already made the batch test neutral). `host::tests::a_refreshed_folder_compacts_after_its_dead_slots_double` still passes (it now compacts after each refresh; same final state) and keeps its name. Restore compacts at 1.5× too (`ntfs/snapshot.rs:40`), so the check covers all of `ntfs::`.

### N1a — live name bytes with shared offsets (gated by M3)

- **Change:** `growth.rs::live_name_bytes` counts each distinct live offset once: a bit set over arena positions (`names.len() / 8` B, `try_reserve_exact`); on allocation failure it returns the plain sum (an upper bound, safe for size hints). Dead names never count.
- **Test first** (in `growth.rs`, tables from `NameTable::from_raw_parts` with shared offsets): two live entries on one offset count once; a dead entry sharing a live offset adds nothing; a dead-only offset adds nothing; the existing rename test passes unedited.
- **Stop if:** a file other than `growth.rs` needs an edit.

### N1b — standalone interner (gated by M3)

- **Read:** `table/mod.rs:1-20`, `Cargo.toml`.
- **Change:** LD6. `hashbrown = { version = "=0.16.1", default-features = false }` (already in `Cargo.lock`). `table/interner.rs`: `#[derive(Debug, Clone)] pub(super) struct Interner { table: HashTable<u32>, hasher: std::hash::RandomState }`, `with_capacity(names: usize) -> Result<Self, TryReserveError>`, `find(&self, names: &str, name: &str) -> Option<u32>`, `try_insert(&mut self, names: &str, offset: u32) -> Result<(), TryReserveError>`, `memory_bytes()`. `table/mod.rs`: only `mod interner;`. No `NameTable` change.
- **Test first** (in `interner.rs`, over a local arena `String`): a found name returns its offset; a missing name returns `None`; two names with one hash bucket stay apart; `try_insert` under `fail_allocations_from(1)` returns `Err` and `find` still answers for earlier names; `memory_bytes` grows with entries.
- **Stop if:** the hasher needs a dependency other than `hashbrown`, or `mod.rs` needs more than the `mod` line.

### N1c — interner in the table (gated by M3)

- **Read:** `table/mod.rs`, `table/memory.rs`, `table/interner.rs`.
- **Change:** `table/mod.rs`: field `interner: Option<Box<Interner>>`, set to `None` in `with_capacity` and `from_raw_parts` (`new` uses `Default`); `pub fn begin_interning(&mut self)` (builds the interner over the current arena; on `Err` it warns and stays off); `pub fn end_interning(&mut self)`; `append_name` returns a found offset without writing while the interner lives, else appends and, while interning, `try_insert`s the new offset (on `Err`: warn, drop the interner). `table/memory.rs`: `memory_parts().names` adds `interner.memory_bytes()`; `column_bytes` adds an `interner` entry.
- **Test first** (second test module in `interner.rs`, through the public table API): 100,000 names from a 1,000-name pool use ≤ the pool's bytes + root; every `name(id)` equals a non-interned twin; renaming one entry that shares a name leaves the others intact; `end_interning` frees the interner (`memory_parts` drops by its bytes); `fail_allocations_from` on the interner's growth turns interning off and every name stays correct; M0's `column_bytes_sum_to_memory_bytes` passes with the interner live.
- **Stop if:** `rename`, `compact.rs` or a fourth file needs a change.

### N3 — search gate with shared names

- **Change:** S0's fixture calls `begin_interning()` before its pushes and `end_interning()` after (the assertions stay). Run LD10 against the base worktree.
- **Report:** the 5 ratios and base spread per query. **Stop if:** the gate fails or stays noisy: N2 is parked, N1b and N1c stay as unused API.

### N2 — intern in builds and compaction

- **Read:** `ntfs/volume_io.rs:54-115`, `table/compact.rs`, `table/compact_oracle_tests.rs`.
- **Change:** `enumerate_volume_sized`: `begin_interning()` after the reserves, `end_interning()` before `trim_slack`. `compact.rs`: `compact` sets `self.interner = None` first; `kept_names` interns through a local `Interner`; capacity `min(bytes, self.names.len())`. Oracle: the raw arena comparison becomes per-id name equality plus `names.len() <= reference.len()`; all other comparisons stay.
- **Test first:** a compaction of a table with 50 % repeated names yields an arena ≤ 60 % of today's; a compaction during `begin_interning` leaves no interner and later pushes append correct names; the `peak` tests pass with `build_sized` ≤ 12 B/entry. L0, L1 unedited; cross-check X.
- **Stop if:** a test other than the oracle's arena comparison changes, or `build_sized` exceeds 12 B (report the number).

### G1 — final ceilings (exempt: guard)

- **Change:** `peak_tests.rs`: `STEADY_PER_ENTRY` and the peak consts set to measured × 1.1 (steady × 1.02). `memory_tests.rs`: `TABLE_BUDGET`, `FILE_ID_BUDGET` likewise. `peak_ratio_tests.rs`: `PEAK_CEILING`, `REBUILD_CEILING` per LD9 and `peaks_stay_within_d109` at 1 M (every LD9 ratio, plus `rename_churn` ≤ 89 B/entry). Rerun `steady_report`, `peak_ratio_report`, `scale_report` (release); log all three; cross-check X.
- **Stop if:** a Done-when target is missed: report the numbers.

### T1 — sized rebuild within 2.2×

- **Found:** a sized rebuild reserves the old index's live names plus a sixteenth. A new build only a few bytes smaller than the old one leaves a spare just over `len / 16`, so the end trim copies the whole arena (about 18 MB per 1 M files) while both indexes are held: rebuild 2.06 → 2.42.
- **Change:** builds whose hint reserve holds trim only spare over `len / 8` (`NameTable::trim_sized_slack`); unsized builds keep `len / 16`. A sized build may hold up to an eighth spare per column until the next load. `sized_rebuild_ratio` builds from `hint(old)` as the service does; `sized_rebuild_stays_within_the_ceiling` (ignored, release) asserts it at 1 M.
- **Measured (release):** rebuild-sized 2.422 → 2.058 / 2.057 / 2.056 at 1 / 2 / 5 M. Build, load, compact, push and steady (51.04 gap 0, 52.55 gap 4) unchanged.
- **Open:** an unsized rebuild stays 2.289 / 2.305 / 2.425: its peak is the arena's growth copy mid-build, not the trim. The service only builds unsized with an old index held after a failed reserve. Meeting 2.2 there needs a non-contiguous arena or a decision that LD9's rebuild case is the sized one.

## Done when

- `steady_report`, gap 0, 2 M: total ≤ 52 B/entry (≤ 99.2 MiB per 2 M); gap 4: ≤ 54 B/entry; reference map ≤ 6.2 B (gap 0). Map directory, overflow and the not-modelled parts are logged apart. With N2 in: the VM's `C:` B/entry logged next to M3's.
- `peaks_stay_within_d109` passes at 1 M; `peak_ratio_report` at 2 M and 5 M shows load, build, push and compaction ≤ `PEAK_CEILING` and rebuild ≤ `REBUILD_CEILING`.
- LD10 passes on the final branch. L0 and L1 pass unedited; cross-check X passes both ways. Service `timing_tests` (`$P cargo nextest run -p navo-index-service --no-tests=fail --run-ignored all -j 1 -E 'test(/timing_tests/)'`) pass.
- Full gate once, `reviewer-senior` on the branch, VM CI green.
- **Windows VM** (never your PC; RAM plan Done-when steps 1–4 for the commands): steady `PagedMemorySize64` after `snapshot restored T:` and 60 s idle ≤ 65 % of the same step on `main`; a journal-reset rebuild peaks ≤ 2.2 × steady; load after restart peaks ≤ 1.7 × steady; Ctrl+K finds `T:` names during and after the build; a `T.index` written by the branch loads in the `main` build and the other way round (restart, `snapshot restored T:` in `$log`, no `build start T:`).

## Log

- 2026-11-05 — Plan written. Q1–Q3 recorded with defaults. Steady model: 84.9 → 51.0–52.5 B/entry (≈ 162 → 97–100 MiB per 2 M); dedup gated on M1.
- 2026-11-05 — Revised after plan check 2 (Terra xhigh, 8 findings, all accepted). LD2: existing keys update first, `EMPTY` on a dense key migrates to `overflow`, one fallible step per path with the page staged before the directory reservation, so failures leave capacity unchanged. LD7: `rich()` shares offsets through `from_raw_parts` and holds a root alias; S0 after L1 so the base worktree has the compat tests; v4 dropped from scope (no atom needs it; a later plan must settle downgrade freshness first). F1a/F1b move every column use to `stored()` so F1c only changes the alias. N1b split into N1b (standalone `Interner`) and N1c (table wiring, memory accounting). LD11: `$P` is written out with `"$HOME"`; M3 builds its own sample. M2: pending renames in one batch with the count asserted, peak and held both reported. K1 checks all of `ntfs::` and `host::`. 22 atoms (5 gated).
- 2026-11-05 — Revised after plan check 1 (Terra xhigh, 9 findings, all accepted). Q1 answered by the owner (2.2× rebuild, 1.7× load and cleanup; D109 amended). LD2: partition invariant, `EMPTY` values to `overflow`, atomic fallible insert, removal rules, oracle with allocation failures. LD3: lazy head rows for any parent, so flag changes keep today's behaviour; C0t pins it first. LD7: compatibility = v3 wire semantics plus cross-version reads (L1, cross-check X); v4 at a distinct path with verified promotion. LD8: arena churn not bounded by slots (R7, measured). LD9: per-scenario baselines with `Probe::mark`, rebuild scenario added (M2). LD10: full fixture, hit assertions, pinned threads, 5 paired rounds with a noise rule, base worktree at `S0_SHA`. LD11: exact Linux and VM commands, `--no-tests=fail`. Split: M0/M1/M2, C0t/C0g/C0, K0/K1, N1a/N1b. Old M1 is now M3. 21 atoms (4 gated).
- 2026-11-05 — M1 stop (Terra): the fixture gave the root reference 100, but restore requires 5. LD1 fixed: root = 5.
- 2026-11-05 — Senior review: ship with fixes. Fix round `208977f9` (trim after the swap, 0.9×/1.1× sized cases, sparse load case, 5 M fallback assert, steady 52, geometric page directory, chunked compaction test). Owner raised two ceilings (`b224920c`): sparse first load 2.5× (measured 2.39), grown-volume sized rebuild 2.6× (2.50). Open: S0 timing on a quiet host, VM CI + memory check, squash with LD9 and the D109 amendment.

- 2026-11-05 — Integration audit: the S0 source file was missing from the completed layout branch and the existing baseline worktree. Added the planned identical fixture and timing test to owned baseline/layout worktrees, based on `b1ac29db` before layout changes. VM memory procedure is not runnable: Get-Volume lists only C: and the system partition; T:, U: and the Navo Index service are absent. No live volumes, service or journal were changed. Real-volume steady/peak/search and snapshot downgrade checks are therefore unverified; portable heap and cross-version fixture checks remain required.

- 2026-11-05 — LD10 quiet-host search gate passed against `b1ac29db` with identical S0 source on both sides; four Rayon workers, five paired release rounds, no VM or other host build. Ratios are layout/base; medians must be ≤ 1.05, base spread ≤ 1.10.

| Query | Base medians (µs) | Layout medians (µs) | Five ratios | Median ratio | Base spread |
| --- | --- | --- | --- | --- | --- |
| doc | 58327, 58347, 60025, 57307, 58199 | 58467, 59330, 58603, 58603, 59018 | 1.0024, 1.0168, 0.9763, 1.0226, 1.0141 | 1.0141 | 1.0474 |
| report-pdf | 18514, 17727, 18961, 18926, 18578 | 18970, 17765, 18791, 18272, 18230 | 1.0246, 1.0021, 0.9910, 0.9654, 0.9813 | 0.9910 | 1.0696 |
| src-main | 41479, 40419, 42351, 41355, 41098 | 41790, 39531, 40708, 40562, 41126 | 1.0075, 0.9780, 0.9612, 0.9808, 1.0007 | 0.9808 | 1.0478 |

- 2026-11-05 — Host gate passed all eight steps for navo-index and navo-index-service: 483 tests passed, 22 skipped. Four cross-version fixture write/read checks passed. All eight release memory/scale/ratio checks passed at up to five million entries: steady 51.05 B/entry (gap 0, 2 M), 52.56 (gap 4); sized rebuild 2.056× at 5 M, unsized 2.425×, first sparse load 2.393×, grown sized rebuild 2.502×. The final S0 lint cleanup only flattened equivalent assertions and documented benchmark output; fixture and timed search calls are unchanged.
- 2026-11-05 — Explicit debug service timings reached the pre-existing ignored test reset_journal_first_populated_answer_within_one_second: 3.735 s vs 1 s. Its ignore reason on main already documents the full-read delay (about 4.5 s). This is not a new failure; release timing validation is recorded separately. No production code or timing limit was changed to hide it.

- 2026-11-05 — Release service timing validation passed all nine tests in 1.567 s total, including the journal-reset scenario. Log: `/tmp/navo-layout-service-timing-release.log`.

- 2026-11-05 — Prepared the final squash over the Git skip integration. Kept D110 and D111 while resolving the D109 conflict, and recorded every owner-approved LD9 ceiling in D109.

- 2026-11-05 — Final combined-tree gate passed all eight steps: 485 index/service tests passed, 22 skipped. Windows branch and exact-final-commit CI remain required before pushing.

- 2026-11-05 — Release preparation audits found a pinned pulldown-cmark advisory and one additional zip 8 advisory. The release candidate pins pulldown-cmark 0.13.6, updates yanked async-executor 1.14.0 to 1.14.1, and documents RUSTSEC-2026-0429 with the same unreachable ZipArchive::extract rationale as the existing Velopack zip exceptions. Verified Velopack 1.2.161 source contains no extract call; Navo's archive dependency is zip 9.0.3. cargo deny check passed; cargo audit passed with the four documented Velopack exceptions (nine existing allowed maintenance warnings). The navo-structured gate passed all eight steps, 110 tests.

- 2026-11-05 — The reproducible S0 baseline is now pinned at `5184cd3c`, based on `b1ac29db` before layout changes. Its host gate passed all eight steps and 317 tests. The benchmark file blob is identical on both layouts (`14e248aefcfcf9dd71c6bbbbd788b9fe127918a4`); lint-only assertion flattening preserved the measured fixture and timed calls.

- 2026-11-05 — Branch VM CI `8a333e7c` ALL GREEN: 12,010 tests passed, 61 skipped. Final combined commit `680f5ac7` VM CI ALL GREEN: 12,165 tests passed, 62 skipped; platform tests, doctests and release build passed. Squash landed and pushed to main. Real-volume memory validation remains unavailable for the documented missing VM prerequisites.
- 2026-11-05 — Final 0.3.0 candidate: all nine release service timing tests passed in 1.547 s. Repeated LD10 on a quiet host against S0 baseline `5184cd3c`, four Rayon workers, five paired release rounds. This integrated candidate includes the previously shipped score-only search optimization from main, so these gains combine that optimization with the smaller layout; the layout-only comparison is recorded above.

| Query | Base medians (µs) | Final medians (µs) | Five ratios | Median ratio | Base spread |
| --- | --- | --- | --- | --- | --- |
| doc | 59707, 59214, 60833, 58581, 60436 | 16553, 16850, 16651, 16440, 16514 | 0.2772, 0.2846, 0.2737, 0.2806, 0.2732 | 0.2772 | 1.0384 |
| report-pdf | 19061, 18514, 19344, 18671, 19010 | 11838, 11838, 11961, 11779, 11976 | 0.6211, 0.6394, 0.6183, 0.6309, 0.6300 | 0.6300 | 1.0448 |
| src-main | 42029, 40267, 43095, 40649, 42336 | 24623, 23919, 23722, 23120, 24242 | 0.5859, 0.5940, 0.5505, 0.5688, 0.5726 | 0.5726 | 1.0702 |
