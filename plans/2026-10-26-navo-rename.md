# Navo rename: names, profile migration, index service upgrade

Status: built, waiting for Windows CI and the owner check · Repo: `navo` · Branch: `feature/navo-rename` (new worktree from `main` e077fb5a) · Started: 2026-10-26

## Goal
The product the user sees is Navo (D78): `navo.exe`, the window, the icon, the profile folders, the
index service. A user with a Ferrite profile keeps every setting, shortcut, session and pending job.
The first Navo start moves them over once, survives a crash at any step, and leaves nothing orphaned.

## Inventory (main e077fb5a, `git grep -i ferrite`, vendor excluded)
1,725 hits in 437 files: crates 1,264 in 366 files (163 are test files), plans 173/36, design 135/10,
docs 73/15, CHANGELOG 30, `.github` 28/1, scripts 10/2, README 5, xtask 4/4, assets 3/1 (+15 file names).

- **Binary/package:** `crates/navo-app/Cargo.toml` `[[bin]] name = "ferrite"`; `build.rs` icon path,
  `ferrite.rc`, version info (FileDescription, InternalName, OriginalFilename, ProductName);
  `cli.rs` USAGE; 11 `ferrite:` prefixes in `main.rs`; `CARGO_BIN_EXE_ferrite` in 4 tests.
  CI: `ferrite.exe/.pdb` staging, `--version` = `ferrite 0.0.1`, artifacts `ferrite-windows-x64-<sha>`,
  `ferrite-linux-x64-<sha>`, `ferrite-perf-*`. Release names and zip come from the deployer brief (outside the repo).
- **Window/About:** title `"Ferrite"` (`window_view/start.rs`, `title.rs`), `app_id: "ferrite"`,
  About `Ferrite {version}` and `aria "Ferrite menu"` (`app_menu/mod.rs`), context-menu header,
  crash title. No tray, no file associations, no registry writes, no installer yet (D73 is beta).
- **Single instance:** mutex `Local\Ferrite-{hash}`, pipe `\\.\pipe\Ferrite-{session}-{hash}`
  (`instance/windows.rs`); Unix `ferrite-{uid}/ferrite-{hash}.lock|.sock`; job lock `data/ferrite.lock`.
- **Index service (installed):** SCM name `Ferrite Index`; binary `%ProgramFiles%\Ferrite\Index\ferrite.exe`
  (+ `.tmp`/`.old`); data `%ProgramData%\Ferrite\index\{X}.index` and `service.json`; pipe
  `\\.\pipe\ferrite-index`; file magic `FERRITE-INDEX`.
- **User data:** `%APPDATA%\Ferrite` (config: `ferrite.toml`, `.defaults-pending`, `keymap.toml`,
  `rename.log`) and `%LOCALAPPDATA%\Ferrite` (data: `ferrite.db` + `-wal`/`-shm`, `ferrite.lock`,
  `cache.db`, `session.bin`, `running`, `logs/ferrite*.log`, `index/walk-*.index`); Linux
  `~/.config/ferrite`, `~/.local/share/ferrite`. `--profile <dir>` uses `config/` and `data/` with the same file names.
- **Format markers:** `FERRITE-SESSION\n`, `FERRITE-INDEX`, `# ferrite rename log format 1`, `ferrite-log `.
- **Names in user folders:** `.ferrite-part`, `.ferrite-rename-`, `.ferrite-held-`, `.ferrite-link-`;
  crash zip `ferrite-report-*.zip`; metrics temp `ferrite-metrics-`; threads `ferrite-io|copy|instance|handoff`;
  window classes `FerriteShellMenu`, `Ferrite clipboard`; font family `Ferrite Preview`.
- **Config values containing "ferrite":** themes `Ferrite Light`/`Ferrite Dark` (no key contains it);
  keymap context `FerritePane` (9 string uses). Internal identifiers (`FerriteWindow` 472,
  `FerritePane` type, `FerriteAction`, `ferrite_*` action namespaces) are not user-facing.
