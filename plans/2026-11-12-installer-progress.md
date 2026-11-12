# Plan — Real progress bar on the Setup Installing page

- **Status:** drafting
- **Owner:** installer wizard (extends `2026-11-09-installer-wizard.md`, `2026-11-09-installer-wizard-ui.md`)
- **Date:** 2026-11-12
- **Base commit (`origin/main`):** `9b37ca09`
- **Decisions:** `IP1–IP7` (last recorded project decision on `main`: `D115` in [`docs/decisions.md:117`](../docs/decisions.md); `IW25–IW31` and `D116` are reserved by the open plan [`2026-11-12-installer-anywhere.md`](2026-11-12-installer-anywhere.md))
- **Appetite:** 1 small batch (~1–2 days across `navo-setup`, `navo-ui`, `navo-l10n`, `navo-ui-gallery`, and a Windows VM run). If any piece grows past `--verbose` log tailing (for instance, forking Velopack), stop and cut back to the pulse fallback.

**Not in scope:**
- Forking or patching Velopack `Setup.exe` / `Update.exe` (IW1) or depending on the `velopack` crate from `navo-setup` (IW14).
- Inventing time-based ETAs, timer-interpolated bars, or a bar that stalls at `99%`.
- Adding external crate dependencies (`notify`, `simplelog`, `walkdir`).
- Changing silent-mode CLI output or exit codes (IW17), `Host` trait signatures, or Navo's in-app self-updater (`navo-app/src/updater/`, which uses `download_updates` for downloads and runs `Update.exe` detached on restart).

---

## Goal

Replace the static-pulse-only bar on the Setup wizard's `Installing` page (`crates/navo-ui/src/setup/pages_b.rs`) with an honest, monotonic determinate progress bar driven by real work completed by Velopack's `Setup.exe` / `Update.exe apply`, while keeping the existing gentle pulse whenever no verified percentage is available or when unbounded post-extract work runs.

---

## Percentage-source findings and ranking

### 1. Does Velopack `1.2.161` report progress externally?

Pin checked: [`Cargo.lock:11100-11104`](../Cargo.lock) (`velopack = "1.2.161"`), [`.github/workflows/release.yml:33`](../.github/workflows/release.yml) (`VPK_VERSION: 1.2.161`, Velopack commit `92d6a1c91716729d449034df5c50307dcce39493`).

| Channel checked | Reports install/apply %? | Exact evidence (`velopack = "1.2.161"` & Navo repo) |
| --- | --- | --- |
| **`velopack` crate API** | **No** (download only) | `UpdateManager::download_updates` (`velopack-1.2.161/src/manager.rs:405`) takes `Option<Sender<i16>>` for HTTP/delta downloads. `UpdateManager::unsafe_apply_updates` (`manager.rs:615-681`) spawns `Update.exe apply` via `process::run_process` (`process_win.rs:367-401`, `STARTUPINFOW` null stdio handles) with no progress callback. `navo-setup` also forbids the `velopack` crate (IW14, `crates/navo-setup/Cargo.toml`). |
| **Named pipe / IPC** | **No** | Neither `setup.rs` nor `update.rs` opens a progress pipe or socket. Under `--silent` (`runner_steps.rs:34, 42`, `upgrade.rs:50`), `install.rs:129-132` creates a dropped `(tx, _)` `mpsc::channel` and `apply_windows_impl.rs:134` calls `show_apply_progress` which returns `NoopProgressReporter` (`src/l18n/src/progress.rs:73-76`). |
| **Exit codes** | **No** | `src/bins/src/setup.rs:60` and `src/bins/src/update.rs:135` return only `0` (`Ok`) or `1` (`Err`). |
| **Stdout lines** | **No (at default level); inferior** | Both `setup.rs:1` and `update.rs:1` build with `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`. Navo spawns children with `Stdio::null()` (`crates/navo-setup/src/host_windows.rs:217-219`). Default `TermLogger` runs at `LevelFilter::Info` (`logging.rs:94-126`), emitting no per-entry progress unless `--verbose` is added. |
| **Log file (`--log <path>`)** | **Yes when `--verbose` is passed; phase-only without it** | Navo already passes `--log <velopack_log>` (`runner_steps.rs:34, 44`, `upgrade.rs:55-56`). `setup.rs:100, 127` and `update.rs:43, 184` pass `verbose` to `init_logging`, lowering `WriteLogger`'s file level from `Info` to `Trace`/`Debug` (`logging.rs:94-126`). At `Info`, Velopack logs only coarse phase lines; with `--verbose`, `extract_lib_contents_to_path` (`bundle.rs:307, 328, 345, 353, 375`) logs `"Extracting {num_files} app files..."` and one line per entry `i` (`"    {i} Extracting '{key}' to '{path}'"`). Notice `apply_windows_impl.rs:149` calls `reporter.set_indeterminate()` right after extraction ends because hooks, directory swaps, and cleanup have unknown runtime. |

### 2. Ranked proxy candidates

