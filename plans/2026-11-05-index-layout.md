# Index layout: a smaller resting index

Status: drafting (plan check next) · Repo: `navo` · Branch: `index-layout` from `main` · Started: 2026-11-05

## Goal

The index service holds about 100 MiB per 2 million files at rest instead of about 160 MiB, back toward D11's 80–100 MB. Search stays as fast as today, every peak stays within D109, and the saved snapshot keeps format v3, so older and newer builds read each other's files.

Sources: D11, D83, D103, D109; `plans/2026-11-04-index-ram.md` (RD1–RD12, shipped at `33a59962`); the two RAM audits of 2026-11-04 (per-column steady parts). Code read at `a3ba33aa`. Plan decisions are `LD1`–`LD11`.

## What exists today (paths under `crates/navo-index/src/`)

- **Table** (`table/mod.rs:38-47`): `names: String` (NUL-terminated names, append-only), `offsets`, `parents: Vec<u32>`, `flags: Vec<u16>` (5 bits used), `children` (`table/children.rs:10-15`: `first`, `next`, `previous: Vec<u32>` per slot, `waiting`), `bytes: Vec<u64>` (file size, or folder total), `counts: Vec<(u32, [u32; 2])>` (folders only, sorted by id, binary search in `totals.rs`).
- **File-reference map** (`usn/frn_map.rs`): `HashMap<u64, u32>` for NTFS references plus `HashMap<u128, u32>` for wide ones. 17 B per bucket at 7/8 load and power-of-two buckets: 35.7 B per entry at exactly 1 M or 2 M entries, 28.5 B at 5 M.
- **Search** (`search.rs:118-150`, service `navo-index-service/src/search.rs`) reads `names`, `offsets`, `flags` and `parents` only. Sizes and counts serve the totals reply (`navo-index-service/src/totals.rs`).
- **Snapshot v3** (`file.rs`, `file/stream.rs`): root, arena, then per slot offsets u32, parents u32, flags u16, sizes u64, counts 2 × u32, then (u128 reference, u32 id) pairs. Children, folder rows and the map layout are rebuilt on load. `compact_if_sparse` runs on load (`ntfs/snapshot.rs:40`) and after each batch (`ntfs/mod.rs:188`) when `slots > 2 * len`.
- **Measurement**: `heap_probe.rs`, `ntfs/peak_tests.rs` (scenarios plus `scale_report` at 2 M and 5 M), `ntfs/memory_tests.rs` (`TABLE_BUDGET = 51`, `FILE_ID_BUDGET = 40` B/entry), `search/tests.rs:220` (`search_million_names_timing`, one run, informational).

Steady bytes per entry, 18-byte names with NUL, 10 % folders, exact fit (audit model, to be confirmed by M0):

| Column | Today | Planned | How |
| --- | --- | --- | --- |
| name arena | 18 | 18 (≈ 13.5 with dedup, if gated in) | LD6 |
| offsets | 4 | 4 | LD6 |
| parents | 4 | 4 | — |
| flags | 2 | 1 | LD4 |
| child heads (`first`) | 4 | 0.8 | LD3 |
| `next` + `previous` | 8 | 8 | LD3 |
| sizes | 8 | 8 | LD5 |
| folder counts | 1.2 | 1.2 | LD5 |
| reference map | 35.7 | 6.0 (record fill 1.0), 7.5 (fill 1.25) | LD2 |
| **Total** | **84.9 B ≈ 162 MiB / 2 M** | **51.0–52.5 B ≈ 97–100 MiB / 2 M** | |

## Shape

- **Appetite:** two days of agent runs, 14 atoms. Work not done by then stops and needs a new decision.
- **Non-goals:** freeing the old index during a rebuild (RD1 stays); search scratch and pipe buffers; the walker's frontier; a dead-name-bytes compaction trigger; sizes or counts in fewer bytes; a snapshot format change.
- **No-gos:** any change that makes a v3 file unreadable, by older or newer builds; any slower search than LD10 allows.

