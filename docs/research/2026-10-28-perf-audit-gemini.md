# Navo (Navo) Windows Performance Audit

Unchecked model output. Orchestrator check: the claim that stripping debug info shrinks the Windows exe by 60–75 % is wrong (MSVC debug info is already in navo.pdb); journal visits run in the background (Sol), not on the UI thread.

On a Windows laptop running CrowdStrike Falcon, kernel minifilter callbacks intercept every `CreateFileW`, `FlushFileBuffers`, `MoveFileExW`, `RegQueryValueExW`, and DLL `LoadLibrary` (`IRP_MJ_CREATE`, `IRP_MJ_WRITE`, `IRP_MJ_CLEANUP`, `PsSetLoadImageNotifyRoutine`). Operations that cost microseconds on Linux page cache cost **0.5–15 ms each** on corporate Windows.

---

## 10,000-File Folder OS Call Estimate (Area 2)

Core directory enumeration in [`win_list.rs`](crates/navo-fs/src/local/win_list.rs) is already Total Commander-style: it uses `FindFirstFileExW` with `FindExInfoBasic` and `FIND_FIRST_EX_LARGE_FETCH` (`L51-L60`), extracting `kind`, `size`, `modified_ms`, and `attributes` (including reparse tags in `dwReserved0`, `L118-L133`) directly from `WIN32_FIND_DATAW`. Row icons map by extension in memory ([`icons.rs`](crates/navo-ui/src/file_list/icons.rs)) with zero `SHGetFileInfoW` calls during listing.

For a **10,000-file folder**, OS calls per navigation are:
- **Directory enumeration:** ~18 syscalls (`1` `GetFileAttributesExW` in [`cached/mod.rs`](crates/navo-ui/src/browser/cached/mod.rs) + `1` `FindFirstFileExW` + ~`15` batched `FindNextFileW` kernel traps + `1` `FindClose` in [`win_list.rs`](crates/navo-fs/src/local/win_list.rs)).
- **Surrounding overhead per folder enter:** **25–45 extra OS/kernel calls**:
  - `2` OS thread spawns + `CreateFileW(FILE_FLAG_BACKUP_SEMANTICS)` + `CreateEventW` + `ReadDirectoryChangesW` in [`watched/mod.rs`](crates/navo-ui/src/browser/watched/mod.rs) and [`win_watch.rs`](crates/navo-fs/src/local/win_watch.rs).
  - Synchronous UI-thread SQLite `INSERT ... ON CONFLICT` + WAL write into `journal.db` in [`frecency.rs`](crates/navo-store/src/db/frecency.rs).
  - LZ4 compression + SQLite `INSERT` into `cache.db` in [`writer.rs`](crates/navo-cache/src/disk/writer.rs).
  - At `150 ms` cursor rest: full child folder enumeration if the cursor lands on a subfolder ([`rest.rs`](crates/navo-ui/src/window_view/rest.rs)).
  - At `300 ms` cursor rest: `SHParseDisplayName` + `IContextMenu::QueryContextMenu` + GDI icon extraction on the highlighted row ([`prefetch.rs`](crates/navo-ui/src/window_view/shell_menu/prefetch.rs)).
  - At `750 ms` idle: atomic `session.json` replacement (`CreateFileW` + `WriteFile` + `FlushFileBuffers` + `MoveFileExW` in [`atomic_write/mod.rs`](crates/navo-store/src/atomic_write/mod.rs)).
- **Exception (watcher incremental patch):** When a watched folder changes without overflowing, [`recheck.rs`](crates/navo-ui/src/browser/watched/recheck.rs) calls `LocalFs::entry` ([`win_details.rs`](crates/navo-fs/src/local/win_details.rs)) per changed path (`FindFirstFileW` + `FindClose` + `fs::metadata` if symlink). A bulk build/git checkout touching 500 files triggers 1,000+ syscalls before falling back to a single `FindFirstFileExW` pass.

---

## Ranked Findings

