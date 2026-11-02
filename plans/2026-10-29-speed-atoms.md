# Speed phase atoms · everything left after SP1a–SP7b

Status: ready · Repo: `navo` · Branch: one `sp<id>` branch per atom from `main` · Started: 2026-10-29

Parent plan: `plans/2026-10-28-speed.md` (targets, SD1–SD5, owner answers SQ1–SQ3). This file holds the remaining atoms, cut so a Sonnet 5.5 medium worker can do each one alone, in parallel waves.

## Goal

Navo stays responsive in 50k–500k-file folders (no sort, filter or status pass on the UI thread), has no idle wakeups while hidden or idle, can live in the tray and come back on Win+Alt+N, and CI fails a build that grows too big, starts too slowly or makes the listing hot paths slower.

## Already done (verified in `git log` on 2026-10-29)

| Atom | Commit | Note |
| --- | --- | --- |
| SP0a slow-disk shim `NAVO_SLOW_FS_MS` | 119eb6e7 | `navo-fs/src/slow.rs`, tests in place |
| SP0b `--trace <file>` JSON per input | b5c5ddc7, a0de115a | |
| SP1a instance pipe on events | 99d443f8 | own overlapped pipe; `interprocess` not needed |
| SP1b index activity stops when idle | 13fd478b | see D3 |
| SP1c drives on `WM_DEVICECHANGE` | 91a05194 | |
| SP2, SP3, SP5, SP6 | 8d2c0f3b, 2e150737, 1eeff83d | |
| SP4 phases 1–2 | fd3b62d1, fa8def8c | companion discovery already runs with a cancel flag off the UI thread; no `SHGetFileInfo` left in the code |
| SP7a, SP7b | a0de115a, 5bfe4925 | per-pane cached views |
| SP8a–c | 6c90d27f, 1c455cc8, 8628ba3311a10bb6e90b943acda185867e1dfe3b | L1 clean: structured, automotive, pdf, diag, index, core, fs, shell, retention |
| SP11 | branch `sp11-prewarm` | merging, not in this plan |

## Decisions

Final. A worker that finds one wrong stops and reports.

- D1 — **Sync limit 20,000 rows.** Sort changes and filter keystrokes on a tab whose full listing has ≤ 20,000 rows stay synchronous on the UI thread (≤ 4 ms by the 100k bench: 17 ms per keystroke at 100k); above it they run on the listing pool. Reason: small folders keep today's exact behaviour and tests; only big folders pay the async cost.
- D2 — **Index vectors on the worker, materialised rows on the UI.** Sort and filter compute `Vec<u32>` orders over the source `Arc<[FsEntry]>` on the worker, then build the shown `Arc<[FsEntry]>` and `Listing` there too. The UI keeps `Arc<[FsEntry]>` rows, so the 47 row consumers stay unchanged. Reason: the UI-thread win is the same; a view type over indices would touch ~50 call sites for ~36 MB saved only in 500k folders.
- D3 — **Index activity keeps its 500 ms poll only while a job runs.** SP1b already stops it when the index is off or idle; a push channel would change `navo-index` hosts for a 2 Hz progress label. Reason: no idle wakeup is left, so the SP1 target is met.
- D4 — **Pure listing view code lives in `navo-fs::view`.** `SortBy`, `SortKey`, `Needle`, `Scratch` move there from `navo-ui` and are re-exported at their old paths. Reason: `navo-fs` owns `FsEntry`, has no GPUI, so the gungraun benches build in about a minute.
- D5 — **Cargo feature is `tracy`, not `trace`.** `--trace <file>` already names the JSON input log; one word for two tools confuses. `navo-app` feature `tracy = ["navo-diag/tracy"]`; `tracy-client` `=0.19.0` and `tracing-tracy` `=0.12.0`, both `default-features = false` plus `enable` on `tracy-client`.
- D6 — **Tracy layer filter.** With the feature on, the log layer gets its existing `EnvFilter` as a per-layer filter and the Tracy layer sees `debug` spans. Without the feature the subscriber is built exactly as today.
- D7 — **Span names are fixed:** `listing.load`, `listing.sort`, `listing.filter`, `listing.view_job`, `status.totals`, `resident.summon`. Each atom that creates one of these functions adds `let _span = tracing::debug_span!("<name>").entered();` as its first line.
- D8 — **Bench guard is gungraun `=0.20.0`** with `gungraun-runner` `0.20.0`, Callgrind `Ir`, soft limit 10 %, base-vs-head on the same runner, in its own workflow `.github/workflows/bench.yml` (Linux, needs valgrind). It runs on pushes to `main` and on pull requests that touch `crates/navo-fs/**` or `crates/navo-core/**`. A regression fails that workflow; releases from `ci.yml` are not blocked (see Q1).
- D9 — **CI perf guards fail builds on Windows only.** `continue-on-error` goes from the Windows binsize step (160 MiB limit, `NAVO_EXE_LIMIT`) and the perf-summary step (median warm first frame ≤ 300 ms, private bytes ≤ 160 MB). Linux binsize stays informational (Linux CI is manual-only).
- D10 — **VSync parking in two steps.** First the VSync thread parks while every window is hidden or minimised (SP1d). Then it parks after 500 ms with no present and no main-thread activity, and wakes on any window message or main-thread dispatch (SP1e). Reason: GPUI on Windows asks for frames by polling `WM_PAINT` every VSync; only the platform layer can stop that.
- D11 — **mimalloc `=0.1.52` behind feature `mimalloc`**, wrapped by the existing `CountingAlloc` (it replaces `System` inside it), off by default. CI uploads a second exe built with the feature. It becomes default only if the owner's A/B shows ≥ 5 % faster warm first usable or ≥ 10 % fewer private bytes with no listing regression.
- D12 — **Tray via `tray-icon` `=0.26.1`** (`default-features = false`, Windows-only target dependency; shares `windows-sys` 0.61). **Hotkey via our own `RegisterHotKey`** on the same thread. Reason: `global-hotkey` 0.9.0 pulls a second `windows-sys` (0.59) and needs the same message loop anyway.
- D13 — **One resident thread** (`navo-resident`, Windows only) owns the tray icon and the hotkey and runs a blocking `GetMessageW` loop: zero wakeups while idle. It talks to the app only through the existing `OpenSender` (hand-off inbox).
- D14 — **Settings place:** a new Settings section "Tray" (Windows only, hidden elsewhere like "Show system files") with `tray.keep-running` (toggle, default off, SQ1) and `tray.summon-key` (text, default `Win+Alt+N`, empty turns it off, SQ2). There is no Settings → Keys section today; this is the closest match.
- D15 — **Close with the tray on:** closing the last window saves the session and hides the window (`SW_HIDE`); other windows close as today. Quit (tray menu or app menu) runs today's save-and-quit path and releases the profile lock.
- D16 — **Summon:** hotkey, tray "Open" and a second `navo.exe` all send `OpenRequest::default()` through the hand-off inbox; `take_arrival` shows a hidden window before `activate_window`. Tray "Quit" sends `OpenRequest { quit: true, .. }`.
- D17 — **Walker trial in `xtask bench-walk`** with new targets `walk-dua` (`dua-core` `=5.0.1`, `walk(root, 4, Order::Completion, Options::default().skip_metadata(), |_| true)`) and `walk-ignore` (`ignore` `=0.4.34`, `WalkBuilder::new(root).standard_filters(false).threads(4).build_parallel()`). CI uploads `xtask.exe` as artifact `navo-bench-tools-windows` so the owner can run it. A walker is adopted only if it is ≥ 20 % faster in files per second on both of the owner's drive types.
- D18 — **L1 lint fixes may change signatures.** Callers outside the atom's scope change only when no other atom of the same wave lists that file; otherwise the item keeps its signature with `#[expect(clippy::<lint>, reason = "caller changes in SP8i")]`, and SP8i removes those.
- D19 — **Stale results are dropped, never merged.** Every off-thread view result carries the tab, its location, a per-tab view revision and `Arc::as_ptr` of the source rows; any mismatch drops it.