| Rank | Candidate | Honesty | Windows-only risk | Code cost | Decision |
| --- | --- | --- | --- | --- | --- |
| **1 (Pick)** | **Tail `--verbose` `--log` file + weight entries by embedded `.nupkg` sizes (`Payload`)** | **High.** Advances only when Velopack completes the `.nupkg` copy (`install.rs:215`) and each zip entry (`bundle.rs:353`, credited once the *next* entry or post-extract milestone confirms write completion). Scales `1..=92%` by uncompressed `lib/` bytes + `.nupkg` bytes; switches to pulse (`None`) before milestones arrive or if `Finalizing` stalls > 2 s. | **Low–medium.** Zero handles held inside `<root>` or `VelopackTemp` (`rename` never blocked). Transient read open per poll with `FILE_SHARE_READ \| FILE_SHARE_WRITE \| FILE_SHARE_DELETE` (`host_files.rs:24` pattern) handles `FileRotate` `.old` rotation (`logging.rs:120`). Couples text matching to pinned `velopack 1.2.161` and needs VM confirmation of `WriteLogger` live flush timing. | **Low.** Pure parser/estimator + transient tailer (`progress_log.rs`, `progress_tail.rs`); adds `--verbose` to `spawn_args`/`apply_args`; no new crates, no `Host` trait change. | **Chosen (`IP1`)** |
| **2 (Reject)** | **`Info`-only `--log` phase weights (no `--verbose`)** | **Low.** Only ~5 `Info` milestones exist (`install.rs:205, 215`, `bundle.rs:307`, `install.rs:240`, `apply_windows_impl.rs:118, 161, 174`). The bar sits frozen at ~15–25% for almost the entire extraction I/O phase, then jumps to the end. | **Very low.** Same `--log` file without `--verbose` flag or `debug!` entries (~5–6 lines total). | **Very low.** ~60 lines of milestone matching; keeps IW3 arrays untouched. | **Rejected** |
| **3 (Reject)** | **Target-dir / `VelopackTemp` byte-or-file polling vs `Payload::space()`** | **Medium.** Fresh `install.rs:220` writes `<root>\current`, whereas upgrade `apply_windows_impl.rs:130, 140` extracts into `<root>\packages\VelopackTemp\tmp_<16>`. At `:162`, `apply_windows_impl.rs` renames old `current\` into a *second* `tmp_<16>` inside `VelopackTemp` before removing both (`:228-229`), briefly doubling folder size then dropping to `0`. | **High.** Periodic `FindFirstFileW` walks inside `current\` and `VelopackTemp` race with `Update.exe`'s own `fs::rename` and `remove_dir_all` (`apply_windows_impl.rs:162, 175, 228`), risking sharing-violation stalls (`retry_io_ex` sleeps 1 s x 30); orphaned `tmp_*` dirs skew totals; NTFS lazily flushes directory-entry sizes until `CloseHandle` on `navo.exe`. | **High.** Separate fresh vs upgrade walkers, `VelopackTemp` baseline snapshot, reparse checks, new `Host` methods + `runner_fake.rs` edits (395 lines). | **Rejected** |

*(Note: Piping stdout via `TermLogger` at `logging.rs:107` was also ruled out because it still requires `--verbose` for per-file lines, fails unless `Host` / `host_windows.rs:217-219` changes `Stdio::null()` to pipes, and adds buffering/handle risk on `windows_subsystem = "windows"` EXEs with no advantage over reading the already-configured `--log` file.)*

---

## Decisions

- **IP1 — Percentage source: `--verbose` `--log` tailer weighted by `Payload` ZIP sizes.** Both `spawn_args` (`crates/navo-setup/src/runner_steps.rs:39-47`) and `apply_args` (`crates/navo-setup/src/upgrade.rs:44-56`) pass `"--verbose"` alongside the existing `"--silent"` and `"--log <velopack_log>"` flags. Notice `update.rs:43` only accepts `--verbose` (not `-v`), so both paths use `--verbose`. `runner_run` builds `PackageWeights` from `payload.package()` (`zip::ZipArchive`) once before waiting.

- **IP2 — Byte-weighted completion accounting (`1..=92%`), never counting unstarted work.**
  - *Fresh install:* `Setup.exe` logs `"Copying nupkg to packages directory..."` (`install.rs:215`), copies `compressed_bytes`, then logs `"Extracting {N} app files to ..."` (`bundle.rs:307`). Once `"Extracting {N} app files"` appears, `copy` is complete (`completed_bytes = compressed_bytes`). Total denominator is `compressed_bytes + lib_extracted_bytes`.
  - *Upgrade:* Navo already staged `.nupkg` and replaced `Update.exe` before spawning (`runner_run.rs:226-240`). Determinate denominator is `lib_extracted_bytes` starting at `"Extracting {N} app files"`.
  - *Entry crediting (`bundle.rs:328, 345, 353, 375`):* Velopack logs `"    {i} Extracting '{key}' to ..."` **before** copying entry `i`. Wait until the *next* entry line or post-extract milestone (`Creating shortcuts...`, `Starting process install hook`, `Skipping --veloapp-obsolete hook.`, `Backing up current dir`, `Replacing current dir`) before adding `key`'s uncompressed byte size to `completed_bytes`. If a `Payload` weight table is absent or a key is unmatched, fall back to `completed_entries / N`. Map completed work across `1..=92%` (`(completed * 92 / total).clamp(1, 92)`) monotonically while in `CopyingPackage` / `Extracting` / early `Finalizing` (`Backing up` -> `94%`, `Replacing` -> `96%`). Never display `100%` before the child exits and passes verification.

- **IP3 — Non-locking file tailing & `.old` rotation.** On Windows (`#[cfg(windows)]`), `LogTailer` opens `velopack_log` transiently on each poll with `OpenOptions::share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)` (`host_files.rs:24` pattern), reads from `offset`, buffers any incomplete trailing line until `\n`, and drops the handle before sleeping. If `metadata.len() < offset`, `LogTailer` checks `<velopack_log>.old` (`logging.rs:120` 1 MB rotation) from `offset` for trailing lines, then resets `offset = 0` on the rotated main log. Never holds any directory or persistent file handle.

