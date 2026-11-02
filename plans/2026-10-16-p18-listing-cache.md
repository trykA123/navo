# P18 · Listing cache, revalidation and pre-warming

Status: ready · Repo: `<repo>` · Branch: `p18-cN` per atom · Started: —

## Goal

Going back into a folder, even a huge one on a network share, shows its listing at once with the cursor where it was, then refreshes quietly. Open panes follow changes on disk by themselves. A share that stops answering shows its last listing marked offline instead of freezing the pane. Every cache stays inside a byte budget that the diagnostics view shows.

The owner hit this on 2026-10-16: on a mapped network folder with thousands of entries, entering a subfolder and pressing Back listed everything again.

## Decisions

From docs/architecture.md ("Speed: caching, lazy loading, pre-warming"), which already holds them; `navo-cache` was never built.

- **D1 — Memory listing cache.** Key: location + volume serial. Budget 64 MB by bytes, LRU. Entries hold the listing rows and the folder's change time at read time; the pane computes sort keys (decided 2026-10-16 after the C1 review). Keys always carry the volume serial or the UNC root (C2 fills it), so a remapped drive letter never serves another share's listing. Bounded and owned by one service.
- **D2 — Stale while revalidating.** A cached listing shows at once, marked as cached. A fresh listing streams in the background and is applied as a diff; cursor, selection and scroll stay on the same names. A folder whose change time is unchanged and has an active watcher is not re-listed.
- **D3 — Watchers on open panes.** `ReadDirectoryChangesW` (works on SMB shares) for each open pane's folder, debounced; events patch the cached listing. A watcher that fails or overflows falls back to revalidation on focus and an interval for network locations (D-beta network refresh modes later).
- **D4 — Pre-warming.** The cursor resting on a folder for 150 ms prefetches its listing (at most two prefetches at once, cancelled when the cursor moves on). The parent of the current folder stays cached while you are inside it, so Back is always warm.
- **D5 — Network.** Listing has a 2 s timeout for the first batch and never blocks the UI. A share that does not answer shows "Offline · showing cached" with the time of the cached listing, and retries in the background.
- **D6 — Disk cache.** `cache.db` keeps listings of pinned and workspace folders only, 32 MB, lz4 compressed, never trusted without revalidation. Used to draw the panes right after start.
- **D8 — Crate direction.** `navo-cache` depends on `navo-core` and `navo-fs`; C6's listing serialisation lives in `navo-cache` and uses `navo-store` for the database, never the other way round.
- **D7 — Visible budgets.** The diagnostics view shows each cache's size against its budget; the soak test (10,000 tabs, 50,000 folders on the fake file system) checks the heap returns within 2 % of baseline.

## Components

| Component | Layer | Contract (in → out) | New / change | Test |
| --- | --- | --- | --- | --- |
| `ListingCache` | navo-cache | `get/put/patch/evict` by key, byte budget, LRU | new | unit, property |
| listing service | navo-fs / app | `list(location) → cached snapshot + fresh stream` | change | fake fs tests |
| diff apply | navo-ui pane | `old rows, new rows → kept cursor, selection, scroll` | change | headless UI tests |
| watcher | navo-fs (Windows) | `watch(location) → change events` | new | Windows CI tests on temp folders |
| prefetch | navo-ui pane | cursor rest 150 ms → prefetch | new | UI tests with the fake fs |
| disk cache | navo-store `cache.db` | pinned listings, lz4 | new | store tests |

## Atoms