## Risks and rollback

- R1 — Async sort/filter could act on rows the user does not see (marks by row index). Prevented: until a view result applies, the shown listing and its marks stay as drawn; results apply through the existing `ListingFiltered`/`ListingRefiltered` events that remap marks by name (D19). SP7i is a senior atom with tests for marks during a pending job.
- R2 — VSync parking could leave a dirty window undrawn. Prevented: wake on every window message and main-thread dispatch; owner checks a file created while Navo is in the background appears within 1 s.
- R3 — Hidden resident window holds memory and the profile lock. Accepted: opt-in (SQ1); Quit is explicit.
- R4 — A bench guard on noisy hardware could flake. Prevented: Callgrind counts instructions, base and head run on the same runner.
- Rollback: each atom is one squash commit on `main`; `git revert <sha>`.

## Components

| Component | Crate / path | Contract (in → out) | New / change | Test |
| --- | --- | --- | --- | --- |
| `profiling` | `navo-diag/src/profiling.rs`, `navo-app/src/logging` | feature `tracy` → Tracy layer, `frame_mark()` | new | `navo-diag` unit, `navo-app` logging tests |
| `ci-guards` | `.github/workflows/ci.yml`, `bench.yml` | build → fail on size, start-up, Ir regression | change, new | CI run |
| `vsync-park` | `vendor/gpui-pre-windows/src/{platform,vsync,events,dispatcher}.rs` | window state, activity → VSync thread runs or parks | change | vendor unit tests on Windows CI |
| `listing-view` | `navo-fs/src/view/` | `&[FsEntry]`, `SortKey`, filter → `Vec<u32>` order | new | `navo-fs` unit + proptest, gungraun benches |
| `browser-view` | `navo-ui/src/browser/{sort,filter,cached,mod}.rs`, new `view_jobs.rs` | listing, sort, filter → shown rows (sync ≤ 20k, worker above) | change | `navo-ui` browser tests |
| `status-totals` | `navo-ui/src/window_view/{status,pane}.rs`, `navo-core/src/selection` | rows Arc + selection → `StatusBarProps` cached | change | `navo-ui` window_view tests |
| `shared-selection` | `navo-core/src/state/tab.rs`, `navo-ui/src/file_list/mod.rs` | `Arc<Selection>` copy-on-write | change | `navo-core` state tests |
| `row-text` | `navo-ui/src/file_list/{row,rows}.rs` | entry → cached display strings | change (conditional) | `navo-ui` file_list tests |
| `allocator` | `navo-app/src/heap.rs`, `Cargo.toml` | feature `mimalloc` | change | `navo-app` heap tests |
| `resident` | `navo-app/src/resident/`, `navo-ui/src/window_view/{closing,hand_off}.rs`, `navo-ui/src/settings` | settings, hotkey, tray → hide, show, quit | new | settings + hand-off tests, Windows CI |
| `walkers` | `xtask/src/bench_walk`, `navo-index/src/walker.rs` | tree → files/s; adopt the winner | change | xtask tests, `navo-index` walker tests |
| `lints-l1` | every remaining crate, `Cargo.toml` | code → passes L1 | change | clippy |

## Shared commands

Every cargo command runs exactly as `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo …`, with the worker's own `CARGO_TARGET_DIR` from the worker rules. Workers never open real windows; GPUI tests run headless (`TestAppContext`).

- Crate gate: `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo xtask check -p <crate> [-p <crate>…]`
- `<L1 flags>` stands for exactly: `-D clippy::needless_pass_by_value -D clippy::semicolon_if_nothing_returned -D clippy::redundant_clone -D clippy::format_push_string -D clippy::needless_for_each -D clippy::trivially_copy_pass_by_ref -D clippy::match_same_arms -D clippy::assigning_clones -D clippy::wildcard_imports -D clippy::ref_option -D clippy::fn_params_excessive_bools`
- Module-scoped L1 count (navo-ui atoms): `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo clippy -p navo-ui --all-targets --message-format=short -- <L1 flags with -W instead of -D> 2>&1 | grep -E '^crates/navo-ui/src/<scope>' | grep -c warning` must print `0`.
- Windows-only code (vendored gpui, `cfg(windows)` blocks) cannot build on Linux. Its check is the `clippy-windows` and `gate-windows` jobs on the branch push; the orchestrator pushes the branch and reads the result.

## Atoms

Roles: `patcher` and `api-builder`/`ui-builder` run on Sonnet 5.5 medium. `senior` runs on Opus 5.5 medium. Pure-backend atoms (SP0f, SP7e, SP8d, SP9a, SP12a, SP12c) may run on DeepSeek V4.1 Flash (dshw) or Codex Sol 6.1 high instead; DeepSeek never runs an atom that touches `navo-ui` or the gallery. Windows CI is the real gate; a Linux run is a quick filter. Atoms in one wave touch disjoint files and at most one of them changes `Cargo.lock`.

