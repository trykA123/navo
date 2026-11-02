# Navo Windows performance audit

Audited `f67654c5`, 28 October 2026. Static analysis only. Gains below are estimates, not measurements. Endpoint-security amplification is the supplied operating context; attributing specific delays to Falcon requires tracing. Total Commander comparisons describe a single-enumeration design, not measured internals.

The recorded owner baseline is 540–591 ms warm, 1745 ms cold, approximately 137 MB private bytes. Cold pre-main alone takes 1281 ms; GPUI takes 257–292 ms (`docs/perf.md:51–63`). These are historical measurements, not timings of this revision. P28 explicitly leaves application versus driver memory unresolved (`plans/2026-10-19-p28-real-pc-startup.md:14`).

## Ranked findings

### 1. Avoid rebuilding the whole process and GPU for every reopening

**Evidence.** Close saves and calls `cx.quit()` (`crates/navo-ui/src/window_view/closing.rs:24–33`). Profile-scoped instance forwarding already exists (`crates/navo-app/src/main.rs:314–343`), and delivered requests activate the window (`crates/navo-ui/src/window_view/hand_off.rs:170–188`). An opt-in tray instance retaining the window/device is feasible; it needs close-to-hide, explicit Quit, and lifecycle handling.

Startup also waits up to 50 ms for disk listings (`crates/navo-app/src/gui.rs:28,306–312`; `crates/navo-cache/src/disk/mod.rs:88–105`). It opens databases before GPUI (`crates/navo-app/src/gui.rs:175–191`), prunes viewer memory (`crates/navo-app/src/files/memory.rs:21–28`), then eagerly registers languages and nine fonts (`crates/navo-ui/src/app.rs:27–31`; `crates/navo-ui/src/assets.rs:124–137`).

**Change/gain.** Retain an opt-in instance; use direct activation or a small launcher so reopening also avoids scanning the large executable. Potentially bypass roughly 450 ms of recorded application startup, plus launch overhead. Make cache warming nonblocking and defer pruning, extra fonts, and grammar registration. Smaller cold-start binaries require removing or separately loading grammar code: dozens are enabled (`crates/navo-ui/Cargo.toml:13–49,77–91`). Lazy registration alone does not remove linked grammars.

The historical executable is 98 MB (`docs/perf.md:63`). Release already uses fat LTO and separate debug configuration (`Cargo.toml:33–38`); Windows CI distributes `navo.pdb` separately (`.github/workflows/ci.yml:116–121`). Do not promise executable savings from stripping MSVC debug information. No Windows binary was inspected. **Effort L; risk:** resident RAM, stale listings, shutdown semantics, delayed first viewer/menu.

### 2. Large-folder CPU work still blocks input and rendering

**Evidence.** Commands notify the root window (`crates/navo-ui/src/window_view/mod.rs:327–335`). Rendering rebuilds every shown pane (`crates/navo-ui/src/window_view/tiles.rs:127–138`); each pane scans all rows for totals and clones selection (`crates/navo-ui/src/window_view/pane.rs:145–165`; `crates/navo-ui/src/window_view/status.rs:6–18`). Thus cursor rendering includes O(sum of visible-pane folder sizes), despite row virtualization (`crates/navo-ui/src/file_list/mod.rs:325–331`). Visible rows allocate names, extension strings, sizes and dates again (`crates/navo-ui/src/file_list/row.rs:103–125,176–203,243–252`).

Worker snapshots already sort and construct names (`crates/navo-fs/src/snapshot/mod.rs:34–50`); Fetch discards that listing (`crates/navo-ui/src/browser/cached/mod.rs:60–64`). UI delivery sorts again, allocating lowercase/extension keys and cloned rows (`crates/navo-ui/src/browser/mod.rs:367–376`; `crates/navo-ui/src/browser/sort/mod.rs:70–104`). Filter keystrokes synchronously scan and clone matching entries/details (`crates/navo-ui/src/browser/filter/mod.rs:187–214,233–264`). Historical Linux sorting/filtering already takes 92–603 ms at 500,000 files (`docs/perf.md:102–107`).