- **IP4 — Poll interval without breaking `runner_tests`.** `steps::wait_with_log` polls `child.wait_timeout(Duration::from_millis(250))` (~4 Hz live bar responsiveness on quick SSD installs) while keeping `elapsed` in whole `Duration::from_secs(...)`. It invokes `progress(...)` only when whole seconds, `percent`, or `stage` changes, keeping `still_working` triggered at `180 s` (`STILL_WORKING_SECONDS`) and updating `runner_tests.rs:281` so `progress.len() == 180` remains invariant.

- **IP5 — Honest fallback to pulse and `Finalizing` stall guard (never stall at 99%).**
  See the explicit fallback matrix below. In particular, once extraction finishes and `Finalizing` begins (hooks, `retry_io_ex` folder swap, temp cleanup, Windows Defender scan, post-exit marker check), `percent` drops back to `None` (calm pulse) if no log milestone advances for `FINALIZE_STALL_GRACE = 2 s`, avoiding a brief sub-second flash on fast NVMe installs while guaranteeing a hung cleanup or antivirus scan never stalls at `92%` or `99%`.

- **IP6 — Visual behaviour, motion, contrast, and accessibility (`design.md`, `web-ui.md`, IW21).**
  - *Determinate (`percent: Some(pct)`):* Reuse `JobPill`'s static fill pattern (`crates/navo-ui/src/job_card/card.rs:146-157`): `c.t.surfaces.progress_track` background with a `c.t.surfaces.progress_fill` child sized via `.w(relative(f32::from(pct.min(100)) / 100.0))`. No shimmer, no striped candy gradient, and no extra width frame-animation (`IW21`). Updates arrive at most ~4 Hz from `wait_with_log`, staying visually calm and zero-flash (`< 3 Hz`).
  - *Indeterminate (`percent: None`):* Keep the existing slow opacity pulse (`BAR_PULSE = Duration::from_millis(2400)`, ~0.42 Hz) frozen solid when `c.reduce_motion` is true (`pages_b.rs:84-92`).
  - *Screen readers (`design.md` — state visible without moving focus):* Keep `.role(Role::ProgressIndicator)` on `installing-bar` with `.aria_label(...)` set to `"Installing, {percent}%"` when determinate or `"Installing"` when pulsing, and keep `Role::Status` on the elapsed/percentage caption (`pages_b.rs:208`). Never use `Role::Alert` for progress ticks and never move keyboard focus.
  - *Visible caption & l10n (`cargo xtask l10n`):* Render `"{pct}%"` (element id `installing-percent`) next to or above `installing-elapsed` (`Elapsed {time}`) when `percent: Some(pct)`; hide `installing-percent` when `None`. All text comes from `crates/navo-l10n/locales/en-US/setup.ftl` (`{ $percent }` Fluent syntax, substituted via `.replace("{percent}", ...)` as in `pages_b.rs:204`) via `Strings` (`xtask/l10n-allow.txt` unchanged).

- **IP7 — Keep files under `xtask size` (`400` code lines limit, `300` aim).** `crates/navo-ui/src/setup/pages_b.rs` already sits at `391` raw / `354` code lines. Extract `Installing` into sibling `crates/navo-ui/src/setup/installing.rs` first (`A1`) before adding determinate bar rendering (`A10`), keeping every modified file well under `300` code lines.

---

## Fallback when no percentage is available

`Progress { elapsed, still_working, percent: Option<u8>, stage: Option<InstallStage> }` treats `percent: None` as the single source of truth for indeterminate UI:

1. **Before child output / log creation:** On entering `Page::Installing` and until Velopack logs `"Extracting N app files..."` (or completes `.nupkg` copy on fresh install), `percent` is `None` -> bar pulses smoothly (`reduce_motion`: static 40% fill), no `0%` freeze.
2. **Missing, locked, or unparseable log:** If `velopack_log` does not exist yet, cannot be opened, or contains unrecognized lines, `LogTailer` ignores errors cleanly (`io::ErrorKind` non-fatal) and keeps `percent = None`.
3. **Zero or empty byte/entry totals:** If `declared_files == Some(0)` or `total_bytes == 0`, emit `None` rather than dividing by zero or jumping straight to `100%`.
4. **Post-extract / Defender / hook / `retry_io_ex` stall:** Once in `Finalizing` without log progress for `2 s`, `Estimator` clears `percent` back to `None` -> bar switches back to pulse (never stalls at `92%` or `99%`).
5. **180 s timeout (`still_working`):** Existing `installing-still` notice replaces tips after `180 s` (`pages_b.rs:219`) regardless of `percent`.
6. **Silent CLI (`cli_run.rs`):** Runs with a no-op progress sink (`&mut |_| {}`); silent output and exit codes stay identical to IW17.

---

## Risks and rollback

| Risk | Impact | Mitigation / Check |
| --- | --- | --- |
| **`WriteLogger` buffers `--log` until process exit on Windows** | Bar stays in pulse (`None`) during the whole install | Explicit **A11 VM check**: inspect live `Update.exe` / `Setup.exe` UIA snapshots during `S1b`/`S2`/`S12`. If `WriteLogger` OS-buffers beyond flush intervals, fallback cleanly keeps the pulse (no broken UI or stall) and we can record an upstream-free fallback decision with the owner. |
| **Velopack log line wording changes on future upgrade** | Entries fail to parse; bar stays pulsing | `VPK_VERSION` is pinned (`1.2.161` in `Cargo.lock` & `release.yml:33`); unrecognized lines leave `percent = None` (safe degradation). |
| **Log rotation past 1 MB (`FileRotate`)** | Offset jump or skipped lines | With `--verbose`, Velopack emits one `debug!` line per zip entry plus `Info` headers (tens of KB for Navo, far below 1 MB). `A4` tests `.old` truncation fallback cleanly. |
| **`cargo xtask size` or `l10n` failure** | CI gate rejects PR | `A1` splits `pages_b.rs` -> `installing.rs`; `A9` adds all UI copy to `setup.ftl` and `Strings`. |
| **Rollback** | Any atom causes regression | Each atom (`A1–A10`) is isolated (≤ 3 files, ≤ 150 lines) and leaves `main` compiling; reverting the tailing commits (`A5–A7`) immediately restores pure elapsed progress + pulse. |

---

## Components

| Crate / Path | Change summary |
| --- | --- |
| `crates/navo-setup/src/progress_log.rs` (new) | Pure ZIP weight table (`PackageWeights::from_nupkg`), Velopack `1.2.161` line parser, byte-weighted `Estimator` + inline unit tests |
| `crates/navo-setup/src/progress_tail.rs` (new) | Windows-safe transient `LogTailer` (`FILE_SHARE_*` + `.old` rotation) + `2 s` `Finalizing` stall guard + inline unit tests |
| `crates/navo-setup/src/runner_types.rs`, `runner_steps.rs`, `runner_run.rs`, `upgrade.rs` | Pass `--verbose`; carry `percent: Option<u8>` & `stage: Option<InstallStage>`; tail `velopack_log` during `wait_with_log` |
| `crates/navo-setup/src/window_view.rs` | Map `Progress` (`percent`, `stage`) to `SetupProgress` |
| `crates/navo-ui/src/setup/mod.rs`, `pages_b.rs`, `installing.rs` (new), `strings.rs` | Extract `installing.rs`; extend `SetupProgress` with `SetupStage` & `percent`; render static `JobPill`-style fill vs fallback pulse |
| `crates/navo-l10n/locales/en-US/setup.ftl` | Add determinate percentage, accessible label, and stage strings |
| `crates/navo-ui-gallery/src/setup_page.rs` | Update gallery `SetupInstalling` (`42%` Extracting) and keep `SetupInstallingSlow` (`None` pulse, `185 s`) for theme checks |
| `setup-e2e/cases-ui.ps1` | VM assertions for UIA progress % / pulse fallback, log size `< 1 MB`, light & dark `S12-*-installing.png` inspection |

---

## Atoms