## Decisions

Final. Workers never reopen them; a worker that finds one wrong stops and reports.

- LD1 — **Measure first.** Steady = heap the `NtfsIndex` holds after `load_with_ids` + `NtfsIndex::restore` of a saved snapshot (exact fit, what a restarted service holds), read by `held()`. Fixture `steady_index(entries, record_gap)`: root, every 10th entry a folder under the root, other entries files under the latest folder, names `entry-{n:07}.txt` (18 B with NUL), reference of entry `n` = `100 + n + n / record_gap` (gap 0 means none; gap 4 gives record fill 1.25). M0 prints per column at 1, 2 and 5 M; M1 prints real-name and real-record numbers from a snapshot; S0 records search medians on `main`.
- LD2 — **Reference map: paged dense array by MFT record, exact keys.** For a reference that fits `u64`: `record = r & 0xFFFF_FFFF_FFFF`, `seq = (r >> 48) as u16`. `DensePages` (`usn/frn_pages.rs`): `PAGE = 1024` records; directory `Vec<Option<Page>>` indexed by `record / PAGE`; `Page { ids: Box<[u32]>, seqs: Box<[u16]>, live: u16 }`, `EMPTY = u32::MAX`; pages are allocated fallibly (`try_reserve_exact`, then boxed). A record is dense only when `record < 1 << 32`. **Budget:** a new page, and directory growth to reach it, is allocated only while `dense bytes after ≤ 24 * (dense_len + 1) + 256 KiB`; otherwise the reference goes to the `overflow: HashMap<u64, u32>`. A slot holding the same record with another `seq` sends the new reference to `overflow`. `FrnMap { dense, overflow, wide }` keeps today's API and exact `HashMap<u128, u32>` semantics: `get` tries dense (seq equal), then `overflow`, then `wide`; `insert` updates `overflow` when it already holds the key; `remove`, `retain` and `remap` work in place and free a page at `live == 0`; `iter` yields dense in record order, then `overflow`, then `wide`; `try_reserve` and `with_capacity` reserve no pages (pages come on demand, never moved); `shrink_to_fit` drops trailing empty directory slots and shrinks both hash maps; `PartialEq` compares pair sets; `memory_bytes` = directory capacity × `size_of::<Option<Page>>()` + pages × 6 KiB + both hash tables. Cost: 6 B per record in live pages. Lost: a sorted `Vec<(u64, u32)>` (12 B, O(n) inserts under the write lock); a static perfect hash (needs an overlay and the keys, ≈ 12 B); table id = record number (every free MFT record costs a 27 B slot).
- LD3 — **Child heads only for folders.** `Children.first: Vec<u32>` becomes `heads: Vec<(u32, u32)>` (folder id, first child), one row per entry pushed as a folder, sorted by id like `counts`; rows leave only through compaction or rebuild. `first_child`, `of` and `attach` find the row by binary search. A child whose parent has no row (unseen, beyond `slots`, or pushed as a file) waits in `waiting`, as an unseen parent does today. `add(id, parents, folder: bool)`, `rebuild(parents, is_live, is_folder)`, `try_reserve_total(entries, folders)` (heads to `folders`). `remapped` keeps its signature and derives new rows from the old rows of kept ids. `next` and `previous` stay. Sibling order stays as today. Cost: 8 B per folder instead of 4 B per slot. Lost: a parent-sorted child array with an overlay (4 B/entry less, but O(siblings) removals in large folders and move-back duplicates); singly linked lists (O(siblings) removals).
- LD4 — **Flags column in one byte.** `Flags(u16)` and `Flags::bits() -> u16` stay public; the column stores `StoredFlags` (alias in `table/flags.rs`, `u16` until F1c, then `u8`). Disk stays u16: `stream.rs` writes `u16::from(flag)`; `file.rs` decodes `StoredFlags::try_from(u16::from_le_bytes(b)).unwrap_or(StoredFlags::MAX)`, so unknown high bits still fail `from_raw_parts`.
- LD5 — **Sizes and counts stay.** Lost: 48-bit sizes with u64 folder totals in the folder rows (saves ≈ 1.2 B; a chosen network folder (D103) can exceed 256 TiB, so folder totals need 64 bits).
- LD6 — **Names: dedup during builds and compaction, gated.** Offsets (u32) and the NUL arena stay. N1–N3 run only if M1 shows a saving ≥ 15 % of live name bytes on real names, and N3 keeps search within LD10. `Interner` (`table/interner.rs`) = `hashbrown::HashTable<u32>` of arena offsets, hashed by name with a `RandomState`; it lives only between `begin_interning` and `end_interning`, and compaction uses a local one. Pushes outside a build and renames append as today; the next compaction folds them. An allocation failure turns interning off with a warning. Lost: front coding (needs sorted names); offset checkpoints every 16 ids (−3.75 B, but conflicts with dedup and needs v3 arena normalisation on load); 24-bit offsets (arenas exceed 16 MiB).
- LD7 — **Snapshot format v3 unchanged.** Every atom that touches `file/`, `table/` or `usn/` keeps `format_v3_bytes_are_unchanged` (L0) passing unedited. A deduplicated arena is valid v3 (shared offsets pass `from_raw_parts`), so older builds read it. An atom that finds it needs a format change stops. **Any later format change** is v4 with a versioned migration: the reader keeps reading v3; v4 is written through `navo_store::write_atomic_streamed` (temporary file, flush, rename); the v3 file stays until the v4 file has been loaded back and verified (checksum, `from_raw_parts`, `validate_ids`), then it is deleted; an older build meets `UnsupportedVersion` and rebuilds; `senior` builds it and `reviewer-senior` checks it.
- LD8 — **Sparse trigger at 1.5×.** `pub fn is_sparse(&self, min_slots: u32) -> bool` on `NameTable` (`table/compact.rs`): `slots >= min_slots && slots as usize > len + len / 2`. `NtfsIndex::compact_if_sparse` and the host's `compact_if_sparse` (`host/scan.rs:262`) call it. A sparse index then stays ≤ ≈ 1.4× its compact size (≈ 138 MiB at 2 M, inside D109). Bad: one O(slots) compaction per `len / 2` deletions instead of per `len`.
- LD9 — **Peak reading** (Q1 default until the owner answers). Each peak ≤ 1.7 × the larger of the resting index before and after the operation (compaction: the sparse index before it). A rebuild keeps the old index whole (RD1) and stays ≤ 2.2 × the old index.
- LD10 — **Search gate.** S0's `search_scale_timing`, release profile, 7 runs per query, median. Run `main` and the branch back to back on the same machine, twice, alternating. Pass: each query's branch median ≤ `main` × 1.05 in both pairs; one rerun of a failing pair is allowed. The service `timing_tests` keep passing.
- LD11 — **Check form** (RD11): `flock ~/.cache/rusty-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 CARGO_TARGET_DIR=<wt>/target cargo nextest run -p <crate> <filter> > <wt>/../<atom>.log 2>&1; echo $?`, then the same prefix with `cargo xtask check -p <crate> >> <wt>/../<atom>.log 2>&1; echo $?`, then `tail -n 40` of the log. Pass = both exit 0. No atom edits Windows-only files (`worker.rs`, `ntfs/windows.rs`).

