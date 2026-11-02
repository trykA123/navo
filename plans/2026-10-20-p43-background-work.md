# P43 · Background work: job pools, folder sizes, benchmarks

Status: drafting (Q1–Q3 open) · Repo: `<repo>` · Branch: `p43-<atom>` per atom · Started: 2026-10-20

## Goal

Navo never waits on slow work. Listings, previews and folder sizes run in their own pools with priorities, and anything the user has moved past is cancelled. Folder sizes show apparent size or size on disk and count hard links once. A benchmark command measures listing, walk and index timing, warm and cold, for `docs/perf.md`.

Source: D71 items 1, 7, 8 and 10 (ideas from yazi, broot, dua-cli and dust, hyperfine). Copy the designs, not the code.

## Phase

After the alpha, except four small atoms that help alpha features in progress: **J0** (P36 drag and drop refusal text) and **B1–B3** (numbers for P40 D25 budgets and C11 `docs/perf.md`). They never block an alpha atom.

## What exists today (checked at f8e7655)

- **I/O queue.** `navo-fs/src/io_queue/`: `IoQueue` (runner.rs) over `Scheduler` (scheduler.rs). Per-volume worker limit from `VolumeKind::limit` (Ssd 8, Network 4, Removable 2, Hdd/Unknown 1), `Priority { Idle, Prefetch, Visible }`, `OwnerGuard` that cancels its jobs on drop, a `CancelToken` per running job, panics caught. Workers start on demand and exit when idle.
- **`set_volume` is never called outside tests**, so every volume is `Unknown` and each queue runs one job per volume.
- **Five separate queues, uncoordinated:** `Browser.queue` (listings, Visible) and `Browser.sizes` (Space folder sizes, Idle) in `navo-ui/src/browser/mod.rs:56-59`; `Prefetches.queue` (Prefetch) in `browser/prefetch/mod.rs:63`; `Preview.reads` and `Preview.scans` in `window_view/preview/mod.rs:72-73`.
- **File previews skip the queue.** `preview/file.rs:69-110` runs `sniff_and_load` on the GPUI background executor with a `CancelOnDrop` flag. A new cursor target replaces `Shown`, which drops the flag: cancel-on-supersede already works for file and folder previews (`preview/mod.rs:166-201`, `folder.rs:81-117`).
- **Folder sizes on Space are never cancelled on leave.** `browser/measure.rs:20-46` submits under one `sizes_owner` that lives as long as the `Browser`; `show_folder_size` only drops the result when the tab moved.
- **File operations are their own engine** (D18): one `navo-job` thread per job (`browser/operate/mod.rs:288-304`), copy workers per drive class inside (`navo-fs/src/job/workers.rs`, `parallel.rs`). Jobs never wait for each other.
- **Copy into itself is refused before submission.** `navo-core/src/op_plan/build/mod.rs:238-242` (`inside_itself`, Windows case rules via `same_place`) gives the plan no steps, tested in `op_plan/tests/risks.rs:202-240`. The earlier UI check `drop_refusal` (`browser/operate/paste.rs:18-37`) compares with `contains`, which is case-sensitive: `C:\A` dropped on `c:\a\b` passes it and reaches the plan, which refuses it with no steps, so the user misses the drop refusal text.
- **Folder size walk.** `navo-fs/src/measure/walk.rs` (depth-first, one thread, `Visit` trait) feeds `folder_size` and `FolderProfile`. `FolderSize { bytes, files, folders, skipped }`. Links are not followed. No size on disk, no hard link handling.
- **Listing data.** `FsEntry { name, kind, size, modified_ms, attributes }`, about 80 struct literals in the workspace. Windows lists with `FindFirstFileExW` (`local/win_list.rs`): no file id, no allocation size. Unix reads `Metadata` per entry (`local/unix.rs:28`): `blocks`, `ino`, `nlink` cost nothing more.
- **Dates already come with the listing.** Ctrl+K index rows fetch size and date lazily (P40 D24).
- **Benchmarks.** `xtask bench-copy` (`xtask/src/bench_copy/`): N runs, median only, no warm-up, no cold runs, no outliers; informational step in Windows CI (`.github/workflows/ci.yml:75-82`). `xtask perf-fixture` makes 5,000 files. `navo-index` has a timing test (`search/tests.rs:189`), no bench target.
- **No hash or module jobs exist yet.** Their pools are defined here; the first users come with P42 (content compare) and plugins.