**Change/gain.** Cache totals by listing/selection revision; isolate pane invalidation; sort once in workers and filter through index vectors with cancellation. Cache display strings after larger costs are removed. Eliminates whole-folder work from cursor frames and potentially hundreds of milliseconds of blocked input in huge folders. Endpoint security is not needed to explain this cost. **Effort L; risk:** selection mapping, stale results, locale/timezone invalidation.

### 3. Cursor pauses trigger unrelated filesystem and shell work

**Evidence.** A 150 ms folder rest launches uncached listing prefetch, including folder metadata and enumeration (`crates/navo-ui/src/window_view/rest.rs:9,32–50`; `crates/navo-ui/src/browser/prefetch/mod.rs:142–175,188–203`; `crates/navo-ui/src/browser/cached/mod.rs:33–63`). A separate 300 ms rest opens a shell menu (`crates/navo-ui/src/window_view/shell_menu/prefetch.rs:14–16,152–172,186–211`). `IContextMenu::QueryContextMenu` runs on the shell worker (`crates/navo-shell/src/shell_menu/win/worker.rs:204–210`; `crates/navo-shell/src/shell_menu/win/loaded.rs:36–63`), potentially loading extensions even when no menu is requested. Startup warming additionally creates an empty file and queries three menu sources (`crates/navo-shell/src/shell_menu/warm.rs:30–35,71–88`).

The rest callback synchronously calls `drive_class`; a cache miss probes a volume with `CreateFileW` and `DeviceIoControl` (`crates/navo-ui/src/window_view/shell_menu/prefetch.rs:69–72,191–195`; `crates/navo-fs/src/drive_class.rs:46–64`; `crates/navo-fs/src/drive_class/windows.rs:35–107`). Worker replacement also waits synchronously for COM initialization (`crates/navo-shell/src/shell_menu/win/mod.rs:110–120`; `crates/navo-shell/src/shell_menu/win/worker.rs:78–101`).

**Change/gain.** Make menu creation demand-driven; classify volumes asynchronously; budget folder prefetch by measured benefit. Removes one speculative enumeration/menu query per qualifying rest and scan/DLL contention; latency benefit is unmeasured and workload-dependent. **Effort M; risk:** slower first menu or uncached folder entry.

### 4. Ordinary action callbacks can synchronously stall the UI

**Evidence.** Copy/paste call clipboard services directly (`crates/navo-ui/src/window_view/clipboard.rs:143–151,177–178`; `crates/navo-app/src/clipboard.rs:58–67`). Windows retries ten times with nine 10 ms sleeps; reads also call `GetClipboardData` (`crates/navo-shell/src/clipboard/windows.rs:60–70,177–183`). Undo directly queries SQLite under a mutex (`crates/navo-ui/src/window_view/undo.rs:79–89`; `crates/navo-app/src/rename_hooks.rs:71–79`). Opening files calls the launcher synchronously, which starts Explorer (`crates/navo-ui/src/browser/open.rs:34–39`; `crates/navo-app/src/launch/mod.rs:7–28`). Editor resolution probes PATH candidates and launches synchronously (`crates/navo-ui/src/window_view/editor.rs:68–69`; `crates/navo-app/src/editor/mod.rs:54–94`). Terminal launch also synchronously spawns (`crates/navo-ui/src/window_view/terminal.rs:23–28`; `crates/navo-app/src/terminal/mod.rs:132–149`).

Db permits five-second SQLite busy waits (`crates/navo-store/src/db/mod.rs:9,90–93`); journal mutex contention adds another blocking route.

**Change/gain.** Move these operations to owned workers, preserving clipboard thread/window ownership. Removes up to 90 ms of explicit clipboard sleeps and variable database/process-start/security delays from input handling. **Effort M; risk:** clipboard ownership, action ordering, late errors.

### 5. Navigation writes expendable state alongside durable state