## Owner questions

Recorded with defaults; the build does not wait.

- Q1 — D109 says peaks ≤ 1.7×, but a rebuild that keeps the old index searchable (RD1) needs about 2×. Accept a rebuild ceiling of 2.2 × the old index (≈ 215 MiB at 2 M, down from ≈ 340 MiB today), and measure compaction against the sparse index before it? **Recommended and default: yes** (LD9).
- Q2 — May M1 run read-only on your PC's index snapshot? It prints counts only (no names, nothing copied) and decides whether dedup is worth it. **Recommended: yes. Default: the VM snapshot only.**
- Q3 — Stop at ≈ 100 MiB per 2 M files (≈ 50 B per file), or aim for 80 MB with a second plan (parent-sorted children, offset checkpoints, ≈ 8 B per file more saved, harder edits)? **Recommended and default: stop at ≈ 100 MiB; revisit after the VM numbers.**

## Risks and rollback

- R1 — The reference map returns a wrong id, so a journal change lands on the wrong entry. Prevented: exact-key semantics with `seq` (LD2), an oracle against `HashMap<u128, u32>` over seeded operation runs, `senior`, `reviewer-senior` on the branch.
- R2 — Dense pages grow on a sparse MFT. Prevented: the LD2 budget with `overflow`; a test with 100,000 references spread up to record `2^31` stays ≤ 24 B per reference + 256 KiB.
- R3 — Lost child links make totals fail, so every start rebuilds. Prevented: existing children, totals and compaction oracle tests pass unchanged; `senior`.
- R4 — Flags written as one byte break the format. Prevented: L0 golden bytes and a corrupt-flags load test after F1c.
- R5 — Dedup slows search. Prevented: N3 gate; N2 is parked when it fails.
- R6 — Downgrade: v3 is unchanged, so an older build reads the new files.
- No user file is written; the index is derived data. Rollback: `git revert <sha>`; a bad snapshot is fixed by deleting `%ProgramData%\Navo\index\<letter>.index`, then the service rebuilds.

