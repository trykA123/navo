# Shared listing · one row set per folder, sorted views as index orders

Status: done · Repo: `navo` · Branch: one `sl<id>` branch per atom from `main` (at or after 1fd013b0) · Started: 2026-11-05

Parent: `plans/2026-10-29-speed-atoms.md` (SP7, D1, D2, D19). This plan continues SP7. It does not repeat SP7e/f/i (index orders and async views already exist); it removes the materialisation step SP7's D2 kept.

## Goal

Sorting a 100k-file folder copies no rows (today about 3.1 MiB of entries per sort, twice when filtered). Two tabs that show the same folder generation with the same sort share one sorted view instead of sorting twice. A watcher patch no longer rebuilds a name map of the whole folder. Cursor, marks and the shown order stay exactly as today.

Appetite: one day, 7 atoms, 2 of them mechanical sweeps. Non-goals: one shared fetch for two panes (see Later), core `Listing` names (Q1), find results, the disk cache format, `navo-index`.

## Decisions

Final. A worker that finds one wrong stops and reports.

- SL1 — **Supersedes speed-atoms D2 (accepted 2026-10-29), part "materialised rows on the UI".** The UI holds `navo_fs::view::Rows` (source `Arc<[FsEntry]>` + optional `Arc<[u32]>` order) instead of a materialised `Arc<[FsEntry]>`. D2's cost argument (~50 call sites) is met by a slice-like API: `get`, `len`, `iter`, `Index<usize>` keep most call sites unchanged. The orchestrator records this as D110 in `docs/decisions.md` when L6 merges.
- SL2 — **One canonical row set per folder generation.** A generation is the `Arc<[FsEntry]>` in `CachedListing.rows`. Every view (sort, filter, visibility) is `Rows` over that Arc. Rows with folder sizes (`remember_sizes`, `size_rows`) stay a per-tab copy, as today; later views sort that copy, so sizes survive.
- SL3 — **Sort over the source, ties by source index.** `resort` and `ViewWork::run` sort `full.details.source()`, never the previous view. `sort_order` already breaks ties by index (`navo-fs/src/view/sort.rs:162-163`), so the result no longer depends on the previous sort. Only names equal when lower-cased can change place (case-sensitive providers); see Q2.
- SL4 — **Row-for-row alignment is the invariant.** For every tab, `rows(tab).get(i).name.to_text() == listing.get(i).name` for all `i`. Cursor (`ViewState.cursor`, a row number) and marks (`Selection` bits by row) stay keyed as today: row numbers in the core `Listing`, remapped by name in `Tab::replace_listing`, `replace_listing_near` and `keep_by_name` (`navo-core/src/state/tab.rs:278-336`). Nothing in this plan changes `navo-core`.
- SL5 — **Persistent name lookup lives in the cache slot.** `store.rs` `Slot` gains `lookup: Option<NameLookup>`, replaced with the slot (put, swap, invalidate, evict), so it never outlives its rows. `NameLookup { seed: RandomState, map: HashMap<u64, u32> }`: key = `seed.hash_one(&name)`, value = row index; no names held. The seed is kept for the lookup's whole life and carried to the next generation. Its bytes count in the slot as `map.capacity() × (8 + 4 + 1)` plus `size_of::<NameLookup>()`.
- SL6 — **Patch in place, never clone the map.** `ListingCache::patch` takes the listing and its lookup out of the slot under one lock (`take_lookup(key) -> (Arc<CachedListing>, Option<NameLookup>)`; the slot keeps the listing, its lookup becomes `None`), patches the map in place, and puts it back with the swap. A concurrent patch finds `None` and builds a new lookup (correct, slower). A `Stale` swap drops the lookup.
- SL7 — **Patch semantics stay exactly `IndexedRows`'** (`navo-cache/src/listing.rs:48-107`): upsert replaces in place or appends; remove drops and keeps order; rename removes a different target first, then moves in place or appends. A new generation is one clone pass of the old rows (unavoidable while names are owned; Q1). Removals renumber the map in one `u32` pass.
- SL8 — **Collision fallback for the whole batch.** When an insert finds its hash taken by a different name, the lookup is dropped and the whole batch restarts from the original rows through today's `IndexedRows`, which keys by the full name. The next patch builds a new lookup with a new seed. Correct, not fast; a test forces it.
- SL9 — **Tabs share sorts through the memo, same bounds.** A cross-tab memo lookup (same source Arc, same key, same `folders_first`) runs in `sorted_full`, in `adopt` (before taking a worker result) and in `fetch` (a hit for the cached rows shows synchronously instead of starting an `Early` job). A hit copies the `Full` (two Arc bumps) into this tab's memo. Per-tab cap `KEEP_SORTED = 2` and the drop in `drop_stale_filters` stay, so retention is bounded by open tabs.
- SL10 — **Meters.** `navo_fs::view::copied_row_bytes()` is a thread-local count of `size_of::<FsEntry>() + name bytes` for every row cloned by `materialize`, `Rows::to_vec` and `Rows::to_arc` on an ordered view; `reset_copied_row_bytes()` zeroes it; `#[doc(hidden)]`, always on. Sorts are counted per `Browser` by `SortCount(Arc<AtomicUsize>)`, cloned into `Fetch`, `Early` and `ViewWork` and bumped in `sorted_rows`, so worker and UI sorts both count; `#[cfg(test)] Browser::sorts()` reads it. `ui_sorts` stays for its existing tests.