| Atom | Goal | Role | Effort | Wave | After | Check | Owner check (SD1) | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| SP0c | Tracy feature + frame marks | api-builder | medium | 1 | — | check -p navo-diag -p navo-app -p navo-ui | trace one session in Tracy | done |
| SP0d | Windows perf guards fail builds; vendor tests in CI | patcher | medium | 1 | — | CI green on branch | — | done |
| SP1d | VSync parks while hidden/minimised | senior | medium | 1 | — | Windows CI | VSyncProvider idle while minimised | done |
| SP7c | Shown rows `Rc` → `Arc` | patcher | medium | 1 | — | check -p navo-ui -p navo-ui-gallery | — | done |
| SP7d | Move `SortBy`/`SortKey`/`Needle` to `navo-fs::view` | patcher | medium | 1 | — | check -p navo-fs -p navo-ui | — | done |
| SP8d | L1: navo-store, navo-cache | api-builder | medium | 1 | — | clippy L1 on both | — | done |
| SP10a | Tray settings fields | ui-builder | medium | 1 | — | check -p navo-ui -p navo-app | — | done |
| SP1e | VSync parks when idle | senior | medium | 2 | SP1d | Windows CI | VSyncProvider idle with Navo visible, unfocused | done |
| SP7e | `navo-fs::view` sort and filter orders | api-builder | medium | 2 | SP7d | check -p navo-fs | — | done |
| SP7j | Status totals cached | api-builder | medium | 2 | — | check -p navo-core -p navo-ui | trace Down×50 in 500k folder | done |
| SP8e | L1: navo-app | api-builder | medium | 2 | SP0c | clippy L1 navo-app | — | done |
| SP12a | bench-walk: dua-core, ignore; ship xtask.exe | api-builder | medium | 2 | — | check -p xtask | run bench-walk on both drives | done |
| SP0f | gungraun listing benches + bench.yml | api-builder | medium | 3 | SP7e | `cargo bench -p navo-fs --no-run` | — | done |
| SP7f | Browser sorts and filters through `navo-fs::view` | api-builder | medium | 3 | SP7c, SP7e | check -p navo-ui | `navo bench` folder rows | done |
| SP7k | `Arc<Selection>` | api-builder | medium | 3 | SP7c, SP7j | check -p navo-core -p navo-ui -p navo-ui-gallery | — | done |
| SP8f | L1: navo-ui `window_view/viewer` | api-builder | medium | 3 | — | module-scoped L1 = 0 | — | done |
| SP10b | Hide on close, show on arrival | senior | medium | 3 | SP10a | check -p navo-ui -p navo-app + Windows CI | close hides, relaunch shows | done |
| SP7g | Fresh listing sorted on the worker | api-builder | medium | 4 | SP7f | check -p navo-ui | open 500k folder: no UI stall | done |
| SP8g | L1: rest of `window_view` | api-builder | medium | 4 | SP7j, SP10b | module-scoped L1 = 0 | — | blocked |
| SP9a | mimalloc feature + A/B exe | api-builder | medium | 4 | — | check -p navo-app | A/B `--metrics` and `navo bench` | done |
| SP7h | Big cached folder sorted on the worker | api-builder | medium | 5 | SP7g | check -p navo-ui | re-enter 500k folder: no stall | done |
| SP8h | L1: navo-ui outside window_view/browser/file_list | api-builder | medium | 5 | SP10a | module-scoped L1 = 0 | — | done |
| SP10c | Resident thread: tray + hotkey | senior | medium | 5 | SP10b | Windows CI | tray icon, Win+Alt+N registers | done |
| SP7i | Async sort/filter above 20k rows | senior | medium | 6 | SP7h | check -p navo-ui | type filter in 500k folder | blocked |
| SP10d | Summon and Quit wiring, summon timing | senior | medium | 6 | SP10c | check -p navo-ui -p navo-app + Windows CI | summon ≤ 50 ms in log | done |
| SP12c | Adopt the faster walker (only if D17 holds) | api-builder | medium | 6 | SP12a + owner numbers | check -p navo-index | index build time | blocked |
| SP7l | Row text cache (only if measured) | ui-builder | medium | 7 | SP7i, SP7k | check -p navo-ui | pane render p95 | blocked |
| SP8i | Deny L1 workspace-wide + leftovers | api-builder | medium | 8 | all SP7, SP8, SP10 | `cargo clippy --workspace --all-targets -- -D warnings` + clippy-windows | — | blocked |

---

### SP0c — Tracy feature with frame marks and spans

