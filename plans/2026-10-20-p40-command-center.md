# P40 · Ctrl+K command center

Status: drafting (Q1–Q3 open) · Repo: `<repo>` · Branch: `p40-<atom>` per atom · Started: 2026-10-20

## Goal

Ctrl+K opens one box that finds commands and files. The user types a few letters, sees the matching commands with their shortcuts, the folders and files they use most, and every file on the disk, and runs or opens the chosen row with Enter. It works like Flow Launcher with Everything, inside Navo, and it never sends anything off the machine.

## What exists today (checked at ab89739)

- **No palette.** `app.command_palette` is a menu-only row (`commands/mod.rs` `MENU_ONLY`, `action: None`). `keymap.toml` binds it to `ctrl-k` under "App menu rows without an action yet; shown in the menu, not bound", so Ctrl+K does nothing. `run_menu_command` (`window_view/app_menu.rs:77-88`) ignores `MenuCommand::CommandPalette`. The app menu row reads "Command palette · Ctrl+K".
- **Title-bar search is a placeholder.** `title_bar/search.rs` draws a static field "Find anywhere: files, folders, commands" with a Ctrl+K key cap. It has no input and no click handler. D58 makes it hideable (`WindowParts::title_search`).
- **Command registry.** `commands::all()` yields about 300 `CommandSpec { name, label, context, action }` built from static groups (`window`, `marks`, `overlays`, `viewer`, `structured`) plus `MENU_ONLY`. Labels are Fluent ids (`cmd-…`, `navo-l10n/locales/en-US/commands.ftl`, 297 lines). `command_registry.snap` pins the list. Contexts: Viewer 61, FileList 47, Tree 24, NavoPane 20, PreviewBrowse 17, SettingsSheet 13, ContextMenu 11, global 10, and smaller overlay contexts. `contexts::is_within` resolves parents.
- **Dispatch by name exists.** The viewer context menu builds an action with `cx.build_action(name, None)` and runs `focus.dispatch_action(..)` inside `window.defer` (`window_view/viewer/menu/mod.rs:80-105`). The shell menu has `run_named` (`window_view/shell_menu/actions.rs:41-52`).
- **Shortcut text.** `commands::shown(key)` formats a keystroke ("Ctrl+Shift+F"); `commands::active(cx)` is the live keymap with user overrides.
- **Core commands with arguments.** `navo_core::Command` has a text parser and printer (`command/parse.rs`, `print.rs`), for example `PaneNavigate { location }`, `PaneOpenTab { location }`, `SelectionAddPattern { pattern, folders }`.
- **Opening like the pane.** `Browser::open_row` launches files through `Launcher` and navigates into folders; `Browser::open_at(folder, name)` navigates and puts the cursor on `name`; `Browser::open_tab_at` opens a tab. `NavoWindow::switch_pane` moves focus to the other pane.
- **User database.** `navo.db` migrations V1–V3 (`navo-store/src/db/migrations.rs`) already create `history (id, location, visited_at)` and `frecency (location PK, score, last_at)`. **Nothing reads or writes either table.** `search_history` is used by the viewer find bar. navo-app reaches the database through `SharedDb = Arc<Mutex<Db>>` and injects closures into navo-ui (`navo-app/src/files/memory.rs`, `FileHooks`).
- **Index crate is empty.** `crates/navo-index` has an empty `lib.rs` and no dependencies. No crate depends on `nucleo`, `nucleo-matcher` or `rayon` (`rayon` is only in `Cargo.lock` transitively).
- **File system pieces.** `navo_fs::Provider::list`, `metadata`, `watch` and `watch_tree` (recursive watcher, debounced batches). `measure/walk.rs` is a single-threaded walker private to folder sizing. The fake file system with fault injection and the hostile-name corpus exist for tests.
- **UI pieces.** `primitives::VirtualList` (uniform rows, scroll handle, row semantics), `primitives::TextField`, `primitives::Kbd`, the D46 context menu (filter list rows, quiet section headers, keys `↑/↓`, Enter, Esc clears then closes), `prompt_dialog`.
- **Activity model (P39 S2, merged).** `Activity::set_background(id, title, progress)` and `end_background(id)` exist with no producer. The strip view (P39 S3) is not built yet.
- **Decisions in force.** D6 and D61 (one command surface, public ids from the registry), D13 (Ctrl+K is core), D23 (own index: MFT + USN in a service, walk fallback), D58 (calm by default, keys shown), D59 (new text in `.ftl`), D60, D62, D63 (result-source interface, registry entries with an enabled flag, no memory gate).
- **Not found.** `docs/decisions.md` stops at D67. The brief's D68 ("Ctrl+K is part of the alpha") is not recorded yet. `docs/roadmap.md` still lists the < 80 MB RAM target and puts global search in v1.1.

## Decisions

Final once the owner answers Q1–Q3. Workers never reopen them; a worker that finds one wrong stops and reports.

### Shape and parts

- **D1 — Three parts that ship separately.** Each part ends with `cargo xtask gate`, Windows CI, an Opus high review, a merge and an alpha build.
  - Part A: commands only. Ctrl+K works as a command palette.
  - Part B: files and folders from sources that exist today (history, open tabs, a live walk of the current folder), path mode and commands with arguments.
  - Part C: the disk index (NTFS MFT and USN journal through a service; walk index as fallback).
  - The plan has 34 atoms. Under the about-15-atoms rule, the orchestrator may save the parts as `p40a-commands`, `p40b-file-sources` and `p40c-disk-index`, with these decisions shared.
- **D2 — Names.** The public id stays `app.command_palette` (user keymaps use it). The label becomes "Command center" in `commands.ftl` and in the app menu row. Code lives in `crates/navo-ui/src/command_center/`. The GPUI key context is `CommandCenter`.

### UI

- **D3 — Where it opens: a floating panel anchored to the title-bar search.**
  - The panel is centred on the title-bar search field. Its input row sits exactly over the field, so the field appears to grow into the box. Width 640 px, clamped to the window width minus 32 px. Height grows with the rows, up to 12 rows, then the list scrolls.
  - With the title-bar search hidden (D58), the panel opens at the same place, top centre under the title bar.
  - Ctrl+K, the app-menu row and a click on the title-bar field all open it. Ctrl+K while open closes it.
  - It does not open over a modal dialog (confirm, prompt). From a viewer it opens and offers the viewer's commands.
  - Trade-off: a real editable field inside the title bar would fight the Windows caption hit-testing and drag areas (D36). An overlay that covers the field looks the same and stays one component.
  - It is a drawer-like overlay over the content (D58 rule 2). Nothing behind it moves or shrinks. A scrim is not used; a click outside closes it.
- **D4 — Rows and groups.**
  - With a query, results show as: one **Top hit** row (the single best result across all sources), then the groups **Commands**, **Folders**, **Files** in that fixed order, with quiet section headers like D46. The top hit is not repeated in its group. Empty groups are omitted.
  - Caps in mixed mode: Commands 5, Folders 5, Files 8. A group with more matches ends with a "Show all N" row; Enter on it types that group's prefix (D6).
  - In a scoped mode (a prefix), one group shows up to 500 rows in a `VirtualList`.
  - Later groups: **Modules**, one per source a module adds (D16), placed after Files.
  - Trade-off: a fixed group order keeps positions predictable for the keyboard; the top hit still puts the best answer first.
- **D5 — A row.** Row height 32 px. Left: a Lucide icon (command, folder, file-type icon). Then the title with matched characters in the accent colour and medium weight. Then muted detail text: for a command, its context in plain words ("File list", "Viewer"); for a folder or file, the parent path with a middle ellipsis. Right: for a command, its shortcut as `Kbd` caps from the active keymap (first key bound in the matching context); for a file or folder, nothing. A command with an argument ends its title with "…".
- **D6 — Prefixes.** A prefix is the first character of the input and stays visible and editable.
  - `>` commands only (VS Code habit).
  - `/` files and folders only.
  - Path mode starts by itself when the input starts with a drive letter and colon (`C:`), `\\`, or `~`. On Linux an absolute path typed after `/` works in files mode, because a term with a separator matches the full path (D20), and an existing folder gets a "Go to" row.
  - No other prefixes in this plan. A hint line under an empty input lists them: "> commands · / files · C:\ paths".
  - Trade-off: `/` cannot start a Linux path in mixed mode; Linux is secondary (D30), and files mode covers it.