## Risks and rollback

- R1 — Misaligned rows and listing would make an action hit the wrong file (open, delete, rename use row → detail). Prevented by SL4 tests in L3, L6 and L7, and the name check in `multi_rename/gather.rs:66`.
- R2 — Stale lookup corrupts a patched listing. Prevented: owned by the slot and taken under the same lock as the listing (SL5, SL6), every hit verified by name, L2's proptest against the old `IndexedRows`.
- R3 — Arc retention. Memo bounded per tab (SL9); lookup dies with its slot; `TotalsCache` keeps `WeakRows`.
- Rollback: each atom is one squash commit; `git revert <sha>`. L1–L5 change no behaviour, so reverting L6 or L7 alone restores today's behaviour.

## Components

| Component | Path | Contract (in → out) | New / change | Test |
| --- | --- | --- | --- | --- |
| `rows-view` | `navo-fs/src/view/{rows,filter}.rs` | source Arc + order → slice-like view, copy meter | new | `view/rows/tests.rs` |
| `name-lookup` | `navo-cache/src/{listing,store,cache}.rs` | rows + changes → next generation, lookup carried | change | `tests/props.rs`, `tests/patch_alloc.rs` |
| `browser-rows` | `navo-ui/src/browser/{mod,filter/mod,view_jobs}.rs` | per-tab `Full`/`Shown`/`details` as `Rows`; `Browser::rows(tab)` | change | browser tests |
| `list-rows` | `file_list/mod.rs`, `window_view/{pane,status}.rs`, gallery | `FileListProps.rows: Rows`, totals by `WeakRows` | change | file_list + status tests |
| `row-consumers` | `window_view/{git_badges,preview/follow,shell_menu/*,multi_rename/gather}.rs` | `rows(tab)` instead of `details(tab)` | change | existing tests |
| `index-views` | `navo-ui/src/browser/{sort/mod,filter/mod,view_jobs}.rs` | sort/filter → ordered `Rows` on UI and worker | change | `sort/tests.rs`, `filter/tests.rs` |
| `shared-sorts` | `navo-ui/src/browser/{cached/mod,cached/early,mod}.rs` | cross-tab memo, sort counter | change | `cached/tests.rs` |

## Atoms

Commands: every cargo call runs as in speed-atoms "Shared commands" (`flock`, `nice -n 10`, `CARGO_BUILD_JOBS=8`, own target dir) and writes to a log: `… > ../<atom>.log 2>&1; echo $?`, then `tail -n 40`.