- **Role:** api-builder · **Wave:** 1 · **After:** — · Changes `Cargo.lock` (only one in wave 1).
- **Read:** `crates/navo-app/src/logging/mod.rs:58-90`, `crates/navo-diag/Cargo.toml`, `crates/navo-diag/src/lib.rs`, `crates/navo-app/Cargo.toml`, `crates/navo-ui/src/window_view/render.rs:1-20`.
- **Change:**
  - `navo-diag/Cargo.toml`: `[features] tracy = ["dep:tracy-client", "dep:tracing-tracy"]`; optional deps `tracy-client = { version = "=0.19.0", default-features = false, features = ["enable"], optional = true }`, `tracing-tracy = { version = "=0.12.0", default-features = false, optional = true }`.
  - New `navo-diag/src/profiling.rs`: `pub fn frame_mark()` (calls `tracy_client::frame_mark()` under `cfg(feature = "tracy")`, empty otherwise) and `pub fn tracy_layer<S>() -> Option<impl Layer<S>>` (Some only with the feature; starts `tracy_client::Client::start()`).
  - `navo-app/Cargo.toml`: `[features] tracy = ["navo-diag/tracy"]`.
  - `navo-app/src/logging/mod.rs`: D6. With the feature, `Registry::default().with(opened.layer.with_filter(filter)).with(navo_diag::profiling::tracy_layer())`; without it, the current line unchanged.
  - `navo-ui/src/window_view/render.rs`: first line of `render` calls `navo_diag::profiling::frame_mark()`.
  - `docs/perf.md`: a short "Profiling with Tracy" section: build with `cargo build --release -p navo-app --features tracy`, the Tracy viewer version that matches `tracy-client-sys` 0.30.1 (from that crate's README table), and `samply record navo.exe` as the sampling alternative.
- **Do not touch:** the `--trace` JSON log (`input_trace`), any listing code (spans for listing come with SP7 atoms, D7).
- **Tests:** `navo-diag`: `frame_mark()` and `tracy_layer()` are callable without the feature (`tracy_layer` returns None). `navo-app` logging: existing tests pass; one test that `install` still writes an `info` event to the log ring.
- **Check:** `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo xtask check -p navo-diag -p navo-app -p navo-ui` and `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo check -p navo-app --features tracy`; `cargo deny check` passes.
- **Owner check:** optional: run a `tracy` build with the Tracy viewer for one session; frames appear.
- **Stop if:** `tracing-tracy` 0.12.0 does not accept `tracing-subscriber` 0.3.23.

### SP0d — CI perf guards fail builds; vendored gpui tests run in CI

- **Role:** patcher · **Wave:** 1 · **After:** —
- **Read:** `.github/workflows/ci.yml:90-215`, `docs/perf.md:1-6`.
- **Change:** in `ci.yml`: remove `continue-on-error: true` and "(informational until the alpha features are in)" from the Windows "Binary size" step and from the "Performance summary" step; leave Linux binsize, copy benchmark, listing benchmark and UIA probe unchanged. Add to `gate-windows` after the nextest step: `- run: cargo test -p gpui-pre-windows --lib`. In `docs/perf.md` line 5, say the job now fails the build on those two limits.
- **Do not touch:** thresholds in `xtask` (already 160 MiB, 300 ms, 160 MB).
- **Tests:** none (CI config).
- **Check:** `taplo check` unaffected; `git diff` shows only those lines; the orchestrator pushes the branch and the Windows jobs are green.
- **Owner check:** none.

### SP1d — VSync thread parks while every window is hidden or minimised

- **Role:** senior (Opus): platform thread, Windows-only, no Linux build. · **Wave:** 1 · **After:** —
- **Read:** `vendor/gpui-pre-windows/src/platform.rs:405-500` (`begin_vsync_thread`), `src/vsync.rs`, `src/events.rs` (`WM_ACTIVATE`, `WM_SHOWWINDOW`, `WM_SIZE` handlers).
- **Change:** D10 step 1. A pure `fn vsync_wanted(windows: impl Iterator<Item = WindowShow>) -> bool` (`WindowShow { visible: bool, minimized: bool }`; true when any window is visible and not minimised). The VSync loop asks it each tick; when false it blocks on a wake primitive (an auto-reset event or `Condvar`) instead of calling `DwmFlush`. `WM_SHOWWINDOW` (shown), `WM_SIZE` (`SIZE_RESTORED`/`SIZE_MAXIMIZED`) and `WM_ACTIVATE` signal it. On wake it runs the device-lost check before the first frame. `stop` still ends the thread.
- **Do not touch:** the device-lost recovery, frame-signal recording, DirectX renderer.
- **Tests:** unit tests for `vsync_wanted` (all hidden → false; one visible not minimised → true; visible but minimised → false); a thread test with a fake wait closure that the loop parks and resumes on signal (bounded by 2 s).
- **Check:** `cargo fmt --all --check` locally; branch push → `clippy-windows` and `gate-windows` green (SP0d's vendor test step runs the new tests; if SP0d has not merged, run them in the PR description command `cargo test -p gpui-pre-windows --lib` on CI by adding the same step).
- **Owner check:** Process Explorer → navo.exe → Threads: `VSyncProvider` CSwitch Delta stays 0 while Navo is minimised for 60 s; restoring draws at once.

### SP7c — Shown rows are `Arc<[FsEntry]>` instead of `Rc<[FsEntry]>`

- **Role:** patcher · **Wave:** 1 · **After:** —
- **Read:** `crates/navo-ui/src/browser/mod.rs:68-90,284-290`, `crates/navo-ui/src/browser/filter/mod.rs:1-30,147-170`, `crates/navo-ui/src/file_list/mod.rs:50-65`.
- **Change:** mechanical, compiler-guided: every `Rc<[FsEntry]>` / `Rc<[navo_fs::FsEntry]>` becomes `Arc<[FsEntry]>`, every `Rc::from([])` for those rows becomes `Arc::from([])`; fix the `use` lines. Files: `navo-ui/src/browser/mod.rs`, `browser/filter/mod.rs`, `file_list/mod.rs`, `file_list/tests/mod.rs`, `window_view/drives.rs`, `shortcuts/page.rs`, `navo-ui-gallery/src/{mouse_page,command_center_page,window_page,kinds_page,pane_page,menu_page,drives_page,pane_sample}.rs`. Exception to the 3-file rule: a type rename only, no logic.
- **Do not touch:** any other `Rc` (selection, lines, closures).
- **Tests:** existing tests only.
- **Check:** `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo xtask check -p navo-ui -p navo-ui-gallery`.
- **Owner check:** none.

### SP7d — `SortBy`, `SortKey`, `Needle`, `Scratch` move to `navo-fs::view`

- **Role:** patcher · **Wave:** 1 · **After:** —
- **Read:** `crates/navo-ui/src/file_list/sort.rs` (14 lines), `crates/navo-ui/src/name_match/mod.rs` (129), `name_match/tests.rs`, `crates/navo-fs/src/lib.rs:1-60`.
- **Change:** D4. New `navo-fs/src/view/mod.rs` (`mod needle; mod sort_key; pub use …`), `view/needle.rs` = `name_match/mod.rs` moved verbatim (items made `pub`), `view/needle/tests.rs` = its tests moved, `view/sort_key.rs` = `SortBy`/`SortKey` moved. `navo-fs/src/lib.rs`: `pub mod view;`. `navo-ui/src/name_match/mod.rs` becomes `pub(crate) use navo_fs::view::{Needle, Scratch};` (plus any other items it exported); `navo-ui/src/file_list/sort.rs` becomes `pub use navo_fs::view::{SortBy, SortKey};`.
- **Do not touch:** callers (paths stay valid through the re-exports).
- **Tests:** moved tests pass in `navo-fs`.
- **Check:** `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo xtask check -p navo-fs -p navo-ui`.
- **Stop if:** `name_match` uses anything from `navo-ui` or GPUI.

### SP8d — L1: navo-store and navo-cache

- **Role:** api-builder · **Wave:** 1 · **After:** —
- **Read:** the clippy output of the check command with `-W` (26 hits on Linux at `b6394a34`).
- **Change:** fix every L1 hit in `crates/navo-store/**` and `crates/navo-cache/**`. Borrow instead of move, drop redundant clones, `write!` instead of `push_str(&format!())`, `for` instead of `for_each`. D18 for callers in other crates (wave-1 files listed by SP7c, SP0c, SP10a are off limits).
- **Do not touch:** SQLite pragmas, journal or safety-net behaviour.
- **Tests:** existing tests only.
- **Check:** `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo clippy -p navo-store -p navo-cache --all-targets -- -D warnings <L1 flags>` and `… cargo xtask check -p navo-store -p navo-cache`.

### SP10a — Tray settings fields

- **Role:** ui-builder · **Wave:** 1 · **After:** —
- **Read:** `crates/navo-ui/src/settings/fields.rs:1-60,200-230`, `settings/general_fields.rs:60-95`, `settings/values.rs` (how `ShowSystem` is read and how a Windows-only field hides), the `en-US` `.ftl` for settings labels.
- **Change:** D14. `Section::Tray` (label "Tray"), fields `Field::KeepInTray` (`tray.keep-running`, `Control::Toggle`, default off, help "Closing the window keeps Navo in the tray") and `Field::SummonKey` (`tray.summon-key`, `Control::Text`, default `Win+Alt+N`, help "Shows Navo from anywhere; leave empty for none"). Both Windows-only, hidden elsewhere like `ShowSystem`. `Values` getters `keeps_in_tray() -> bool` and `summon_key() -> &str`. A pure parser `parse_summon_key(&str) -> Result<Option<SummonKey>, SummonKeyError>` in `navo-ui/src/settings/summon_key.rs` (`SummonKey { win, ctrl, alt, shift: bool, key: char }`, key `A–Z` or `0–9`, at least one modifier, empty → `Ok(None)`); the field shows the error text for an invalid value.
- **Do not touch:** keymap, Shortcuts page.
- **Tests:** parser: `Win+Alt+N`, `ctrl+shift+1`, empty, `N` (no modifier → error), `Win+Alt+F13` (error); default values; field hidden on non-Windows. Headless settings tests as the existing ones.
- **Check:** `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo xtask check -p navo-ui -p navo-app` (add `-p navo-l10n` if `.ftl` changed).
- **Owner check:** none (behaviour comes with SP10b–d).

### SP1e — VSync parks when nothing presents

- **Role:** senior (Opus) · **Wave:** 2 · **After:** SP1d
- **Read:** SP1d's diff; `vendor/gpui-pre-windows/src/dispatcher.rs` (main-thread dispatch), `events.rs:1290-1370` (paint path), `directx_renderer.rs` present call.
- **Change:** D10 step 2. A pure `IdleGate { last_present, last_activity }` with `should_park(now) -> bool` (true when both are older than 500 ms). The renderer's present records `last_present`; every window message in the main `wnd_proc` and every main-thread dispatch records activity and signals the SP1d wake primitive. Animation frames keep presenting, so they never park.
- **Do not touch:** gpui core; frame pacing while active.
- **Tests:** `IdleGate` unit tests (fresh present → run; 600 ms idle → park; activity after park → run).
- **Check:** as SP1d.
- **Owner check:** with Navo visible but another app focused, idle 60 s: `VSyncProvider` CSwitch Delta ≈ 0; a file created in the shown folder appears within 1 s; hover and typing stay smooth.

### SP7e — `navo-fs::view` sort and filter orders

- **Role:** api-builder · **Wave:** 2 · **After:** SP7d
- **Read:** `crates/navo-ui/src/browser/sort/mod.rs:63-137` (semantics to copy), `crates/navo-ui/src/browser/filter/mod.rs:230-300` (`is_visible`, `is_reserved`, `shown`), SP7d's `navo-fs/src/view/`.
- **Change:** new `view/sort.rs`: `pub fn sort_order(rows: &[FsEntry], key: SortKey, folders_first: bool, cancel: &CancelToken) -> Option<Vec<u32>>`: keys precomputed once per row (lowercase name, extension, is_file) exactly as `keyed`; same comparison as `compare`; final tie-break by row index for determinism; checks `cancel` every 4,096 rows while building keys and once after sorting; `None` when cancelled. New `view/filter.rs`: `pub struct ViewFilter<'a> { pub needle: Option<&'a Needle>, pub show_hidden: bool, pub show_system: bool }` and `pub fn filter_order(rows: &[FsEntry], order: &[u32], filter: &ViewFilter, cancel: &CancelToken) -> Option<Vec<u32>>` with today's rules (reserved names via `navo_core::brand::is_reserved_name` on `name.to_text()`, hidden/system, needle `matches`). `pub fn materialize(rows: &[FsEntry], order: &[u32]) -> Vec<FsEntry>`. Spans `listing.sort` and `listing.filter` (D7). Rows over `u32::MAX` are out of scope: `try_from` and return `None`.
- **Do not touch:** `navo-ui` (wired in SP7f).
- **Tests:** each `SortBy` ascending/descending; folders first on/off; ties by lower name; cancelled token → `None`; filter: hidden, system, reserved, needle with `^` and `*`; a proptest that `sort_order` is a permutation and sorted by the comparator.
- **Check:** `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo xtask check -p navo-fs`.

### SP7j — Status totals cached per rows and selection

- **Role:** api-builder · **Wave:** 2 · **After:** —
- **Read:** `crates/navo-ui/src/window_view/status.rs` (19), `window_view/pane.rs:150-175`, `crates/navo-core/src/selection/mod.rs:45-100`.
- **Change:** `navo-core` `Selection::marked_rows(&self) -> impl Iterator<Item = u64> + '_` (set bits only, no allocation). `status.rs`: `pub(super) struct TotalsCache` holding, per `TabId`, `(rows_ptr: usize, rows_len: usize, Totals { folders, files, bytes })`; folder/file/byte totals recompute only when `Arc::as_ptr`/len change; marked count and bytes come from `marked_rows()` over the rows (O(marked)). Span `status.totals`. `pane_status` uses it; the cache lives in a `RefCell` field on the window view (add the field in `window_view/mod.rs`).
- **Do not touch:** `StatusBarProps` shape, the status bar UI.
- **Tests:** same rows Arc twice → totals computed once (test counter); new Arc → recomputed; marked count/bytes follow selection changes; `marked_rows` on empty, one word, across word boundary.
- **Check:** `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo xtask check -p navo-core -p navo-ui`.
- **Owner check:** `navo --trace t.jsonl` in a 500k folder, press Down 50 times: p95 key-to-frame (report the number; target ≤ 16 ms after SP7k).

### SP8e — L1: navo-app

- **Role:** api-builder · **Wave:** 2 · **After:** SP0c
- **Read:** check output with `-W` (39 hits on Linux at `b6394a34`); Windows-only hits appear in `clippy-windows`.
- **Change:** fix every L1 hit in `crates/navo-app/**` (D18). Read each `#[cfg(windows)]` block in files you touch and fix the same patterns there by reading.
- **Do not touch:** `Cargo.toml`, crash, journal and retention logic beyond the lint fix.
- **Check:** `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo clippy -p navo-app --all-targets -- -D warnings <L1 flags>` and `… cargo xtask check -p navo-app`.

### SP12a — bench-walk targets for dua-core and ignore; ship xtask.exe

- **Role:** api-builder · **Wave:** 2 · **After:** — · Changes `Cargo.lock` (only one in wave 2).
- **Read:** `xtask/src/bench_walk/mod.rs:1-80`, `bench_walk/index.rs:1-63`, `bench_walk/options.rs`, `xtask/Cargo.toml`.
- **Change:** D17. `xtask/Cargo.toml`: `dua-core = "=5.0.1"`, `ignore = "=0.4.34"`. `options.rs`: targets `walk-dua`, `walk-ignore`, compared against the existing `walk` (no `find`: it is content search). New `bench_walk/walkers.rs` with one measure function per target on the same `size-tree` fixture as `walk`; each counts entries and fails if the count differs from our `walk`. Report adds a "files/s" column. `ci.yml` `build-windows`: `cargo build --release -p xtask` and upload `target/release/xtask.exe` as artifact `navo-bench-tools-windows`.
- **Do not touch:** `navo-index`, `navo-fs`.
- **Tests:** the tiny-fixture test in `bench_walk/index.rs` extended to the new targets (counts equal).
- **Check:** `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo xtask check -p xtask`; `cargo deny check`.
- **Owner check:** `xtask.exe bench-walk D:\navo-walk --targets walk,walk-dua,walk-ignore --runs 5` on the internal NVMe and on the second drive type; send both tables.

### SP0f — gungraun benches for listing hot paths, bench workflow

- **Role:** api-builder · **Wave:** 3 · **After:** SP7e · Changes `Cargo.lock` (only one in wave 3).
- **Read:** `crates/navo-fs/Cargo.toml`, `navo-fs/src/fake/mod.rs:55-100`, SP7e's `view/`, gungraun 0.20 `library_benchmark` docs in the registry source.
- **Change:** D8. `navo-fs/Cargo.toml`: dev-dependency `gungraun = "=0.20.0"`, `[[bench]] name = "listing" harness = false`. `navo-fs/benches/listing.rs`: seeded generated rows (no disk): `sort_order` on 100,000 rows for each `SortBy`; `filter_order` with needle `sensor` on 100,000; `load_folder` through `FakeFs` with 50,000 entries. `Callgrind::default().soft_limits([(EventKind::Ir, 10.0)])`. New `.github/workflows/bench.yml`: ubuntu, `apt-get install valgrind`, `cargo install gungraun-runner --version 0.20.0 --locked`, run benches on the base commit with `--save-baseline=base`, then on head with `--baseline=base`; the job fails on a soft-limit breach (check the exact flags with `cargo bench -p navo-fs --bench listing -- --help`). Pin actions by SHA as in `ci.yml`.
- **Do not touch:** `ci.yml`.
- **Tests:** benches compile.
- **Check:** `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo bench -p navo-fs --bench listing --no-run`; `… cargo xtask check -p navo-fs`; `cargo deny check`. Valgrind is not installed locally; the workflow proves the run.

### SP7f — Browser sorts and filters through `navo-fs::view` (still synchronous)

- **Role:** api-builder · **Wave:** 3 · **After:** SP7c, SP7e
- **Read:** `crates/navo-ui/src/browser/sort/mod.rs`, `browser/filter/mod.rs`, SP7e's API.
- **Change:** `sorted_rows` calls `sort_order` + `materialize` and builds the `Listing` from the materialised rows; `shown` calls `filter_order` over the identity order of `full` + `materialize`. Remove `Keyed`, `keyed`, `compare`, `by`, `is_visible`, `is_reserved` from `navo-ui`. A never-cancelled `CancelToken` on this sync path. Expose `pub(super) fn sorted_full(rows: &Arc<[FsEntry]>, key: SortKey, folders_first: bool) -> Full` as a free function (no `&self`) so SP7g can call it on a worker.
- **Do not touch:** `cached/mod.rs`, `mod.rs`.
- **Tests:** all existing `browser/sort/tests.rs` and `browser/filter/*tests.rs` pass unchanged; one test that a 25,000-row sort equals the old order (copy the old comparator into the test as the oracle).
- **Check:** `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo xtask check -p navo-ui`.
- **Owner check:** `navo bench` folder listing: 500k sort per key and filter keystroke (before: 121–603 ms, 92–150 ms on Linux).

### SP7k — `Arc<Selection>` copy-on-write

- **Role:** api-builder · **Wave:** 3 · **After:** SP7c, SP7j
- **Read:** `crates/navo-core/src/state/tab.rs:15-30,90-100,195-210,320-350`, `crates/navo-ui/src/file_list/mod.rs:50-65`, `window_view/pane.rs:160-175`.
- **Change:** `Tab.selection: Arc<Selection>`; `selection()` still returns `&Selection`; new `selection_arc(&self) -> &Arc<Selection>`; `selection_mut` uses `Arc::make_mut`. `FileListProps.marked: Arc<Selection>` filled with `tab.selection_arc().clone()`; gallery `mouse_page.rs` and `pane_page.rs` wrap with `Arc::new`.
- **Do not touch:** `Selection` itself.
- **Tests:** cloning a tab state then mutating the selection leaves the clone unchanged; render props share the pointer until a change (`Arc::ptr_eq`).
- **Check:** `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo xtask check -p navo-core -p navo-ui -p navo-ui-gallery`.
- **Owner check:** with SP7j: the Down×50 trace p95.

### SP8f — L1: navo-ui `window_view/viewer/**`

- **Role:** api-builder · **Wave:** 3 · **After:** —
- **Change:** fix every L1 hit under `crates/navo-ui/src/window_view/viewer/` (50 on Linux). D18 for callers outside that folder.
- **Check:** module-scoped L1 count with scope `window_view/viewer/` prints `0`; `… cargo xtask check -p navo-ui`.

### SP10b — Hide on close; a hidden window shows on arrival

- **Role:** senior (Opus): window lifecycle and Win32. · **Wave:** 3 · **After:** SP10a
- **Read:** `crates/navo-ui/src/window_view/closing.rs:60-100`, `window_view/hand_off.rs:60-80,140-200`, `crates/navo-app/src/gui.rs` (where `OpenHooks` is built).
- **Change:** D15 and the show half of D16. `OpenHooks` gains `window_shown: Option<Arc<dyn Fn(RawWindowHandle, bool) + Send + Sync>>` (hide/show). In `close_saves_and_quits`, when `keeps_in_tray()` and this is the last window: save as today, call the hook with `false`, return without quitting. `take_arrival` calls it with `true` before `activate_window`. `OpenRequest.quit: bool`; an arrival with `quit` runs today's save-and-quit. New `navo-app/src/resident/window.rs` (`cfg(windows)`): `ShowWindow(hwnd, SW_HIDE | SW_SHOW)`; non-Windows: no hook (close quits as today).
- **Do not touch:** session format, autosave timing.
- **Tests:** headless: with the setting on, close calls the hide hook and does not quit; with it off, quits; an arrival calls show then activate; a `quit` arrival quits.
- **Check:** `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo xtask check -p navo-ui -p navo-app`; Windows CI green.
- **Owner check:** with "Keep in tray" on, X hides Navo; starting `navo.exe` again shows the same window at once.

### SP7g — Fresh listings sorted on the worker

- **Role:** api-builder · **Wave:** 4 · **After:** SP7f
- **Read:** `crates/navo-ui/src/browser/cached/mod.rs:15-85,155-215`, `browser/mod.rs:326-370`, SP7f's `sorted_full`.
- **Change:** `Fetch` gains `sort: (SortKey, bool)`; `Fresh::Rows` becomes `Fresh::Rows { rows: Arc<[FsEntry]>, full: Full }` where the worker calls `sorted_full` after reading (span `listing.load` around `Fetch::run`). `fresh_listing` puts `full` into the `sorted` memo for `(rows, key)` and uses it; if the tab's key changed meanwhile, it falls back to the memo/sync path. The patched-rows path (`patched`) also sorts on the worker.
- **Do not touch:** cache-hit display (`show_cached`), filters.
- **Tests:** a test-only counter of UI-thread sorts: loading a 30,000-row `FakeFs` folder does 0 UI sorts; a key change during the load still shows the new order; existing cached tests pass.
- **Check:** `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo xtask check -p navo-ui`.
- **Owner check:** open the 500k folder of `navo bench --keep` with `--trace`: no input waits longer than 16 ms while it loads.

### SP8g — L1: rest of `window_view`

- **Role:** api-builder · **Wave:** 4 · **After:** SP7j, SP10b
- **Change:** fix every L1 hit under `crates/navo-ui/src/window_view/` except `viewer/`, `pane.rs`, `status.rs`, `closing.rs`, `hand_off.rs`, `render.rs` (those go to SP8i).
- **Check:** module-scoped L1 count with scope `window_view/` minus the excluded paths prints `0` (`grep -v` them); `… cargo xtask check -p navo-ui`.

### SP9a — mimalloc behind a feature, A/B exe from CI

- **Role:** api-builder · **Wave:** 4 · **After:** — · Changes `Cargo.lock` (only one in wave 4).
- **Read:** `crates/navo-app/src/heap.rs`, `crates/navo-app/Cargo.toml`, `.github/workflows/ci.yml` `build-windows`.
- **Change:** D11. `mimalloc = { version = "=0.1.52", optional = true, default-features = false }`, feature `mimalloc = ["dep:mimalloc"]`. In `heap.rs` the inner allocator is `mimalloc::MiMalloc` under the feature and `System` otherwise; `CountingAlloc` stays the `#[global_allocator]`. `ci.yml` `build-windows`: after staging, `cargo build --release -p navo-app --features mimalloc` into `target/mimalloc` (`--target-dir`) and upload `navo.exe` as `navo-windows-x64-mimalloc-${{ github.sha }}`.
- **Tests:** existing heap counter tests pass with and without the feature.
- **Check:** `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo xtask check -p navo-app` and `… cargo test -p navo-app --features mimalloc heap`; `cargo deny check`.
- **Owner check:** four warm `--metrics` runs of each exe (first usable, private bytes) and `navo bench --files 1000000` folder listing with each; the orchestrator records them in `docs/perf.md` and applies D11.

### SP7h — Big cached folders sorted on the worker

- **Role:** api-builder · **Wave:** 5 · **After:** SP7g
- **Read:** `browser/cached/mod.rs:155-235` (`fetch`, `show_cached`), `browser/mod.rs` `Message`.
- **Change:** in `fetch`, a cache hit with more than 20,000 rows (D1) is not shown synchronously. `Fetch` carries it as `early: Option<Arc<CachedListing>>`; `Fetch::run` first sorts it (`sorted_full`) and sends `Message::CachedSorted { tab, request, rows, full }` (then continues the normal refresh). `accept` handles it like `show_cached` (same `ListingCached` event, focus restore) when the request is still current (D19), and seeds the memo.
- **Tests:** a 30,000-row cache hit shows through `CachedSorted` with 0 UI sorts; a stale `CachedSorted` (newer request) is dropped; ≤ 20,000 rows still show synchronously (existing tests).
- **Check:** `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo xtask check -p navo-ui`.
- **Owner check:** leave and re-enter the 500k folder with `--trace`: no input waits longer than 16 ms.

### SP8h — L1: navo-ui outside window_view, browser and file_list

- **Role:** api-builder · **Wave:** 5 · **After:** SP10a
- **Change:** fix every L1 hit under `crates/navo-ui/src/` except `window_view/`, `browser/`, `file_list/`, `name_match/`.
- **Check:** module-scoped L1 count over `crates/navo-ui/src/` with those four paths removed prints `0`; `… cargo xtask check -p navo-ui`.

### SP10c — Resident thread: tray icon and summon hotkey

- **Role:** senior (Opus): Win32 message loop. · **Wave:** 5 · **After:** SP10b · Changes `Cargo.lock` (only one in wave 5).
- **Read:** SP10a parser, SP10b `resident/window.rs`, `crates/navo-app/Cargo.toml`, `navo-app/src/instance/windows.rs` (thread and shutdown style).
- **Change:** D12, D13. `[target.'cfg(windows)'.dependencies] tray-icon = { version = "=0.26.1", default-features = false }`. New `navo-app/src/resident/{thread.rs, tray.rs, hotkey.rs}`: `Resident::start(config, sink) -> Resident` spawns `navo-resident`, creates the tray icon (app icon, tooltip "Navo", menu Open / Quit) when `keeps_in_tray`, registers the parsed summon key with `RegisterHotKey` (failure → `tracing::warn!` and a flag the Settings field shows as "Taken by another app"), loops on `GetMessageW`. Events map to `ResidentEvent::{Summon(Instant), Quit}` sent to `sink`. `Drop` posts `WM_QUIT`, unregisters, removes the icon, joins. Non-Windows: `Resident::start` returns an inert value.
- **Tests:** Windows-only test that start/drop leaves no thread (bounded join); cross-platform test that the event mapping from menu ids is right.
- **Check:** `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo xtask check -p navo-app`; Windows CI green; `cargo deny check`.
- **Owner check:** tray icon appears only with the setting on; Win+Alt+N registers (no warning in the log).

### SP7i — Sort changes and filter keystrokes above 20,000 rows run on a worker

- **Role:** senior (Opus): async state with marks (R1). · **Wave:** 6 · **After:** SP7h
- **Read:** `browser/filter/mod.rs`, `browser/sort/mod.rs`, `browser/mod.rs:30-70,326-370`, SP7f–SP7h diffs.
- **Change:** D1, D19. New `browser/view_jobs.rs`: per tab `ViewJob { revision: u64, cancel: CancelToken }`; a new request cancels the previous one. `set_filter`/`resort` on a full listing over 20,000 rows store the new filter text or sort key at once (filter bar and header update), submit a listing-pool job (`Priority::Visible`, span `listing.view_job`) that runs `sorted_full` and/or `filter_order` + `materialize`, and return true. `Message::Viewed` applies through the existing `ListingFiltered`/`ListingRefiltered` path only if D19 holds; then `land_cursor`. A filter result with no matches restores the previous text (today a no-match key is refused).
- **Do not touch:** the ≤ 20,000-row path.
- **Tests:** 25,000-row `FakeFs`: two quick keystrokes apply only the last result; a result after navigation is dropped; marks made while a job is pending stay on the same names after it applies; no-match keystroke restores the text; sort change while a filter job runs ends in the right order.
- **Check:** `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo xtask check -p navo-ui`.
- **Owner check:** type `sensor` in the 500k folder with `--trace`: p95 key-to-frame ≤ 16 ms; rows follow within the bench's filter time.

### SP10d — Summon and Quit wiring, summon timing

- **Role:** senior (Opus) · **Wave:** 6 · **After:** SP10c
- **Read:** SP10b, SP10c; `navo-app/src/gui.rs`, `navo-ui/src/window_view/hand_off.rs:140-200`.
- **Change:** D16. `gui.rs` starts `Resident` with the current settings and restarts it when `tray.keep-running` or `tray.summon-key` change (existing live-settings hook). `Summon(at)` → `OpenRequest { summoned_at: Some(at), ..Default }`; `Quit` → `OpenRequest { quit: true, .. }`. In `take_arrival`, when `summoned_at` is set, `window.on_next_frame` logs `tracing::info!(summon_ms, "summoned")` (span `resident.summon`). Focus lands on the active pane's file list.
- **Tests:** headless: a summon request shows, activates and focuses the list; quit request quits; settings change restarts the resident (fake `Resident` behind a trait).
- **Check:** `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo xtask check -p navo-ui -p navo-app`; Windows CI green.
- **Owner check:** hide with X, press Win+Alt+N ten times over a minute; the log's `summon_ms` p50 ≤ 50 ms; tray Quit exits and a new start does not report a locked profile.

### SP12c — Adopt the faster walker (conditional)

- **Role:** api-builder · **Wave:** 6 · **After:** SP12a and the owner's numbers meeting D17 · Changes `Cargo.lock`.
- **Change:** only for the winner: `navo-index/src/walker.rs` reads local folders with it (Local provider only; other providers keep the current path), keeping `Limits`, cancel, progress every 10,000 entries and `Outcome`. If neither wins, the orchestrator closes SP12 with the numbers in `docs/perf.md` and this atom does not run.
- **Tests:** existing `walker/tests.rs` pass; one test that both paths give the same `NameTable` for a temp tree.
- **Check:** `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo xtask check -p navo-index`; `cargo deny check`.
- **Owner check:** `navo bench` index time before/after.

### SP7l — Row display text cached per rows (conditional)

- **Role:** ui-builder · **Wave:** 7 · **After:** SP7i, SP7k · Runs only if the owner's trace after SP7k shows pane render p95 above 4 ms.
- **Read:** `crates/navo-ui/src/file_list/row.rs:108-292`, `file_list/rows.rs`.
- **Change:** a `RowTexts` cache in `ListContext` keyed by `(Arc::as_ptr(rows), row)` holding `name_text`, extension, `size_text`, `date_text` as `SharedString`, filled lazily for rendered rows, dropped when the rows pointer or the visible parts change.
- **Tests:** second render of the same rows formats nothing (counter); a new rows Arc reformats.
- **Check:** `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo xtask check -p navo-ui`.

### SP8i — Deny L1 in workspace lints; fix leftovers

- **Role:** api-builder · **Wave:** 8 · **After:** every other SP7, SP8 and SP10 atom
- **Change:** add the eleven L1 lints as `"deny"` to `[workspace.lints.clippy]` in `Cargo.toml`. Fix remaining hits: `navo-ui/src/{browser,file_list,name_match}/`, `window_view/{pane,status,closing,hand_off,render}.rs`, `navo-ui-gallery` (9), `xtask` (8), `navo-index/src/ntfs/mod.rs` (wildcard import), `navo-pdf/src/client/helper.rs` (match arms), and every D18 `#[expect]` left by earlier atoms. Windows-only hits come from the `clippy-windows` log on the branch push; fix them and push again.
- **Check:** `flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 cargo clippy --workspace --all-targets -- -D warnings`; `clippy-windows` green.

## Not in this plan

Research libraries that serve later features, each with its milestone (D88: each comes in only with its feature, pinned, after a short test or measurement):

| Library | Milestone |
| --- | --- |
| `grep-searcher`, `grep-regex`, `encoding_rs_io` | Find content search, end of the beta (beta G17) |
| `nucleo-matcher` (have) / `frizbee` 0.13 | Find ranking, after a benchmark (beta G18); `nucleo` never as a hard dependency without a fallback |
| `imara-diff` 0.2 | Compare and sync, v1.0 |
| `blake3` 1.8 | Checksums, duplicates, v1.1 |
| `CopyFile2` | Copy engine (done in P30); `reflink-copy` is not in the D88 map |
| `zip`, `sevenz-rust2`, `tar`; trial of `zstd`, `liblzma` for writing | Archives as folders, v1.0; no RAR (D88) |
| `calamine`, `rust_xlsxwriter` | Spreadsheets in F3, v2.0 |
| `IShellItemImageFactory` in a separate process, `fast_image_resize` | Thumbnails, v1.0 (beta I10, I11a, I11b) |
| `bzip2`, `backhand`, `turbojpeg`, `jxl-oxide` | Not in the D88 map; each needs an owner decision first |
| `autosar-data`, `mf4-rs` | Automotive formats |
| `gix` | Git, v1.2 |
| `russh` + `russh-sftp`, `opendal` | Remote locations, v2.0 |
| `mlua`, `wasmtime` | Plugins and scripting, v2.0 |
| `alacritty_terminal` | Embedded terminal, v2.0 |
| Velopack | Installer and updates, v1.0 |
| `crash-handler` + `minidumper` | Crash reports (P29 C4, C5) |
| `ntfs-reader`, `usn-journal-rs` | Index cross-checks, later |
| `notify` + `notify-debouncer-full` | Not needed: own watcher, no measured problem |
| `interprocess` | Not needed: SP1a's own overlapped pipe is done |
| `global-hotkey` | Not used: D12 |
| `divan`, `puffin`, `cargo-mutants` | Not used for speed: gungraun (D8) and Tracy (D5) cover it |

## Owner questions

- Q1 — The gungraun guard runs on Linux (valgrind). Should a red bench workflow block the pre-release? Default: no, it only shows red (Linux CI never blocks).
- Q2 — The tray settings sit in a new "Tray" section (no Keys section exists). Default: as planned (D14).

## Done when

- Every row above is done or closed with its number (SP12c, SP7l are conditional).
- `cargo clippy --workspace --all-targets -- -D warnings` passes with L1 denied, on Linux and in `clippy-windows`.
- CI fails on exe > 160 MiB, warm first frame median > 300 ms, private bytes > 160 MB (Windows), and the bench workflow fails on > 10 % `Ir` regression.
- The owner's numbers for SP1d, SP1e, SP7g–SP7i, SP9a, SP10d and SP12a are in `docs/perf.md`.

## Log

- 2026-10-29 — Plan written (architect, Opus 5.5). L1 hits counted on Linux at `b6394a34`: navo-ui 147 (window_view/viewer 50, rest of window_view 59, outside 38), navo-app 39, navo-cache 19, navo-ui-gallery 9, xtask 8, navo-store 7, navo-index 1, navo-pdf 1. Versions checked with `cargo search`/`cargo info`: tracy-client 0.19.0, tracing-tracy 0.12.0, gungraun 0.20.0, mimalloc 0.1.52, dua-core 5.0.1, ignore 0.4.34, tray-icon 0.26.1 (windows-sys 0.61), global-hotkey 0.9.0 (windows-sys 0.59, rejected).
- 2026-10-29 — Amended from the 2026-10-28 research (D88). DeepSeek V4.1 Flash or Sol 6.1 high may run the pure-backend atoms (never UI); Windows CI is the real gate. "Not in this plan" follows the D88 library map: ranking stays on `nucleo-matcher` unless `frizbee` wins, archives without RAR, thumbnails in a separate process; `bzip2`, `backhand`, `turbojpeg`, `jxl-oxide` and `reflink-copy` are not adopted. No atom changed: SP0c, SP1a, SP9a, SP10c and SP12a/c already match D88.
- 2026-10-30 — Table statuses brought up to date from main: waves 1–5 done except SP8g; left: SP8g, SP7i, SP7l, SP8i, and SP12c (needs owner numbers).
- 2026-10-30 — Quality lead stopped at the weekly usage limit. Landed on `part-quality`: the VSync park timeout fix (55da4686, VM CI green except the VM-only fat-LTO memory crash, fixed in the VM CI script) and the mimalloc A/B plan and script (`plans/2026-10-30-mimalloc-ab.md`, `scripts/mimalloc-ab.ps1`, untested, waits for a VM slot). In progress, unstaged in their worktrees: SP8g (`navo-sp8g`, L1 fixes in `window_view`, hit list measured: 54 lines with duplicates) and SP7i (`navo-sp7i`, async sort/filter, tests first). Next: finish SP8g and SP7i, review their diffs, then SP8i; SP12c waits for the owner's walker numbers; SP7l only if the trace shows pane render p95 above 4 ms.
- 2026-10-30 — SP7i state (patch `sp7i-wip.patch` in `navo-sp7i`, unstaged): `browser/view_jobs.rs` and 13 tests written, 11 passed, the 2 failures were wrong job counts in the tests (fixed, not rerun); `Browser::with_pools` was split by merging `show_hidden` and `show_system` into one `shows` field (not compiled yet). Next: rerun `xtask check -p navo-ui`, then fresh review. Open points to settle before landing: a measured folder size replaces the rows and resubmits a pending job each time (a size stream could delay a big filter); `reveal_name` clears the filter but a pending filter job can still apply and hide that file again.
- 2026-10-30 — SP8g state (patch `sp8g-wip.patch` in `navo-sp8g`, unstaged): 26 files edited (36 lines), not yet compiled or measured against the L1 count. Next: run the module-scoped L1 count and `xtask check -p navo-ui`, read the diff, land.