### 1. [Areas 1, 5, 7] Speculative Shell Context-Menu Preload (`300 ms`) & Startup Temp-File Warming
- **What:** Startup synchronously spawns an STA thread, creates `%TEMP%\navo-shell-warm\menu-<pid>.txt`, queries `IContextMenu` three times, expands all submenus, and deletes the file. During use, resting the cursor on any item for `300 ms` builds its full Windows shell `IContextMenu` and extracts GDI icons in the background.
- **Evidence:** [`gui.rs:152`](crates/navo-app/src/gui.rs), [`worker.rs:78-124`](crates/navo-shell/src/shell_menu/win/worker.rs), [`warm.rs:30-91`](crates/navo-shell/src/shell_menu/warm.rs), [`rest.rs:25`](crates/navo-ui/src/window_view/rest.rs), [`prefetch.rs:14-15,153-212`](crates/navo-ui/src/window_view/shell_menu/prefetch.rs), [`loaded.rs:53-77`](crates/navo-shell/src/shell_menu/win/loaded.rs).
- **Why it is slow:** `QueryContextMenu` loads every registered third-party shell extension DLL (7-Zip, Git, Tortoise, OneDrive, CrowdStrike) into `navo.exe`. CrowdStrike scans each DLL on load and intercepts the scratch file creation/deletion at startup and the target file opens performed by shell extensions every time the cursor stops.
- **Proposed change:** Remove `warm::warm_the_shell` and `watch_menu_rest`/`preload_menu`. Initialize the STA thread lazily on first right-click or `Shift+F10`.
- **Expected gain:** Eliminates 100–300 ms of startup I/O, ~20–40 MB of shell DLL private bytes, and continuous COM/AV churn after every cursor stop.
- **Effort:** S | **Risk:** Low (first right-click pays ~30–80 ms).

### 2. [Areas 3, 5, 7] Seven `journal.db` Opens, Synchronous WAL Checkpoints & Startup Retention Pass
- **What:** Startup opens `journal.db` **7 separate times** across 3 threads (`main`, `navo-settings-start`, `retention-maintenance`). Five connections close immediately; [`Db::open`](crates/navo-store/src/db/mod.rs) sets `SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE = 0`, forcing a blocking WAL checkpoint + `FlushFileBuffers` on each close. Worse, `MaintenanceTimer::start` runs its first retention pass with `delay = Duration::ZERO` at startup.
- **Evidence:** [`main.rs:193,200`](crates/navo-app/src/main.rs), [`settings/start.rs:207`](crates/navo-app/src/settings/start.rs), [`legacy.rs:36-41`](crates/navo-app/src/crash/store/legacy.rs), [`gui.rs:181-189,229,342`](crates/navo-app/src/gui.rs), [`memory.rs:22-27`](crates/navo-app/src/files/memory.rs), [`jobs/mod.rs:46`](crates/navo-app/src/jobs/mod.rs), [`db/mod.rs:94-110`](crates/navo-store/src/db/mod.rs), [`timer.rs:135-146,192,230,247`](crates/navo-app/src/jobs/maintenance/timer.rs).
- **Why it is slow:** Opening, locking, migrating, and WAL-checkpointing `journal.db` 7 times concurrently causes SQLite `busy_timeout` stalls and dozens of CrowdStrike-hooked file/WAL syncs during startup.
- **Proposed change:** Open one shared `Db` connection (`PRAGMA synchronous = NORMAL`), keep `SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE = 1` until final shutdown, defer `prune` and `prune_viewer_memory` to an idle timer (30 s after launch), and change `MaintenanceTimer::cycle` initial `delay` from `Duration::ZERO` to ` Duration::from_secs(60)`.
- **Expected gain:** Cuts 80–200 ms from pre-window startup and eliminates startup disk contention.
- **Effort:** M | **Risk:** Low.