| Atom | Component | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| L1 | `rows-view` | api-builder | medium | — | `cargo xtask check -p navo-fs` | done |
| L2 | `name-lookup` | api-builder | high | — | `cargo xtask check -p navo-cache` | done |
| L3 | `browser-rows` | api-builder | medium | L1 | `cargo xtask check -p navo-ui` | done |
| L4 | `list-rows` | ui-builder | medium | L3 | `cargo xtask check -p navo-ui -p navo-ui-gallery` | done |
| L5 | `row-consumers` | patcher | medium | L3 | `cargo xtask check -p navo-ui` | done |
| L6 | `index-views` | senior | high | L4, L5 | `cargo xtask check -p navo-ui` | done |
| L7 | `shared-sorts` | senior | high | L6 | `cargo xtask check -p navo-ui` | done |

Waves: {L1, L2} → L3 → {L4, L5} → L6 → L7. L2 may run on DeepSeek; L3–L7 never do (navo-ui).

### L1 — `Rows` view type (navo-fs)

- **Read:** `crates/navo-fs/src/view/mod.rs`, `view/filter.rs:39-83` (`filter_order`, `filter_all`, `materialize`).
- **Files (4):** new `view/rows.rs`, new `view/rows/tests.rs`, `view/mod.rs` (export), `view/filter.rs` (`materialize` bumps the meter).
- **Change:** `#[derive(Clone, Debug, Default)] pub struct Rows { source: Arc<[FsEntry]>, order: Option<Arc<[u32]>> }`. `Rows::all(Arc<[FsEntry]>)`, `Rows::ordered(Arc<[FsEntry]>, Arc<[u32]>)` (debug-asserts every index `< source.len()`), `len`, `is_empty`, `get(usize) -> Option<&FsEntry>`, `iter() -> impl ExactSizeIterator<Item = &FsEntry> + DoubleEndedIterator`, `source() -> &Arc<[FsEntry]>`, `order() -> Option<&Arc<[u32]>>`, `narrowed(&self, keep: Vec<u32>) -> Rows` (keep = source indices, as `filter_order` returns), `same(&Rows, &Rows) -> bool` (source and order `Arc::ptr_eq`, both-`None` counts as equal), `to_vec`, `to_arc` (identity returns `Arc::clone(source)`, no copy), `downgrade() -> WeakRows` with `WeakRows::is_view_of(&Rows) -> bool`. `impl Index<usize>`, `From<Arc<[FsEntry]>>`, `From<Vec<FsEntry>>`, `FromIterator<FsEntry>`. SL10 copy meter.
- **Tests:** identity and ordered `get`/`iter`/`len` agree with `materialize` (proptest, random rows and permutation); `narrowed` over `filter_order` equals `materialize` of the same keep; `to_arc` on identity is `ptr_eq` and meters 0; ordered `to_vec` and `materialize` meter exactly the row bytes; `same` and `WeakRows` after the source drops.
- **Check:** `cargo xtask check -p navo-fs` exits 0.
- **Stop if:** a fifth file needs an edit.

### L2 — Persistent name lookup in the cache (navo-cache)

- **Read:** `crates/navo-cache/src/listing.rs:1-107`, `store.rs:1-120`, `cache.rs:54-74`, `tests/patch.rs`, `tests/props.rs:1-60`.
- **Change:** SL5–SL8. `listing.rs`: `NameLookup` (`build(&[FsEntry]) -> Option<Self>`, `find(&self, rows, &ProviderPath) -> Option<u32>`, `bytes()`); `CachedListing::patched_with(&self, changes, changed_at, lookup: Option<NameLookup>) -> (Self, Option<NameLookup>)`; `IndexedRows` stays as the SL8 fallback; `patched` wraps `patched_with(…, None)`. `store.rs`: `Slot`/`SizedListing` carry `lookup` and its bytes; `take_lookup(key)`; `swap` installs the returned lookup. `cache.rs::patch` uses them; the retry loop and the `read_at > changed_at` guard stay.
- **Tests:** `tests/props.rs`: today's `IndexedRows` copied verbatim into the test as the oracle; proptest over random unique names (include names equal when lower-cased) and change sequences of Added/Modified/Removed/Renamed (onto an existing name, onto itself, from a missing name): rows equal the oracle, the lookup finds every row. Forced collision (a `#[cfg(test)]` constant hasher) mid-batch still equals the oracle. Two patches in a row reuse the seed and keep `map.capacity()` within 2 × rows. New `tests/patch_alloc.rs` with its own counting `#[global_allocator]`, end to end through `ListingCache::put` + `patch`: 100k rows, one warm patch, then one `Modified` patch. Target: bytes allocated ≤ the row clone (`Σ size_of::<FsEntry>() + name bytes`) + 64 KiB, and ≤ 0.6 × the oracle measured the same way. Print all three numbers.
- **Check:** `cargo xtask check -p navo-cache` exits 0; existing `race.rs`, `patch.rs`, `disk*.rs` pass unchanged.
- **Stop if:** a `CachedListing` struct literal outside navo-cache would need a new field (it must not), or the disk codec changes.