**Evidence.** Full fetches request persistence (`crates/navo-ui/src/browser/cached/mod.rs:69–77`), restricted to open folders (`crates/navo-cache/src/disk/mod.rs:198–205`). The writer serializes all rows before digest deduplication (`crates/navo-cache/src/disk/writer.rs:22–30`), then writes a complete LZ4 blob (`crates/navo-cache/src/disk/codec.rs:50–77`; `crates/navo-cache/src/disk/table.rs:45–46`). Logical disk budget is 32 MiB (`crates/navo-cache/src/disk/mod.rs:28`). Cache reads update persisted access times; cache WAL uses synchronous OFF (`crates/navo-store/src/cache_db/mod.rs:51–61,114–123`).

Folder visits, throttled per folder to two seconds, run in background (`crates/navo-ui/src/window_view/visits.rs:8,46–57`), each with a score scan, upsert, history insert/prune and commit (`crates/navo-store/src/db/frecency.rs:25–61`). Ordinary Db uses WAL/NORMAL; journal and retention override to FULL (`crates/navo-store/src/db/mod.rs:156–162`; `crates/navo-store/src/journal/mod.rs:90–94`; `crates/navo-store/src/retention/mod.rs:50–53`). Close checkpoints are re-enabled; these configurations set no custom autocheckpoint threshold (`crates/navo-store/src/db/mod.rs:90–111`; `crates/navo-store/src/cache_db/connection.rs:31–36`). Physical WAL bytes/checkpoint frequency remain unmeasured.

Settings write the whole text per changed field (`crates/navo-app/src/settings/mod.rs:146–182`; `crates/navo-store/src/settings/saving.rs:110–114`). Atomic replacement writes, syncs, backs up and renames; Windows directory sync is a no-op (`crates/navo-store/src/atomic_write/mod.rs:130–157,185–187`). Session saving occurs on close, retains up to 200 rows/tab, and waits up to 300 ms for cache completion (`crates/navo-ui/src/window_view/closing.rs:13–20`; `crates/navo-app/src/startup/mod.rs:7,32–33`; `crates/navo-core/src/session/convert.rs:37–74`; `crates/navo-app/src/gui.rs:29,281–291`). Session payload is variable-sized Postcard plus a 26-byte header (`crates/navo-store/src/session/mod.rs:36–52`). Logging writes one variable-sized line synchronously per enabled event, without explicit sync (`crates/navo-diag/src/log_layer/mod.rs:99–107`; `crates/navo-diag/src/log_layer/rolling.rs:34–51`), rotating five files at 5 MiB (`crates/navo-app/src/logging/mod.rs:12–13`). Optional rename logging appends a batch and calls `sync_data`, with a 10 MiB rotation threshold (`crates/navo-app/src/rename_log/rotation.rs:5–30`).

**Change/gain.** Coalesce visits/settings/snapshots; keep cache access ages in memory; queue log writes. Preserve FULL journal durability. Potentially removes most redundant state writes and endpoint interceptions during navigation; quantify first. **Effort M; risk:** bounded loss of recent preferences/history, crash diagnostics ordering.

### 6. Cache accounting misses duplicated row ownership

Raw cache budget is 64 MiB (`crates/navo-cache/src/cache.rs:9`), with pinned entries exempt from eviction (`crates/navo-cache/src/store.rs:213–228`). Two sorted snapshots per tab are retained (`crates/navo-ui/src/browser/cached/mod.rs:14,183–205`), each owning cloned details and separate name Strings (`crates/navo-ui/src/browser/sort/mod.rs:89–104`); filtering adds another copy (finding 2). Raw accounting counts struct/name bytes only (`crates/navo-cache/src/listing.rs:25–42`).

**Change/gain.** Share immutable row storage and use order/filter indices; budget retained snapshots together. Potentially saves tens of MB in large multi-pane sessions; driver share remains unknown. **Effort L; risk:** row identity and cache invalidation errors.

### 7. Idle still wakes platform and discovery work

VSync continuously checks device health and invalidates windows (`vendor/gpui-pre-windows/src/platform.rs:453–491`); this proves wakeups, not full application rendering every refresh. Drive refresh repeats every two seconds in background (`crates/navo-ui/src/window_view/drives.rs:15,70–85`), calling drive/network APIs (`crates/navo-app/src/drives/mod.rs:115–124`; `crates/navo-shell/src/drives/windows.rs:157–178`); details cache lasts 30 seconds (`crates/navo-app/src/drives/mod.rs:9–30`). Index activity wakes every 500 ms, even when disabled (`crates/navo-ui/src/window_view/index_activity.rs:9–42`). Network watcher fallback relists every five seconds (`crates/navo-ui/src/browser/watched/mod.rs:14,204–213,301–315`). Native watchers instead block on events and channels (`crates/navo-fs/src/local/win_watch.rs:195–197`; `crates/navo-fs/src/watch/mod.rs:161–165`).