### 3. [Area 5] 98 MB Unstripped Binary (`line-tables-only`, 50+ Grammars) Scanned Before `main`
- **What:** Cold start spends `1,281 ms` (warm `95–114 ms`) inside the OS loader before `main` runs ([`docs/perf.md:55-63`](docs/perf.md)). Release builds set `debug = "line-tables-only"` and `strip = "none"`, and statically link ~50 Tree-sitter C grammars and 9 `.ttf` fonts into a 98 MB `navo.exe`.
- **Evidence:** [`Cargo.toml:36-37`](Cargo.toml), [`grammars.rs:15-109`](crates/navo-ui/src/languages/grammars.rs), [`assets.rs:124-139`](crates/navo-ui/src/assets.rs), [`gui.rs:135`](crates/navo-app/src/gui.rs).
- **Why it is slow:** CrowdStrike and Windows Defender hash and scan the entire PE image when creating the process.
- **Proposed change:** Set `strip = "symbols"`, `debug = false` (use split PDBs), `lto = "thin"`, `codegen-units = 1` in `[profile.release]`. Call `register_languages()` lazily when opening the viewer/preview instead of at [`gui.rs:135`](crates/navo-app/src/gui.rs).
- **Expected gain:** Shrinks `navo.exe` by 60–75%, cutting cold pre-`main` AV scan time by 500–800 ms.
- **Effort:** S | **Risk:** Low.

### 4. [Areas 1, 2, 7] `150 ms` Cursor-Rest Subfolder Prefetch & `cache.db` Writes
- **What:** Resting the cursor on any folder row for `150 ms` enumerates that subfolder in the background, sorts it, and writes it to `ListingCache` (and `cache.db`).
- **Evidence:** [`rest.rs:9,32-52`](crates/navo-ui/src/window_view/rest.rs), [`prefetch/mod.rs:108-204`], [`cached/mod.rs:33-83`].
- **Why it is slow:** Stepping through folders with arrow keys repeatedly triggers `FindFirstFileExW` scans and SQLite `cache.db` writes on folders the user never enters.
- **Proposed change:** Remove cursor-rest subfolder prefetch (or gate it behind `500 ms` and in-memory-only without `cache.db` persistence).
- **Expected gain:** Eliminates speculative directory scans and SQLite writes during navigation.
- **Effort:** S | **Risk:** Low (`FindFirstFileExW` on enter takes <5 ms for normal folders).

### 5. [Areas 1, 4, 6] UI-Thread Double Sorting, Synchronous Filter/Sort & 4x Per-Row Duplication
- **What:** Worker thread [`load_folder_reporting`] allocates lowercase names, sorts all entries, and builds a `Listing` (`Vec<Entry>`) that [`Fetch::run`] immediately discards. Then the **UI thread** in [`sort/mod.rs:89-105`] clones all `FsEntry` items, allocates `lower: String` and `extension: String` per item, **sorts the entire folder a second time on the UI thread**, and builds another `Listing`. `Browser::sort_by` and `filter_push`/`filter_pop` also run 100% synchronously on the UI thread.
- **Evidence:** [`snapshot/mod.rs:26-49`], [`cached/mod.rs:60-64`], [`sort/mod.rs:47-56,89-105`], [`filter/mod.rs:101-123,187-269`], [`listing.rs:5-13`].
- **Why it is slow:** For 10k–500k files, UI-thread sorting/filtering blocks frames for 15–600 ms ([`docs/perf.md:102-103`](docs/perf.md)). Every row is stored 4 times in RAM: `Arc<[FsEntry]>` in `ListingCache`, `Rc<[FsEntry]>` in `Browser::details`, `Rc<[FsEntry]>` in `unfiltered`, and `Arc<[Entry]>` (`String` per row) in `navo-core::Tab`.
- **Proposed change:** Stop sorting/building `Listing` inside `load_folder_reporting`. Precompute sort keys on the worker thread, share a single `Arc<[FsEntry]>` between cache and UI, and represent filtered views as an index slice (`Arc<[u32]>`) instead of cloning `FsEntry` and `Entry` arrays.
- **Expected gain:** Cuts folder-open CPU and memory per row by >60%; eliminates UI freezes on sort/filter.
- **Effort:** M | **Risk:** Medium.