### L3 — Browser keeps `Rows`, identity only (navo-ui)

- **Read:** `browser/filter/mod.rs:16-31,152-177,206-264,288-326`, `browser/mod.rs:38-60,236-242`, `browser/view_jobs.rs:13-66,100-130,150-260`.
- **Change:** no behaviour change. `Full.details`, `Shown.details` and `Browser.details` become `Rows`; every construction is `Rows::all` or `.into()`. Slices for `filter_all`, `materialize` and `sorted_full` come from `to_arc()` (zero copy on identity). `view_jobs`: `ViewJob.rows: Rows`, `Arc::ptr_eq` → `Rows::same`, `resized` takes `(&Rows, &Rows)`. Add `pub fn rows(&self, tab: TabId) -> Rows` next to `details()`, same loaded guard; `details()` returns `rows(tab).to_arc()`. One-token conversions (`.into()`, `Rows::all`, `.to_arc()`) are allowed in `browser/{sort/mod,cached/mod,cached/early,find,find/leave,size_rows,open}.rs`; nothing else there.
- **Tests:** all browser tests pass unchanged. Add one: after load, sort, filter and a size measure, `rows(tab)` and the listing are aligned (SL4).
- **Check:** `cargo xtask check -p navo-ui` exits 0.
- **Stop if:** a file outside the lists above needs an edit, or an edit is more than a type or conversion change.

### L4 — File list and status read `Rows` (navo-ui, gallery)

- **Read:** `file_list/mod.rs:60-90`, `window_view/pane.rs:140-185`, `window_view/status.rs:1-83`.
- **Change:** `FileListProps.rows: Rows`. `pane.rs::list_props` and the status call use `self.browser.rows(tab.id())`. `TotalsCache` keeps `(WeakRows, Totals)` and hits on `is_view_of`; `totals_of` iterates `Rows`. Mechanical sweep, `rows: X` → `rows: X.into()` (or the page field becomes `Rows`), in `navo-ui-gallery/src/{mouse_page,kinds_page,git_badges_page,drives_page,window_page,menu_page,pane_page,multi_rename_preview_page,command_center_page}.rs`, `navo-ui-gallery/src/window_page/card.rs:14` and `file_list/tests/mod.rs:62`. The gallery's own `pane_page::status_of(&[FsEntry])` stays. File list code using `.get`/`.len` stays as is.
- **Tests:** existing file_list and status tests; add one: two `Rows` over one source with different orders compute totals once each, and a dropped source misses.
- **Check:** `cargo xtask check -p navo-ui -p navo-ui-gallery` exits 0.
- **Stop if:** a file_list module other than `mod.rs` needs more than the sweep edit.

### L5 — Remaining consumers use `rows()` (navo-ui)