**Change/gain.** Gate inactive-window VSync; use device notifications and adaptive discovery/status polling. Reduces refresh-rate wakeups and periodic OS/security work; responsiveness gain uncertain, idle power gain more likely. **Effort M; risk:** missed hotplug/status updates and delayed repaint.

## Folder/cursor call ledger

A fresh ordinary Windows folder costs one metadata query plus one native enumeration: one FindFirstFileExW/LARGE_FETCH and O(N) FindNextFileW API calls, with metadata from returned records, not N file opens (`crates/navo-ui/src/browser/cached/mod.rs:33–63`; `crates/navo-fs/src/local/win_list.rs:45–57,78–103,123–146`). Kernel I/O counts depend on enumeration buffering. New watched locations add a directory handle and two threads; watch establishment queues a refresh, potentially adding another enumeration (`crates/navo-fs/src/local/win_watch.rs:62–97`; `crates/navo-fs/src/watch/mod.rs:124–139`; `crates/navo-ui/src/browser/watched/mod.rs:156–167`). A settled cached arrival can stop after metadata (`crates/navo-ui/src/browser/cached/mod.rs:43–50`). Watch batches with at most 64 distinct names can patch through targeted metadata; larger batches relist (`crates/navo-ui/src/browser/watched/mod.rs:13,35–42`; `crates/navo-ui/src/browser/cached/mod.rs:85–114`).

The core cursor command makes no filesystem call (`crates/navo-core/src/apply/pane.rs:61–68`); delayed work is finding 3. Browser command bookkeeping also revisits cache keys and can start background volume-identity probes after 30 seconds (`crates/navo-ui/src/browser/mod.rs:321–323`; `crates/navo-ui/src/browser/cached/mod.rs:232–242`; `crates/navo-cache/src/keys.rs:10,144–177`). Open preview adds file loading or folder listing/scanning after 150 ms (`crates/navo-ui/src/window_view/preview/mod.rs:37,165–225`; `crates/navo-ui/src/window_view/preview/folder.rs:147–191`). Row icons use internal type/extension lookup, not SHGetFileInfo (`crates/navo-ui/src/file_list/kind.rs:19–27`). Navo already has single-enumeration-quality row metadata; surrounding speculative, persistence and refresh work creates the excess.

## Verification and limits

Created only `PERF-AUDIT-SOL.md`. Ran `git status --short` (initially clean; finally only this report), `git worktree list`, `git branch --show-current`, `git rev-parse --short HEAD`, and targeted `rg`, `cat`, `nl`/`sed` reads. Missing guessed paths were resolved through source searches. `git diff --exit-code` passed: tracked files unchanged. `wc -w` and an inline Python citation/line-bound check passed. No cargo, builds, tests, server, benchmark or Windows runtime trace ran. Open risks: gain estimates, checkpoint volume, extension behavior and GPU memory attribution require owner-PC measurements.

## First five changes and before/after measurements

1. **Opt-in resident/direct activation; defer startup extras.** Measure cold/warm/reopen-to-usable phases, executable sections, DLL loads and private bytes.
2. **Remove whole-folder work from UI frames.** Measure key-to-present p50/p95/p99, UI CPU and allocations at 5k/100k/500k rows with one/four panes.
3. **Make shell menus demand-driven and budget prefetch.** Trace opens, DLL/registry activity and enumeration counts during identical cursor/rest sequences; compare menu latency.
4. **Offload blocking action services.** Measure longest UI task and key-to-feedback latency during clipboard contention, undo, editor and terminal launch.
5. **Coalesce expendable persistence.** Trace write bytes/counts, flushes, renames, WAL/checkpoints and folder-enter p95; verify crash recovery on isolated profiles.