## Components

| Component | Paths (`crates/navo-index/src/`) | Contract | Test |
| --- | --- | --- | --- |
| `steady-tests` | `ntfs/peak_tests.rs`, `ntfs/memory_tests.rs` | fixture → bytes per column | itself |
| `snapshot-stats` | `file/stats_tests.rs` | v3 file → counts | itself (ignored) |
| `search-timing` | `search/timing_tests.rs` | 2 M table → medians | itself (ignored) |
| `frn-pages` | `usn/frn_pages.rs` | record, seq → id | in file |
| `frn-map` | `usn/frn_map.rs`, `usn/frn_map_oracle_tests.rs` | u128 → id (exact) | oracle |
| `table-children` | `table/children.rs`, `table/mod.rs`, `table/growth.rs` | parent → ordered children | in file, totals, compaction oracle |
| `table-flags` | `table/flags.rs`, `table/mod.rs`, `table/totals.rs`, `table/change.rs`, `table/memory.rs`, `file.rs`, `file/stream.rs` | flags column ↔ v3 u16 | L0, `file`, `table` |
| `table-interner` | `table/interner.rs`, `table/mod.rs`, `ntfs/volume_io.rs`, `table/compact.rs` | names → shared offsets | in file, compaction oracle |
| `sparse-trigger` | `table/compact.rs`, `ntfs/compact.rs`, `host/scan.rs` | slots, len → compact? | `table/compact.rs` tests |

## Atoms

`api-builder` runs on DeepSeek. Each atom shows its new test failing first, except **exempt** ones (measurements, tidies, guards). Atoms in `navo-index` build one at a time. Line numbers are at `a3ba33aa`.

