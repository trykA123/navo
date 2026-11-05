# Navo roadmap

Updated 5 November 2026. The visual version is [`design/roadmap.html`](../design/roadmap.html). Scope rules are in [decisions.md](decisions.md) (D13, D14). Plain-language specs for the bigger features are in [features/](features/).

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

Goal: a file manager the owner can use every day on Windows.

Signed off by the owner on 4 November 2026 and released as v0.2.0, together with the navo-bench benchmark tool. The remaining Windows checks and the heavy benchmark happen during daily use. Stage names (alpha, beta) are milestones, not version numbers; versions follow D106.

All four alpha features are built. They are in the pre-release alpha-2026-10-26. Status is shown instead of percentages, because the old percentages mixed building with release checks.

| Feature | Built | Still open |
| --- | --- | --- |
| Multi-select and drag and drop | Marking, drops between panes, folders, tabs, path crumbs and drives, Alt-drop shortcuts, drag to and from Explorer, right-button drop choices, auto-scroll near list edges, and folders, tabs and drives that open on hover | Checked by the owner on a Windows PC. Linux external drops are Copy-only. |
| Rich ELF and MAP | Rich views, memory models, companion discovery and live module prefixes | Final review and Windows checks |
| Ctrl+K command center | Commands, recent files, tabs, folder and path sources, disk index, Search settings and the optional Windows service | Works on a Windows PC; the owner checks again with the leaner index service |
| Multi-rename | Journaled rename with safe undo and crash recovery; edited, imported, pasted and copied names; sorting, ordering and folder counters; optional rename log. Rename works on FAT and exFAT drives; undo there checks size and time first. | Checked by the owner on a Windows PC. The FAT and exFAT check is set aside for now. |

Windows CI is green: 7,118 tests passed. Linux passed all 10 gate steps.

Merged after that release: six built-in colour themes. They are Open sea (the default), Red bill, Black cap, Fine mesh, Grain, and Ink and amber. Each has a light and a dark form. Pick one in Settings > Appearance > Colour theme. It applies at once.

### Merged since that release

- **Navo.** The app has a new name and the `n_` icon. Your Navo profile (settings, tabs, history) moves to Navo once, safely, with crash recovery. The old folder is renamed to "Navo.migrated-to-navo". An installed Navo Index service is upgraded from a one-time notice.
- **Start-up splash.** A small native Windows window with the `n_` icon and the "navo" wordmark. It shows only when start-up takes longer than half a second, and fades out. It does not appear on Linux.
- **New right-click menu.** The real Windows shell menu in Navo's style. The filter sits at the top. Submenus open in place, with a back row. Shift adds the extended items. If anything fails, the classic Windows menu opens instead.
- **Fixes from the final reviews** of the Ctrl+K index service and of Rich ELF and MAP.

### Done in v0.3.0

- **Faster folders:** tabs share folder rows and sorted views; sorts and filters avoid copying every row.
- **Git badges skip unchanged repositories:** watches cover the open folder, its parents and direct children. Refresh and Git commands check again.
- **Leaner index:** about 97 MiB per two million entries in the synthetic fixture, with snapshot compatibility and measured rebuild limits. Real-volume VM memory measurements remain unavailable because the required test volumes and service are absent.

### Now in progress

Status on 5 November 2026:

- **In-app updates (built, VM test next):** a test feed for tester builds and a nine-case install and update test in the Windows VM.

- **Compare and sync (v1.0).** The engine is about a third done: big files are read once with steady memory, changes are found down to the word, content checks use every core, and Git repositories can be read at any revision. Next: the compare screen, then sync.
- **Installer.** Setup.exe ships with the next pre-release, next to the portable zip. Updates from inside the app follow.
- **Safer undo for moves.** Ctrl+Z after a move checks that each file is still the same one before it moves it back.

Done since 27 October: the fixes from the owner's testing (pane keys, self-saving workspaces, tab drag between panes, system defaults, settings badges), the user guide (Shift+F1), Navo in the tray, faster start, PDFs ready sooner, quiet when idle, and colours from the theme everywhere. Release builds no longer carry performance or trace tooling; that lives in a separate tester build.

Still open for alpha sign-off:

- Owner checks on a Windows PC: the leaner index service, the Rich ELF and MAP views, the profile move to Navo, and the splash. Explorer drag and multi-rename are checked; the FAT or exFAT USB stick check is set aside for now.
- A heavy benchmark: millions of indexed files, search latency, long lists, memory, and animation smoothness recorded on a Windows PC.
- Two weeks of daily use with no lost or damaged files.

The latest downloadable alpha is the pre-release alpha-2026-10-26d. It has all four alpha features, the colour themes, the splash and the new right-click menu. The Navo rename comes in the next build. DBC/A2L viewer screens, automotive colours and viewer chrome are beta work (D75).

The foundation already includes two panes, tabs, history, session restore, file operations, virtual lists, themes, the viewer, selection and the activity strip. The beta and later phases below retain their existing scope.