- **Read:** `window_view/git_badges.rs:15-30,95-120,180-200`, `preview/follow.rs:55-75`, `shell_menu/targets.rs:1-40`, `shell_menu/prefetch.rs:88-97`, `shell_menu/mod.rs:70-80`, `multi_rename/gather.rs:50-70`.
- **Change:** `browser.details(` → `browser.rows(`; parameters `&[FsEntry]` → `&Rows`; `Arc<[FsEntry]>` fields → `Rows`; `Arc::ptr_eq` → `Rows::same`. Exactly these call sites, no logic change.
- **Check:** `cargo xtask check -p navo-ui` exits 0, and `grep -rn '\.details(' crates/navo-ui/src crates/navo-app/src | grep -v test | grep -v input_trace/provider.rs` prints nothing.
- **Stop if:** a helper needs a slice-only method `Rows` lacks.

### L6 — Sort and filter build index views (navo-ui)

- **Read:** `browser/sort/mod.rs:39-96`, `browser/filter/mod.rs:206-230,288-326`, `browser/view_jobs.rs:100-130,225-260`, `browser/sort/tests.rs:1-70,256-312`, `browser/cached/tests.rs:285-312`.
- **Change:** SL2, SL3. `sorted_rows(rows: &Arc<[FsEntry]>, key, folders_first, cancel) -> Option<Full>` returns `Rows::ordered(Arc::clone(rows), order.into())` and `listing_of(&Rows)`. `sorted(full, …)` sorts `full.details.source()`. `ViewWork::run` (`view_jobs.rs:108-113`) sorts `self.base.details.source()` with the job's cancel token and returns `None` when cancelled; a source carrying measured sizes is sorted as is. `shown_until`: keep the reserved-name check (`filter/mod.rs:302`) exactly; the shortcut shares `full` only when text is empty, both flags are on and no reserved row exists; else `filter_order(source, order, …)` (or `filter_all` for identity) → `full.details.narrowed(keep)`. No `materialize` left in `sort/` or `filter/`. Migrate the pointer tests at `cached/tests.rs:299,310` to `Rows::same(&…, &browser.rows(tab))`.
- **Tests:** equivalence (`sort/tests.rs`): every `SortBy` × descending × folders-first, with and without a filter, with hidden, system and reserved rows (reserved rows with both visibility flags on): names from `rows(tab)` equal `oracle_order` and today's listing order, aligned (SL4). Through filter, unfilter and a tab switch away and back: alignment, hidden marks (`hidden_marks`), cursor restore by name, and measured folder sizes on the right row. Async path (> 20k rows): same order as the sync path, a cancelled job leaves the shown view untouched, sizes measured during a job survive. Gain: 100k rows through `sorted_full` on the test thread: `copied_row_bytes()` = 0 (print today's equivalent, Σ row bytes, ≥ 3.1 MiB) and `rows.source()` `ptr_eq` the input. Pin SL3 with one case-only tie test.
- **Check:** `cargo xtask check -p navo-ui` exits 0; `grep -n materialize crates/navo-ui/src/browser/sort/mod.rs crates/navo-ui/src/browser/filter/mod.rs` prints nothing.
- **Stop if:** an existing test needs its expected order changed (other than the SL3 tie case), or a file outside `sort/`, `filter/`, `view_jobs.rs` and `cached/tests.rs` needs more than a signature follow-up.

### L7 — Tabs share sorts (navo-ui)

- **Read:** `browser/cached/mod.rs:16-56,190-260,293-325`, `browser/cached/early.rs:1-90`, `browser/mod.rs:46-130`, `browser/cached/tests.rs:370-390`, `cached/tests_early.rs:55-85`.
- **Change:** SL9 and the sort counter of SL10. `SortCount` defined in `sort/mod.rs`, a field in `Browser`, cloned into `Fetch`, `Early` and `ViewWork`; `sorted_full`/`sorted_rows` take `&SortCount` (signature follow-ups in `sort/mod.rs` and `view_jobs.rs` allowed). Cross-tab memo helper `shared_sort(rows, key, folders_first) -> Option<Full>` in `cached/mod.rs`, used by `sorted_full`, `adopt` (before the worker result) and `fetch` (before building `Early`).
- **Tests (`cached/tests.rs`, real loads through `FakeFs`):** two tabs open the same folder, same key: `Browser::sorts()` grows by 1 for the second tab's arrival (today 2), both `rows(tab)` are `Rows::same`. Same folder, different keys: 2 sorts, each aligned. Same key, one tab filtered: one sort, the filtered tab narrows the shared view, both aligned, marks and cursor of each tab kept. Same folder above 20k rows (Early path): the second tab shows from the memo without an `Early` job. Closing a tab drops its memo; the other tab's view is unaffected.
- **Check:** `cargo xtask check -p navo-ui` exits 0.
- **Stop if:** sharing needs `Fetch` to read the memo on the worker, or the existing `ui_sorts` tests need changed expectations.