| Atom | Summary & Platform | Files (`<= 3`, line counts) | Est. diff | Check |
| --- | --- | --- | --- | --- |
| **A1** | Extract `installing.rs` from `pages_b.rs` + `from_elapsed` ctor (`[all OS]`) | `installing.rs` (new), `pages_b.rs` (391→~285), `mod.rs` (234→~240) | ~120 moved / +15 | `cargo nextest run -p navo-ui setup && cargo xtask size` |
| **A2** | Switch external `SetupProgress` literals to `from_elapsed` (`[all OS]`) | `window_view.rs` (170), `pages_b_tests.rs` (237), `setup_page.rs` (179) | ~8 | `cargo nextest run -p navo-ui -p navo-setup && cargo check -p navo-ui-gallery` |
| **A3** | ZIP byte weights + Velopack log parser & `Estimator` (`[all OS]`, Red→Green) | `crates/navo-setup/src/main.rs` (220→221), `progress_log.rs` (new) | ~135 | Red then Green: `cargo nextest run -p navo-setup progress_log` |
| **A4** | Shared-open `LogTailer`, `.old` rotation & 2 s stall guard (`[all OS]`, Red→Green) | `crates/navo-setup/src/main.rs` (221→222), `progress_tail.rs` (new) | ~120 | Red then Green: `cargo nextest run -p navo-setup progress_tail` |
| **A5** | Pass `--verbose` to `Update.exe apply` (`[all OS]`, Red→Green) | `upgrade.rs` (153), `upgrade_tests.rs` (379), `runner_tests.rs` (376) | ~6 | Red then Green: `cargo nextest run -p navo-setup apply_args` |
| **A6a** | Pass `--verbose` to `Setup.exe` (`[all OS]`, Red→Green) | `runner_steps.rs` (237), `runner_tests.rs` (376) | ~8 | Red then Green: `cargo nextest run -p navo-setup spawned_arrays` |
| **A6b** | Add `percent`/`stage` to `Progress` & `steps::wait_with_log` (`[all OS]`, Red→Green) | `runner_types.rs` (132→~155), `runner_steps.rs` (237→~285), `window_tests.rs` (445→~448) | ~75 | Red then Green: `cargo nextest run -p navo-setup runner_steps window_tests` |
| **A7** | Wire `PackageWeights` + `velopack_log` in `runner_run` (`[all OS]`, Red→Green) | `runner_run.rs` (380→~390), `runner_tests.rs` (376→~405) | ~40 | Red then Green: `cargo nextest run -p navo-setup runner` |
| **A8** | Add `SetupStage`, `percent`/`stage` to `SetupProgress` & `window_view` (`[all OS]`, Red→Green) | `crates/navo-ui/src/setup/mod.rs` (240→~265), `window_view.rs` (170→~182), `window_tests.rs` (448→~465) | ~45 | Red then Green: `cargo nextest run -p navo-setup -p navo-ui` |
| **A9** | Add progress/stage strings in Fluent & `Strings` (`[all OS]`) | `setup.ftl` (71→~80), `strings.rs` (192→~212), `setup/tests.rs` (97→~105) | ~40 | `cargo nextest run -p navo-l10n -p navo-ui && cargo xtask l10n` |
| **A10** | Render determinate bar vs fallback pulse + Red→Green UI & Gallery tests (`[all OS]`) | `installing.rs` (~110→~155), `pages_b_tests.rs` (237→~278), `setup_page.rs` (179→~192) | ~110 | Red then Green: `cargo nextest run -p navo-ui setup && cargo check -p navo-ui-gallery` |
| **A11** | Windows VM live `Setup.exe` & `Update.exe` log flush + UIA + theme shot check (`[Windows]`) | `setup-e2e/cases-ui.ps1` (177→~205) | ~30 | Real Windows VM `S1a`, `S1b`, `S2`, `S12` + inspect light & dark screenshots |

---

### A1 — Split `Installing` page out of `pages_b.rs` (`[all OS]`)

- **Closed inputs:** `crates/navo-ui/src/setup/pages_b.rs:19-23, 75-103, 155-235` (391 raw / 354 code lines), `crates/navo-ui/src/setup/mod.rs:75-80, 220-234` (234 raw / 194 code lines).
- **Change:**
  - Create sibling `crates/navo-ui/src/setup/installing.rs` (~110 lines) holding `BAR_HEIGHT`, `TIP_CYCLE`, `TIP_AREA_HEIGHT`, `BAR_PULSE`, `bar`, `tip_card`, `tips`, and `pub(super) fn installing(c: &Ctx) -> AnyElement`.
  - Remove those items from `pages_b.rs` (dropping it to ~285 raw / ~250 code lines).
  - Declare `mod installing;` in `crates/navo-ui/src/setup/mod.rs`, route `SetupPage::Installing => installing::installing(&ctx)` (`:223`), and add `SetupProgress::from_elapsed(elapsed: Duration, still_working: bool) -> Self`.
- **Check:** `cargo nextest run -p navo-ui setup && cargo xtask size`. (Zero behaviour change.)

### A2 — Switch external `SetupProgress` constructors to `from_elapsed` (`[all OS]`)

- **Closed inputs:** `crates/navo-setup/src/window_view.rs:47-50` (170 lines), `crates/navo-ui/src/setup/pages_b_tests.rs:52-58` (237 lines), `crates/navo-ui-gallery/src/setup_page.rs:48-59` (179 lines).
- **Change:** Replace the three `SetupProgress { elapsed, still_working }` literals with `SetupProgress::from_elapsed(...)` so later adding `percent` / `stage` to `SetupProgress` keeps every atom `<= 3` files.
- **Check:** `cargo nextest run -p navo-ui -p navo-setup && cargo check -p navo-ui-gallery`.

### A3 — Pure ZIP weight extractor, log parser, and `Estimator` (`[all OS]`)