Exit: two weeks of daily use on a Windows PC with no lost or damaged files.

## v1 Beta

Goal: fast search, workspaces and tiled panes, safe deletes.

The owner reordered the beta on 27 October 2026 (D85). Built in this order:

1. core: search index like Everything: Ctrl+K answers at once from the Navo Index service, even while it updates; no folder walks on NTFS drives; the service starts from its saved index; less memory; a benchmark next to Everything. The busy-answer fix is done. Memory peaks are cut: loading the index and rebuilding it need far less extra memory (load 22 → 8 bytes per file, rebuilds sized from the old index, cleanup a third of before), one drive builds at a time, and network drives are indexed only when you list them, with a cap.
2. core: delete safety net (D65): Shift+Del and overwrites keep the old files 1–7 days, a "Recently deleted" view, Ctrl+Z restores. On by default since alpha-2026-10-28e; review and docs left.
3. core: workspaces (Alt+1…9) with the tray: named workspaces, drafts, Save and Discard, rename, move, delete with Ctrl+Z, and a status-bar button. Done.
4. core: up to four panes, tiled like Hyprland (D80): five layouts and your own on Ctrl+L, split, close, swap and resize from the keyboard, Shift+drag swap, divider drag, a strip in narrow windows. Done.
5. core: find files by content and attributes in Ctrl+K, with results as a tab you can work in; virtual collections (saved searches, snapshots, files picked by hand). Query, content matcher, collection store, results tab, Ctrl+K modes and Content mode (search inside files, Enter opens at the line) done; results tab look and hit lines next.
6. module: clipboard history in a Ctrl+K-style palette, as a module you can switch off. Built: it records text, links and file lists you copy, opens with Ctrl+Shift+V or Win+Alt+V from any app, pastes back, and can be paused, cleared or deleted. Saved items (snippets, paths), encrypted secrets and folder notes are built, with a manage view on F2. A pass on a real Windows desktop is done: the pop-up takes focus, secrets clear after 30 s and when Navo quits, and Delete all wipes the data. The summon hotkey and tray are done.
7. core: background work that never waits: job pools with priorities, folder sizes that cancel when you leave, size on disk, hard links counted once.
8. core: accessibility basics: Narrator reads lists, dialogs and Ctrl+K, focus visible everywhere, reduced motion, contrast themes, UI scale. Mostly done.

Done earlier in the beta: responsive layout and new fonts (D80), clone a tab into the other pane, open a terminal here, cache sizes in diagnostics, Follow on Windows, the DBC and A2L outline in F3, every shortcut changeable in Shortcuts (conflict checks, hints that show your keys, old keymaps carried over), a right-button setting for Total Commander or Windows behaviour, and your own commands like Total Commander's: a commands file names a program, its arguments (cursor file, marked files, both folders) and a key, and each command runs from the file list or Ctrl+K. Speed pass: large folders sort by size or date about 2.5× faster, many small files copy about 2× faster, and Ctrl+K searches once per 100 ms while a folder walk grows.

Exit: no open data-safety bugs; start-up and memory targets met in CI.

## v1.0

Replaces Total Commander for daily work (D39). Parity features are built to beat TC, not copy it.

1. core: compare and sync, better than Beyond Compare where it counts (D40): folder compare in the panes, a calm full-window look with a change map on each scroller (chosen), sync as a plan with undo, cached BLAKE3 hashes, syntax-aware text diff, paged diff for huge and binary files, image compare; engine in the shared `navo-compare` crate.
2. infra: signed portable `.exe` and the Velopack installer with in-app updates (D89, replaces the NSIS plan in D73), whose uninstaller also removes the index service; user guide; soak test; steadier Windows tests.
3. core: optional default file manager (Win+E).
4. core: inspector column with preview and properties, thumbnails, archives as folders (zip, 7z, tar with gz, xz and zst). File details, owner and hashes are done.
5. core: module registry with runtime on/off and safe mode; settings window generated from module schemas; notifications for long operations.

Exit: every target is met on a clean Windows 11 machine, and a week of work runs without opening Total Commander.

## v1.1 · See and find

- module: auto switch view.
- package: language packages for the F3 viewer (D38).
- core: checksums (BLAKE3, SHA-256, SHA-1, MD5) with verify; images in F3.
- package: Total Commander keymap.

## v1.2 · Git

- module: Git: status in the file list, history, diff and the usual commands. Built: status badges on repositories and a menu (click or Alt+G) that hands commit, log and diff to your Git tool, with a safe fast-forward pull and an unforced push; a pass on a real Windows desktop is left.

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

- module: duplicates and disk usage.
- package: import of Zed and VS Code colour themes.
- Native editing for each kind: a table edits like a spreadsheet; a tree edits keys and values in place.
- module: DBC and A2L in F3, the rest: find in the outline, the DBC and A2L inspectors, problems, the narrow layout and bookmarks.