### 6. [Areas 4, 6] Full-Window Render, O(N) Status Scan, `BTreeSet` Clone & Per-Frame Row Allocations
- **What:** Every cursor move calls `cx.notify()` on `NavoWindow`, re-rendering all up to 4 panes. On every frame, each pane:
  1. Scans **all `N` rows** in [`status_of`](crates/navo-ui/src/window_view/status.rs) to sum folder/file counts, bytes, and marked bytes.
  2. Clones the entire `Selection` (`BTreeSet<u64>`) into `Rc::new(tab.selection().clone())` ([`pane.rs:159`](crates/navo-ui/src/window_view/pane.rs)).
  3. Runs [`Browser::run`](crates/navo-ui/src/browser/mod.rs) (`retain_size_owners`, `retain_finds`, `sync_pins`, `sync_disk_folders` mutex lock, `sync_watches`) even on `PaneCursorMove` (`effects` is empty).
  4. Although [`VirtualList`](crates/navo-ui/src/primitives/virtual_list.rs) virtualizes visible rows via `uniform_list`, each visible row allocates 6–8 `String`s per frame: `kind::spoken` (`format!` + `Local` date formatting for ARIA label, [`file_list/mod.rs:319`](crates/navo-ui/src/file_list/mod.rs), [`kind.rs:43-58`](crates/navo-ui/src/file_list/kind.rs)), `name_text`, `extension.to_owned()`, `size_text`, and `date_text` (`chrono::Local` conversion per row per frame, [`row.rs:125,195,202,252`](crates/navo-ui/src/file_list/row.rs)).
- **Evidence:** [`pane.rs:90-92,145-166`](crates/navo-ui/src/window_view/pane.rs), [`status.rs:6-18`](crates/navo-ui/src/window_view/status.rs), [`selection/mod.rs:11`](crates/navo-core/src/selection/mod.rs), [`browser/mod.rs:196-198,304-324`](crates/navo-ui/src/browser/mod.rs).
- **Why it is slow:** Holding `j`/`Down` at 60 Hz across 4 panes with 50k files scans 200k entries per frame on the UI thread, clones `BTreeSet`s, and shapes/allocates hundreds of strings per frame on Intel UHD graphics.
- **Proposed change:** Cache `StatusBarProps` on `Tab`/listing and update incrementally on mark change; store `Arc<Selection>` inside `Tab`; skip `sync_pins`/`sync_disk_folders`/`sync_watches` when `effects.is_empty()`; cache formatted date/size `SharedString`s on row entries.
- **Expected gain:** Drops cursor-move CPU time by 70–85% in large folders.
- **Effort:** M | **Risk:** Low.

### 7. [Areas 1, 3] Synchronous UI-Thread SQLite Queries & Writes
- **What:** Four user actions execute SQLite queries/writes directly on the UI thread:
  1. Opening any folder calls `remember_visit` (`INSERT ... ON CONFLICT` into `journal.db`).
  2. Opening/closing the file viewer reads/writes `viewer_memory` in `journal.db`.
  3. Startup `after_open` and job completion call `newest_undoable` on `journal.db`.
  4. Opening Multi-Rename (`Ctrl+M`) opens a **new** SQLite connection (`open_db`), runs migrations, queries `rename_presets`, and closes/checkpoints `journal.db` on the UI thread; saving/deleting presets does the same.
- **Evidence:** [`visits.rs:58-72`](crates/navo-ui/src/window_view/visits.rs), [`frecency.rs:11-32`](crates/navo-store/src/db/frecency.rs), [`memory.rs:35-66`], [`undo.rs:113-125`](crates/navo-ui/src/window_view/undo.rs), [`rename_hooks.rs:14-60`](crates/navo-app/src/rename_hooks.rs).
- **Why it is slow:** Any WAL write or connection close on `journal.db` triggers CrowdStrike file hooks and blocks the UI thread (up to `5,000 ms` `busy_timeout` if a background job holds `journal.db`).
- **Proposed change:** Route all `journal.db` reads/writes through a single background actor channel; cache `viewer_memory` and `rename_presets` in memory.
- **Expected gain:** Eliminates 10–100 ms UI stalls on folder open, `F3` viewer open, and `Ctrl+M`.
- **Effort:** M | **Risk:** Low.