- **Closed inputs:** `crates/navo-setup/src/main.rs:12-38` (220 lines), `crates/navo-setup/src/payload.rs:119-137` (250 lines, `zip::ZipArchive` pattern), `velopack-1.2.161/src/bundle.rs:307-354`, `install.rs:205,215,240,244`, `apply_windows_impl.rs:118,161,174`.
- **Change:**
  - Declare `pub mod progress_log;` in `crates/navo-setup/src/main.rs`.
  - Create `crates/navo-setup/src/progress_log.rs` (~135 lines including inline tests):
    - `PackageWeights::from_nupkg(bytes: &[u8])`: reads `zip::ZipArchive`, sums `compressed_bytes = bytes.len() as u64`, collects uncompressed `size()` for `lib/*/*` entries (excluding `Squirrel.exe` and `*_ExecutionStub.exe`, matching `bundle.rs:308-347`) keyed by exact zip entry name.
    - `pub enum InstallStage { Preparing, CopyingPackage, Extracting, Finalizing }`.
    - `Estimator::new(upgrade: bool, weights: Option<PackageWeights>)` + `feed_line(&mut self, line: &str, now: Duration)`: implements `IP2` (Fresh copies `.nupkg` first, completes copy when `"Extracting N app files"` logs; entry `i` credits its byte size only when entry `i+1`, `"Skipped"`, `"Creating symlink"`, or a post-extract line arrives; monotonic `1..=92%` inside extract, `94%`/`96%` on `Backing up` / `Replacing`, `None` when total is `0` or no extract/copy milestone has been seen).
- **Check:**
  - *Red:* Add inline `#[cfg(test)]` suite in `progress_log.rs` with stubbed `feed_line` returning `None`; run `cargo nextest run -p navo-setup progress_log` and record failure.
  - *Green:* Implement parser and byte-weighted math; `cargo nextest run -p navo-setup progress_log` passes (fresh, upgrade, key mismatch fallback to entry count, zero total → `None`).

### A4 — Transient Windows-safe `LogTailer`, rotation & `Finalizing` stall guard (`[all OS]`)

- **Closed inputs:** `crates/navo-setup/src/main.rs:12-38` (221 lines), `crates/navo-setup/src/host_files.rs:20-37` (`FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE`), `crates/navo-setup/src/progress_log.rs` (from A3).
- **Change:**
  - Declare `mod progress_tail;` in `crates/navo-setup/src/main.rs`.
  - Create `crates/navo-setup/src/progress_tail.rs` (~120 lines including inline tests):
    - `LogTailer`: opens `velopack_log` transiently (with `#[cfg(windows)] share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)`), seeks to `offset`, reads complete UTF-8 lines, drops `File` immediately. If `len < offset`, drains `<name>.old` from `offset` (`logging.rs:120`) before resetting `offset = 0`. Missing/locked/error returns cleanly leaving estimator unchanged.
    - `snapshot(&mut self, now: Duration) -> (Option<u8>, Option<InstallStage>)`: delegates to `Estimator`, returning `percent = None` once `stage == Some(InstallStage::Finalizing)` has spent `>= 2 s` without log advancement (`IP5`).
- **Check:**
  - *Red:* Inline tests in `progress_tail.rs` (partial lines across chunks, `.old` rotation, missing file → `None`, `Finalizing` idle `2 s` → reverts to `None` and never holds at `99%`); run `cargo nextest run -p navo-setup progress_tail` and observe failure.
  - *Green:* Implement tailer and stall guard; re-run `cargo nextest run -p navo-setup progress_tail` to pass.

### A5 — Pass `--verbose` to `Update.exe apply` (`[all OS]`)

- **Closed inputs:** `crates/navo-setup/src/upgrade.rs:44-56` (153 lines), `crates/navo-setup/src/upgrade_tests.rs:259-280` (379 lines), `crates/navo-setup/src/runner_tests.rs:193-211` (376 lines).
- **Change:**
  - Append `"--verbose".into()` to `apply_args` in `upgrade.rs`.
  - Update `expected_upgrade` (`runner_tests.rs:193`) and `apply_args_matches_the_library_array_plus_log` (`upgrade_tests.rs:259`). Note: long flag `--verbose` matches `update.rs:43` (upgrade path does not accept `-v`).
- **Check:**
  - *Red:* Update the two test expectations first; run `cargo nextest run -p navo-setup apply_args` and verify failure.
  - *Green:* Update `upgrade.rs:44-56`; `cargo nextest run -p navo-setup` passes.

### A6a — Pass `--verbose` to `Setup.exe` (`[all OS]`)

- **Closed inputs:** `crates/navo-setup/src/runner_steps.rs:24-48` (237 lines), `crates/navo-setup/src/runner_tests.rs:181-190` (376 lines).
- **Change:** Append `"--verbose".into()` to the fresh `Setup.exe` argument vector in `runner_steps::spawn_args` (`setup.rs:100`), and update `expected_fresh` (`runner_tests.rs:181`).
- **Check:**
  - *Red:* Update `expected_fresh` first; run `cargo nextest run -p navo-setup spawned_arrays` and observe failure.
  - *Green:* Update `spawn_args`; `cargo nextest run -p navo-setup spawned_arrays` passes.

### A6b — Add `percent`/`stage` to `Progress` and implement `wait_with_log` (`[all OS]`)

