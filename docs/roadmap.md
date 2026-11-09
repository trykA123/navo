# Navo roadmap

Updated 9 November 2026. This document tracks current implementation status. The [original visual outline](../design/roadmap.html) shows the milestone scope. Scope and priority decisions are in [decisions.md](decisions.md), especially D85 and D106. Plain-language specs are in [features/](features/).

Alpha, Beta and v1.0 are product milestones. Release numbers follow Cargo versions; a feature release increments the minor version. Windows packages are on [GitHub releases](https://github.com/trykA123/navo/releases).

Kinds: **core** is always on; **module** can be turned off; **package** is data only (themes, keymaps); **infra** is build, CI and release work.

## v1.0 targets

| Measure | Target |
| --- | --- |
| Cold start to first usable frame | ≤ 400 ms |
| Warm start | ≤ 150 ms |
| Summon from tray (beta) | ≤ 50 ms |
| RAM, two panes, idle | < 80 MB |
| Portable `.exe` | no limit (D44); CI guard 160 MiB |

## Design · done

Product discussion, prototypes A–L (B chosen), architecture, plugin model, decisions D1–D26; plans P1–P8 in `plans/`.

## Foundation · v0.1 · done

Done 15 October 2026: P1–P8 passed their gates; PoC 1 and PoC 2 passed on a Windows PC.

Goal: prove the stack and build the factory.

- infra: foundation plan; Cargo workspace with the eleven crates; quality gate; Windows CI producing a portable `.exe` per push.
- infra: GPUI spike on design B with 100k entries, three panes and the drawer; `nucleo` against `frizbee` on 2 million names; built on `gpui-kit`, including its AccessKit support.
- core: storage layer with SQLite from day one (`navo.db`, `cache.db`, migrations) and the session snapshot (D17).
- core: diagnostics with redacted logs, memory and performance statistics and a hand-saved report (D20).
- core: serialisable commands, layout tree, multi-window state and `apply()` with property tests.
- core: locations and providers, record and replay, one error model, I/O queues per volume, experimental flags (D21).
- infra: fake file system with fault injection and a hostile-name corpus.

Exit: the spike meets the targets on Linux and a Windows PC, otherwise D2 is reopened.

## v1 Alpha · done

The owner signed off the Alpha on 4 November 2026, released as v0.2.0 with the separate navo-bench tool. Daily-use and performance follow-ups continue below; they do not reopen the signed-off milestone.

| Feature | Delivered | Follow-up |
| --- | --- | --- |
| Multi-select and drag and drop | Marking, drops between panes, folders, tabs, path crumbs and drives, shortcuts, drag to and from Explorer, right-button choices and auto-scroll | Owner checked Explorer drag. Linux external drops are Copy-only. |
| Rich ELF and MAP | Rich views, memory models, companion discovery and live module prefixes | Validate the reviewed Rich views on a Windows desktop |
| Ctrl+K command center | Commands, recent files, tabs, folder and path sources, disk index, Search settings and the optional index service | Real-volume memory and large-index measurements |
| Multi-rename | Journaled rename and undo, crash recovery, edited and imported names, sorting, counters and optional rename log | Owner checked rename. The FAT/exFAT USB check is set aside. |

The delivered app also includes:

- Two panes, tabs, navigation history, session restore, file operations, selection, virtual lists, the viewer and activity strip.
- Six colour themes: Open sea, Red bill, Black cap, Fine mesh, Grain, and Ink and amber, each in light and dark forms.
- The Navo name and `n_` icon. The old Ferrite profile moves to Navo with crash recovery; the preserved old folder is named `Ferrite.migrated-to-navo`.
- The Windows start-up splash, shown only when start-up exceeds half a second, and the styled Windows shell menu with classic-menu fallback.
- The user guide on Shift+F1, tray and summon support, faster start, earlier PDF availability and quiet idle behavior.
- Setup.exe and the portable zip on the release page. The installer is already shipping.

## Release scope · v0.3.0

The three parts below are implemented and merged on main. They add to the existing v0.2.3 app.

| Change | Completed behavior | Evidence |
| --- | --- | --- |
| Faster folders | Tabs share folder rows and sorted views. Sorts and filters avoid copying every row. | [Shared listing plan](../plans/2026-11-05-shared-listing.md) |
| Git badges skip unchanged repositories | A recent status can be reused when a live watch and Git fingerprint prove no change. Watches cover the listing folder, its ancestors and direct children. Refresh, Git commands and returning to a left folder force a check. | [Git skip plan](../plans/2026-11-05-git-skip.md), D111 |
| Smaller file index | About 97 MiB per two million entries in the synthetic fixture, with compatible version-three snapshots and measured rebuild limits. | [Index layout plan](../plans/2026-11-05-index-layout.md), D109 |

Also included: the Linux rename-notification fix and targeted dependency updates. The watched-folder size/date fix and earlier cache, fuzzy-search, table-sort and PDF-search optimizations shipped in v0.2.3.

Windows VM checks, host gates, snapshot compatibility checks and release search timings passed. The real-volume VM memory procedure remains unavailable: the required test volumes and service are absent. The synthetic result is not a whole-service private-memory measurement.

## Release scope · v0.4.0

| Change | Completed behavior | Evidence |
| --- | --- | --- |
| Own index service exe | The service runs `navo-index.exe` as "Navo Index". Install over the old `navo.exe` service migrates with one prompt and rolls back on failure; remove leaves no Program Files folder, or schedules what Windows refuses. | IX8v, IX8s in the [beta plan](../plans/2026-10-27-beta.md) |
| Repository folders stay renameable | The long-lived `git cat-file` child runs outside the repository and closes after 2 s idle. | Windows tests in `navo-git` |
| Update test feed | Tester builds read a local feed; the nine-case VM update suite runs. | `testing/ix8r-report.md` |

## Release scope · v0.4.1

| Change | Completed behavior | Evidence |
| --- | --- | --- |
| Git stops before file operations | Rename, delete and move of a folder pause Navo's own Git reads in it and end them first, so Windows no longer reports the folder in use. A fetch, Pull or Push is never stopped: Navo waits up to 3 s, then reports the folder locked by `git.exe`. | [Git-stop plan](../plans/2026-11-09-git-stop-before-fileops.md) |

## Active work

- **In-app update validation:** built and run in the Windows VM for v0.4.0. The tester-only folder feed exists, and the nine-case update suite passed seven cases; cases 8–9 passed their visible checks. The index service runs as `navo-index.exe`, and the senior review fixes are in. Still open before v1.0: the owner counts the UAC prompts (update cases 8–9, service checks 1–3) and checks Task Manager, the service checks 5–6 are rerun after the review fixes, and `docs/features/updates.md` is written (IX8t). See the [beta plan](../plans/2026-10-27-beta.md).
- **Installer wizard:** part 1 is being built. The per-user setup checks folders, holds the chosen folder, prepares the package and runs Velopack's install or upgrade; next are the Windows checks in the VM and the wizard screens. See the [installer plan](../plans/2026-11-09-installer-wizard.md).
- **Git and file operations:** done in v0.4.1. Rename, delete and move stop Navo's own Git commands in that folder before they act.
- **Compare and sync:** engine work has landed for large-file reading, text differences, parallel content checks and Git revisions. The compare screen and sync workflow remain; see the [compare plan](../plans/2026-10-20-p42-compare.md).
- **Move-undo hardening:** finish validation that Ctrl+Z restores only the same file after a move and reports files changed on disk.
- **Beta completion:** finish results-pane appearance and hit lines, background job work and accessibility follow-up, in the priority order below.

### Validation still to finish

- Windows desktop follow-up for Rich ELF/MAP, profile migration, the splash and the leaner index service. Explorer drag and multi-rename are checked; the FAT/exFAT USB check remains set aside.
- A heavy Windows benchmark covering millions of indexed files, search latency, long lists, memory and animation smoothness.
- The long daily-use check for lost or damaged files.

## v1 Beta · in progress

Goal: fast search, workspaces and tiled panes, safe deletes.

The owner reordered the beta on 27 October 2026 (D85). This is the priority order; each item states its current status:

1. core: search index like Everything: Ctrl+K answers at once from the Navo Index service, even while it updates; no folder walks on NTFS drives; the service starts from its saved index; less memory; a benchmark next to Everything. The busy-answer fix is done. Memory peaks are cut: loading the index and rebuilding it need far less extra memory (load 22 → 8 bytes per file, rebuilds sized from the old index, cleanup a third of before), one drive builds at a time, and network drives are indexed only when you list them, with a cap.
2. core: delete safety net (D65): Shift+Del and overwrites keep the old files 1–7 days, a "Recently deleted" view, Ctrl+Z restores. On by default since alpha-2026-10-28e; review and docs left.
3. core: workspaces (Alt+1…9) with the tray: named workspaces, drafts, Save and Discard, rename, move, delete with Ctrl+Z, and a status-bar button. Done.
4. core: up to four panes, tiled like Hyprland (D80): five layouts and your own on Ctrl+L, split, close, swap and resize from the keyboard, Shift+drag swap, divider drag, a strip in narrow windows. Done.
5. core: find files by content and attributes in Ctrl+K, with results as a tab you can work in; virtual collections (saved searches, snapshots, files picked by hand). Query, content matcher, collection store, results tab, Ctrl+K modes and Content mode (search inside files, Enter opens at the line) done; results tab look and hit lines next.
6. module: clipboard history in a Ctrl+K-style palette, as a module you can switch off. Built: it records text, links and file lists you copy, opens with Ctrl+Shift+V or Win+Alt+V from any app, pastes back, and can be paused, cleared or deleted. Saved items (snippets, paths), encrypted secrets and folder notes are built, with a manage view on F2. A pass on a real Windows desktop is done: the pop-up takes focus, secrets clear after 30 s and when Navo quits, and Delete all wipes the data. The summon hotkey and tray are done.
7. core: background work that never waits: job pools with priorities, folder sizes that cancel when you leave, size on disk, hard links counted once. Still planned as B10; existing speed improvements do not complete this work.
8. core: accessibility basics: Narrator reads lists, dialogs and Ctrl+K, focus visible everywhere, reduced motion, contrast themes, UI scale. Mostly done.

Done earlier in the beta: responsive layout and new fonts (D80), clone a tab into the other pane, open a terminal here, cache sizes in diagnostics, Follow on Windows, the DBC and A2L outline in F3, every shortcut changeable in Shortcuts (conflict checks, hints that show your keys, old keymaps carried over), a right-button setting for Total Commander or Windows behaviour, and your own commands like Total Commander's: a commands file names a program, its arguments (cursor file, marked files, both folders) and a key, and each command runs from the file list or Ctrl+K. Speed pass: large folders sort by size or date about 2.5× faster, many small files copy about 2× faster, and Ctrl+K searches once per 100 ms while a folder walk grows.

Exit: no open data-safety bugs; start-up and memory targets met in CI.

## v1.0

Replaces Total Commander for daily work (D39). Parity features are built to beat TC, not copy it.

1. core: compare and sync, better than Beyond Compare where it counts (D40): folder compare in the panes, a calm full-window look with a change map on each scroller (chosen), sync as a plan with undo, cached BLAKE3 hashes, syntax-aware text diff, paged diff for huge and binary files, image compare; engine in the shared `navo-compare` crate.
2. infra: code signing, full installer/update validation, the soak test and steadier Windows tests. The Velopack installer, portable package, user guide and in-app updater are built; the tester feed, nine-case update suite and final review remain open (D89 supersedes D73). The uninstaller includes index-service removal.
3. core: optional default file manager (Win+E).
4. core: inspector column with preview and properties, thumbnails, archives as folders (zip, 7z, tar with gz, xz and zst). File details, owner and hashes are done.
5. core: module registry with runtime on/off and safe mode; settings window generated from module schemas; notifications for long operations. Then the automotive viewers move into one module (D112).

Exit: every target is met on a clean Windows 11 machine, and a week of work runs without opening Total Commander.

## v1.1 · See and find

- module: auto switch view.
- package: language packages for the F3 viewer (D38).
- core: checksums (BLAKE3, SHA-256, SHA-1, MD5) with verify; images in F3.
- package: Total Commander keymap.

## v1.2 · Git

- module: the remaining Git history, diff and command workflows. Already built: repository badges, reuse of unchanged status, Alt+G or click menus for the chosen Git tool, fast-forward pull and an unforced push. A real Windows desktop follow-up remains.

## v2.0 · Open platform

- core: WASM plugins (versioned WIT API, permissions, sandbox); Lua scripting.
- infra: plugin SDK, template, dev mode with hot reload.
- core: new slots for metadata providers, view modes, transforms, automation rules and status badges; module packs (D24).
- module: SFTP; S3 and rclone are still open.
- package: language packs, Romanian first.
- core: undo for every job from a history list; per-case confirmations.
- core: refresh mode per network location, ignore list; Parallel and Large files copy modes, copy options, profiles and rules by destination; date and size formats.
- package: custom colour themes and icon themes (D16).
- module: more in F3: Raw/Pretty for JSON, XML and YAML; spreadsheets; colouring for m4, OIL and DCM files; CSV and CAN trace column resize.
- Jenkins links to network folders; optional built-in web view; workflows and automations; a visual workflow builder.
- module: Everything (optional), embedded terminal (ConPTY).

Exit: a third-party plugin is built, installed and updated without touching Navo's code.

## Later

- Accepted ideas not yet in a plan: [ideas](ideas.md).
- module: duplicates and disk usage.
- package: import of Zed and VS Code colour themes.
- Native editing for each kind: a table edits like a spreadsheet; a tree edits keys and values in place.
- module: DBC and A2L in F3, the rest: find in the outline, the DBC and A2L inspectors, problems, the narrow layout and bookmarks.