| Atom | Component | Files | Role | Effort | After | Check: crate, filter | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| M0 | `steady-tests` | `ntfs/peak_tests.rs`, `table/memory.rs`, `table/children.rs` | api-builder | medium | — | `navo-index`, `steady` | todo |
| M1 | `snapshot-stats` | `file/stats_tests.rs` (new), `file.rs` | api-builder | medium | — | `navo-index`, `file`; VM run | todo |
| S0 | `search-timing` | `search/timing_tests.rs` (new), `search.rs` | api-builder | medium | — | `navo-index`, `search` | todo |
| P1 | `frn-pages` | `usn/frn_pages.rs` (new), `usn/mod.rs` | **senior** | high | M0 | `navo-index`, `frn_pages` | todo |
| P2 | `frn-map` | `usn/frn_map.rs`, `usn/frn_map_oracle_tests.rs` (new) | **senior** | high | P1 | `navo-index`, `usn file ntfs`; `navo-index-service`, `tests` | todo |
| C0 | `table-children` | `table/children.rs`, `table/mod.rs`, `table/growth.rs` | **senior** | high | M0 | `navo-index`, `table` | todo |
| F1a | `table-flags` | `table/flags.rs`, `table/mod.rs`, `table/change.rs` | patcher | medium | C0 | `navo-index`, `table` | todo |
| F1b | `table-flags` | `table/totals.rs`, `table/memory.rs`, `table/totals_tests.rs` | patcher | medium | F1a | `navo-index`, `table` | todo |
| F1c | `table-flags` | `table/flags.rs`, `file.rs`, `file/stream.rs` | **senior** | medium | F1b | `navo-index`, `file table` | todo |
| K1 | `sparse-trigger` | `table/compact.rs`, `ntfs/compact.rs`, `host/scan.rs` | patcher | medium | — | `navo-index`, `compact host peak` | todo |
| N1 | `table-interner` | `table/interner.rs` (new), `table/mod.rs`, `navo-index/Cargo.toml` | api-builder | high | F1c, M1 gate | `navo-index`, `table` | blocked (LD6) |
| N3 | `search-timing` | `search/timing_tests.rs` | patcher | medium | N1, S0 | LD10 runs | blocked (LD6) |
| N2 | `table-interner` | `ntfs/volume_io.rs`, `table/compact.rs`, `table/compact_oracle_tests.rs` | **senior** | medium | N3, K1 | `navo-index`, `table compact ntfs peak` | blocked (LD6) |
| G1 | `steady-tests` | `ntfs/peak_tests.rs`, `ntfs/memory_tests.rs` | patcher | medium | P2, F1c, K1, N2 or parked | `navo-index`, `peak memory steady` | todo |

### M0 — steady per column (exempt: measurement)

- **Read:** `ntfs/peak_tests.rs`, `table/memory.rs`, `table/children.rs:1-30,130-145`, `heap_probe.rs:60-105`.
- **Change:** `#[cfg(test)] pub(crate) fn column_bytes(&self) -> Vec<(&'static str, usize)>` in `memory.rs` (capacity × size per column: `names`, `offsets`, `parents`, `flags`, `children.first`, `children.next`, `children.previous`, `children.waiting`, `bytes`, `counts`), using a `#[cfg(test)] fn column_bytes()` in `children.rs`. In `peak_tests.rs`: LD1's `steady_index`, saved and loaded through `file::save_with_ids` / `load_with_ids` + `NtfsIndex::restore`; `#[ignore = "steady report"] fn steady_report()` prints `STEADY <M> gap <g>: <column> <B/entry> … frn <B/entry> total <B/entry> held <KiB>` for 1, 2, 5 M and gaps 0 and 4; a passing `steady_stays_within_budget` at 1 M, gap 0: held / entries ≤ `STEADY_PER_ENTRY` (measured × 1.02, rounded up) and the column sum plus `ids.memory_bytes()` within 1 % of `held`.
- **Report:** the full report lines (also into the Log). **Stop if:** the column sum and `held` differ by more than 1 % (a column is missing from the accounting).

### M1 — real snapshot counts (exempt: measurement)