- **Closed inputs:** `crates/navo-setup/src/runner_types.rs:127-132` (132 lines), `crates/navo-setup/src/runner_steps.rs:49-65` (237 lines), `crates/navo-setup/src/window_tests.rs:172-180` (445 lines).
- **Change:**
  - Extend `runner_types::Progress` with `pub percent: Option<u8>` and `pub stage: Option<InstallStage>` (re-export `InstallStage` from `progress_log`), adding `Progress::from_elapsed(elapsed, still_working)`.
  - In `runner_steps.rs`, add `pub(super) fn wait_with_log(child: &mut dyn Child, log: Option<(&Path, bool, Option<&PackageWeights>)>, poll_step: Duration, progress: &mut dyn FnMut(Progress)) -> io::Result<i32>` using `LogTailer` (`250 ms` polls emitting on whole-second, `percent`, or `stage` change). Keep `steps::wait` delegating to `wait_with_log(child, None, Duration::from_secs(1), progress)`. Update `window_tests.rs:172` to use `Progress { .., percent: None, stage: None }`.
- **Check:**
  - *Red:* Add unit test in `runner_steps.rs` feeding a synthetic `TempDir` log across multiple `FakeChild` `250 ms` ticks expecting `percent: Some(...)` then fallback `None` after `2 s` in `Finalizing`; observe failure.
  - *Green:* Implement `wait_with_log` and `Progress` fields; `cargo nextest run -p navo-setup runner_steps window_tests` passes.

### A7 — Wire `PackageWeights` and `velopack_log` in `runner_run` (`[all OS]`)

- **Closed inputs:** `crates/navo-setup/src/runner_run.rs:111-131` (380 raw / 333 code lines), `crates/navo-setup/src/runner_tests.rs:279-290` (376 lines).
- **Change:**
  - In `runner_run.rs:128`, compute `let weights = PackageWeights::from_nupkg(payload.package()).ok();` and call `steps::wait_with_log(&mut *child, Some((velopack_log, upgrade, weights.as_ref())), Duration::from_millis(250), progress)`.
  - Update `runner_tests.rs:281` loop from `0..180` to `0..(180 * 4)` (`progress.len() == 180` preserved cleanly!) and add one end-to-end `runner::run` test with a temp log proving real `percent` and `None` pulse reach the callback.
- **Check:**
  - *Red:* Add the `runner_tests.rs` temp-log test before editing `runner_run.rs`; run `cargo nextest run -p navo-setup runner` and see `percent` stay `None`.
  - *Green:* Edit `runner_run.rs:128` + `0..(180 * 4)`; `cargo nextest run -p navo-setup runner` passes (including `still_working` at `180 s`).

### A8 — Add `SetupStage` and map `Progress` to `SetupProgress` (`[all OS]`)

- **Closed inputs:** `crates/navo-ui/src/setup/mod.rs:75-80` (240 lines post-A1), `crates/navo-setup/src/window_view.rs:47-50` (170 lines), `crates/navo-setup/src/window_tests.rs:172-181` (448 lines).
- **Change:**
  - Add `pub enum SetupStage { Preparing, CopyingPackage, Extracting, Finalizing }` and `pub percent: Option<u8>, pub stage: Option<SetupStage>` to `SetupProgress` (`navo-ui/src/setup/mod.rs`), adding `SetupProgress::new(...)` while keeping `from_elapsed` setting `None, None`.
  - Update `window_view.rs:47-50` to map `p.percent` and `p.stage` across.
  - Extend `window_tests.rs` to assert `Reply::Progress(Progress { percent: Some(42), stage: Some(InstallStage::Extracting), .. })` shows up in `shell.view().progress`.
- **Check:**
  - *Red:* Add `window_tests.rs` assertion for `Some(42)` & `Extracting` before wiring `window_view.rs`; run `cargo nextest run -p navo-setup window_tests`.
  - *Green:* Update `mod.rs` and `window_view.rs`; `cargo nextest run -p navo-setup -p navo-ui` passes.

### A9 — Add Fluent l10n strings for percentages and stages (`[all OS]`)

- **Closed inputs:** `crates/navo-l10n/locales/en-US/setup.ftl:24-26` (71 lines), `crates/navo-ui/src/setup/strings.rs:30-36, 107-112` (192 lines), `crates/navo-ui/src/setup/tests.rs:1-97` (97 lines), `xtask/src/gate/steps.rs:21`.
- **Change:**
  - Add keys to `setup.ftl` (matching `{ $time }` Fluent syntax at `:25`):
    - `setup-installing-bar-percent = Installing, { $percent }%`
    - `setup-installing-percent = { $percent }%`
    - `setup-installing-stage-preparing = Preparing…`
    - `setup-installing-stage-copying = Copying package…`
    - `setup-installing-stage-extracting = Extracting files…`
    - `setup-installing-stage-finalizing = Finishing…`
  - Add matching `SharedString` fields in `crates/navo-ui/src/setup/strings.rs` and verify non-empty loading in `crates/navo-ui/src/setup/tests.rs`.
- **Check:** `cargo nextest run -p navo-l10n -p navo-ui && cargo xtask l10n`.

### A10 — Render determinate bar vs pulse in `installing.rs` + Gallery + Red/Green UI tests (`[all OS]`)