| Atom | Component | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| C1 | `ListingCache` (D1) | Red checker, Green api-builder | medium | — | `cargo test -p navo-cache` | done |
| C2 | stale-while-revalidate and diff apply (D2) | ui-builder-high | high | C1 | UI tests: Back is instant, cursor kept, diff applied | done |
| C3 | watchers (D3) | api-builder (Opus high) | high | C1 | Windows CI tests | done |
| C4 | pre-warming (D4) | ui-builder | medium | C2 | UI tests | done |
| C5 | network timeout and offline state (D5) | ui-builder-high | high | C2 | fake fs with a stalled share | done |
| C6 | disk cache (D6) | api-builder | medium | C1 | store tests | done |
| C7 | budgets in diagnostics and soak test (D7) | api-builder | medium | C1–C6 | soak test in CI | ready |
| C9 | native Windows listing: `FindFirstFileExW` with `FindExInfoBasic` and `FIND_FIRST_EX_LARGE_FETCH` (architecture "Listing folders" phase 1), benchmarked against `std::fs::read_dir` and Explorer on a share | api-builder (Opus high) | high | — | Windows CI benchmark on a local folder of 50k entries; owner timing on the mapped share | done |
| C8 | review | reviewer (Opus high) | high | all | verdict | blocked |

## Done when

- On the owner's mapped share, Back into a folder of thousands of entries shows it at once with the cursor kept, then refreshes without a jump.
- A file created in Explorer appears in an open pane within a second.
- Pulling the network shows "Offline · showing cached" instead of a frozen pane.
- The soak test passes; the gate and Windows CI are green; the reviewer passes.

## Log

- 2026-10-16 — Plan written after the owner's network test.
- 2026-10-16 — C1 (f40ec96): `ListingCache` over `Arc<[FsEntry]>`, 39 tests. Review PASS WITH FIXES: patch outside the lock, pinned byte counter, conditional put; sort keys stay in the pane; volume serial filled by C2. Fixes running.
- 2026-10-16 — Owner: the mapped network folder takes 5–8 s to list; scroll is very fast, no freezes. The local provider still lists through `std::fs::read_dir`, not the planned `FindFirstFileExW` large fetch: C9 added. Baseline asked: entry count and Explorer time.
- 2026-10-16 — Owner, on the build with C9 (native listing) and the cache: the share "e foarte rapid" now. C9 confirmed.
- 2026-10-16 — C3 (dd721a6, e75e905; review fixes c2119b5, 28a7389, short-name tweak) merged after two green Windows runs: one watcher per open folder (ReadDirectoryChangesW / inotify), 100 ms quiet and 500 ms max debounce, patches up to 64 distinct names else relists, background tabs relist when they return, 8.3 names relist, network fallback every 5 s, Ctrl+R.
- 2026-10-16 — C6 merged after review PASS WITH FIXES: no pinned or workspace folders exist yet, so cache.db (table `listing`, 32 MiB LRU, lz4_flex, CRC over header and body, FORMAT 2) keeps the folders open in tabs; warm-up reads at start within 50 ms, probes all drives in parallel and never restarts a failed probe; rows are put only when absent and never trusted (`folder_changed = None`); writes go through a bounded queue and only for open folders; exit flush capped at 300 ms; cache.db open no longer runs quick_check (navo.db keeps it).
- 2026-10-17 — C5 merged (b6b4b61) after review PASS WITH FIXES: jobs and renames relist only answering tabs, Ctrl+R on an offline tab retries; an early NetworkLost or Timeout goes offline and keeps cached rows; an offline tab stays offline for every error except NotFound, AccessDenied, NameInvalid, PathTooLong; Windows codes 55, 59, 1232, 1236, 2250 map to NetworkLost; folder metadata counts as progress; the status shows the date when cached rows are older than today. Known gap: a share that answers metadata but never sends a batch shows Waiting with no retry.
- 2026-10-19 — C4 merged after review fixes: a 150 ms rest on a folder prefetches it on its own IoQueue (one worker per volume, so prefetches never delay visible listings or operations); moving on cancels it; Enter on a folder still prefetching adopts that read as the tab's listing (one provider call; the stall timer still applies); prefetch errors are dropped. Back stays warm through C2's parent pins.
- 2026-10-19 — C7: `navo --diag-report` runs without a window, so its report shows "listings in cache.db" (read from the profile's cache.db) and "diagnostic events" only; the in-memory listing cache lives in the GUI process and appears in the report saved from the crash-report prompt.