- **Read:** `file.rs:105-145`, `file/stream.rs:120-175`, `table/mod.rs:93-100`.
- **Change:** `file/stats_tests.rs` (`#[cfg(test)] #[path = "file/stats_tests.rs"] mod stats_tests;` in `file.rs`), one `#[ignore = "real snapshot stats"]` test `snapshot_stats` reading the path in `NAVO_INDEX_STATS` via `load_with_ids`. Prints `STATS <key> <value>` for: slots, live, folders, arena bytes, live name bytes, unique live name bytes, dedup saving %, mean name bytes, references, wide references, max record, dense pages (distinct `record / 1024` with `record < 2^32`), dense B/reference (`pages * 6144 / narrow references`), records with more than one reference. Never prints a name or path.
- **VM run (orchestrator):** elevated shell, `NAVO_INDEX_STATS=$env:ProgramData\Navo\index\C.index`, the LD11 command with `--run-ignored only --no-capture snapshot_stats`. Same on your PC only if Q2 is yes. Log the lines. **Gate:** dedup saving ≥ 15 % unblocks N1.
- **Stop if:** reading needs a change outside these two files.

### S0 — search baseline at 2 M (exempt: measurement)

- **Read:** `search.rs:1-60`, `search/tests.rs:210-235`.
- **Change:** `search/timing_tests.rs` (`#[cfg(test)] #[path = "search/timing_tests.rs"] mod timing_tests;` in `search.rs`). `fn fixture() -> NameTable`: 2 M entries, every 10th a folder, 30 % of names drawn from a fixed pool of 5,000 names (xorshift, seed 7), the rest unique, about 18 B each. `#[ignore = "timing"] search_scale_timing`: three queries — `[Name("doc")]`, `[Name("report"), Ext(&["pdf"])]`, `[Path("src"), Name("main")]` — limit 500, 7 runs each after one warm-up; prints `SEARCH <label>: median <µs> µs`.
- **Run on `main`:** the LD11 prefix with `cargo nextest run -p navo-index --cargo-profile release --run-ignored only --no-capture search_scale_timing`; log the medians.
- **Stop if:** a query returns no hits (fix the fixture names, not the search).

### P1 — dense pages

- **Read:** `usn/frn_map.rs`, `heap_probe.rs:1-60`.
- **Change:** `DensePages` per LD2 in `usn/frn_pages.rs` (`mod frn_pages;` in `usn/mod.rs`, `pub(super)`): `get(record: u64, seq: u16) -> Option<u32>`, `try_insert(record, seq, id) -> Result<Placed, TryReserveError>` with `enum Placed { Inserted, Updated(u32), Refused }` (`Refused` = budget, `record >= 2^32`, or slot held by another `seq`), `remove(record, seq) -> Option<u32>`, `retain(FnMut(u64, u16, u32) -> bool)`, `remap(FnMut(u32) -> Option<u32>)`, `iter() -> impl Iterator<Item = (u64, u16, u32)>` in record order, `len`, `clear`, `shrink_to_fit`, `memory_bytes`.
- **Test first** (in file): 1 M consecutive records use ≤ 6.1 B/record; a lone record `2^31` is `Refused` (the directory alone would break the budget); 100,000 records spread to `2^31` stay ≤ 24 B each + 256 KiB; a page is freed when its last record goes; another `seq` in a held slot is `Refused`; `remap` and `retain` never allocate (probe); an injected allocation failure returns `Err` and leaves the map unchanged (`heap_probe::fail_allocations_from`).
- **Stop if:** `Page` cannot stay ≤ 6 KiB + 48 B, or a test needs `frn_map.rs`.

### P2 — `FrnMap` on dense pages

- **Read:** `usn/frn_map.rs`, `usn/frn_pages.rs`, `file/stream.rs:140-155`, `file/tests.rs:540-570`, `navo-index-service/src/tests.rs:380-400`.
- **Change:** LD2's `FrnMap { dense, overflow, wide }`; manual `PartialEq`; same public API. Rewrite only the in-file tests that read `narrow`/`wide` fields (`memory_counts_allocated_buckets`, `remap_rewrites_both_widths_…`) to assert through `memory_bytes`, `len` and `get`.
- **Test first** (`frn_map_oracle_tests.rs`, `#[cfg(test)] #[path] mod`): 50 seeded runs of 2,000 operations (insert, remove, get, retain, remap, iter, clear) against `HashMap<u128, u32>`, references mixing small records, records ≥ `2^32`, `u64::MAX`, wide ones, and same-record-other-seq pairs; after each operation `len`, every `get` and the sorted `iter` agree. Memory: 1 M references `100 + n` ≤ 6.2 B each.
- **Check also:** L0 passes unedited; `navo-index-service` `tests` pass. **Stop if:** any test outside `frn_map.rs` needs a change.