- **Env vars:** `FERRITE_LOG` (user), `FERRITE_CRASH_TEST` (CI), `FERRITE_COMMIT` (build-internal),
  test-only `FERRITE_EXPECT_PDFIUM`, `_EXFAT_ROOT`, `_FAT32_ROOT`, `_SMB_ROOT`, `_LOCK_TEST_PATH`,
  `_SAMPLE_FONT`, `_BIN_DIR`. `RUSTY_EXE_LIMIT` stays.
- **Strings:** Fluent 2 (`settings.ftl:41`, `viewer.ftl:13`); about 30 Rust literals in navo-ui, navo-app,
  navo-core, navo-store, navo-fs, navo-diag, navo-shell, navo-index.
- **Assets:** `assets/brand/ferrite{,-16,-24,-32,-48,-wordmark}.svg`, `ferrite.ico`, 7 PNG files, `ferrite.desktop`;
  `scripts/build-icons.sh`, `scripts/install-local.sh`.
- **Codename leaks:** none in Fluent, window, assets or version info. Only the gallery (dev tool),
  log targets (`rusty_app::…`) and backtraces show `rusty`; they stay.

## Decisions
- D1 — One branch, one squash merge. No atom reaches `main` alone: new paths without the migration would
  start users on an empty profile.
- D2 — New names: `Navo`/`navo` everywhere the user looks. Roots `%APPDATA%\Navo`, `%LOCALAPPDATA%\Navo`,
  `~/.config/navo`, `~/.local/share/navo`; files `navo.toml`, `navo.db`, `navo.lock`, `logs/navo.log`;
  service `Navo Index` in `%ProgramFiles%\Navo\Index`, `%ProgramData%\Navo`, pipe `navo-index`;
  mutex `Local\Navo-{hash}`; crash zip `navo-report-*`; CI artifacts `navo-windows-x64-<sha>`.
- D3 — Binary format markers (`FERRITE-SESSION`, `FERRITE-INDEX`) stay forever. Text headers
  (`rename.log`, command log) write `navo` and readers accept both.
- D4 — Temporary names write `.navo-*`. Every recogniser accepts `.ferrite-*` and `.navo-*` forever.
- D5 — Internal identifiers and `rusty-*` crates stay. A later mechanical plan may rename them.
- D6 — Migration copies; it never moves across roots. Staging folders sit next to their targets, so
  each commit is one same-volume rename. This also covers redirected `%APPDATA%`.
- D7 — Migrated: `navo.toml`, `.defaults-pending`, `keymap.toml`, `rename.log` (+ `.1`), `navo.db` (+ `-wal`),
  `session.bin`. Not migrated (rebuilt): `cache.db`, `logs/`, `index/`, `running`, `ferrite.lock`, `-shm`.
- D8 — Verification is byte equality of every copied file, then `PRAGMA quick_check` on the copy only
  to log it. A damaged old database moves as it is; the existing quarantine (P35) handles it.
- D9 — Migration runs after the Navo instance lock and before logging, crash markers and any `ensure()`.
  Nothing may create a Navo root earlier. `--metrics` profiles never migrate.
- D10 — `--profile <dir>` migrates in place: same roots, old file names to new names, same protocol.
- D11 — Env: `NAVO_LOG` wins; `FERRITE_LOG` is read when `NAVO_LOG` is unset and logs one warning,
  until 1.0. `NAVO_CRASH_TEST` and test-only vars rename without fallback. `FERRITE_COMMIT` becomes `NAVO_COMMIT`.
- D12 — Settings read `Ferrite Light`/`Ferrite Dark` as the default light/dark theme of the
  `ui/navo-themes` branch and write the new name on the next save. Keymap reads `FerritePane` as `NavoPane`.