### 8. [Area 3] Heavy Atomic File Writes (`FlushFileBuffers`) & Unbuffered Logging
- **What:**
  - **Session save:** `750 ms` after any navigation/tab/layout change, [`keep.rs:124`](crates/navo-store/src/session/keep.rs) calls `replace_atomic` (`CreateFileW` `.navo-tmp-*` + `write_all` + `FlushFileBuffers` + `CloseHandle` + `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`, [`atomic_write/mod.rs:130-144`](crates/navo-store/src/atomic_write/mod.rs)).
  - **Settings save:** Every setting toggle reads `navo.toml` 3 times, parses it twice, copies `navo.toml.bak`, and calls `replace_atomic` with `FlushFileBuffers` ([`saving.rs:29-123`], [`atomic_write/mod.rs:95-128`]), which simultaneously triggers the `navo.toml` directory watcher ([`settings/mod.rs:89`](crates/navo-app/src/settings/mod.rs)) to re-read and re-parse `navo.toml`.
  - **Logging:** [`LogLayer::on_event`](crates/navo-diag/src/log_layer/mod.rs) locks a global `Mutex<RollingFile>` and calls unbuffered `File::write_all` ([`rolling.rs:48`](crates/navo-diag/src/log_layer/rolling.rs)) synchronously on the caller thread (including the UI thread) for every `INFO` event.
  - **SQLite PRAGMAs:** `journal.db` uses `WAL` + `synchronous = FULL` ([`journal/mod.rs:93`]); `cache.db` and `index.db` use `WAL` + `synchronous = NORMAL` ([`connection.rs:64`]).
- **Why it is slow:** Temp-file creation + `FlushFileBuffers` + atomic rename is the single most expensive pattern under CrowdStrike (3 separate minifilter inspections per save). Unbuffered `write_all` on `navo.log` issues a `WriteFile` syscall per log line.
- **Proposed change:** Increase session debounce to `3–5 s` and skip `sync_all` (`FlushFileBuffers`) on non-critical `session.json` saves; wrap `RollingFile` in a `BufWriter` flushed on a timer or background thread; ignore self-generated `navo.toml` watcher events by generation.
- **Expected gain:** Cuts background disk/AV I/O by >80% during normal browsing.
- **Effort:** S | **Risk:** Low.