## Decisions

- **D1 — Pools, not a new engine.** `navo-fs::io_queue::Pools` holds one `Arc<IoQueue>` per `JobKind { Listing, Preview, Size, Hash, Module }`, created once per process (`Pools::shared()`, a `OnceLock`). Listings and prefetches share `Listing`, so a visible listing goes before a prefetch. File operations keep their engine (D18); Q1 decides whether they queue.
- **D2 — Worker limits per pool and volume kind** (`Limits { ssd, hdd, network, removable }`): Listing 8/1/4/2 (today's), Preview 2/1/2/1, Size 2/1/1/1, Hash 2/1/1/1, Module 1/1/1/1. Unknown counts as hdd. Constants in `pools.rs`, one-line changes later.
- **D3 — Volume kind on first use, off the UI thread.** A volume starts `Unknown` (limit 1). The worker that takes its first job calls the queue's classifier, default `drive_class` on `VolumeId::root()`, and sets the kind; later jobs get the real limit. Tests inject a classifier.
- **D4 — The cancel race.** `IoQueue::submit_then(slot, work: FnOnce(&CancelToken) -> T, deliver: FnOnce(T))`: after `work` returns, the worker checks the token under the queue lock and calls `deliver` only if it was not cancelled. A cancelled job's result never reaches the UI, even when it finished in the same instant. Cancellation stays cooperative; a job stuck in a system call keeps its worker until the call returns.
- **D5 — Supersede rules.** A new cursor target cancels the old preview (exists). Leaving a folder cancels its folder-size jobs (one size owner per tab, replaced on each listing). A keystroke in Ctrl+K cancels older searches (P40 D16, D22).
- **D6 — Sizes fill in, rows stay put.** When a size arrives the row updates in place. A list sorted by size is not re-sorted by arriving sizes; choosing the sort again re-sorts. No spinner per row; unmeasured folders show the empty size as today.
- **D7 — Size of shown folders (opt-in).** `files.measure-shown-folders = false`. When on, after a listing loads, every folder row is measured in the `Size` pool at `Idle`, top of the list first, at most 500 per listing, on Ssd and Hdd volumes only (network and removable keep Space). Q2.
- **D8 — Two size modes.** `files.size-mode = "apparent" | "on-disk"`, default `"apparent"` (what Explorer's Size column shows). On disk is the allocated size (clusters, compression and sparse files). Where a provider cannot tell, the on-disk number is the apparent one and the folder preview says "size on disk not known here".
- **D9 — Hard links counted once** in both modes (dust's rule). Unix: files with `nlink > 1` are keyed by `(dev, ino)`. Windows: per S0's answer; the default plan keys every file by its 128-bit file id in one set per walk, capped at 1,000,000 ids (about 32 MB, freed when the walk ends); past the cap, links may count twice and the preview says so. Q3.
- **D10 — Size data without touching `FsEntry`.** `Provider::list_sizes(folder, sink, cancel)` yields `SizeEntry { name, kind, size, on_disk: Option<u64>, link: Option<LinkKey> }`, with a default built on `list` (`on_disk: None`, `link: None`). Only the measure walk calls it.
- **D11 — Benchmarks are informational.** `cargo xtask bench-walk` never gates CI or a merge (D63; owner: features before performance). Report: warm-up runs (default 2), runs (default 10), mean ± σ, median, min, max and outliers (modified Z-score above 3.5 on the median absolute deviation), in a markdown table.
- **D12 — Cold runs are honest.** `--prepare "<command>"` runs a shell command before each cold run (for example a cache flush the user may run). Without it, "cold" is the first run on a freshly generated fixture and the table says "cache not flushed".

## Components

| Component | Layer | Contract (in → out) | New / change | Test |
| --- | --- | --- | --- | --- |
| `browser::operate::paste` | ui | `drop_refusal(target, sources)` uses `same_place` | change | `browser/operate/tests/drop.rs` |
| `io_queue` limits and classifier | lib | `IoQueue::with(Limits, Classifier)`; `VolumeId::root()` | change | `io_queue/tests/scheduler.rs`, `volume.rs` |
| `io_queue::submit_then` | lib | `(slot, work, deliver)`; deliver only if not cancelled | new | `io_queue/tests/runner.rs` |
| `io_queue::pools` | lib | `Pools::shared().queue(JobKind) → &Arc<IoQueue>` | new | `io_queue/tests/pools.rs` |
| `browser` queues | ui | listings, prefetch, sizes on `Pools`; size owner per tab | change | `browser/tests.rs` |
| `preview` queues | ui | folder reads and scans, file loads on `Pools::Preview` | change | `window_view/preview/tests/` |
| `browser::operate` job queue | ui | second job on a shared slow drive waits (Q1) | change | `browser/operate/tests/` |
| `provider::list_sizes` | adapter | `SizeEntry` stream; default from `list`; `FakeFs` fields | new | `fake/tests/` |
| `local` sizes | adapter | Unix `Metadata`; Windows `GetFileInformationByHandleEx` | new | `local/tests/listing.rs`, `windows.rs` |
| `measure::walk` | lib | `FolderSize { …, on_disk, links_merged, links_unchecked }` | change | `measure/tests/mod.rs` |
| `[files]` size keys | store | `size_mode`, `measure_shown_folders` | change | `navo-store` settings tests |
| `xtask::stats` | tool | `samples → Summary { mean, sd, median, min, max, outliers }` | new | `xtask/src/stats.rs` tests |
| `xtask::bench_walk` | tool | `bench-walk <dir> [--targets] [--warmup] [--runs] [--prepare]` | new | `xtask/src/bench_walk/tests.rs` |

## Atoms

| Atom | Component | Role | Effort | After | Check | Status | Cost |
| --- | --- | --- | --- | --- | --- | --- | --- |
| J0 | `paste` drop refusal (alpha) | patcher (Sonnet) | medium | — | `cargo nextest run -p navo-ui browser::operate::tests::drop` | done | |
| J1 | `io_queue` limits and classifier | api-builder (Sonnet) | high | — | `cargo nextest run -p navo-fs io_queue` | done | |
| J2 | `io_queue::submit_then` | api-builder (Sonnet) | high | J1 | `cargo nextest run -p navo-fs io_queue::tests::runner` | done | |
| J3 | `io_queue::pools` | api-builder (Sonnet) | medium | J1 | `cargo nextest run -p navo-fs io_queue::tests::pools` | done | |
| J4 | `browser` on `Pools` | ui-builder (Sonnet) | medium | J3 | `cargo nextest run -p navo-ui browser` | done | |
| J5 | size owner per tab | ui-builder (Sonnet) | high | J4 | `cargo nextest run -p navo-ui browser::tests` | done | |
| J6 | preview folder reads on `Pools` | ui-builder (Sonnet) | medium | J3 | `cargo nextest run -p navo-ui window_view::preview` | done | |
| J7 | preview file loads on `Pools` | ui-builder-high (Opus) | high | J2, J6 | `cargo nextest run -p navo-ui window_view::preview` | done | |
| J8 | file jobs wait on a shared slow drive | ui-builder (Sonnet) | high | Q1 | `cargo nextest run -p navo-ui browser::operate` | done | |
| J9 | review Part J | reviewer (Opus) | high | J5, J7, J8 | review notes; `cargo xtask gate`; Windows CI | blocked | |
| S0 | Windows size facts | researcher (Sonnet) | medium | — | answers with sources, in the Log | done | |
| S1 | `provider::list_sizes` + `FakeFs` | api-builder (Sonnet) | medium | — | `cargo nextest run -p navo-fs fake` | done | |
| S2 | Unix `list_sizes` | api-builder (Sonnet) | medium | S1 | `cargo nextest run -p navo-fs local::tests::listing` | done | |
| S3 | Windows `list_sizes` | api-builder (Opus) | high | S0, S2 | Windows CI `cargo nextest run -p navo-fs local::tests::windows` | done | |
| S4 | `measure::walk` on disk and links | api-builder (Sonnet) | high | S1 | `cargo nextest run -p navo-fs measure` | done | |
| S5 | `[files]` size keys | api-builder (Sonnet) | medium | — | `cargo nextest run -p navo-store settings` | done | |
| S6 | size mode in the list and settings | ui-builder (Sonnet) | medium | S4, S5, J4 | `cargo nextest run -p navo-ui browser settings` | ready | |
| S7 | measure shown folders | ui-builder (Opus) | high | S6, J5, Q2 | `cargo nextest run -p navo-ui browser::tests` | blocked | |
| S8 | preview: on disk and link note | ui-builder (Sonnet) | medium | S4, S5 | `cargo nextest run -p navo-ui window_view::preview`; gallery shots | ready | |
| S9 | review Part S | reviewer (Opus) | high | S3, S7, S8 | review notes; gate; Windows CI | blocked | |
| B1 | `xtask::stats` (alpha) | api-builder (Sonnet) | medium | — | `cargo nextest run -p xtask stats bench_copy` | done | |
| B2 | `bench-walk`: list and size (alpha) | api-builder (Sonnet) | medium | B1 | `cargo nextest run -p xtask bench_walk`; `cargo xtask bench-walk <tmp> --runs 3` prints the table | done | |
| B3 | `bench-walk`: index targets (alpha) | api-builder (Sonnet) | medium | B2 | `cargo nextest run -p xtask bench_walk`; `--targets index` prints rows | done | |
| B4 | docs and informational CI step | infra-builder (Sonnet) | medium | B3 | `cargo xtask gate` (docs links); CI step `continue-on-error` | done | |

### J0 — drop refusal follows Windows case rules

- **Read:** `crates/navo-ui/src/browser/operate/paste.rs:1-40`; `navo-core/src/location/mod.rs:120-131` (`same_place`); `browser/operate/tests/drop.rs`.
- **Change:** in `drop_refusal`, compare with `same_place`: `sources.iter().any(|s| s.same_place(&location))` and the same for `AlreadyHere` with the parent.
- **Check:** new tests `C:\A` on `c:\a\b` → `IntoItself`; `C:\A\x` on `c:\a` → `AlreadyHere`; Linux paths stay case-sensitive.
- **Stop if:** any other caller relies on the exact compare.

### J1 — limits and classifier

- **Read:** `navo-fs/src/io_queue/mod.rs`, `scheduler.rs`, `runner.rs:60-130`, `drive_class.rs:1-40`, `io_queue/tests/scheduler.rs`, `volume.rs`.
- **Change:** `Limits { ssd, hdd, network, removable }` with `Limits::LISTING` equal to today's `VolumeKind::limit`; `Scheduler` keeps a `Limits`; `IoQueue::with(limits, classifier)` where `Classifier = Arc<dyn Fn(&VolumeId) -> VolumeKind + Send + Sync>`; `IoQueue::new()` keeps today's limits and the `drive_class` classifier. `VolumeId::root() -> Option<Location>`. The worker taking the first job of an `Unknown` volume classifies it before running the job (D3), then spawns more workers if the new limit allows.
- **Check:** limits per kind; a fake classifier raises a volume from 1 to 8 workers after the first job; the classifier runs once per volume and never on the submitting thread.
- **Stop if:** `drive_class` can block for seconds on a network root; report the time.

### J2 — the cancel race

- **Read:** `io_queue/runner.rs`, `tests/runner.rs`.
- **Change:** `submit_then` per D4; `submit` stays.
- **Check:** deliver runs once when not cancelled; never after `cancel_owner` before or during `work`; a cancel raced in a loop of 1,000 runs never delivers after the owner's drop returned; a panicking `work` does not deliver.

### J3 — pools

- **Read:** `io_queue/mod.rs`, J1's `Limits`, `navo-fs/src/lib.rs:20-30`.
- **Change:** `io_queue/pools.rs`: `JobKind`, the D2 table, `Pools::shared()`, `queue(kind)`; export from `lib.rs`.
- **Check:** each kind has its own queue and limits; `shared()` returns the same instance; a Size job does not wait behind a running Listing job on the same volume.

### J4 — browser on pools

- **Read:** `browser/mod.rs:53-130,255-285`, `browser/measure.rs`, `browser/prefetch/mod.rs:55-75,180-205`.
- **Change:** listings and prefetch use `Pools::shared().queue(Listing)`, sizes use `Size`; owners stay per `Browser`. Remove the three own `IoQueue` fields.
- **Check:** `cargo nextest run -p navo-ui browser` passes unchanged.

### J5 — size owner per tab

- **Read:** `browser/measure.rs`, `browser/mod.rs:231-250`, `browser/tests.rs:290-320`.
- **Change:** `size_owners: HashMap<TabId, OwnerGuard>`; replaced when the tab lists a new location; closing a tab drops it (D5).
- **Check:** a slow fake walk is cancelled when the tab moves (the fake provider sees the cancel); a size for the old folder never shows; the same folder relisted by refresh keeps its jobs.

### J6 — preview folder reads

- **Read:** `preview/mod.rs:60-100`, `preview/folder.rs:66-130`, `navo-fs/src/measure/live/mod.rs:60-90`.
- **Change:** `reads` and `scans` become `Pools::shared().queue(Preview)`; owners as today.
- **Check:** preview tests pass unchanged.

### J7 — preview file loads

- **Read:** `preview/file.rs`, `viewer/open.rs:170-200` (oneshot pattern), J2's `submit_then`.
- **Change:** `start_file` submits `sniff_and_load` with `submit_then` on the Preview pool under a per-load `OwnerGuard` held in `Loading` (replaces `CancelOnDrop`); deliver sends through a `futures` oneshot that the GPUI task awaits. GPUI rule: spawn from the handler, never from render.
- **Check:** moving the cursor drops the old load and its result never shows; a stalled fake read on volume A does not delay a preview on volume B.

### J8 — file jobs on a shared slow drive (only if Q1 is yes)

- **Read:** `browser/operate/mod.rs:250-320`, `navo-fs/src/job/workers.rs`, P39 strip states.
- **Change:** a new job whose source or target shares an Hdd or removable volume with a running job waits as "Queued" in the strip and starts when that job ends; cancel works while queued. Text in `.ftl`.
- **Check:** two fake jobs on one Hdd run in order; on Ssd they run at once; cancelling a queued job never touches files.

### S0 — Windows size facts

- **Questions:** does `GetFileInformationByHandleEx` with `FileIdExtdDirectoryInfo` (or `FileIdBothDirectoryInfo`) give `AllocationSize` and the file id for every entry, and which `windows-sys` 0.61 features it needs; is there any enumeration class that also gives the link count; is `AllocationSize` right for compressed, sparse, OneDrive placeholder and SMB files; 64-bit versus 128-bit ids on NTFS and ReFS; how Explorer's "Size on disk" differs. Answer with sources; mark guesses UNVERIFIED.

### S1–S4 — size data and the walk

- **S1:** `provider/mod.rs`: `SizeEntry`, `LinkKey(u128, u64)` (id, volume), `list_sizes` default per D10; `FakeFs` gets optional on-disk sizes and link keys per file.
- **S2:** `local/unix.rs`, `local/mod.rs`: `list_sizes` from the `Metadata` already read (`blocks × 512`, `(dev, ino)` when `nlink > 1`). Test with a real hard link in a temp folder.
- **S3:** `local/win_sizes.rs` (new) and `local/mod.rs`: per S0. Each `unsafe` has a `SAFETY:` line. Test on Windows CI with a hard link (`std::fs::hard_link`) and a sparse file.
- **S4:** `measure/walk.rs`, `measure/mod.rs`: walk calls `list_sizes`; `FolderSize` adds `on_disk`, `links_merged`, `links_unchecked`; the D9 set and cap. Tests: two links count once in both modes; cap reached sets `links_unchecked`; `on_disk: None` falls back to apparent; cancel still stops.

### S5–S8 — settings and screens

- **S5:** `navo-store/src/settings/model.rs` `Files`: `size_mode: SizeMode { Apparent, OnDisk }`, `measure_shown_folders: bool`, defaults per D7–D8, round trip and unknown-value tests.
- **S6:** `browser/measure.rs` keeps the whole `FolderSize` per row and shows the mode's number; `window_view/settings_save.rs` passes the mode; two settings sheet rows in `settings/fields.rs` with one sentence each (D58). Text in `.ftl`.
- **S7:** when `measure_shown_folders` is on, a loaded listing submits D7 jobs through J5's owner; D6 for sorting. Check: 600 fake folders submit 500; a network volume submits none; sorting by size does not move rows while sizes arrive.
- **S8:** `preview/stats.rs`: the size card shows the mode's number, the other one as its sub-line, and "N hard links counted once" or the D9 cap note. Gallery shots light and dark.

### B1–B4 — benchmark harness

- **B1:** `xtask/src/stats.rs`: `Summary::of(&[f64])` per D11; `bench_copy/report.rs` uses it (median stays, mean ± σ and outliers added). Tests: known samples, one outlier flagged, two samples, empty input refused.
- **B2:** `xtask/src/bench_walk/{mod,options}.rs`, `main.rs` registration: targets `list` (`LocalFs::list` of the 5,000-file `perf-fixture` folder) and `size` (`folder_size` on a generated tree of 100,000 entries in 1,000 folders); warm-up, runs, `--prepare` per D12. Options parsing tests like `bench_copy/options.rs`.
- **B3:** `bench_walk/index.rs`, `xtask/Cargo.toml` (`navo-index`): targets `walk` (`walker::walk` of the tree), `search` (five fixed queries on 1,000,000 synthetic names, as in `search/tests.rs:189`), `save` and `load` (`navo-index::file`).
- **B4:** `docs/perf.md` section "Walk and index timing" with how to run it; a Windows CI step after the copy benchmark, `continue-on-error: true`, `timeout-minutes: 10`, `--runs 3`, table to the step summary.

## Done when

- Previews, listings and sizes run in `Pools`; leaving a folder or moving the cursor cancels superseded work (J5, J7 tests); a cancelled result never shows (J2).
- Folder sizes show apparent or on-disk per the setting and count hard links once, on Linux and Windows CI (S2–S4).
- `cargo xtask bench-walk` prints warm and cold tables with outliers; CI shows it without failing (B4).
- `cargo xtask gate` passes; three reviews done.

## Questions for the owner

All three answered 2026-10-20: the owner accepted the recommendations.

- **Q1 — Should a second copy wait when it shares a hard disk or USB drive with a running copy?** Two copies on one spinning disk make both slower. **Decided:** yes for Hdd and removable, no for SSD and network.
- **Q2 — Should folder sizes fill in by themselves?** **Decided:** off by default (Space measures, as now); a setting turns it on for local drives only.
- **Q3 — On Windows, is about 32 MB during a big folder-size walk acceptable to count hard links once?** **Decided:** yes, capped at 1,000,000 files, with a note past the cap.

## Log

- 2026-10-20 — Plan written from D71 items 1, 7, 8, 10 and the code at f8e7655. Found: `IoQueue` already has priorities, owner cancel and per-volume limits, but no volume is ever classified; five queues are separate; Space sizes are not cancelled on leave; copy into itself is refused by the plan, the UI pre-check is case-sensitive.
- 2026-10-20 — Owner accepted Q1–Q3 as recommended; J8 can proceed on that rule.
- 2026-10-27 — J1–J7 on main (e3f795d1). J8, S1, S5, B1 started.
- 2026-10-27 — S0 answers. (1) `GetFileInformationByHandleEx` with `FileIdExtdDirectoryInfo` (0x13) returns `FILE_ID_EXTD_DIR_INFO` per entry: `EndOfFile`, `AllocationSize`, attributes, reparse tag and a 128-bit `FILE_ID_128` ([docs](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_extd_dir_info)). `FileIdBothDirectoryInfo` (0xa) gives the same sizes with a 64-bit id ([docs](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_both_dir_info)). windows-sys features: `Win32_Storage_FileSystem` (already used). (2) No directory class gives the link count; that needs one open per file (`FileStandardInfo.NumberOfLinks`), too slow. Keep D9: key every file by its id in one set per walk. (3) Some volumes refuse the 128-bit class with `ERROR_INVALID_PARAMETER`, SMBv1 with `ERROR_INVALID_LEVEL`; Boost falls back per directory iterator, not for good ([Boost patch](https://www.boost.org/patches/1.82.0/0002-filesystem-fix-win-smbv1-dir-iterator.patch/)). S3 plan: Extd → Both (64-bit id, volume serial as `LinkKey.1`) → plain listing with `on_disk: None, link: None`. (4) FAT ids come from the directory entry offset and can change ([docs](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_both_dir_info)); stable within one walk, which is all D9 needs. (5) UNVERIFIED: `AllocationSize` for compressed and sparse NTFS files is the allocated size (what Explorer calls "Size on disk"); for OneDrive cloud-only placeholders it is 0 or small; over SMB the server reports it. S3's Windows CI test with a sparse file and a hard link settles (5).
- 2026-10-27 — J8, S1, S2, S4, S5, B1, B2 done (Codex hit its limit at 07:30; S3 and B3 finished by Claude agents). bench-walk on Linux, warm medians: list 5,000 files 4.1 ms; size of 100,000 entries 110 ms; 5 searches on 1,000,000 names 486 ms; index save 121 ms, load 327 ms. Next: S3 on Windows CI, S6–S8, J9 review.
- 2026-10-29 — Amended from the 2026-10-28 research (D88): `bench-walk` also compares `dua-core` =5.0.1 and `ignore` =0.4.34 with our walker (speed SP12a, targets `walk-dua` and `walk-ignore`); only the faster walker stays (SP12c). If `dua-core` wins, its batched directory reads are a candidate for the size walk (`measure::walk`) too, as a separate measured atom after S9. B4 keeps the bench informational on Linux; Windows CI and the owner's laptop give the numbers that count. The remaining backend atom B3 may run on DeepSeek V4.1 Flash or Sol 6.1 high; S6–S8 touch the UI and do not.
- 2026-10-30 — Part lead Yavanna took over. Found on main: J0 (drop_refusal uses `same_place`, case tests in `operate/tests/drop.rs`), S3 (`local/win_sizes.rs`, fc4879cc, 9d7c3604) and B3 (`bench_walk/index.rs`, 1a23fdce) are done; no S3/B3 worker is running. S3's Windows tests (hard link, sparse file) still need one VM CI run. Ready now: S6, S8, B4. Order: S6 → S7 → J9; S8 → S9; B4.
- 2026-10-30 — B4 done: `docs/perf.md` section "Walk and index timing" and an informational Windows step in `checks.yml` (`continue-on-error`, 10 min, `--runs 3`). The step has not run on CI yet.
- 2026-10-30 — S3 verified: its hard-link, folder and sparse-file tests passed on Windows in the VM CI of c531a642, which contains fc4879cc and 9d7c3604.
- 2026-10-30 — Stopped by the weekly usage rule. In progress, on their own branches and not on the part branch: S6 (`beta-s6`, worktree `navo-beta-s6`) and S8 (`beta-s8`, worktree `navo-beta-s8`); each worker was told to commit if green or leave a `STOP-backup.patch` in its worktree. Next: land S6 and S8 after reading their diffs, then S7 (after S6), then the J9 and S9 reviews. Windows tests are unrun for B4's CI step and for S3 beyond c531a642.
- 2026-10-30 — Stop details. S6 (`navo-beta-s6`): code done, 508 `browser`/`settings` tests passed, `xtask check` not run, nothing committed; `STOP-backup.patch` in the worktree. It edited `preview/folder.rs` (one line, `Measured.bytes` became `size`), which S8 may also touch; in on-disk mode a folder preview shows the apparent number until S8 passes the full `FolderSize`. S8 (`navo-beta-s8`): `preview/size_card.rs` with four tests, strings, `Preview.size_mode`; the last check failed on one clippy error (`#[allow(dead_code)]` on `SizeMode::OnDisk` needs a reason); no screenshots yet; `STOP-backup.patch` in the worktree. Both S6 and S8 define their own `SizeMode` inside `navo-ui` because `navo-ui` does not depend on `navo-store`: on resume, keep one definition (S6's `values.rs`), have `Preview.size_mode` set from the `files.size-mode` setting, and drop the allow.