- D13 — Old assets are deleted from the tree (history keeps them). Past plans, design prototypes,
  CHANGELOG history and the decisions log stay as written; the orchestrator marks D29 superseded by D78.
- D14 — Owner answers (2026-10-26): Q1 **rename whole folders** to `Ferrite.migrated-to-navo` after the
  migration commits (old Ferrite then starts with defaults; no `MOVED-TO-NAVO.txt` marker needed, the
  renamed folder is the marker); Q2 one-time notice with an Upgrade button; Q3 "Try again" and "Start fresh".
  The folder rename is the last step after the new profile is verified; a crash before it resumes it.
### Migration protocol (atoms A4, A5)

Journal `Navo-migration.json` beside the Navo data root, written with `navo_store::write_atomic`.
Phases: `detached → staged → committed → retired → done`.

1. **Guard.** Hold the Navo instance lock and `Navo-migration.lock`. Probe the old Ferrite: open
   `Local\Ferrite-{hash(old data)}` (Unix: try-lock `ferrite-{hash}.lock`) and try an exclusive lock on
   `ferrite.lock`. Never wait. If Ferrite runs: dialog "Close Ferrite, then try again" (Try again, Quit).
2. **Decide.** Migrate only if the journal is absent, the Navo roots hold no `navo.toml`/`navo.db`, and
   the old roots hold data without `MOVED-TO-NAVO.txt`. A Navo root with user data and no journal: no migration, no merge.
3. **Detach.** Rename `ferrite.db`, `-wal`, `session.bin` to `*.navo-migrating` in place (atomic;
   fails while a Ferrite holds them, which also proves it is closed). Journal `detached`.
   From now on no Ferrite can replay the old job journal.
4. **Stage.** Delete stale `Navo.staging` folders; copy into `Navo.staging` next to each target; flush;
   re-read and compare bytes. Journal `staged`.
5. **Commit.** Rename data staging → `Navo`, then config staging → `Navo`. Each step checks what already
   exists, so a resume repeats only the missing rename. Journal `committed`.
6. **Retire.** Write `MOVED-TO-NAVO.txt` in both old roots; rename `*.navo-migrating` to
   `*.migrated-to-navo`. Old `ferrite.toml`, `keymap.toml` and `rename.log` stay. Journal `retired`,
   then `migrated-from-ferrite.json` in the Navo data root, then delete the journal.
7. **Resume/rollback.** Before `committed`: delete staging, rename detached files back, start again.
   From `committed`: roll forward only. Normal start-up then recovers pending jobs exactly once, in Navo.

An old Ferrite started later finds its settings and keymap, an empty job journal and no session.
It cannot touch Navo's files. Navo never reads the old roots again (step 2 marker).
### Index service upgrade (atoms A7, A8)

Navo checks for `Ferrite Index` at start (query only). If present, a quiet notice offers "Upgrade the
index service" (UAC). Elevated `navo --index-service install`: read the old `service.json`, install and
start `Navo Index` with it, wait until `navo-index` answers, then stop and delete `Ferrite Index` and
remove `%ProgramFiles%\Ferrite\Index` and `%ProgramData%\Ferrite`. A crash after the new install leaves
the old service stopped or running but detected; the next install finishes removal. `remove` deletes
both names. Old index files are not copied; the new service rebuilds them. Until the upgrade, Navo uses
the walk index (Auto already falls back).

## Owner questions
- Q1 — Old Ferrite folder after migration: keep in place with `MOVED-TO-NAVO.txt`, journal and session
  moved aside (recommended), or rename whole folders to `Ferrite.migrated-to-navo` (old Ferrite then
  starts with defaults)? Deletion later via Settings, "Remove old Ferrite files".
- Q2 — Index service upgrade: a one-time notice with an Upgrade button (recommended), or only in Settings?
- Q3 — When migration fails for a reason other than a running Ferrite (disk full, access denied):
  offer "Try again" and "Start fresh" (recommended; Start fresh leaves the old folder untouched and
  never migrates again), or only "Try again" and "Quit"?