- **D7 — Keys inside the box.** All live in the `CommandCenter` context, are rebindable in `keymap.toml` and get `cmd-` labels.

  | Key | Action |
  | --- | --- |
  | ↑ / ↓, PgUp / PgDn | Move the selection |
  | Ctrl+Home / Ctrl+End | First / last row (Home and End stay in the input) |
  | Tab / Shift+Tab | Next / previous group; in path mode, complete the selected folder name |
  | Enter | Run the command, or open the item like Enter in the pane: a folder opens in the active pane, a file opens with its program |
  | Shift+Enter | Reveal: open the parent in the active pane with the cursor on the item |
  | Ctrl+Enter | The same as Enter or Shift+Enter, but in the other pane (folder opens there; a file is revealed there) |
  | Alt+Enter | Open the folder (or the file's parent, cursor on the file) in a new tab (Q3) |
  | Ctrl+Shift+C | Copy the full path of the selected file or folder |
  | Esc | Clear the query; on an empty query, leave the argument step or close |

  - A single left click runs the row like Enter. Hover only moves the highlight. **Nothing runs on hover.**
  - A footer line shows the keys for the selected row kind ("Enter open · Shift+Enter reveal · Ctrl+Enter other pane · Alt+Enter new tab"). It follows the window's key-hints part (D58 rule 7).
  - Closing returns focus to the element that had it before.
- **D8 — Empty state.** With an empty input: **Recent** shows the last 5 commands run from the box and the 8 most frecent folders, then the prefix hint line. With no history yet it shows only the hint line. No tips, no banners (D58).
- **D9 — Motion.** The panel fades in and drops 4 px in 120 ms and leaves in 90 ms. Reduced motion: fade only. Rows never animate while typing. No flashing.

### Commands

- **D10 — The registry is the source; entries become owned (D63).** `commands::all()` keeps its signature for existing callers. A new `commands::entries()` returns `Vec<CommandEntry { spec: CommandSpec, enabled: bool, in_palette: bool, arg: Option<ArgKind> }>`. `enabled` is true for every built-in entry today; modules set it later. `MENU_ONLY` shrinks: `app.command_palette` gets the action `command_center::Toggle` in the global group.
- **D11 — Which commands show.**
  - A command shows when its context is `global` or is in the focused element's key-context stack at the moment the box opened. So viewer commands show only when a viewer had focus, and overlay commands (dialogs, menus, settings fields) never show.
  - `in_palette` is false for movement and editing primitives: cursor and page moves, caret moves, text-field keys, the menu and dialog navigation keys, and `app.command_palette` itself. The list is one const in `commands/palette_hidden.rs`, pinned by a snapshot.
  - `enabled == false` entries never show.
- **D12 — Commands run through the same dispatch as keys.** The box closes, then `window.defer` focuses the element that had focus before and calls `focus.dispatch_action(action)`, as the viewer menu does today. The command sees the same state as when pressed by key. Destructive commands keep their confirmations, because they are the same actions (F8 asks per settings; Shift+Del shows its warning).
- **D13 — Commands with an argument.** `ArgKind::Path` or `ArgKind::Text`. Choosing one turns the input into an argument step: a chip with the command label, an empty input and a placeholder. Enter builds the core text form and runs it through `navo_core::Command::parse` and `Browser::command` on the active pane (D61). The first set:

  | Entry | Text form | Arg |
  | --- | --- | --- |
  | Go to folder… | `pane.navigate location=<path>` | Path, with path-mode completion |
  | Open folder in new tab… | `pane.open_tab location=<path>` | Path |
  | Mark by pattern… | `selection.add_pattern pattern=<text> folders=false` | Text |
  | Unmark by pattern… | `selection.remove_pattern pattern=<text> folders=false` | Text |

  A parse error shows under the input in the error colour; the box stays open.
- **D14 — Command use is remembered.** Each command run from the box adds to its frecency (D18). Keys pressed outside the box do not count; the box learns what the user looks up, not what they already know by key.

### Ranking

- **D15 — Matcher.** `nucleo-matcher` (the low-level crate of `nucleo`), smart case, Unicode normalisation on, path-aware scoring for paths. It runs inside `navo-index::fuzzy`. The high-level `nucleo` crate is not used: it keeps its own UTF-32 copy of every item (about 4 bytes per character), which doubles the memory of a 2-million-name index. Large tables are scored in parallel chunks with `rayon` (D18 of the architecture: rayon for CPU work).
- **D16 — Result-source interface (D63).** In `command_center/source.rs`:
  - `trait ResultSource: Send + Sync { fn id(&self) -> &'static str; fn group(&self) -> Group; fn scopes(&self) -> Scopes; fn search(&self, request: &Request, sink: &HitSink, cancel: &Cancel); }`
  - `Request { query: Query, generation: u64, here: Option<Location>, limit: usize }`. `Query` is the parsed input (scope, terms, path mode).
  - `Hit { source: &'static str, group: Group, title: SharedString, detail: SharedString, score: i64, indices: Vec<u32>, target: Target, shortcut: Option<SharedString> }`.
  - `Target` is closed: `Action(&'static str)`, `Core(Command)` (an argument command after its step), `Arg(&'static str, ArgKind)`, `Open { location: Location, folder: bool }`, `GoTo(Location)`, `ShowAll(Group)`.
  - A module adds a source by registering a `Box<dyn ResultSource>` at start-up through an internal Rust interface (D62). Its hits must use one of these targets, so a module never runs its own code from a row; it runs a registry command or opens a location. Module results get their own group named by the source.
  - Every source runs on the background executor. A source that needs UI state (commands, open tabs) gets a snapshot taken when the box opens. Each keystroke bumps the generation and cancels older searches. Commands search on every keystroke; file sources start after 40 ms without typing.
- **D17 — Score.** `final = match + 12·ln(1 + frecency) + bonus`, where `match` is the nucleo score, `frecency` is the aged weight (D18) and `bonus` is +30 when the name equals the query (case-insensitive) and +15 when the name starts with it. Ties keep source order, then shorter paths. The constants live in one block in `command_center/rank.rs` with tests; tuning them is a later one-line change.
- **D18 — Frecency (zoxide rule).** A use adds 1 to the score and sets `last_at`. The weight at query time is `score × 4` if used in the last hour, `× 2` in the last day, `× 0.5` in the last week, else `× 0.25`. When the sum of all scores in a table passes 10,000, every score is multiplied by 0.9 × 10,000 / sum (zoxide's exact form, so the sum lands at 9,000) and rows below 1 are deleted. A3 multiplies by a flat 0.9, which is the same while each use adds 1; switch to the exact form if anything ever adds more than 1 at once. Folders and files use `frecency`; commands use a new `command_use` table. A folder visit counts when its listing loads (from keys, the path bar or the box), at most once per folder per 2 s. A file counts when it is opened with Enter (pane or box).

### File search engine

- **D19 — Phase 1 (Part B): what exists, no persistent index.**
  - **Recent** source: the top 2,000 rows of `frecency` (folders and files), loaded when the box opens.
  - **Tabs** source: the location of every tab in every pane and window.
  - **Here** source: a live walk of the active pane's folder tree, started on the first file query and reused while the folder is the same and the walk is under 60 s old. It stops at 300,000 entries; past that, the Files header says "First 300,000 entries of C:\ · the disk index finds the rest". Results stream in while the walk runs.
  - **Paths** source: path mode (D6) lists the child folders of the typed parent through the provider, with the network timeout rules of D19 (offline shows a note, never blocks).
  - Not used: the listing cache. It holds only folders visited lately, so its results would look random. Trade-off accepted. Recent and Tabs show a place once: the Tabs row wins, with the higher score.
- **D20 — Query terms.** Space-separated terms must all match. A term without a separator matches the name. A term with `\` or `/` matches the full path. `ext:rs,toml` filters by extension. `size:`, `dm:` and `regex:` come after the alpha.
- **D21 — Name table, shared by the walk and the index.** `navo-index::table::NameTable`: one UTF-8 name buffer, then per entry a `u32` name offset, a `u32` parent id and a `u16` flags field (folder, hidden, system, reparse point, deleted). About 10 bytes plus the name per entry. Paths are rebuilt from the parent chain only for shown rows. Size and dates are not stored (D24).
- **D22 — Phase 2 (Part C): the disk index.**
  - **NTFS through a service.** A Windows service reads each NTFS volume with `FSCTL_ENUM_USN_DATA` (names, parents, attributes) and then follows the USN journal with `FSCTL_READ_USN_JOURNAL`. It saves the journal id and the next USN per volume, so a restart only catches up; a changed journal id or a wrapped journal starts a new enumeration.
  - **The service is a mode of `navo.exe`** (`navo.exe --index-service`), like the PDF helper (D48). This amends D23's separate `navo-indexd` binary: one portable exe, one version, one signature. The service is named "Navo Index", runs as LocalSystem, starts automatic (delayed), and restarts on failure.
  - **Install** through the commands "Install index service" and "Remove index service" (in the box and in the settings page). Each starts `navo.exe --index-service install|remove` elevated (one UAC prompt). Nothing installs without the user's click.
  - **Talk** over a named pipe `\\.\pipe\navo-index`, whose DACL allows only SYSTEM, Administrators and the SID of the user who installed it. Frames are `postcard`: `Query { request, generation }` → `Hits` (top 500 with full paths and flags) and `Status` (volumes, entries, progress, errors). A newer generation on the same connection cancels the running search in the service, so a keystroke never waits for an older query. The service never opens a file and holds only read-only volume handles.
  - **Fallback walk index** (no service, non-NTFS, network, Linux): the same `NameTable` built by the walker for chosen roots, saved to disk, refreshed by a low-priority rescan after each start, and kept current by `watch_tree` while Navo runs.
  - Trade-off: a per-session elevated helper would need a UAC prompt on every start; running Navo elevated would break drag and drop with Explorer (UIPI). A service matches what the owner already runs for Everything at work (D23).
  - **D22 amendment (C7 security review).** Install copies only `navo.exe` to `%ProgramFiles%\Navo\Index\navo.exe` through a temporary file and registers that quoted path. Both directories use a protected DACL: Administrators owns them; SYSTEM, Administrators and TrustedInstaller have full access; Users have read and execute. Handles pin the directories throughout installation. A directory that is not trusted is moved aside and created fresh with its protected DACL under a verified parent, never repaired in place; reparse points and non-directories are refused with the path. Only install repairs; the service and remove only verify. Reinstall copies to the temporary file, renames the running `navo.exe` to `navo.exe.<nonce>.old`, moves the new copy in, then stops the service, updates its registration and starts it; on any error it puts the old image back and starts the service again. `.old` files are deleted after start or on the next install. Remove stops and deletes the service, deletes the copy and the Index directory if empty, then deletes `%ProgramData%\Navo\index` with its files and the Navo folder if empty. Recovery restarts after 5 seconds, then 60 seconds, then takes no action; the count resets after one day. Configuration and storage verification errors report a clean stopped state with a service-specific exit code; only unexpected faults restart. A pipe bind failure keeps the service running, records the reason in status errors and retries after 1 s, 5 s, 30 s, then every 60 s until stop. The account lookup accepts only SidTypeUser.
- **D23 — Storage.** One file per root or volume: a versioned little-endian header (format version, volume serial, journal id, next USN, root, built-at) followed by the name buffer and the entry arrays. Written with `navo_store::atomic_write`; a bad or old file is deleted and rebuilt. Loaded fully into memory: no `rkyv`, no memory map, so no `unsafe` in the loader. Walk index files live in `%LOCALAPPDATA%\Navo\index\` (portable mode: beside the exe). Service files live in `%ProgramData%\Navo\index\` with an ACL of SYSTEM and Administrators only.
- **D24 — Size and dates.** `FSCTL_ENUM_USN_DATA` gives no size or times. Shown rows fetch them lazily with one metadata call each, off the UI thread. Sorting or filtering by size and date waits for raw `$MFT` reading after the alpha; evaluate the `mft` crate (0.7.0, MIT/Apache, quality not reviewed) against own code then.
- **D25 — Budgets (targets, not gates; D63).**
  - Memory: ≤ 50 bytes per indexed entry in the process that holds the table (≈ 100 MB for 2 million entries). With the service, Navo itself grows by ≤ 15 MB for the box. Measured by `NameTable::memory_bytes` in a test with 2 million synthetic names and recorded in `docs/perf.md`.
  - Speed: commands in < 5 ms; first file results in < 50 ms on 1 million entries on the work laptop; enumeration of a 1-million-entry NTFS volume in < 10 s.
  - Start-up: nothing before the first usable frame. After it, a background task connects to the service pipe (50 ms timeout) or loads the walk snapshots. No UI-thread step above 1 ms.
  - Freshness: USN changes visible within 2 s; watcher changes within 2 s; changes made while Navo was closed (walk index) appear after the next rescan.
- **D26 — What is indexed.** Setting `[search]`:
  - `index = "auto" | "service" | "walk" | "off"`: auto uses the service when it answers, else the walk index.
  - `drives = []`: empty means every local fixed drive for the service; see Q2 for the walk default.
  - `exclude = ["$Recycle.Bin", "System Volume Information"]`: names or full paths skipped while indexing.
  - `network = false`, `removable = false`.
  - `remember = true`: records history and frecency. "Clear search history" empties `history`, `frecency` and `command_use`.
  - Hidden and system entries are indexed but shown only when the window shows them (D58).
  - Linux: walk index only, default root the home folder, no watcher on the whole tree.
- **D27 — While the index builds.** Results come from the partial table at once. The Files header shows "Indexing C: 41 %". The activity strip shows a Background item with the same text and progress (P39), and drops it when done. A failure (service stopped, volume refused) shows once in the strip and in the settings page, never as a dialog.
- **D27 amendment (orchestrator, 2026-10-21; alpha).** The walk host serves only its last complete snapshot; no partial tables and no index rows before the first snapshot exists. The existing note shows “Indexing C: 12,345 files so far” from host walk counts or service status. Progress is indeterminate, with no percentage.
- **D28 — Privacy.** Nothing leaves the machine: no network calls, no telemetry. Queries and file names are never logged (D20 canary list already includes search queries); the diagnostic report counts entries only. The pipe is local and ACL-restricted. On a shared machine the service answers only the installing user; it shows names the user may lack rights to list, as Everything does by default; access filtering comes later.
- **D28 amendment (C15).** The service applies the exclusion and drive settings copied into service.json at install time before selecting its top 500 results; hidden and system visibility stays on the client, with no pipe protocol change.

### Safety

- **D29 — Enter on a file opens it exactly like Enter in the pane** (the same `Launcher`). A folder navigates. Executables get no extra prompt, as in the pane.
- **D30 — Destructive commands keep their confirmations** (D12). The box never offers a file operation of its own; file operations stay registry commands that go through the planner and journal (D61).
- **D31 — No command runs on hover, on selection change or on a timer.** Only Enter, a click, or the argument step's Enter runs anything.

### Text

- **D32 — All new text goes into `.ftl` files** (D59): labels in `commands.ftl`, box texts in `app.ftl`, index status in `files.ftl`.

## Components

| Component | Layer | Contract (in → out) | New / change | Test |
| --- | --- | --- | --- | --- |
| `navo-index::fuzzy` | lib | `(&Query, &str) → Option<Match { score, indices }>` | new | `navo-index/src/fuzzy/tests.rs` |
| `navo-index::table` | lib | `NameTable`: add, remove, rename, path(id), memory_bytes | new | `table/tests.rs` |
| `navo-index::search` | lib | `(&NameTable, &Query, limit, &Cancel) → Vec<TableHit>` | new | `search/tests.rs` |
| `navo-index::walker` | adapter | `(&dyn Provider, root, Limits, &Cancel, progress) → NameTable` | new | `walker/tests.rs` (fake fs) |
| `navo-index::file` | adapter | `NameTable + Header ↔ file` | new | `file/tests.rs` |
| `navo-index::host` | service | roots → live tables, status events | new | `host/tests.rs` |
| `navo-index::ntfs` | adapter (Windows) | volume → `NameTable`, USN batches | new | `ntfs/tests.rs` (Windows CI), `usn` pure tests |
| `navo-index::usn` | lib | `UsnRecord → TableChange` | new | `usn/tests.rs` |
| `navo-index::pipe` | adapter | `postcard` frames over a named pipe; client and server | new | `pipe/tests.rs` |
| `navo-app::index_service` | service | `--index-service run|install|remove` | new | `navo-app/src/index_service/tests.rs` |
| `navo-store::db` frecency | adapter | V4 migration; `record_visit`, `frecent`, `record_command`, `command_use`, `clear_search_history` | change | `db/tests/frecency.rs` |
| `navo-store::settings` `[search]` | adapter | settings keys D26 | change | `settings/tests` |
| `commands` entries | lib (ui) | `entries() → Vec<CommandEntry>`; `palette_hidden` | change | `commands/tests.rs`, snapshots |
| `command_center::query` | lib (ui) | `&str → Query` (scope, terms, path mode) | new | `command_center/tests/query.rs` |
| `command_center::source` | lib (ui) | `ResultSource`, `Request`, `Hit`, `Target`, `HitSink` | new | `command_center/tests/source.rs` |
| `command_center::results` | lib (ui) | hits → rows (top hit, groups, caps, selection) | new | `command_center/tests/results.rs` |
| `command_center::rank` | lib (ui) | `(match, frecency, name, query) → i64` | new | `command_center/tests/rank.rs` |
| `command_center::commands_source` | service (ui) | snapshot of entries → hits | new | `command_center/tests/commands_source.rs` |
| `command_center::file_sources` | service (ui) | Recent, Tabs, Here, Paths → hits | new | `command_center/tests/file_sources.rs` |
| `command_center::index_source` | service (ui) | pipe or host → hits | new | `command_center/tests/index_source.rs` |
| `command_center::view` | ui | `CommandCenter` element: props in, `CommandCenterEvent` out | new | `command_center/tests/view.rs`, gallery shot |
| `window_view::command_center` | ui container | open, close, focus, run hits | new | `window_view/tests/command_center.rs` |
| `navo-app::search_hooks` | wiring | DB-backed closures for frecency and command use; source registration | new | `navo-app/src/search_hooks/tests.rs` |
| `settings` Search page | ui | `[search]` rows, service status and install | new | `settings/tests` |

## Atoms

Roles follow the owner's current rules: Sonnet 5.5 (medium or high) for decided backend and UI work; Opus 5.5 medium for senior or cross-cutting UI; Opus 5.5 high for data-safety work and reviews. No Codex or Gemini runs (D35). Claude models never above high.

| Atom | Component | Role | Effort | After | Check | Status | Cost |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **Part A — commands** | | | | | | | |
| A0 | box look, 3–4 directions | designer (Opus) | medium | — | prototype file opens; owner picks | done | owner: direction 1 + top hit row of 3 |
| A1 | `navo-index::fuzzy` | api-builder (Sonnet) | medium | — | `cargo nextest run -p navo-index fuzzy` | done | |
| A2 | `commands` entries, hidden list, Toggle action | ui-builder (Sonnet) | medium | — | `cargo nextest run -p navo-ui commands` | done | |
| A3 | `navo-store::db` V4, frecency and command use | api-builder (Opus) | high | — | `cargo nextest run -p navo-store db` | done | |
| A4 | `command_center::query` + `source` contract | api-builder (Sonnet) | high | — | `cargo nextest run -p navo-ui command_center::tests::query command_center::tests::source` | done | |
| A5 | `command_center::rank` + `results` | api-builder (Sonnet) | high | A4 | `cargo nextest run -p navo-ui command_center::tests::rank command_center::tests::results` | done | |
| A6 | `command_center::commands_source` | ui-builder (Sonnet) | medium | A1, A2, A4 | `cargo nextest run -p navo-ui command_center::tests::commands_source` | done | |
| A7 | `command_center::view` + gallery page | ui-builder (Opus) | medium | A0, A5 | `cargo nextest run -p navo-ui command_center::tests::view`; gallery shots light and dark | done | |
| A8 | `window_view::command_center` container | ui-builder (Opus) | medium | A6, A7 | `cargo nextest run -p navo-ui window_view::tests::command_center` | done | |
| A9 | `navo-app::search_hooks` (command use) | api-builder (Sonnet) | medium | A3, A8 | `cargo nextest run -p navo-app search_hooks` | done | |
| A10 | review Part A | reviewer (Opus) | high | A9 | review notes; `cargo xtask gate`; Windows CI | done | |
| **Part B — file sources** | | | | | | | |
| B1 | visit and open hooks, frecency writer | ui-builder (Sonnet) | medium | A9 | `cargo nextest run -p navo-ui window_view::tests::visits`; `-p navo-app search_hooks` | done | |
| B2 | `navo-index::table` | api-builder (Sonnet) | high | — | `cargo nextest run -p navo-index table` | done | |
| B3 | `navo-index::search` | api-builder (Sol 6.1) | xhigh | A1, B2 | `cargo nextest run -p navo-index search` | done | |
| B4 | `navo-index::walker` | api-builder (Sol 6.1) | xhigh | B2 | `cargo nextest run -p navo-index walker` | done | |
| B5 | Recent and Tabs sources | ui-builder (Sonnet) | medium | A5, B1 | `cargo nextest run -p navo-ui command_center::tests::file_sources::recent` | done | |
| B6 | Here source | ui-builder (Sonnet) | high | B3, B4, B5 | `cargo nextest run -p navo-ui command_center::tests::file_sources::here` | done | |
| B7 | Paths source (path mode) | ui-builder (Sonnet) | medium | B5 | `cargo nextest run -p navo-ui command_center::tests::file_sources::paths` | done | |
| B8 | argument step | ui-builder (Opus) | medium | B7, A10 | `cargo nextest run -p navo-ui window_view::tests::command_center::args` | done | |
| B9 | open, reveal, other pane, new tab, copy path | ui-builder (Opus) | medium | B5, A10 | `cargo nextest run -p navo-ui window_view::tests::command_center::open browser::open` | done | |
| B10 | review Part B | reviewer (Opus) | high | B6, B8, B9 | review notes; gate; Windows CI | done (fixes in B11, B12) | |
| B11 | thread caps for Paths, Here and look-up | ui-builder (Opus) | medium | B10 | `cargo nextest run -p navo-ui file_sources look_up` | done | |
| B12 | Enter, Esc and location-key fixes | ui-builder (Sonnet) | medium | B8, B9 | `cargo nextest run -p navo-ui window_view::tests::command_center` | done | |
| **Part C — disk index** | | | | | | | |
| C0 | Windows facts for the index | researcher (Sonnet) | medium | — | answers with sources, in the Log | done | |
| C1 | `navo-index::file` | api-builder (Sol 6.1) | xhigh | B2 | `cargo nextest run -p navo-index file` | done | |
| C2 | `[search]` settings keys | api-builder (Sonnet) | medium | — | `cargo nextest run -p navo-store settings` | done | |
| C3 | `navo-index::host` (walk index) | api-builder (Sonnet) | high | B4, C1, C2 | `cargo nextest run -p navo-index host` | done | |
| C4 | `navo-index::usn` | api-builder (Sonnet) | high | B2, C0 | `cargo nextest run -p navo-index usn` | done | |
| C5 | `navo-index::ntfs` | api-builder (Sonnet) | high | C4 | Windows CI `test-windows` (`cargo nextest run -p navo-index ntfs`) | built, waits for Windows CI | |
| C5b | NTFS index save and restore | api-builder (Sol 6.1) | high | C5 | `cargo nextest run -p navo-index file ntfs` | done | |
| C6 | `navo-index::pipe` | api-builder (Opus) | high | C0, B3 | `cargo nextest run -p navo-index pipe`; Windows CI | done | |
| C7 | `navo-app::index_service` | api-builder (Opus) | high | C5, C5b, C6, C1 | `cargo nextest run -p navo-app index_service`; Windows CI; owner check | done (owner check after release) | |
| C8 | `command_center::index_source` | ui-builder (Sonnet) | high | C3, C6, B10 | `cargo nextest run -p navo-ui command_center::tests::index_source` | done | |
| C9 | Search settings page | ui-builder (Sonnet) | medium | C2, C7 | `cargo nextest run -p navo-ui settings`; gallery shot | built; service hook wired with C7 | |
| C9b | service status, Install and Remove wired | api-builder (Sol 6.1) | high | C7, C9 | Windows CI | done | |
| C10 | index activity in the strip | ui-builder (Sonnet) | medium | C8, P39 S3 | `cargo nextest run -p navo-ui window_view::tests::activity` | done | |
| C11 | docs: architecture, perf, roadmap | patcher (Sonnet) | medium | C10 | `cargo xtask gate` (docs links) | blocked | |
| C12 | review Part C | reviewer (Opus) | high | C9, C10, C11 | review notes; gate; Windows CI; owner check list | reviewed (do not ship); fixes C13–C15; re-review next | |
| C14 | pipe and saved-index hardening | api-builder | high | C5b, C6, C7 | `cargo xtask check -p navo-index` | done | |
| C13 | service install, reinstall and repair hardening | api-builder | high | C7 | `cargo xtask check -p navo-app`; Windows CI | done (Windows CI pending) | |
| C15 | service runtime and Ctrl+K review fixes | api-builder (Codex) | high | C7, C8, C9b, C10 | counting status hook; exclusion and fallback regressions; blocked retirement; app, UI and index check | done | |
| C16 | service install trust and rollback fixes | worker (Sonnet) | medium | C15 | stock ProgramData parent ACL, service.json backup, fresh-install cleanup, was-running restart, restore retry; app check | done | |
| C17 | index restore, exclusion speed, coverage and accept fixes | worker (Sonnet) | medium | C15 | restore with a removed folder; per-candidate exclusion; error clearing; service-status grace; accept backoff; index and app check | done | |

### A0 — box look

- **Read:** this plan's D3–D9; `design/prototypes/` (the D46 context menu and D67 activity strip prototypes for the house style); `crates/navo-ui/src/title_bar/search.rs`; `crates/navo-ui/src/tokens/`.
- **Change:** one HTML prototype with 3–4 directions behind one picker with neutral labels. Every direction keeps D3 placement, D4 groups, D5 row content, D7 keys and D8 empty state; they differ in density, header style, match highlight, footer and the argument chip. Show light and dark at 1100 px and 1600 px window widths (a desktop app; no phone width). Show states: empty, mixed query, `>` scoped, path mode, argument step, indexing note.
- **Check:** the file opens; each direction shows every state. The orchestrator publishes it as an artifact and asks the owner.
- **Stop if:** a direction needs a decision outside D3–D9.

### A1 — fuzzy matcher

- **Read:** `crates/navo-index/Cargo.toml`, `crates/navo-index/src/lib.rs`, the workspace `Cargo.toml` lints.
- **Change:** add `nucleo-matcher` (latest stable, pinned) to `navo-index`. `pub mod fuzzy` with `pub struct Fuzzy` holding a `nucleo_matcher::Matcher` and a parsed pattern; `Fuzzy::new(query: &str, paths: bool)`; `fn score(&mut self, haystack: &str) -> Option<Match>`, where `Match { score: u32, indices: Vec<u32> }`. Smart case, normalisation on. Path config when `paths`.
- **Check:** `cargo nextest run -p navo-index fuzzy`. Tests: subsequence match, no match, smart case, indices on a multi-byte name, path config prefers the last segment, empty query matches everything with score 0.
- **Stop if:** `nucleo-matcher` needs `unsafe` in our code or conflicts with a pinned dependency.

### A2 — registry entries

- **Read:** `crates/navo-ui/src/commands/mod.rs`, `window.rs:1-30`, `tests.rs`, `snapshots/command_registry.snap`, `keymap.toml:1336-1352`, `crates/navo-ui/src/window_view/app_menu.rs:60-104`, `crates/navo-l10n/locales/en-US/commands.ftl` (`cmd-app-command-palette`).
- **Change:**
  - `CommandEntry`, `ArgKind` and `entries()` per D10; `palette_hidden.rs` with the D11 list.
  - A GPUI action `command_center::Toggle`; move `app.command_palette` from `MENU_ONLY` into the global group with it. Remove it from the "not bound" comment block in `keymap.toml`.
  - Label "Command center" in `commands.ftl` and the app menu row label.
  - Update the registry snapshot and add a `palette_entries.snap` (name, context, in_palette).
- **Check:** `cargo nextest run -p navo-ui commands app_menu`; both snapshots reviewed; `cargo xtask gate` l10n step passes.
- **Stop if:** another module relies on `app.command_palette` having no action (`modifier_hints` tests at `modifier_hints/tests.rs:392`, `window_view/tests/hints.rs:250`): update those asserts only if they test "unbound", else report.

### A3 — user database V4

- **Read:** `crates/navo-store/src/db/migrations.rs`, `db/mod.rs`, `db/viewer.rs` (style), `db/tests/`, `crates/navo-app/src/crash/test_damage.rs`.
- **Change:**
  - Migration V4: `ALTER TABLE frecency ADD COLUMN is_folder INTEGER NOT NULL DEFAULT 1;` and `CREATE TABLE command_use (name TEXT PRIMARY KEY, score REAL NOT NULL, last_at INTEGER NOT NULL);`.
  - `db/frecency.rs`: `record_visit(location, is_folder, now_ms)` (also inserts into `history`, capped at 10,000 rows), `frecent(limit, now_ms) -> Vec<Frecent { location, is_folder, weight }>`, `record_command(name, now_ms)`, `command_use(limit, now_ms) -> Vec<(String, f64)>`, `clear_search_history()`. Aging and pruning per D18, in one transaction.
- **Check:** `cargo nextest run -p navo-store db`. Tests: V3 → V4 on a database with rows in every table keeps them; weights per age band; the 10,000 decay; history cap; clear empties three tables only; a crash between statements leaves V3 or V4, never half.
- **Stop if:** `PRAGMA user_version` handling or the P35 quarantine code needs a change.

### A4 — query and source contract

- **Read:** this plan's D6, D16, D20; `crates/navo-core/src/location/` (public API), `crates/navo-core/src/command/mod.rs:1-60`.
- **Change:** `command_center/mod.rs`, `query.rs`, `source.rs` with the types of D16. `Query::parse(&str) -> Query { scope: Scope, terms: Vec<Term>, path: Option<String> }`; `Term::Name`, `Term::Path`, `Term::Ext(Vec<String>)`. `HitSink` is a `Send` collector keyed by generation that drops stale hits. `Cancel` wraps an `AtomicBool`.
- **Check:** `cargo nextest run -p navo-ui command_center::tests::query command_center::tests::source`. Tests: each prefix, path mode for `C:`, `\\srv`, `~`, Linux `/x/y` in files mode, `ext:` lists, stale generation dropped.
- **Stop if:** `Location` cannot be built from text without a provider.

### A5 — rank and results

- **Read:** D4, D8, D17, D18; `command_center/source.rs` from A4.
- **Change:** `rank.rs` with the D17 formula and constants; `results.rs`: `Results::merge(hits, scope) -> Vec<Row>` with top hit, fixed group order, caps, "Show all N" rows, the empty-state rows, and `Selection` moves (↑/↓, page, first/last, next/previous group).
- **Check:** `cargo nextest run -p navo-ui command_center::tests::rank command_center::tests::results`. Tests: exact and prefix bonus, frecency lifts a weaker match, top hit not repeated, caps, scoped mode up to 500, Tab across groups skips headers.
- **Stop if:** a test needs a GPUI context.

### A6 — commands source

- **Read:** `commands/mod.rs` (after A2), `commands/keymap/shown.rs`, `commands/contexts.rs`, `command_center/source.rs`.
- **Change:** `CommandsSnapshot::take(cx, context_stack) -> CommandsSnapshot` on the UI thread (visible entries per D11, label from Fluent, first shortcut text, context in plain words, command-use weights passed in). `CommandsSource` scores labels and names with `Fuzzy`.
- **Check:** `cargo nextest run -p navo-ui command_center::tests::commands_source`. Tests: viewer commands hidden from a file-list stack, shown from a viewer stack; hidden list respected; disabled entries hidden; a user keymap override changes the shortcut text; the box's own toggle never shows.
- **Stop if:** GPUI exposes no way to read the focused key-context stack; report the API found instead.

### A7 — the view

- **Read:** A0's chosen direction; `context_menu/row.rs`, `context_menu/panel.rs` (row and header style), `primitives/virtual_list.rs`, `primitives/text_field.rs`, `primitives/kbd.rs`, `title_bar/search.rs`, `command_center/results.rs`.
- **Change:** `command_center/view.rs` and `view/row.rs`: `CommandCenter` element per D3–D9. Props: rows, selection, query, argument chip, footer kind, status note, anchor rect. Events: `QueryChanged`, `Moved`, `Run(row, Modifiers)`, `Dismissed`. No state, no I/O. A gallery page with every A0 state.
- **Check:** `cargo nextest run -p navo-ui command_center::tests::view`; gallery shots at 1100 and 1600 px in light and dark, looked at.
- **Stop if:** the anchor rect of the title-bar field is not available to the window; report it.

### A8 — container

- **Read:** `window_view/mod.rs:100-200`, `window_view/app_menu.rs`, `window_view/viewer/menu/mod.rs:60-120`, `window_view/title.rs`, `command_center/*`.
- **Change:** `window_view/command_center.rs`: open from `Toggle`, the app-menu row and a title-bar field click; refuse while a modal dialog is open; take snapshots; run sources on the background executor with generation and cancel; close and restore focus; run `Target::Action` through D12; `CommandCenter` key bindings in `keymap.toml`. `window_view/mod.rs` only gets the field and one `child`.
- **Check:** `cargo nextest run -p navo-ui window_view::tests::command_center`. Tests: Ctrl+K opens and closes; Esc clears then closes; Enter on "Show or hide jobs" toggles the job card; a file-list command runs against the pane that had focus; F8 from the box shows the confirm dialog; hover never runs; focus returns to the file list.
- **Stop if:** `window_view/mod.rs` (427 lines today) would grow by more than 10 lines; report instead of growing it.

### A9 — command-use wiring

- **Read:** `navo-app/src/files/memory.rs`, `navo-app/src/gui.rs` (where `FileHooks` are attached), `window_view` hook types.
- **Change:** `navo-app/src/search_hooks.rs`: `SearchHooks { command_use, record_command, frecent, record_visit, clear }` closures over `SharedDb`, attached like `FileHooks`. A9 wires only the command parts; B1 adds the visit parts.
- **Check:** `cargo nextest run -p navo-app search_hooks` with an in-memory `Db`.
- **Stop if:** hooks need a new type in `navo-ui` public API beyond `SearchHooks`.

### A10 — review Part A

- **Read:** the Part A diff, D1–D18 and D29–D32.
- **Check:** one verdict; `cargo xtask gate`; Windows CI green; the orchestrator runs Ctrl+K on a Windows PC build.

### B1 — visits and opens

- **Read:** `browser/open.rs`, `browser/mod.rs:140-200` (listing loaded events), `window_view/pane.rs`, `navo-app/src/search_hooks.rs`.
- **Change:** call `record_visit(location, true)` when a folder listing loads successfully, at most once per folder per 2 s; call `record_visit(location, false)` when the launcher opens a file. Respect `search.remember` (default true until C2 lands).
- **Check:** `cargo nextest run -p navo-ui window_view::tests::visits` and `-p navo-app search_hooks`.
- **Stop if:** listings load through more than one path that would need separate hooks.

### B2 — name table

- **Read:** D21, D25; `navo-index/src/lib.rs`.
- **Change:** `table.rs`: `NameTable` with `push(parent, name, flags) -> u32`, `remove(id)` (marks deleted), `rename(id, parent, name)`, `path(id) -> String`, `name(id)`, `flags(id)`, `len`, `memory_bytes`, `compact()` (drops deleted entries, rewrites ids, returns the id map).
- **Check:** `cargo nextest run -p navo-index table`. Tests include 2,000,000 synthetic names with average 20 bytes and `memory_bytes() / len ≤ 50`.
- **Stop if:** the layout cannot meet 50 bytes per entry.

### B3 — table search

- **Read:** D15, D17, D20; `fuzzy.rs`, `table.rs`.
- **Change:** `search.rs`: `search(table, query, limit, cancel) -> Vec<TableHit { id, score, indices }>`. `rayon` chunks of 64k entries; name terms on names, path terms on rebuilt paths of candidates only; `ext:` first as a cheap filter; deleted entries skipped; top-N kept with a bounded heap.
- **Check:** `cargo nextest run -p navo-index search`. Tests: AND terms, path terms, ext filter, cancel stops early, results stable for equal scores. A `cargo bench -p navo-index search` on 1,000,000 names prints its time (informational).
- **Stop if:** `rayon` conflicts with the GPUI executors in tests.

### B4 — walker

- **Read:** `navo-fs/src/measure/walk.rs`, `navo-fs/src/provider/mod.rs:55-90`, the fake file system and hostile-name corpus.
- **Change:** `walker.rs`: breadth-first, 4 worker threads, `Limits { max_entries, exclude }`, skips reparse points (no loops), reports progress every 10,000 entries, stops on cancel, keeps going past unreadable folders (counts them).
- **Check:** `cargo nextest run -p navo-index walker`: fake tree, hostile names, a junction loop, an access-denied folder, cancel, the entry cap.
- **Reparse points (architect, 2026-10-20):** only name-surrogate reparse points (junctions, symlinks, mount points) can loop; the provider reports them as `EntryKind::Link`. The walker records a link as an entry (flag REPARSE) and never descends into it. Other reparse points (OneDrive and cloud placeholders, dedup) are real content: walk and index them as normal files and folders, without reading file contents. A depth limit of 256 is the backstop.
- **Stop if:** the provider cannot list a folder without hydrating cloud placeholders.

### B5 — Recent and Tabs

- **Read:** `command_center/source.rs`, `results.rs`, `search_hooks.rs`, `navo-core/src/state/` (tabs).
- **Change:** `file_sources/recent.rs`, `file_sources/tabs.rs` per D19; the empty state uses Recent (D8).
- **Check:** `cargo nextest run -p navo-ui command_center::tests::file_sources::recent`.
- **Stop if:** —

### B6 — Here

- **Read:** `walker.rs`, `search.rs`, `file_sources/recent.rs`.
- **Change:** `file_sources/here.rs` per D19: one walk per root, reused for 60 s, streaming re-search as it grows, the 300,000 cap note, cancelled when the box closes or the root changes.
- **Check:** `cargo nextest run -p navo-ui command_center::tests::file_sources::here` on the fake file system.
- **Stop if:** —

### B7 — Paths

- **Read:** D6; `file_sources/recent.rs`; `navo-fs` offline and timeout handling (P14).
- **Change:** `file_sources/paths.rs`: "Go to <path>" row when the folder exists; child folders of the typed parent, filtered by the last segment; Tab completion text; offline note.
- **Check:** `cargo nextest run -p navo-ui command_center::tests::file_sources::paths`.
- **Stop if:** —

### B8 — argument step

- **Read:** D13; `window_view/command_center.rs`, `command_center/view.rs`, `navo-core/src/command/parse.rs`.
- **Change:** the argument step state in the container and the chip in the view; Path args use the Paths source; Enter parses and runs through `Browser::command`; errors show under the input.
- **Check:** `cargo nextest run -p navo-ui window_view::tests::command_center::args`: Go to folder navigates; Mark by pattern marks; a bad path keeps the box open with the error; Esc leaves the step.
- **Stop if:** a text form in D13 does not parse with the core parser.

### B9 — opening results

- **Read:** D7, D29; `browser/open.rs`, `window_view/mod.rs:330-345` (`switch_pane`), `window_view/clipboard.rs` (copy path).
- **Change:** `Browser::open_location(location, folder)` (navigate or launch, the same launcher); container handling for Enter, Shift+Enter, Ctrl+Enter, Alt+Enter and Ctrl+Shift+C on `Target::Open` and `Target::GoTo`.
- **Check:** `cargo nextest run -p navo-ui window_view::tests::command_center::open browser::open`.
- **Stop if:** Q3 is unanswered (Alt+Enter); ship the rest and leave Alt+Enter unbound.

### B10 — review Part B

- **Read:** the Part B diff, D13, D19–D21, D29–D31. **Check:** one verdict; gate; Windows CI.

### C0 — Windows facts

- **Questions:** rights needed for `FSCTL_ENUM_USN_DATA` and `FSCTL_READ_USN_JOURNAL` (volume handle access, admin); behaviour on ReFS, exFAT and Dev Drive; the current `windows-service` crate (version, licence, maintenance); how to set a named-pipe DACL for a LocalSystem server and one user SID with `windows-sys`; whether the pipe needs `PIPE_REJECT_REMOTE_CLIENTS`; how Everything's service limits clients. Answers with links go into the Log.

### C1 — index file

- **Read:** D23; `table.rs`; `navo-store/src/atomic_write/mod.rs`.
- **Change:** `file.rs`: `save(path, header, table)`, `load(path) -> Result<(Header, NameTable), FileError>`; version check; length and checksum check.
- **Check:** `cargo nextest run -p navo-index file`: round trip; truncated, wrong version and bad checksum files are refused; a 2,000,000-entry file loads.
- **Stop if:** `navo-index` would need to depend on more of `navo-store` than `atomic_write`.

### C2 — `[search]` keys

- **Read:** `navo-store/src/settings/model.rs`, `settings/view.rs`, D56 tests.
- **Change:** a `Search` section per D26 with defaults; Q2 decides the walk default root.
- **Check:** `cargo nextest run -p navo-store settings`; the complete-file snapshot shows the new section.
- **Stop if:** —

### C3 — walk index host

- **Read:** `walker.rs`, `file.rs`, `navo-fs/src/watch/mod.rs`, D22, D25–D27.
- **Change:** `host.rs`: per root load the snapshot, rescan at low priority, apply `watch_tree` batches to the table, save after changes (debounced 30 s and on close), publish `HostStatus` (entries, progress, errors). It starts after the first usable frame.
- **Check:** `cargo nextest run -p navo-index host` on the fake file system: load then rescan, a created and a deleted file appear within one batch, overflow triggers a rescan.
- **Stop if:** the watcher cannot watch a drive root.

### C4 — USN records

- **Read:** C0 answers; `table.rs`.
- **First:** look at `usn-journal-rs` 0.5.0 (MIT, quality not reviewed) for at most 15 minutes. Use it for C4 and C5 only if its record parsing is safe Rust on untrusted bytes (bounds-checked, no pointer casts), it pulls in no async runtime (D18) and it reads V2–V4; else write own code as below. Log the choice and the reason.
- **Change:** `usn.rs`: parse `USN_RECORD_V2/V3` bytes and map reasons (create, delete, rename old and new name, close) to `TableChange`; detect a journal id change. Pure, no Windows calls.
- **Check:** `cargo nextest run -p navo-index usn` with recorded record bytes.
- **Stop if:** —

### C5 — NTFS enumeration

- **Read:** C0 answers; `usn.rs`; `navo-fs/Cargo.toml` (`windows-sys` features).
- **Change:** `ntfs/mod.rs` (Windows only): open a volume read-only, enumerate with `FSCTL_ENUM_USN_DATA` into a `NameTable` (file reference → id map; root reference 5), read the journal from a saved USN, return batches. Each `unsafe` block has a `SAFETY:` line.
- **Check:** Windows CI `cargo nextest run -p navo-index ntfs` on a VHD or the runner's volume, with admin; the Linux build compiles it out.
- **Stop if:** the CI runner cannot open a volume handle.

### C5b — NTFS index save and restore

- **Read:** D21–D23; C3–C5 Log lines; `file`, `ntfs`, `usn`, `table`, `host`.
- **Change:** format version 2 adds a counted `FrnMap` of little-endian u128 references and u32 entry ids. Both index kinds use this version; walk snapshots write an empty map through the existing API. Keep atomic writes, complete in-memory loading and safe Rust. Refuse old versions, malformed files and mismatched counts; the caller deletes and re-enumerates.
- **API:** `file::save_with_ids` / `load_with_ids` persist the map. `NtfsIndex::snapshot` borrows the table, map and saved journal position only when no rename is pending. `NtfsIndex::restore` validates entry-id bounds, the expected volume serial and journal id, and restores the saved next USN. Pending renames are never persisted. Existing resume checks still require enumeration after a journal change or wrap.
- **Check:** `cargo nextest run -p navo-index file ntfs`: restore then apply a later journal batch exactly like the original; refuse old and truncated files, invalid entry ids and wrong journal ids; defer snapshots during pending renames. The walk host still saves and loads. Finish with `cargo xtask check -p navo-index`.

### C6 — pipe

- **Read:** C0 answers; D22.
- **Change:** `pipe.rs`: frames (`postcard`, length-prefixed, max 4 MiB), server with the D22 DACL and remote clients rejected, client with a 50 ms connect and 200 ms query timeout. Linux: a stub that always reports "not available".
- **Check:** `cargo nextest run -p navo-index pipe` (round trip, oversized frame refused, timeout); Windows CI.
- **Stop if:** the DACL cannot be built with `windows-sys`.

### C7 — the service

- **Read:** `navo-app/src/pdf_helper.rs` (second mode of the exe), `ntfs`, `pipe`, `file`, C0.
- **Change:** `navo-app/src/index_service/`: `--index-service run` (service main loop: volumes from settings or all fixed NTFS drives, enumerate, follow, answer queries), `install` and `remove` (create or delete the service, delayed auto start, restart on failure), elevated start from the app through `ShellExecuteExW` with `runas`. Index files under `%ProgramData%\Navo\index\` with the D23 ACL.
- **Check:** `cargo nextest run -p navo-app index_service` for argument parsing and config; Windows CI build; the owner installs it on a Windows PC and searches.
- **Stop if:** the service needs any write access to user files.

### C8 — index source

- **Read:** `command_center/source.rs`, `host.rs`, `pipe.rs`.
- **Change:** `index_source.rs`: per D26 `auto` mode, ask the service, else the host; merge with frecency; lazy size and date for shown rows; the indeterminate indexing note with a count (D27 amendment).
- **Check:** `cargo nextest run -p navo-ui command_center::tests::index_source` with a fake pipe and a fake host.
- **Stop if:** —

### C9 — Search settings page

- **Read:** `settings/` sheet pages, D26, D58 rule 5.
- **Change:** a Search page: index mode, drives, exclusions, network, removable, remember history, "Clear search history", service status with Install and Remove.
- **Check:** `cargo nextest run -p navo-ui settings`; gallery shot.
- **Stop if:** —

### C10 — activity

- **Read:** `activity/mod.rs`, the P39 S3 strip, `host.rs` status.
- **Change:** feed `set_background` and `end_background` from index status (D27).
- **Check:** `cargo nextest run -p navo-ui window_view::tests::activity`.
- **Stop if:** P39 S3 is not merged.

### C11 — docs

- **Change:** `docs/architecture.md` "Global search" per D21–D28 (service as an exe mode, `FSCTL_ENUM_USN_DATA`, no `rkyv`); `docs/perf.md` index memory and speed lines; `docs/decisions.md` D68 if the orchestrator has not added it.
- **Check:** `cargo xtask gate`.

### C12 — review Part C

- **Read:** the Part C diff, D21–D28. Security focus: the pipe DACL, remote clients, the service's handles, frame limits, what the index file ACL allows. **Check:** one verdict; gate; Windows CI; the owner check list (install, search a file created a second ago, remove).

## Done when

- Ctrl+K, the app-menu row and a click on the title-bar field open the box; it lists commands for the focused place with their shortcuts, and Enter runs them through the same path as keys (A8 tests; owner check).
- Typing a folder name the user visits often puts it first; Enter opens it, Ctrl+Enter opens it in the other pane (B9 tests).
- "Go to folder…" completes paths and navigates (B8 tests).
- With the service installed on a Windows PC, a file anywhere on C: appears within 50 ms of typing, and a file created a second ago is found (owner check).
- Without the service, the walk index answers for the chosen roots, and the activity strip shows indexing progress (C3, C10 tests).
- No query, file name or path appears in logs or the diagnostic report (canary test).
- `cargo xtask gate` and Windows CI pass for each part; each part is in an alpha build.

## Questions for the owner (answered 2026-10-20)

- Box look (A0): direction 1, "compact list" (32 px rows, shortcuts right-aligned, key-hint footer with the result count or the indexing note), with direction 3's large top-hit row. Owner: "I mean his decision" (the designer's pick). Prototype: `design/prototypes/ctrlk-a0.html` (`?v=1`).
- Q1–Q3: the owner took the recommendations ("I think he knows it better than me"): the service; the user folder by default without it; Alt+Enter opens a new tab.

The original questions:


- **Q1 — Service.** Part C installs a Windows service "Navo Index" (one UAC prompt, LocalSystem, starts with Windows, like Everything's service). Is that acceptable on a Windows PC, or do you prefer a helper that asks for admin at every Navo start? Recommendation: the service.
- **Q2 — Default scope without the service.** Should the walk index cover only your user folder by default (light, calm), or every local fixed drive (Everything-like, but a full walk and more memory inside Navo after each start)? Recommendation: the user folder; whole drives come with the service.
- **Q3 — Alt+Enter.** In the box, Alt+Enter opens the result in a new tab. In Explorer, Alt+Enter shows Properties. Keep "new tab", or use Alt+Enter for Properties and Ctrl+T for new tab?

## Log

- 2026-10-20 — Plan written from D58–D67, the brief's D68 (not yet in `decisions.md`) and the code at ab89739. Found: `history` and `frecency` tables exist but are unused; `navo-index` is empty; Ctrl+K is bound but does nothing.
- 2026-10-20 — Owner picked the A0 look (direction 1 with the top-hit row of direction 3) and accepted Q1–Q3 as recommended.
- 2026-10-20 — A6 done: `CommandsSnapshot::build` (pure, testable) and `CommandsSource`; one `ArgKind` (the registry's). Left for A8: reading the focused key-context stack and command-use weights; context text in `detail` is empty; menu-only commands (no action) are skipped.
- 2026-10-20 — B2 done: `navo-index::table` (`NameTable`, u32 ids, 31 bytes per entry on 1M and 2M synthetic names).
- 2026-10-20 — A7 done: `CommandCenter` view (input with argument chip, top-hit row, groups, Show all, prefix hint, no-match line, footer with count or note and key hints), `VirtualList` from 64 rows; events `QueryChanged`, `Moved`, `Run(index, Modifiers)`, `Dismissed`; seven gallery pages `p40-a7-*`. Key actions and bindings stay with A8.
- 2026-10-20 — B4 first run stopped: `Attributes` has no reparse flag. Decided: skip only `EntryKind::Link` (name surrogates), walk other reparse points, depth limit 256.
- 2026-10-20 — C1 done: version 1 little-endian index snapshots, CRC32 over metadata and payload, atomic writes, stable deleted slots; `built_at` uses UTC Unix seconds. Twelve file tests pass; 2M entries save in 247.5 ms and load in 559.8 ms (debug); gate: all 10 steps passed.
- 2026-10-20 — B3 done: parallel table search in 64k chunks with bounded top-N heaps, AND name and path terms, extension filters and cancellation; 14 search tests passed; 1M synthetic names in 6.19 ms (release, warm, limit 50); all 10 gate steps passed. Borrowed terms and a cancellation callback keep the index independent of the UI; base matcher scores and table-order ties leave D17 ranking to the UI.
- 2026-10-20 — B4 done: four-worker breadth-first walker fills the name table, records links without following them, limits depth to 256, supports cancellation and exact exclusions, counts unreadable folders, and reports partial tables every 10,000 entries and at completion. The cap includes the root. All 14 walker tests passed after failing against the initial scaffold; the full gate passed all 10 steps.
- 2026-10-20 — A8 done: `window_view::command_center` (open on Ctrl+K, the app-menu row and a title-bar field click; refused under a prompt or pending confirm; focus returns on close; 90 ms fade, none with reduced motion). Sources run on the background executor with a generation and `Cancel`; stale results are dropped. Eleven `command_center.*` key commands in the `CommandCenter` context (Ctrl+Home/End per D7); footer keys come from the active keymap, hints without a bound key are left out. The focused key-context stack feeds `CommandsSnapshot::build`; extra sources register in `CommandCenterState::sources`. Argument step: `Target::Arg(name, kind)` is read as a core command id plus `location` (Path) or `pattern` (Text), parsed by `Command::parse`; A6 still passes registry names, so B8 must map them. Files open by navigating to the parent with the cursor on them (no launch until B9). Menu-only `window.new`, `workspace.list`, `help.key_map`, `app.quit` stay unlisted: no dispatch path in the registry.
- 2026-10-20 — C0 answers (researcher; Microsoft Learn and voidtools; verify the UNVERIFIED items on Windows):
  - Rights: FSCTL_ENUM_USN_DATA / FSCTL_READ_USN_JOURNAL on `\\.\X:` (no trailing backslash), NTFS; opening a volume needs administrative rights (CreateFileW), so the index service runs as LocalSystem and opens with GENERIC_READ and FILE_SHARE_READ|WRITE|DELETE. Minimum access mask for the two FSCTLs is not documented: test it. Everything's author: a standard user can read NTFS directly only on non-system volumes (forum).
  - Journal wrap: a saved USN below LowestValidUsn or a changed journal id means a full re-enumerate (exact error codes unverified).
  - ReFS: MFT_ENUM_DATA_V1 documents ReFS (Max version 3, 128-bit ids, USN_RECORD_V3) but the FSCTL page says NTFS: test on a real ReFS volume. Parse V2/V3/V4 by MajorVersion. Dev Drive is ReFS (Windows 11 22621.2338+); USN there untested. exFAT/FAT, SMB: no journal, walker + watch fallback. Never assume the file system from the drive letter; fall back to the walker whenever the FSCTL fails.
  - Service crate: `windows-service` 0.8.1 (2026-05-08, MIT/Apache-2.0, Mullvad); check for duplicate `windows-sys` in Cargo.lock.
  - Pipe: never a NULL security descriptor (default gives Everyone read). DACL (an SDDL string: SYSTEM and Administrators full access, the installing user read and write, nothing else) via ConvertStringSecurityDescriptorToSecurityDescriptorW (LocalFree after), on every instance; FILE_FLAG_FIRST_PIPE_INSTANCE on the first (stops squatters); avoid FILE_GENERIC_WRITE in the user ACE (it includes FILE_CREATE_PIPE_INSTANCE); PIPE_REJECT_REMOTE_CLIENTS on every instance.
  - Everything's service serves names only to its own signed client; any local user can list all NTFS names through it. Owner decided (2026-10-20): the index shows all file names, like Everything; a per-user filter may come later as an option.
- 2026-10-20 — Checked against the Linux-tools idea list (D71): D18 aging written in zoxide's exact form (A3's flat 0.9 is equivalent while uses add 1); D22 a newer pipe generation cancels the running service search; C4 evaluates `usn-journal-rs` before own code; D24 names `mft` for raw `$MFT` later. Folder sizes, job pools and the benchmark harness live in P43.
- 2026-10-20 — A9 done: `SearchHooks { command_use, record_command, clear }` in `window_view` (passed in `StartOptions.search`), DB-backed in `navo-app::search_hooks`. Running a command from the box records it on the background executor (actions by action name, argument commands by registry name); `CommandsSource::with_use` loads the aged weights per search and ranks with `rank::score`. Folders and files are not recorded; `frecent` and `record_visit` come with B1.
- 2026-10-20 — C2 done: `[search]` adds index (auto/service/walk/off), drives, walk-roots, exclude, network, removable and remember, using the existing serde validation and load-error path. Empty drives and walk-roots retain the owner defaults for their later consumers. Default snapshots and the 51-key ledger expectations updated; four search tests cover defaults, parsing and round trips, invalid-value fallback and unknown keys. Store and app check: all 7 steps passed; 846 tests passed, 1 skipped. No search.remember placeholder exists in this checkout.
- 2026-10-20 — C4 done: safe V2/V3 parsing, validated V4 skipping, reason mapping and journal resume decisions; `NameTable::apply` uses a separate `usn::FrnMap` from full u128 file references to u32 entry ids. Eighteen USN tests and eight table-change tests pass, including 6,000 fixed-seed random or mutated buffers; all seven crate check steps passed. Snapshot persistence and compaction remapping of the FRN map remain for C5.
- 2026-10-20 — C4: `usn-journal-rs` 0.5.0 rejected (raw-pointer casts and `read_unaligned` in record parsing; V2/V3 only); own safe parser. Names use `ProviderPath::from_wide(..).to_text()`, the walker's lossless text form.
- 2026-10-20 — C4 review (orchestrator): offsets match USN_RECORD_V2/V3/V4. For C5: `apply` returns UnknownFileReference for a Rename or Add whose file or parent is not in the table (a file moved in from outside the indexed roots, a child before its parent); C5 decides the policy (treat an unknown Rename as Add, queue a rescan of the parent), and reasons accumulate until close, so repeated Add records for one file are normal.
- 2026-10-20 — C3 done: each walk host loads its snapshot after explicit start, queues idle rescans, publishes partial tables and status, applies watcher batches, rescans on overflow, and saves after 30 s without table changes and once on drop. Six fake-provider tests cover loading before refresh, batched creation and deletion, overflow recovery, the injected clock, shutdown, and exclusions. Index check: all 7 steps passed; 77 tests passed, 1 skipped. App wiring owns root selection (home when walk-roots is empty), network/removable filtering, per-root snapshot paths, and starting after the first usable frame. Linux retains its existing folder-only watch; renames and folder changes refresh through a rescan.
- 2026-10-20 — C3 review fixes: rescan callbacks publish status only; the live table keeps its snapshot and watcher patches until a complete scan replaces it. Cancelled, failed and limited walks retain the live table; close saves only dirty data. Renames and folder changes refresh deduplicated parent subtrees; overflow and unknown parents refresh the root. Fake-provider tests with an injected clock reproduce six failures before the fix and cover watcher events queued during a scan. Host lifecycle and scan handling live in separate modules. Index check: all 7 steps passed; 84 tests passed, 1 skipped.
- 2026-10-20 — C5 built, waits for Windows CI: pure enumeration/journal loops, root-reference mapping, unknown Rename-to-Add recovery and deferred-parent reporting; read-only Windows V0 adapter with three documented unsafe blocks. All 17 Linux NTFS tests pass; crate check: all 7 steps passed, 114 tests passed and 1 skipped. Ignored elevated Windows test covers 4,096 system-volume records, the root and `$Extend`; Windows compilation and volume access remain unverified. FRN snapshot persistence/compaction and C4 rename pairs split across journal batches remain open.
- 2026-10-20 — C4 fix: a rename whose old and new names land in different journal reads no longer removes the folder and its contents. `UsnBatch::into_changes` takes a `PendingRenames` carry-over (kept in `NtfsIndex`) that holds unpaired old names for up to `MAX_CARRIED_READS` (2) later non-empty reads, pairs a later new name into a Rename, and only then removes. `NtfsIndex::restart_at` drops the carry-over on a restart; a re-enumeration builds a fresh index. Six tests added (four USN, two NTFS); `unpaired_old_removes_and_unpaired_new_adds` was split because its first half encoded the batch-end Remove. Index check: all 7 steps passed; 133 tests passed, 1 skipped.
- 2026-10-20 — B1 done: a successful folder load (one path, `Browser::accept`) is recorded at most once per folder per 2 s, and a file opened with Enter in the pane or the box is recorded, both on the background executor; `SearchHooks` gains `frecent` and `record_visit`; `[search] remember` gates all recording, read at start and on every settings reload; the box opens a file by revealing it in its folder.
- 2026-10-20 — B5 done: `RecentSource` (the 2,000 top `frecent` rows, loaded once on the first search or the empty state, off the UI thread; Open targets, Folders/Files hits) and `TabsSource` (a snapshot of every tab in every pane and window; GoTo hits) match D20 terms and score with D17. The empty box now shows the 8 most frecent folders under Recent; the last 5 commands are still missing because `command_use` carries weights only, not run order. Recent and Tabs do not remove a folder that appears in both. Eleven source tests and one container test; navo-ui check: all 7 steps passed, 2172 tests passed, 1 skipped.
- 2026-10-21 — B5 follow-up: the merge shows a place once when Recent and Tabs both find it (the first Tabs row wins, with the higher score, compared with `Location::same_place`); the empty box now shows the last 5 commands run from the box (new `Db::recent_commands` by `last_at`, `recent_commands` search hook, off the UI thread with the folders), then the 8 most frecent folders. Store, merge and container tests added.
- 2026-10-21 — B6 done: `HereSource` and `HereWalks` (`file_sources/here.rs`). One `navo-index` walk per root runs on its own thread, starts on the first file query, is kept in the window's command-center state, and is reused while the root is the same and a finished walk is under 60 s old. A walk that is still running is reused at any age; one cancelled by closing the box or by a root change is dropped. The cap is 300,000 entries; past it the Files header reads "Files · First 300,000 entries of <root> · the disk index finds the rest" (`Heading::FilesCapped`). The search task now loops: it searches the latest published table, then waits (`ResultSource::wait_for_more`) for the walk to grow and searches again, so hits stream in; a later round keeps the row the user moved to. The walker publishes every 10,000 entries and at the end, so a tree under 10,000 entries shows at completion. Here hits are Open targets like Recent; the merge shows a place once, with order Tabs, then Recent, then Here. Seven source tests, one merge test, one window test.
- 2026-10-21 — B7 done: `PathsSource` (`file_sources/paths.rs`). Path mode (`C:`, `\`, `~`) splits the input at the last separator, resolves the parent with `typed_location`, and lists its folders through the provider on a worker thread. The source waits 2 s (the P14 listing wait) and polls the cancel flag, so a keystroke or a dead share ends the wait. A hung or lost listing shows the header note "<path> does not answer" (`Heading::Offline`, carried by the new `Notes` next to `Capped`) and no rows. The typed folder gets a "Go to" row when it exists; children are filtered by the last segment. Tab in path mode types the selected folder name and a separator (`completion`). The merge order is Tabs, Recent, Paths, Here. Eleven source tests.
- 2026-10-21 — B7 review note (orchestrator): for B10: check that a hung network listing in path mode cannot leave one blocked worker thread per keystroke.
- 2026-10-21 — C5b done: format version 2 persists the counted full-width FRN map; existing walk save/load calls use an empty map. Quiet snapshots omit pending renames; restore validates entry ids, volume serial and journal id, then uses the saved next USN with existing resume checks. Thirteen new pure tests cover persistence, later journal changes, malformed counts and references, identity mismatches, pending renames and wrap recovery; the initial test run failed on the missing APIs. Targeted file/NTFS run: 54 passed. Index check: all 7 steps passed (10.6 s); 146 tests passed, 1 skipped. Windows volume access remains unverified.
- 2026-10-21 — A10 fix round 2 (matching): `Fuzzy` matches on plain terms (no `!`, `^`, `'`, `$` operators) over the name's chars, so match indices are char positions for every caller (combining marks, emoji sequences); command rows show their context in words (`File list`, `Viewer`, ...) and the first key bound in an active context, own context first; the commands source accepts `/` and `\` in `>` scope and loads use weights once per open; age bands use `<` in rank and SQL; with `[search] remember = false` frecent, command use and recent commands read as empty.
- 2026-10-21 — A10 fix round 1 (window side): Enter during a running round waits for the matching generation and runs the then-selected row (typing clears it); commands show at once and file sources start after 40 ms without typing (D16); one generation counter runs across opens, cancelled rounds no longer update the window; a command is recorded only after its action is built, and a missing action logs an error and shows a status notice; A8 tests added for F8 confirm, refusal under a prompt or confirm, and viewer commands.
- 2026-10-21 — B9 done: `Browser::open_location` navigates a folder or launches a file with the pane's launcher and records it. In the box (D7): Enter opens, Shift+Enter reveals, Ctrl+Enter opens a folder or reveals a file in the other pane (the active pane when there is no other), Alt+Enter opens a new tab, Ctrl+Shift+C copies the full path and closes the box; all go through the stale-round wait; a click maps Alt, Ctrl and Shift the same way. Each open checks the location first (2 s, as the path input); a missing one shows a notice and changes nothing; a found entry's kind wins over the hit's. Location keys do nothing on command rows and in the argument step. 15 tests; navo-ui and navo-app check: all 7 steps passed.
- 2026-10-21 — B8 done: the four D13 commands are registry entries with an argument (`commands::entries()` appends them; ids map `_` to `-` and the value goes in braces, as the core parser needs). Choosing one opens the step with the chip. A Path step lists folders through the Paths source only, with its Go to row, Tab completion and offline note. Enter waits for the current round, then runs the selected folder row or the typed text; the text is resolved, checked for existence (2 s) and parsed by `Command::parse` before `Browser::command`. Errors (not found, not a folder, a parse error) show under the input and the box stays open. Esc leaves the step and restores the query that found the command; a second Esc closes. 7 tests; navo-ui check: all 7 steps passed.
- 2026-10-21 — Orchestrator: the dialog commands "Mark by pattern" and "Unmark by pattern" are hidden from Ctrl+K, so it lists only the argument versions; their keys still open the dialog.
- 2026-10-21 — C6 built, waits for Windows CI: bounded postcard frames, restricted local pipe, cancellable query generations and Linux stub; five Linux pipe tests passed. Crate check: all 7 steps passed, 138 tests passed and 1 skipped. All Win32 API names and features checked against windows-sys 0.61.2; Windows compilation and three real-pipe tests remain unverified.
- 2026-10-21 — C6 security fix: require a LocalSystem pipe owner, secure all four instances with the same DACL, recover after early disconnects, wait on cancellation events, retain partial frames when a receive is cancelled, trim oversized Hits by postcard size, drop malformed connections and reject requests above 64 terms or 1 KiB per term; eight Linux pipe tests passed; crate check: all 7 steps passed, 141 tests passed and 1 skipped; Windows regressions await CI. C7 must read the installing user's S-1-5-21 account SID only from the service registry key or ProgramData protected by an administrator-only ACL; reject well-known group SIDs.
- 2026-10-21 — C7 built, waits for Windows CI: service argument dispatch, direct SCM install/remove, LocalSystem delayed start and failure restart, runas launch, protected ProgramData directories and an install-time search settings copy with the account SID. NTFS snapshots restore and resume or enumerate again; journal updates feed cancellable pipe searches; quiet and stop saves require a complete snapshot. Seven Linux service tests passed; app and index check: all 7 steps passed (11.5 s), 551 tests passed and 1 skipped. Windows API bindings passed a Linux probe with handle and volume stand-ins; native compilation, UAC, ACLs, service lifecycle and the owner search remain unverified.
- 2026-10-21 — C7 security fix round: install and reinstall use a quoted, atomically replaced ProgramFiles binary with pinned directories, exact protected ACLs and path-specific verification errors; remove stops and deletes the service before cleaning the copy. Recovery is 5 seconds, 60 seconds, then none with a one-day reset; configuration, storage verification and bind errors stop cleanly, while caught worker panics enter status errors and cancel for recovery. WTS session identity replaces the elevated account SID; another account uses default volumes when its settings cannot be read without impersonation. Elevation waits for the child and returns typed exit results. Enumeration checks cancellation each batch; snapshots use the opened volume serial; save failures back off 30 seconds then five minutes; known-folder buffers are freed on failure. The cancellation regression first failed against the old API; 21 focused Linux tests passed. App and index check: all 7 steps passed (17.9 s), 560 tests passed and 1 skipped; the first gate stopped on SDDL spelling aliases, corrected to numeric SIDs. Windows API and runtime types passed a Linux probe with handle and path trait stand-ins; native compilation and service tests remain pending. Existing non-crash recovery flag changes may require a Windows reboot.
- 2026-10-21 — C7 reliability round (amends D22): reinstall copies to a temporary file, renames the running navo.exe to navo.exe.<nonce>.old, moves the new copy in, then stops, configures and starts; any failure puts the old image back and calls StartServiceW again; .old leftovers are deleted after start and on the next install; the real copy error survives a failed temp cleanup. A pipe bind failure no longer stops the service: it reports running, records the reason in status errors and retries after 1 s, 5 s, 30 s, then every 60 s until stop. Account lookup accepts only SidTypeUser. Install takes ownership of a planted ProgramData or ProgramFiles folder (Administrators owner, protected DACL, low-level set without propagation) and verifies again; reparse points and non-directories are refused with the path; the service never repairs. Remove also clears and deletes ProgramData\Navo\index and the empty Navo folder after the same verification, without following links. Fourteen Linux tests added; app and index check passed; native Windows compilation remains unverified.
- 2026-10-21 — C7: a folder that install repairs is emptied before it is verified again, so planted index files or service.json never reach the SYSTEM service; D22 amendment now describes the current install, reinstall and remove behaviour.
- 2026-10-21 — C6 and C7 pass Windows CI (clippy, build, gate) on p40-c7; merged.
- 2026-10-21 — B12 done (B10 review fixes): Enter and click wait for a settled round (a generation set only by the files-phase publish, or by the fast publish when no file source is active), so an early Enter runs the Top hit, not the commands-only list. A look-up that ends after a dialog opened changes no focus and no pane. Esc in a step clears the text, then leaves the step and restores the query, then follows normal D7 (clear, close); the one-shot close after a step is gone. Alt+Enter and the location keys do nothing on command rows and in a step; a modifier click on a command row or in a step is Open. Enter on an empty step shows "Type a folder" or "Type a pattern". A path look-up waiting over 250 ms shows "Checking {path}…" under the input; a look-up that times out shows the offline text and does not navigate. Tests: early Enter, command-row and step keys, Esc order, empty step, dialog during open, and argument quoting for backslash, quote, control characters and a Windows path.
- 2026-10-21 — C9 built: the Search page (Search section between Copy and Display) has File index, Drives, Skipped names, Network drives, Removable drives and Remember searches rows on the six `[search]` keys, then an Index service block (status, Install, Remove) and a History block (Clear search history through `SearchHooks::clear`). Drives and Removable show on Windows only; elsewhere the service block says it is a Windows feature. `SearchHooks::service` (`ServiceHooks`: status, install, remove; `ServiceStatus`) defaults to "Not available on this system" with both buttons off; C7 wires the real hook in `navo-app`. List keys (drives, exclude) are comma-separated text in the sheet and TOML arrays in the file (`SettingValue::List`). Only Remember searches is applied now; the other rows show "Not applied yet" until C3, C7 and C8 read them. Switching section resets the sheet's scroll to the top.
- 2026-10-21 — C9b wires service status through read-only SCM queries and Install and Remove through the existing elevated executable. Settings actions run on the background executor; completion requests a render that reads status again, preserving the hook shape. UAC cancellation is silent. Hooks remain available without a history database; Linux reports not available with both actions disabled. Eleven focused Linux tests passed; the UI regression first failed against synchronous click handlers. App and UI check: all 7 steps passed (151.9 s), 2803 tests passed and 2 skipped. Native Windows compilation still needs Windows CI; SCM access and UAC lifecycle need a Windows runtime check.
- 2026-10-21 — B11 done: a dead network folder no longer piles up threads. `command_center::flights` runs at most one job per root (drive or UNC share, `navo_fs::volume_of`) on its own thread, up to 4 threads per flight set, and shares it with every waiter for the same place; past the cap a new root waits 2 s, then gets no answer, so a hung share never delays C:\; a place that timed out or answered "network lost" is not asked again for 30 s. Path mode keeps one `PathListings` in the window's command-center state across opens: filter changes reuse the running or finished listing of the same parent, a new parent on the same root waits for the old listing (2 s) and otherwise shows the old parent's offline note, and a finished listing is reused only within the same open. Here walks are no longer cancelled when the box closes, so a reopen on the same root reuses the running walk; no walk starts when the pane shows the folder as offline or its path listing timed out. `look_up` runs `metadata` through the same rules on a `look-up` thread, never on the background pool; its signature and `None` on timeout are unchanged. 11 tests; the Here test for cancel on close became one for reuse on reopen.
- 2026-10-21 — Orchestrator note for the Part C review: the settings sheet reads the service status (an SCM query) on each render; check its cost on Windows, cache it if it shows.
- 2026-10-21 — C8 done: auto/service/walk/off index source, cancellable pipe queries with a 250 ms round budget and one worker, complete-snapshot host fallback, frecency ranking and Recent deduplication, shown-row metadata through B11 flights, and the existing note with an indeterminate count (D27 alpha amendment). App wiring starts after the first usable frame, defaults the walk to home, and applies mode, drives, skipped names, network and removable settings; those Search rows now show as applied. Gallery shows index hits and the count note. Ten focused fake-backend tests passed, including command rows during a hung pipe and cancellation on a new query; the four-crate check ended with all 7 steps passed (184.0 s), 2,979 tests passed and 3 skipped. Windows runtime verification and gallery screenshots remain with the orchestrator.
- 2026-10-21 — C10 done: dedicated `IndexHooks::status` reads in-process host walk counts without a search; a busy host lock keeps the last status. The window polls off the UI thread every 500 ms, reuses `command-center-indexing` and the existing indeterminate Background strip entry, updates the count, and ends it on completion or off mode. The pipe has no standalone status request, so service progress is omitted. Five fake-hook tests cover appearance, count updates, completion, off mode and two status polls per second with zero host or pipe searches; all 36 activity tests passed. The first check found excessive poller nesting; after splitting it, the UI, app and index check ended with all 7 steps passed (199.1 s), 2,977 tests passed and 3 skipped. Native screenshots and Windows runtime checks remain unverified.
- 2026-10-21 — C14 done: every pipe session error disconnects only its client, logs once at debug and leaves the other instances accepting. Snapshot loading checks limits and the complete file length before allocation: 64,000,000 entries, at most 4 GiB of names (the format holds 4 GiB minus one byte), and no more FRN records than entry slots. Snapshot buffers, strings, arrays, FRN maps and cycle state use fallible reservation. Restore rejects wrong roots, invalid or dead folder parents, cycles and dead FRN targets; direct NTFS restore also checks the root reference. Fake clients cover stalled readers, malformed frames, another instance answering and reuse of the disconnected instance. Small malformed snapshots and direct restore regressions failed before their fixes; valid round trips, including two million entries, still pass. Corrected the round-trip fixture's mismatched root metadata. Index tests: 172 passed, 1 skipped; check: all 7 steps passed (12.2 s). Native Windows compilation and pipe I/O remain unverified here.
- 2026-10-21 — C13: install moves untrusted directories aside and creates fresh protected directories under verified, pinned parents; it never resets a planted directory's permissions. Reinstall tracks completed steps, restores the old image over the replacement, restores the captured SCM configuration and calls Start after any failure. The corrected image regression first failed; 34 focused Linux service tests passed, including failures after replace at Stop, Configure and Start, configuration restoration, move-before-create ordering and refusal after a failed move. The first crate check caught test length and nesting limits; extracting its step executor fixed both. The app check ended with all 7 steps passed (22.2 s), 431 tests passed and none skipped. Native Windows compilation, ACL and sharing behavior, and SCM rollback remain for Windows CI.
- 2026-10-21 — C15 built: Search caches service status and refreshes off the UI thread on open and after Install or Remove; the service builds an exclusion and drive mask while indexing and applies it before the top 500; auto merges walk results from uncovered, network and failed volumes and deduplicates places before ranking; one retirement queue prevents a second host for a root pending teardown. The counting render regression failed first (four hook calls); 23 focused tests passed, including 600 excluded matches plus three allowed matches, skipped folder traversal, refused-volume fallback and five reconfigurations with blocked teardown. D28 records the install-time settings copy, client visibility and unchanged pipe protocol. App, UI and index check pending; native Windows compilation, SCM/UAC and NTFS/USN runtime checks remain unverified.
- 2026-10-21 — C15 done (Codex hit its usage limit while fixing one test; the orchestrator ran fmt and the check: app, ui, index check all 7 steps passed). Cached service status refreshed off the UI thread; the service applies its copied exclusions and drives before the top-500 cut; auto merges walk hits for roots the service does not cover; one retirement worker, and no new host while a root's retirement is pending. Open: a root skipped while its retirement is pending starts again only on the next reconfiguration.
- 2026-10-21 — C12 second review (Opus): do not ship. Must-fix: (1) the new parent trust check rejects stock C:\ProgramData (Users have AD/WD there), so install, run and remove fail; reject only DELETE, FILE_DELETE_CHILD, WRITE_DAC, WRITE_OWNER and generic write for non-privileged SIDs on the parent. (2) Restore rejects every saved index that contains a deleted folder (tombstones keep parent ids); skip DELETED slots in the structure check. (3) The exclusion mask is rebuilt under the write lock on every journal tick; queries stall past their budget. Should-fix: sticky volume errors make covered volumes "uncovered"; desired roots flip with each service reply; one accept error stops all pipe instances; service.json not rolled back; fresh-install rollback leaves the exe; rollback starts a service that was stopped. Next: C16 (service install and rollback) and C17 (runtime, index, pipe accept, roots), on Codex.
- 2026-10-21 — C16 done: the parent trust check now rejects only DELETE, FILE_DELETE_CHILD, WRITE_DAC, WRITE_OWNER and generic write for non-privileged SIDs, so a stock ProgramData passes; a failed install restores service.json (or deletes a new one), a failed fresh install removes the copied exe and the folders it created, a failed reinstall starts the old service only if it was running, and the restore rename retries 5 times at 200 ms. One planner or helper test per fix; native Windows compilation and SCM runtime are unverified.
- 2026-10-21 — C17 done: a saved index with a removed folder loads again (structure check skips deleted slots); the service filters exclusions per candidate with a per-query ancestor memo instead of a mask rebuilt on each journal tick; a volume's update and save errors clear after its next good tick and coverage needs a usable index; Search keeps the last good service status 60 s and caches drives and settings per reconfiguration (30 s refresh); a pipe accept error logs once, backs off 100 ms to 5 s and keeps serving.

- 2026-10-26 — C18 senior-review fixes: Remove cleans up after an absent service and retries mapped-image deletion; failed queries and service actions invalidate coverage, while coverage equality ignores progress/count/error-text churn. Search uses worker-local ancestor memos and constructs normalized exclusions once (one-million-candidate counter probe). Install restores a sole old image before cleanup, refuses ambiguity, restores service.json atomically and re-observes the image after trust checks. Nine initial probes failed before fixes. Final app/index check: all 7 steps passed (9.1 s), 680 tests passed and one skipped. Sol 6.1 xhigh review clear after a Windows-only line-limit extraction. Windows compilation and runtime remain pending. Details: [senior-review fix plan](2026-10-26-index-service-senior-fixes.md).