### 9. [Area 1] UI-Thread Watcher Spawns & Synchronous Shell/Registry Lookups
- **What:**
  - `Browser::sync_watches` runs on the UI thread during folder navigation, spawning a `folder-watch-rdcw` OS thread ([`win_watch.rs:65`](crates/navo-fs/src/local/win_watch.rs)) and a `watch-release` OS thread ([`watched/mod.rs:346`]).
  - Pressing `Enter` on a file calls `SHGetFileInfoW(SHGFI_EXETYPE)` synchronously on the UI thread ([`files/mod.rs:33,221-235`]) before spawning.
  - Opening the editor/terminal picker (`F4` choose / `Ctrl+` `) walks Registry keys and checks candidate `.exe` paths with `Path::is_file()` synchronously on the UI thread ([`companions.rs:144-273`]).
- **Why it is slow:** OS thread creation, `SHGetFileInfoW` (which opens the PE header on disk), and `Path::is_file()` across `Program Files` trigger synchronous CrowdStrike scans on the UI thread.
- **Proposed change:** Move watcher start/stop onto an existing worker pool; remove `is_console_program` (`SHGetFileInfoW`) or run `open_file` on a background task; cache discovered companion apps after the first background probe.
- **Expected gain:** Eliminates 20–150 ms UI hitches on folder change and file launch.
- **Effort:** S | **Risk:** Low.

### 10. [Area 7] Idle Background Polling Loops (`200 ms`, `500 ms`, `2 s`)
- **What:** Three loops wake the process continuously even when Navo is completely idle:
  1. `navo-instance` thread polls `ConnectNamedPipe` on a `PIPE_NOWAIT` pipe every **`200 ms`** (5 wakeups/sec, [`windows.rs:91-97`], [`protocol.rs:8`](crates/navo-app/src/instance/protocol.rs)).
  2. `poll_index_activity` wakes the UI thread and spawns a background task every **`500 ms`** whenever `IndexMode != Off` ([`index_activity.rs:9-43`](crates/navo-ui/src/window_view/index_activity.rs)).
  3. `poll_drives` runs every **`2 s`** ([`window_view/drives.rs:15,70-86`]), calling `GetLogicalDrives` + `GetDriveTypeW` + `WNetGetConnectionW` ([`drives/windows.rs:83-102`]). For removable/optical drives (`every(kind) == Duration::ZERO`, [`app/drives/mod.rs:29`]), it spawns a thread calling `GetVolumeInformationW` + `GetDiskFreeSpaceExW` **every 2 seconds** (and every 30 s for all fixed/network drives plus `WNetOpenEnumW`).
- **Why it is slow:** Prevents CPU C-state residency and repeatedly queries slow corporate SMB/network/USB volumes.
- **Proposed change:** Use overlapped I/O (`FILE_FLAG_OVERLAPPED` + `WaitForMultipleObjects` on a stop event) for `navo-instance`; replace `poll_index_activity` with an event channel; replace `2 s` drive polling with `WM_DEVICECHANGE` notifications + on-demand refresh when opening the drive menu or switching panes.
- **Expected gain:** Zero idle CPU/syscall wakeups.
- **Effort:** M | **Risk:** Low.

---

## Resident (Tray) Instance Feasibility (Area 5)

**Highly feasible (Effort: M) and the single biggest win for perceived launch speed.**
- Navo already has single-instance mutex + named-pipe IPC forwarding `Request { paths, new_window }` to the running process ([`main.rs:163-181`](crates/navo-app/src/main.rs), [`gui.rs:197-223`](crates/navo-app/src/gui.rs)).
- Today, closing the last window calls `cx.quit()` in [`closing.rs:101-106`](crates/navo-ui/src/window_view/closing.rs).
- Keeping the process resident in the system tray (or hidden window) bypasses **both** the `1,281 ms` cold / `114 ms` warm CrowdStrike process-start scan **and** the `260–290 ms` Intel UHD DirectX/DirectWrite GPUI initialization ([`docs/perf.md:67-68`](docs/perf.md)), bringing invocation latency under **15 ms**. Prerequisite: fix Finding #10 so the resident process consumes 0% CPU when idle.

---

## Top 5 Changes to Make First

1. **Kill speculative shell context-menu warming and `300 ms` cursor-rest preload (Finding #1)**
   - *Measure before/after:* Process Monitor (`ProcMon`) count of `LoadImage` and `CreateFile` events during startup and during 20 arrow-key steps across files; startup-to-interactive time (`--startup-Timing`).
2. **Strip release binary, use split PDBs, and lazy-load Tree-sitter grammars (Finding #3)**
   - *Measure before/after:* `navo.exe` file size in MB; cold and warm `process_start -> main` ms via `xtaskStartup-bench`.
3. **Consolidate `journal.db` to one connection, disable checkpoint-on-close during startup, and delay `MaintenanceTimer` (Finding #2)**
   - *Measure before/after:* `main -> gui_run_start` ms in `--startup-Timing`; `ProcMon` count of `journal.db*` opens and `FlushFileBuffers` calls in the first 2 seconds.
4. **Eliminate O(N) status-bar iteration, `Selection` `BTreeSet` clone, per-frame row string/date allocations, and UI-thread double sort (Findings #5 & #6)**
   - *Measure before/after:* Frame time (p50/p99 ms) while holding `Down` in a 10,000-file folder with 4 tiled panes; `folder_open` ms in `perf_gate`.
5. **Replace `200 ms` pipe polling, `500 ms` index polling, and `2 s` drive polling, then add resident tray mode (Finding #10 & Area 5)**
   - *Measure before/after:* Windows Performance Analyzer (WPA) / `ProcMon` syscall and context-switch count over a 60-second idle window; warm window-summon latency via second-instance IPC.