## Components
| Component | Layer | Contract (in → out) | New / change | Test |
| --- | --- | --- | --- | --- |
| `brand` | lib (navo-core) | consts `NAME`, `SLUG`, `LEGACY_NAME`, `LEGACY_SLUG`, temp prefixes | new | `brand/tests.rs` |
| `temp-names` | lib (navo-core, navo-fs) | new prefixes; `is_temp(name)` accepts both | change | `op_plan`, `job/rename` tests |
| `store-dirs` | adapter (navo-store) | `resolve` → Navo roots; `legacy()` → Ferrite roots; file-name methods | change | `dirs/tests.rs` |
| `migration` | service (navo-app) | `(old, new: StoreDirs, Fs) → Migrated \| Skipped \| Blocked \| Failed` | new | `migration/tests.rs` |
| `legacy-probe` | adapter (navo-app) | `old data dir → Running \| Closed` | new | `migration/probe_tests.rs`, Windows CI |
| `start-order` | app (navo-app main) | migration between instance and logging; dialogs | change | `main` tests |
| `instance-names` | adapter (navo-app) | Navo mutex, pipe, socket names | change | `instance` tests |
| `index-service` | adapter (navo-app, navo-index) | Navo names; legacy detect, upgrade, remove | change | `index_service` tests |
| `env-bin` | app | `navo` bin, version info, `NAVO_*` vars with D11 fallback | change | `cli`, `logging` tests |
| `legacy-values` | adapter (navo-store, navo-ui) | theme and keymap context aliases | change | settings, keymap tests |
| `ui-strings` | ui | every user-visible "Ferrite" → "Navo" | change | snapshots, l10n ratchet |
| `brand-assets` | assets | `n_` variant A SVGs, hand-hinted 16 px PNG, `navo.ico`, `navo.desktop` | new | `build-icons.sh` output |
| `ci-scripts` | ops | workflow, scripts, xtask perf, test env vars | change | CI run |
| `docs` | docs | current docs say Navo | change | grep |

## Atoms
All atoms work in the `feature/navo-rename` worktree. Per-crate check: `cargo xtask check -p <crate>`.
Mechanical word swaps (A11, A12, A14, A15) may touch more than 3 files; each edit is one name.

| Atom | Component | Role | Effort | After | Check | Status | Cost |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | `brand`, `temp-names` (core) | patcher | medium | — | `cargo xtask check -p navo-core` | done | |
| A2 | `temp-names` (fs) | patcher | medium | A1 | `cargo xtask check -p navo-fs` | done | |
| A3 | `store-dirs` | api-builder | medium | A1 | `cargo xtask check -p navo-store` | done | |
| A4 | `migration` | senior (Opus) | medium | A3 | `cargo xtask check -p navo-app` | done | |
| A5 | `legacy-probe`, `start-order` | senior (Opus) | medium | A4, A6 | `cargo xtask check -p navo-app` + Windows CI | done | |
| A6 | `instance-names` | patcher | medium | A1 | `cargo xtask check -p navo-app` | done | |
| A7 | `index-service` names | patcher | medium | A1 | `cargo xtask check -p navo-app -p navo-index` | done | |
| A8 | `index-service` upgrade | senior (Opus) | medium | A7 | `cargo xtask check -p navo-app` + Windows CI | done | |
| A9 | `env-bin` | patcher | medium | A1 | `cargo xtask check -p navo-app` | done | |
| A10 | `legacy-values` | api-builder | medium | A3, `ui/navo-themes` merged | `cargo xtask check -p navo-store -p navo-ui` | done | |
| A11 | `ui-strings` (navo-ui) | ui-builder | medium | A10 | `cargo xtask check -p navo-ui` | done | |
| A12 | `ui-strings` (other crates, Fluent) | patcher | medium | A9 | `cargo xtask check -p navo-app -p navo-core -p navo-store -p navo-fs -p navo-diag -p navo-shell -p navo-index -p navo-l10n` | done | |
| A13 | `brand-assets` | ui-builder | medium | `ui/navo-splash` merged | `scripts/build-icons.sh` | done | |
| A14 | `ci-scripts` | patcher | medium | A9, A13 | `cargo xtask gate` (once, by orchestrator) | done | |
| A15 | `docs` | patcher | medium | A1–A14 | grep below | done | |
### A1 — Brand constants and core temp names
- **Read:** `navo-core/src/lib.rs`, `op_plan/mod.rs:1-30`, `op_plan/names.rs`, `rename/order.rs:180-200`.
- **Change:** new `navo-core/src/brand.rs` (D2 names, legacy names, `TEMP_SUFFIX = ".navo-part"`,
  `LEGACY_TEMP_SUFFIX`, rename prefix and legacy prefix); `order.rs` uses the new prefix.