### C0 — child heads for folders only

- **Read:** `table/children.rs`, `table/mod.rs:56-70,200-275,336-345`, `table/totals.rs:150-185`, `table/growth.rs:36-75,140-215`.
- **Change:** LD3. `mod.rs`: `push` calls `children.add(id, &self.parents, flags.contains(Flags::FOLDER))`; `relink` passes `is_folder` from the flags column. `growth.rs`: `try_reserve_total` calls `children.try_reserve_total(entries, folders)`; in its test, the heads capacity expectation becomes 10,000 (folders). `capacities()` returns `[heads, next, previous]`.
- **Test first** (in `children.rs`): a 1 M-file table under one folder holds ≤ 8 B per slot + 64 KiB in `Children`; a child pushed under a file id waits and is not listed; `of` on a file id is empty. Existing tests in `table/`, `totals_tests.rs`, `compact_oracle_tests.rs` and `ntfs/compact_*tests.rs` pass unedited.
- **Stop if:** `compact.rs` or `remapped`'s signature must change, or any other test changes.

### F1a — flags alias, part 1 (exempt: tidy)

- **Change:** `table/flags.rs`: `pub(crate) type StoredFlags = u16;`, `DELETED: StoredFlags`, `pub(super) const fn stored(self) -> StoredFlags`, `from_stored(StoredFlags)`. `table/mod.rs`: field, `raw_parts`, `from_raw_parts` use `StoredFlags`. `table/change.rs`: the three `flags.bits()` writes become `flags.stored()`.
- **Stop if:** anything but a rename of types is needed.

### F1b — flags alias, part 2 (exempt: tidy)

- **Change:** `totals.rs:129` closure takes `&StoredFlags`; `memory.rs` uses `size_of::<StoredFlags>()` (both places); `totals_tests.rs:261` helper takes `&mut Vec<StoredFlags>`.
- **Stop if:** anything but a rename of types is needed.

### F1c — flags in one byte

- **Change:** LD4. `StoredFlags = u8`; `Flags::stored` narrows (all defined bits fit); `from_stored` widens. `stream.rs:39-41` writes `u16::from(flag).to_le_bytes()`. `file.rs:119` decodes per LD4.
- **Test first** (`table` tests): a 1 M-slot table's flags column is 1 B/slot. L0 passes unedited; the corrupt-flags case at `file/tests.rs:389` still rejects.
- **Stop if:** L0 bytes change, or a file outside these three needs an edit.

### K1 — sparse trigger at 1.5×

- **Read:** `table/compact.rs`, `ntfs/compact.rs`, `host/scan.rs:255-270`.
- **Change:** LD8. `NameTable::is_sparse` in `table/compact.rs`; `ntfs/compact.rs::compact_if_sparse` returns `false` unless `self.table.is_sparse(min_slots)`, and its doc says "once dead slots pass a third of all slots"; `host/scan.rs:263` uses `table.is_sparse(COMPACT_MIN_SLOTS)`.
- **Test first** (`table/compact.rs` tests): 150,001 slots with 100,000 live is sparse at `min_slots = 100_000`, 150,000 slots is not, and a table below `min_slots` is not.
- **Stop if:** an existing test in `ntfs/compact_tests.rs`, `peak_tests.rs` or `host/tests.rs` changes.

### N1 — interner (blocked by the M1 gate)