- **Closed inputs:** `crates/navo-ui/src/setup/installing.rs` (~110 lines from A1), `crates/navo-ui/src/setup/pages_b_tests.rs:52-60, 103-140` (237 lines), `crates/navo-ui-gallery/src/setup_page.rs:48-59` (179 lines), `crates/navo-ui/src/job_card/card.rs:146-157`.
- **Change:**
  - In `installing.rs`:
    - Read `percent` & `stage` from `c.view.progress`.
    - When `Some(pct)` (`clamped = pct.min(100)`): render `div().id("installing-bar").role(Role::ProgressIndicator).aria_label(c.strings.installing_bar_percent.replace("{percent}", &clamped.to_string()))` containing a static `c.t.surfaces.progress_fill` child at `.w(relative(f32::from(clamped) / 100.0))` (no `.with_animation` pulse, no shimmer), and render `installing-percent` text (`"{clamped}%"`) plus optional `installing-stage` caption beside/above `installing-elapsed`.
    - When `None`: render existing `BAR_PULSE` opacity pulse (`2400 ms`, static when `c.reduce_motion` is true), `.aria_label(c.strings.installing_bar.to_string())`, no `installing-percent` element.
  - In `pages_b_tests.rs`: test `percent: Some(42)` (`installing-percent` reads `"42%"`, `installing-bar` label is `"Installing, 42%"`, tips/elapsed remain intact), `percent: None` (pulse fallback, no `installing-percent`, `"Installing"` label), and `reduce_motion = true` (static render).
  - In `setup_page.rs`: update `Page::SetupInstalling` to `percent: Some(42), stage: Some(SetupStage::Extracting)` and keep `Page::SetupInstallingSlow` at `percent: None, stage: Some(SetupStage::Finalizing), still_working: true` so gallery covers both modes in dark and light themes.
- **Check:**
  - *Red:* Write the new `#[gpui_kit::test]` assertions in `pages_b_tests.rs`; run `cargo nextest run -p navo-ui setup` and observe failure.
  - *Green:* Update `installing.rs` and `setup_page.rs`; `cargo nextest run -p navo-ui setup && cargo check -p navo-ui-gallery && cargo xtask size && cargo xtask l10n` pass. Gallery screenshots checked in both light and dark themes (visual contrast `>= 3:1` on track/fill, no text clipping).

### A11 — Windows VM End-to-End Check (`[Windows]`)

- **Closed inputs:** `setup-e2e/cases-ui.ps1:122-131, 140-164` (177 lines).
- **Change:** Update `Wait-Installed` in `setup-e2e/cases-ui.ps1` to sample UIA `Get-UiText` repeatedly during install, recording seen progress labels (`%` or fallback `Installing`) and asserting `*-velopack.log` has no `.old` rotation bloat (`< 1 MB`) or lock failure.
- **Check (`Windows VM`):** Run `S1a` (silent), `S1b` (fresh GUI), `S2` (upgrade GUI), and `S12` (both dark & light themes). Inspect `S12-dark-installing.png` and `S12-light-installing.png`, verify live `Setup.exe` & `Update.exe` log lines actually trigger UI percentage progression (or verify `WriteLogger` flush latency) and never stall at `99%`.

---

## Done when

- All atoms `A1–A10` pass `cargo xtask gate` cleanly (`fmt`, `clippy`, `nextest`, `xtask size`, `xtask l10n`, `xtask colors`).
- **A11** passes on the Windows VM for both fresh (`Setup.exe`) and upgrade (`Update.exe apply`) installs, and light/dark `S12-*-installing.png` screenshots are manually inspected.
- An empty/missing/unreadable log or stalled `Finalizing` phase (`> 2 s`) is verified by tests to show the calm fallback pulse rather than a fake timer or `99%` stall.

---

## Open questions for the owner

1. **Finalizing transition (`IP5`):** When `extract` reaches `92%` and Velopack starts post-extract work (shortcuts, folder rename, Defender scan), should the bar switch immediately to the indeterminate pulse (matching Velopack's own `reporter.set_indeterminate()` at `apply_windows_impl.rs:149`), or hold the determinate % for a `2 s` grace first to avoid a sub-second mode flip on fast SSDs?
2. **Stage caption on screen (`A9`/`A10`):** Should the Installing page show `"42%"` only beside `Elapsed 0:42`, or also display stage text (`"Preparing…"`, `"Copying package…"`, `"Extracting files…"`, `"Finishing…"`)?
3. **Arg stability (`IW3` / `D117`):** Confirm adding `--verbose` to Velopack's argument arrays in `runner_steps::spawn_args` and `upgrade::apply_args` (`IP1`) requires no separate entry in `docs/decisions.md` (or reserve `D117` after `D116` in `2026-11-12-installer-anywhere.md`).

---

## Log

- **2026-11-12:** Investigated `velopack = "1.2.161"` crate + `src/bins` sources (`92d6a1c91716729d449034df5c50307dcce39493`) and Navo `navo-setup` / `navo-ui` / `setup-e2e`; drafted plan on `plan/installer-progress-h1` (`9b37ca09`).