## Done when

- L6 gain test prints 0 row bytes copied per sort at 100k (baseline ≥ 3.1 MiB); L7 test shows one sort for two tabs; L2 test meets its end-to-end allocation target. Numbers go to `docs/perf.md` in the merge commit.
- All SL4 alignment, cursor, mark and size tests pass; the full gate passes on main; Windows VM CI is green.
- `navo bench --folder` on the 100k generated folder shows no slower sort or open than the last baseline.
- D110 recorded (SL1); speed-atoms D2 marked superseded in part.

## Later

- A shared fetch for two panes on one folder needs its own plan: flight ids (not one per location), completion for queued jobs that are cancelled before they run, shared network progress, and a gate test at queue level. Until then, a watcher patch relists each pane separately and each makes its own generation, so those two views are not shared.

## Open questions

- Q1 (owner) — The core `Listing` still copies every name once per sort and filter (`Entry.name: String`, about 37 call sites in `navo-core` and `navo-ui`), and each patch still clones the rows once. Plan shared names (`Arc<str>`) separately after L6's numbers? **Recommended: yes, a separate plan, only if L6's bench shows sort time or memory still matters at 100k.**
- Q2 (owner) — SL3: in case-sensitive folders (WSL, SFTP later), two names that differ only by case keep the folder's order on every re-sort instead of the previous sort's order. **Recommended: accept; the order becomes stable.**
  With measured sizes a resort sorts the per-tab sized copy, whose source is the previous display order, so case-only ties follow the previous sort (accepted, review 2026-11-05).
- Q3 (orchestrator) — L1 needs 4 files, and L4 and L5 are sweeps over more than 3 files (one mechanical edit each). **Recommended: accept; merging them into L3 would make a 15-file atom.**

## Log

- 2026-11-05 — Planned by Turgon (Opus 5.5) from the orchestrator's direction. Facts checked on main: `IndexedRows` rebuild per patch (`listing.rs:48-107`), `materialize` in `sorted_rows` and `shown_until`, per-tab memo `sorted` with `KEEP_SORTED = 2`, cursor and marks keyed by row and remapped by name in `tab.rs`, one `submit` per tab, watcher fix 1fd013b0 (changes before the folder-time shortcut). Tie break by index in `navo-fs/src/view/sort.rs:162-163`.
- 2026-11-05 — Plan check (Terra xhigh): fix first, 10 findings. Orchestrator decisions applied: pane single flight dropped (old L7, findings 1–4) and moved to Later; reserved-name check kept and tested (5); `ViewWork::run` sorts the source with cancel and sizes (6); sort counter covers worker and UI, new L7 shares sorts through the memo with real two-tab tests (7); L2 collision fallback for the whole batch, kept seed, capacity in bytes, in-place map, end-to-end allocation target (8); L1 four files, two gallery consumers added to L4, L5 grep excludes the provider method (9); pointer tests move to `Rows::same`, filter and tab-switch tests added (10).
- 2026-11-05 — VM bench (100k files, 3 runs): not slower anywhere; 5k folder list 24.2 → 9.7 ms, sorts 0.13–0.41×. Squash rebased onto main as `sl-main` `1c8e6ee3` with the VM numbers in docs/perf.md. Next: VM CI on the exact HEAD after rebasing onto the docs commit, push after v0.2.3 is out.

- 2026-11-05 — Landed on main as `e547aba4`, with exact-commit VM CI ALL GREEN (12,024 tests passed). Included in v0.3.0 release preparation.