- **Read:** `table/mod.rs`, `table/growth.rs:75-90`, `Cargo.toml`.
- **Change:** LD6. `hashbrown = { version = "=0.16.1", default-features = false }` (already in `Cargo.lock`). `table/interner.rs`: `Interner { table: HashTable<u32>, hasher: RandomState }`, `find(&self, names: &str, name: &str) -> Option<u32>`, `try_insert(&mut self, names: &str, offset: u32) -> Result<(), TryReserveError>`. `NameTable`: `interner: Option<Box<Interner>>`, `pub fn begin_interning(&mut self)`, `pub fn end_interning(&mut self)`; `append_name` returns a found offset without writing; `memory_parts().names` includes the interner while it lives; `live_name_bytes` returns `min(sum, names.len())`.
- **Test first:** 100,000 names from a 1,000-name pool use ≤ the pool's bytes + root; every `name(id)` equals a non-interned twin; renaming one entry that shares a name leaves the others' names intact; `end_interning` frees the interner; an injected allocation failure turns interning off and every name stays correct.
- **Stop if:** `rename` or `from_raw_parts` needs a change.

### N3 — search gate with shared names

- **Change:** S0's fixture calls `begin_interning()` before its pushes and `end_interning()` after. Run LD10 against S0's `main` medians.
- **Report:** medians, both pairs. **Stop if:** a query is over +5 %: N2 is parked, N1 stays as unused API.

### N2 — intern in builds and compaction

- **Read:** `ntfs/volume_io.rs:54-115`, `table/compact.rs`, `table/compact_oracle_tests.rs`.
- **Change:** `enumerate_volume_sized`: `begin_interning()` after the reserves, `end_interning()` before `trim_slack`. `compact.rs::kept_names` interns through a local `Interner`; capacity `min(bytes, self.names.len())`. Oracle: the raw arena comparison becomes per-id name equality plus `names.len() <= reference.len()`; all other comparisons stay.
- **Test first:** a compaction of a table with 50 % repeated names yields an arena ≤ 60 % of today's; the `peak` tests pass with `build_sized` ≤ 12 B/entry.
- **Stop if:** a test other than the oracle's arena comparison changes, or `build_sized` exceeds 12 B (report the number).

### G1 — final ceilings (exempt: guard)

- **Change:** `STEADY_PER_ENTRY`, `TABLE_BUDGET`, `FILE_ID_BUDGET` and the peak consts set to measured × 1.1 (steady × 1.02); add `peaks_stay_within_d109`: each peak scenario's (held + peak) / held ≤ 1.7, compaction against the sparse index before it. Rerun `steady_report` and `scale_report`; log both.
- **Stop if:** a Done-when target is missed: report the numbers.

## Done when

- `steady_report`, gap 0, 2 M: total ≤ 52 B/entry (≤ 99.2 MiB per 2 M); gap 4: ≤ 54 B/entry. Reference map ≤ 6.2 B (gap 0). With N2 in: the VM's `C:` B/entry logged next to M1's.
- `peaks_stay_within_d109` passes at 1 M; `scale_report` at 2 M and 5 M shows every ratio ≤ 1.7 (rebuild ≤ 2.2 per Q1).
- LD10 passes on the final branch. L0 passes unedited. Service `timing_tests` (`--run-ignored all -j 1 timing_tests`) pass.
- Full gate once, `reviewer-senior` on the branch, VM CI green.
- **Windows VM** (never your PC; RAM plan Done-when steps 1–4): steady `PagedMemorySize64` after `snapshot restored T:` and 60 s idle is ≤ 65 % of the same step on `main`; a journal reset build peaks ≤ 2.2 × steady; Ctrl+K finds `T:` names during and after the build; a snapshot written by the branch loads in the `main` build and the other way round.

## Log

- 2026-11-05 — Plan written. Q1–Q3 recorded with defaults. Steady model: 84.9 → 51.0–52.5 B/entry (≈ 162 → 97–100 MiB per 2 M); dedup gated on M1.