- **Check:** crate check passes; a test proves both prefixes are recognised.
- **Stop if:** a temp name is matched by prefix outside `navo-core` and `navo-fs`.
### A2 — fs temp names
- **Read:** `navo-fs/src/job/aside.rs`, `job/rename.rs:1-40`, `local/link/windows.rs:30-45`.
- **Change:** write `.navo-held-`, `.navo-rename-`, `.navo-link-`; `rename.rs:32` accepts both prefixes. **Check:** crate check; a test renames over a leftover `.ferrite-rename-*` and treats it as temp.
### A3 — Store dirs
- **Read:** `navo-store/src/dirs/mod.rs`, `dirs/tests.rs`.
- **Change:** D2 roots; `legacy(windows, var)` returning Ferrite roots; methods `settings_file`
  (`navo.toml`), `journal_file`, `lock_file`, `legacy_settings_file`, `legacy_journal_file`.
- **Check:** tests for Windows and XDG resolution, new and legacy, empty variables.
### A4 — Migration engine (senior)
- **Read:** this plan's protocol, `navo-store/src/atomic_write/mod.rs`, `dirs/mod.rs`, `profile/mod.rs`.
- **Change:** `navo-app/src/migration/{mod.rs,journal.rs,tests.rs}`; the file system behind a trait so
  tests inject faults. Hidden exit hook `NAVO_MIGRATION_TEST_EXIT=<phase>`, honoured only with `NAVO_CRASH_TEST=1`.
- **Check:** tests prove: fault at every copy, flush, rename and journal write; crash between each two
  writes then resume to one profile; rollback before commit restores the old files byte for byte;
  target with user data is never merged; marker blocks a second run; `--profile` in place; WAL content survives.
- **Stop if:** the protocol needs a step it does not list.
### A5 — Probe and start order (senior)
- **Read:** `main.rs:158-260`, `instance/windows.rs:25-60`, `instance/unix.rs:20-40`, `jobs/mod.rs:50-75`, `gui.rs:260-275`.
- **Change:** `migration/probe.rs` (Windows `OpenMutexW`, Unix try-lock, exclusive `ferrite.lock`);
  call migration in `open_window` after `prepare_instance`; `gui.rs`, `settings/start.rs` use A3 file methods;
  dialogs per Q3.
- **Check:** crate check; Windows CI test: a held old mutex yields `Blocked` without waiting.
### A6 — Instance names
- **Read:** `instance/{windows.rs,unix.rs,runtime.rs,server.rs}`, `instance/windows_tests.rs:50-60`.
- **Change:** `Navo-` mutex and pipe, `navo-{uid}`, `navo-{hash}`, thread names. **Check:** crate check; tests assert the new names.
### A7 — Index service names
- **Read:** `scm/windows.rs:20`, `install_plan.rs:10-20,165-175,265-275`, `config.rs:65-75`, `navo-index/src/pipe/mod.rs:21`.
- **Change:** D2 names; `ferrite.exe.` nonce pattern becomes `navo.exe.`. Magic unchanged (D3). **Check:** crate checks; install-plan tests assert Navo paths.
### A8 — Index service upgrade (senior)
- **Read:** `installation/windows.rs`, `installation/configuration.rs`, `install_plan.rs`, `storage/windows.rs`, `search_hooks/service.rs`.
- **Change:** legacy constants; `install` carries the old `service.json`, verifies the new pipe, then
  removes the old service and folders; `remove` handles both; status exposes `legacy_present`; notice per Q2.
- **Check:** `install_rollback_tests.rs`/`reliability_tests.rs` gain cases: old present, crash after new
  install, old delete fails, both removed. Windows CI passes.
### A9 — Binary, CLI, env
- **Read:** `navo-app/Cargo.toml`, `build.rs`, `cli.rs:20-30`, `main.rs`, `logging/mod.rs:1-80`, 4 `env!("FERRITE_COMMIT")` sites.
- **Change:** `[[bin]] name = "navo"`, version info `Navo`/`navo.exe`, `navo:` prefixes (icon path moves in A13),
  `NAVO_COMMIT`, `NAVO_CRASH_TEST`, D11 log fallback, tests use `CARGO_BIN_EXE_navo`.
- **Check:** crate check; `navo --version` prints `navo 0.0.1`; log fallback test.
### A10 — Legacy setting values
- **Read:** `settings/model.rs:40-50`, `settings/snapshots/default_settings.snap`, `keymap_file/patch.rs`, `navo-ui/src/window_view/keys.rs:6`, `ui/settings/values.rs:95-100`.
- **Change:** D12 aliases on read; context string `NavoPane`; snapshots updated. **Check:** tests: old theme value loads the new default and saves the new name; old keymap context binds.
### A11 — navo-ui strings
- title, `app_id`, About, menu label, context header, crash title, viewer and settings messages, preview font family. **Check:** crate check; `git grep -n '"[^"]*Ferrite' crates/navo-ui/src` lists only test fixtures and A10 aliases.
### A12 — Other strings and Fluent
- literals listed in the inventory; 2 Fluent strings; crash zip and metrics prefixes; window classes; diag summary. **Check:** crate checks; the same grep over those crates lists only legacy constants and D3 markers.
### A13 — Brand assets
- **Read:** `design/prototypes/navo-icon-splash/index.html` (variant A), `scripts/build-icons.sh`, `scripts/install-local.sh`, `assets/brand/ferrite.desktop`.
- **Change:** `navo.svg`, `navo-24|32|48.svg` (glyph outlined), hand-drawn `navo-16.png`, `navo-wordmark.svg`,
  `navo.desktop`; the script packs the 16 px bitmap as is and writes `navo.ico`; `build.rs` icon path; delete Ferrite assets.
- **Check:** script prints `Wrote …/navo.ico`; screenshot of the 7 sizes at 1x on light and dark.
### A14 — CI, scripts, xtask, test env vars
- `ci.yml` names and `NAVO_*`; `install-local.sh` (`NAVO_BIN_DIR`); xtask perf heading; test env vars; `docs/testing/windows-identities.md`. **Check:** orchestrator runs `cargo xtask gate`; Windows CI green.
### A15 — Docs
- README, `docs/architecture.md`, `docs/features/*`, `docs/perf.md`, `docs/dev-tools.md`, `docs/writing.md`, `docs/roadmap.md`; new CHANGELOG entry. **Check:** `git grep -il ferrite -- README.md docs ':!docs/decisions.md' ':!docs/spike'` lists only lines about migration.

## Order
A1 → (A2, A3, A6, A7, A9 in parallel) → A4 → A5; A7 → A8; A10 after `ui/navo-themes`; A13 after
`ui/navo-splash`; A11, A12 → A14 → A15 → gate → review (Opus medium on A4, A5, A8) → owner Windows list → merge.

## Windows verification (owner's Windows PC, before merge)
1. Fresh user: only `%APPDATA%\Navo` and `%LOCALAPPDATA%\Navo` appear.
2. Ferrite profile with settings, shortcuts, two tabs, rename history and one killed copy job: Navo
   starts with all of it; the job is recovered once; both old folders are renamed to
   `Ferrite.migrated-to-navo`, and `%LOCALAPPDATA%\Navo` holds `migrated-from-ferrite.json`.
3. `NAVO_CRASH_TEST=1`, `NAVO_MIGRATION_TEST_EXIT` at each phase: the next start resumes or rolls back; no `Navo.staging` remains.
4. Ferrite open: Navo says "Close Ferrite", migrates nothing; after closing, Try again works.
5. Old `ferrite.exe` after migration: starts with old settings, no session, no jobs; Navo unchanged.
6. Ferrite and Navo side by side; a second `navo.exe` hands off to the first; no hang.
7. `Ferrite Index` installed and running: Upgrade (UAC) leaves only `Navo Index` in services.msc;
   `%ProgramFiles%\Ferrite`, `%ProgramData%\Ferrite` gone; Ctrl+K uses the service.
8. Kill the elevated install after the new service starts: the next Upgrade removes the old one.
9. Task Manager, taskbar, Alt+Tab, Explorer: name Navo, sharp `n_` at 16 px at 100, 125, 150 % scale.
10. `FERRITE_LOG=debug` still logs, with one warning; a crash writes `navo-report-*.zip`.

## Rollback
1. Revert the squash commit; republish the previous Ferrite build.
2. A user back on Ferrite: close Navo; rename both `Ferrite.migrated-to-navo` folders back to
   `Ferrite`; copy `%LOCALAPPDATA%\Navo\navo.db` (+ `-wal`) to `%LOCALAPPDATA%\Ferrite\ferrite.db`;
   delete `%LOCALAPPDATA%\Navo\migrated-from-ferrite.json`.
3. Service: `navo --index-service remove`, then `ferrite --index-service install`.

## Done when
- The protocol tests in A4, A8 pass; Windows CI is green; `cargo xtask gate` passes once on the branch.
- `git grep -i ferrite -- crates` shows only legacy constants, D3 markers, aliases and test fixtures.
- The Windows list passes on a Windows PC.

## Log
- 2026-10-26 — Plan drafted from the inventory at e077fb5a.
- 2026-10-26 — A1 done: `navo-core::brand` (names, temp prefixes, `is_part_name`, `is_rename_temp`); `TEMP_SUFFIX` now `.navo-part`, rename temps `.navo-rename-`.
- 2026-10-26 — `ui/navo-splash`: native Win32 splash (D79) on its own thread, timing core tested; masks from `scripts/build-splash.sh` in `assets/brand/splash`. A13 can start after it merges.
- 2026-10-26 — A7 done: service `Navo Index`, pipe `navo-index`, `%ProgramFiles%\Navo\Index\navo.exe` with `navo.exe.<nonce>` temporaries, `%ProgramData%\Navo\index`; index magic unchanged.
- 2026-10-26 — A8 done: `install` carries the Ferrite `service.json`, installs Navo Index, waits up to 30 s for `navo-index`, then removes Ferrite Index and its trusted folders; a crash or failed delete keeps the old one detected for the next install. `remove` handles both names. Status has `legacy_present`; a quiet notice offers Upgrade until dismissed (marker `index-upgrade-dismissed` in the data root). Windows CI must still compile the SCM glue.
- 2026-10-26 — Final-review fixes: Windows folder sync is best-effort in the migration (error logged); `rename.log.1` migrates; `FERRITE_LOG` warnings reach the log and name their variable; a user profile migrates without the instance lock; generic "file in use" dialog; the Ferrite `service.json` is carried only when Navo has none; leftover Ferrite index files are a warning, not a failed install.
