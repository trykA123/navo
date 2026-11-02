# Navo architecture

Status: proposal for review, 13 October 2026. Decisions are logged in [decisions.md](decisions.md); plugins have their own document, [plugins.md](plugins.md).

## Goals, in priority order

1. **Never lose or damage the user's files.** No destructive action without an explicit command, and nothing irreversible without a confirmation the user can read.
2. **Never corrupt our own state.** Settings, sessions and job journals survive crashes, power loss and two instances.
3. **Feel instant.** The first frame comes from what we already know; fresh data arrives without blocking.
4. **Stay bounded.** Every cache, queue, history and channel has a limit, and every background task has an owner.
5. **Open to extension, closed to harm.** Plugins add capabilities through the same commands and safety checks as the app.

## Crates

A Cargo workspace; dependencies point down the table.

| Crate | Job | Platform |
| --- | --- | --- |
| `navo-core` | Pure model and logic: paths, entries, panes, tabs, layout tree, workspaces, pins, selection, sort/filter, the command set, operation planning. No I/O. | any |
| `navo-store` | Settings (TOML), session snapshot (`postcard`), SQLite databases `navo.db` and `cache.db`; atomic writes, versions, migrations, recovery. Built in the Foundation phase | any |
| `navo-fs` | Listing, metadata, watching, the job engine (copy, move, recycle, delete) with progress, cancel, verification and the undo journal | Windows first, Linux for development |
| `navo-cache` | Bounded caches: listings, icons, thumbnails, enrichments; budgets and eviction | any |
| `navo-index` | Fuzzy matching (`nucleo`), folder search (`ignore`), global search through Navo's own index and the `navo-indexd` service (D23) | Windows for global |
| `navo-shell` | Windows shell: icons, thumbnails, native context menu, Properties, Recycle Bin, drag and drop, Mark-of-the-Web, ShellExecute | Windows |
| `navo-clipboard` | Clipboard history: model, admit rules, size limits, its own `clipboard.db`, search, and a service on a worker thread that records and serves snapshots. `navo-app` wires it; `navo-ui` sees only a hooks trait. `navo-shell` supplies the clipboard watcher, private write and paste back; `navo-ui` draws the palette and commands | any |
| `navo-term` | Terminal providers: Windows Terminal, external command templates, embedded ConPTY + `alacritty_terminal` | Windows |
| `navo-plugins` | WASM component host, capability checks, plugin lifecycle | any |
| `navo-ui` | Navo's component library and views on `gpui-kit`, styled with the design tokens from the prototype; draws state, sends commands (see UI components) | any |
| `navo-l10n` | Fluent files (`locales/en-US/*.ftl`) embedded in one process-wide bundle; `t!`, `tr` and `tr_args` return text by message id. Depends on no other Navo crate | any |
| `navo-diag` | Logging setup, redaction, in-memory event ring, memory statistics, diagnostic reports. Built in the Foundation phase | any |
| `navo-app` | The binary: wiring, single instance, tray, global hotkey, crash handling | Windows |

`navo-core`, `navo-store`, `navo-cache`, `navo-index` (matching) and `navo-plugins` build and test on Linux, so most work happens on the development machine. Windows-only crates sit behind traits with a fake implementation for tests.

## One command system

Every action is a typed command with a stable id, for example `pane.open_tab { path }`, `path.copy { as: Wsl }`, `drawer.add { view: Compare }`, `files.recycle { selection }`.

- Shortcuts, the F-key bar, the Ctrl+K palette, menus, the drawer, macros and plugins all dispatch the same commands.
- A command is **validated** (arguments, current state), **planned** (what exactly will happen), optionally **confirmed**, then **executed** as effects.
- Results go to the Activity view; reversible ones go to the undo journal.

This gives the palette, custom shortcuts and plugins one surface, and gives safety checks one place to live.

## Core foundations

Structure that is cheap now and expensive to add later (D21). Most of it is types and tests, not runtime weight.

| Foundation | What it means | Phase |
| --- | --- | --- |
| **Locations, not paths** | Panes work with `Location { provider, path }`. Local disks, UNC shares, archives, SFTP and search results are providers that declare what they support (write, watch, recycle, rename) | Foundation |
| **Serialisable commands** | Every command round-trips through text (`pane.open_tab { path: "…" }`), so macros, the command line, plugins and replay share one format | Foundation |
| **Record and replay** | The redacted command stream can be recorded and replayed against the fake file system; a reported bug becomes a test | Foundation |
| **Fake file system with fault injection** | Disk full, access denied, sharing violation, file locked, network lost halfway through a copy; used by the job engine tests on Linux CI | Foundation |
| **Hostile names** | Long paths (`\\?\`), names that are invalid UTF-16, `CON` and `NUL`, trailing dots and spaces, Windows case-insensitive comparison; a corpus of such names runs in CI | Foundation |
| **One error model** | Every error has a kind, a user message and recovery actions (Retry, Skip, Skip all, Run as admin, Show what locks the file); every error dialog is the same component | Foundation |
| **I/O queue per volume** | Priorities (visible, prefetch, idle) and a concurrency limit per disk and per network share, set from the drive type | Foundation |
| **Multi-window state** | `AppState` holds windows; v1 opens one | Foundation |
| **Experimental flags** | `experimental.*` settings hide unfinished features in test builds | Foundation |
| **Key contexts** | Bindings apply in contexts such as `FileList && !Renaming`, with chords; mouse gestures such as the long right-click are bindings too | Alpha |

Small core features that depend on these:

- **Explorer clipboard:** copy and cut in Navo, paste in Explorer and back (`CF_HDROP`, preferred drop effect). Alpha.
- **Who locks a file:** the Restart Manager names the process ("used by WINWORD.EXE") in the error dialog. Alpha.
- **Command line:** `navo <path>`, `navo --new-tab <path>`, `navo cmd <command>` sent to the running instance over its pipe. Alpha.
- **Per-monitor DPI:** each monitor keeps its own scale; window placement is saved per monitor setup. Alpha.
- **Accessibility:** everything works from the keyboard, focus is always visible, a high-contrast theme exists, the UI scales. Screen-reader support depends on GPUI and is checked in the spike.
- **Default file manager:** optional; Win+E and "Open folder" open Navo, with a clean way back to Explorer. v1.0.

## State

### One owner, one direction

- `AppState` in `navo-core` is plain data: workspaces, panes, tabs, selection, drawer, pins, settings. It is changed only by `apply(command) -> Result<Effects>`, a pure function that can be unit- and property-tested on Linux.
- Effects (list a folder, start a job, write the session) run in the runtime. Their results come back as events, which are applied the same way.
- GPUI entities hold view state only and read `AppState`. They never own file-system data.

### No leaks

- **Every background task has an owner.** GPUI `Task`s are cancelled when dropped, so a pane owns the tasks for its listing and enrichments, and the job manager owns copy jobs. Closing a pane drops its tasks. Detached tasks are not allowed outside the job manager.
- **No strong cycles.** Background work holds weak handles (`WeakEntity`) to UI state; subscriptions are dropped with their owner.
- **Stale results are discarded.** Each pane has a generation counter, incremented on navigation. A listing or thumbnail that comes back for an old generation is dropped.
- **Everything is bounded:** channels, caches (by bytes), undo history (by count and age), Activity log, recent folders, clipboard history.
- **Checked in CI:** a soak test opens and closes 10,000 tabs and navigates 50,000 folders on a fake file system; heap must return to within 2% of baseline (`dhat`). A diagnostics drawer view shows live cache sizes and task counts.

### No corruption

| Data | Format | Rules |
| --- | --- | --- |
| Settings and keymaps | TOML, hand-editable | Validated on load; invalid files are reported and **never overwritten**; the last valid copy keeps running |
| Session and workspaces | Versioned binary (`postcard`) with header and checksum | Debounced saves (1 s), atomic replace, previous file kept as `.bak`, recovery from `.bak` on a bad checksum; a file from a newer version is kept as `.v<version>` and never overwritten |
| User data (`navo.db`, SQLite) | History, frecency, tags, saved searches, module key-value data, job and undo journals | WAL mode; one transaction per job step; `PRAGMA user_version` migrations; `integrity_check` on start-up when the `running` marker of the previous run is still there |
| Job journal (copies, moves, deletes) | Table in `navo.db`, one row per step, committed per step | Lets an interrupted job resume or roll back after a crash |
| Undo journal | Table in `navo.db` | Bounded; entries expire |
| Cache (`cache.db`, SQLite) | Listings, icons, thumbnails as blobs, folder sizes | Safe to delete at any time; rebuilt on demand; a corrupt file is replaced, never repaired |
| Global search index | Own file format under `index/`, memory-mapped (`rkyv`) | Rebuilt when missing or from an older version |

- **Atomic writes:** write `name.tmp`, flush, then `ReplaceFileW` / `MoveFileExW(MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)`.
- **Failed integrity check:** a corrupt database writes a sticky `navo.db.check-failed` file next to it; while it exists every start runs the check and no job is recovered or journaled; only a passing check removes it.
- **Single writer:** one process per user session (a named mutex); a second launch hands its arguments to the first instance and exits.
- **Migrations:** each format has a version (`PRAGMA user_version` for SQLite); migrations are pure functions with fixture tests for every old version.
- **SQLite from the start:** `rusqlite` with bundled SQLite is part of `navo-store` from the Foundation phase, so every later feature stores its data in the same place.
- **Crashes:** a panic hook writes a redacted crash report (see Diagnostics) and the last session; the next start offers to restore it and to save the report.

Storage lives in `%APPDATA%\Navo` (settings, `navo.db`), `%LOCALAPPDATA%\Navo` (session, `cache.db`, index). **Portable mode:** if `navo.toml` sits next to the executable, everything lives beside it instead.

## Speed: caching, lazy loading, pre-warming

### Start-up states

| State | Meaning | Target to first usable frame |
| --- | --- | --- |
| Cold | First launch after boot; OS file cache empty; our caches may exist | ≤ 400 ms |
| Warm | Launched again; OS cache and our caches warm | ≤ 150 ms |
| Hot | Already running in the tray; summoned by hotkey | ≤ 50 ms |

Start-up order:

1. Read the session snapshot (small, one file) and draw the last layout immediately. The snapshot also holds about 100 visible rows per pane, marked as cached, so the first frame needs one file read and **no database**.
2. Open `navo.db` and `cache.db` in the background, fill the rest of the panes from the listing cache and revalidate everything.
3. Start everything else **lazily**: plugins, global index connection, git, thumbnails and terminals start on first use or after the app is idle.

Targets are measured with `tracing` spans in every build and checked by a start-up benchmark in CI on Windows.

### Listing folders

- **Phase 1, names and basics:** `FindFirstFileExW` with `FindExInfoBasic` and `FIND_FIRST_EX_LARGE_FETCH` returns names, attributes, sizes and times in one pass. Results stream to the pane in batches, so the first rows appear before a 100k-entry folder finishes.
- **Phase 2, enrichment, only for what is visible:** icons, thumbnails, git status, folder sizes, hashes and plugin columns are computed for rows in or near the viewport, through a priority queue. Scrolling away cancels the work.
- Lists are virtualised (GPUI `uniform_list`); sorting keys are computed once per listing; filtering runs on a background thread.

### Caches

| Cache | Key | Budget | Invalidation |
| --- | --- | --- | --- |
| Listings (memory) | path + volume serial | 64 MB, LRU | File watcher for open panes; folder change time on revisit |
| Listings (`cache.db`) | same | pinned and workspace folders only, 32 MB, `lz4` compressed | Same; never trusted without revalidation |
| Icons (memory and `cache.db`) | extension, or file for `.exe`, `.lnk`, `.ico` | 8 MB | Icon theme change, or the file's change time; the Windows shell is not touched before the first frame |
| Thumbnails (`cache.db` blobs) | path + size + change time | 256 MB, LRU | Key change; purged when the file is deleted through Navo |
| Enrichments (git, folder sizes) | path + change time | 16 MB | Watcher events |

- **Stale while revalidating:** a cached listing shows at once and is replaced by a diff when fresh data arrives; selection and scroll position are kept.
- **Network shares** get a timeout and an "offline, showing cached" state instead of a frozen pane.
- When the window is hidden or minimised, enrichment pauses and thumbnails are dropped from memory; listings stay.
- Privacy: thumbnails stay in `%LOCALAPPDATA%`, can be turned off, and can be cleared from settings.

### Pre-warming

- The cursor resting on a folder for 150 ms prefetches its listing (at most two prefetches at once).
- After start-up goes idle: pins and workspace folders are listed, the connection to `navo-indexd` is opened, and fonts and icons for the visible panes are loaded.
- A terminal is pre-started only if the drawer's terminal view was open in the last session.

## Runtime and libraries

No Tokio in the core (D18). GPUI already has a foreground and a background executor whose tasks are cancelled when dropped, and file-system calls on Windows block anyway.

| Work | Runs on |
| --- | --- |
| UI and pane-owned tasks (listing, enrichment) | GPUI executors |
| Copy, move, recycle, delete | The job engine's own pool of `std` threads with bounded channels; copies use `CopyFileExW` for progress, cancel and attributes |
| CPU-parallel work (folder sizes, large sorts) | `rayon` |
| Folder walks | `ignore` or `jwalk` |
| Watching | `notify` (ReadDirectoryChangesW) |
| Network (remotes, downloads) | Inside the module that needs it; a module may start its own Tokio runtime on first use |

Main libraries (versions are pinned and checked in the Foundation plan):

| Role | Crate |
| --- | --- |
| UI | `gpui-kit` (pins the matching `gpui` and re-exports `gpui-base` and `gpui-component`) |
| Windows API | `windows` |
| Errors | `thiserror` in libraries, `anyhow` only in `navo-app` |
| Logging and timing | `tracing`, `tracing-subscriber`, `tracing-appender` (size-limited) |
| Serialisation | `serde`, `toml`, `postcard`, `rkyv` |
| Database | `rusqlite` (bundled SQLite) |
| Fuzzy matching | `nucleo`; `frizbee` is compared in the spike |
| Channels | `crossbeam-channel` (bounded) |
| Hashing | `xxhash` for cache keys, `sha2` for copy verification |
| Compression | `lz4_flex` |
| File names | `compact_str` (short names stored inline) |
| Translation | `fluent-bundle` |
| Tests and benchmarks | `proptest`, `insta`, `dhat`, `divan` |

- Sorting by name uses `StrCmpLogicalW`, the same order as Explorer (`file2` before `file10`).
- A file-list row costs about 56 bytes, so 100,000 entries take about 6 MB.
- Release profile: `lto = "fat"`, `codegen-units = 1`, `strip = true`, `panic = "unwind"` so a panic in a background task stops that task, not the app. `mimalloc` is adopted only if the spike shows a gain.

## UI components

Reusable components from the first screen (D25). Rule: the app is assembled from components; no view draws raw elements for something a component already does.

**Foundation:** `gpui-kit` (formerly gpui-component, Apache-2.0). One dependency pins a matching GPUI release, which contains GPUI's API churn. It brings virtual lists and tables for hundreds of thousands of rows, a serialisable dock layout, inputs, menus and overlays, AccessKit accessibility and headless UI tests. Its JavaScript extension runtime (`gpui-shell`) is not used; plugins stay WASM (D4).

**Layers**, each depending only on the ones above it:

| Layer | Contents | Rule |
| --- | --- | --- |
| Tokens | Colours, type, spacing, radii, motion from the active theme (D16), mapped onto the `gpui-kit` theme | No literal colour or size outside the theme |
| Primitives | `Button`, `IconButton`, `Input`, `Toggle`, `Menu`, `Popover`, `Tabs`, `Table`, `VirtualList`, `Splitter`, `Tooltip`, `Kbd` | Thin wrappers over `gpui-kit` with Navo's variants; the rest of the app never imports `gpui-kit` directly |
| Navo components | `FileList`, `PaneTabs`, `PathBar`, `DriveBar`, `Rail`, `Palette`, `Drawer`, `JobRow`, `StatusBar`, `FKeyBar`, `ErrorDialog`, `ConfirmDialog`, `SettingsForm` (generated from a schema), `WidgetHost` (renders plugin widgets) | Props in, commands out; no file-system access, no stored state |
| Views | `Window`, `Pane`, `Workspace`, `SettingsWindow` | Containers: read `AppState`, own tasks, render components |

- **One primitive per concept** with variants (size, tone, density), never a copy per screen.
- **`ConfirmDialog` is core-only.** Plugin widgets render through `WidgetHost`, which cannot produce it.
- **Plugin widgets are a subset of these components**, so plugins look and behave like the app.
- **Component gallery:** a `navo-ui-gallery` binary shows every component in every state, both themes and three densities. It is the review surface for design changes and the fixture for headless UI tests (input, focus, layout, accessibility names).
- **Accessibility through AccessKit:** every interactive component has a role, a name and keyboard access; tests check them.
- Wrapping `gpui-kit` behind Navo's primitives keeps a way out: if a `gpui-kit` component does not fit, it is replaced in one place.

## Safety

### Destructive operations

- **Delete goes to the Recycle Bin** (`IFileOperation` with recycle flags). **Shift+Del deletes permanently** after a warning that lists the count, total size and first paths; Enter on the warning is never the default action.
- **Plan, then act.** Every copy, move, rename or delete builds a plan first: sources, targets, conflicts, totals. Large or risky plans (over 100 items, over 1 GB, anything permanent, anything in a protected location) are shown before they run.
- **Protected locations** (drive roots, `C:\Windows`, `Program Files`, the user profile root, Navo's own folders) need a typed confirmation.
- **Links are never followed when deleting or moving.** Symlinks and junctions are removed as links; recursive operations stop at reparse points.
- **Re-check at execution.** Paths are canonicalised and re-validated just before each step, so a changed folder can't redirect an operation.
- **Copies are safe:** written to a temporary name, verified (size, `xxh3` or SHA-256; see Copy engine), then renamed. Conflicts follow an explicit policy (ask, skip, rename, overwrite older).
- **Overwrites keep the old file until the job ends:** the job journal records the old target, then it moves into a `.navo-held-<pid>-<time>-<run>` folder next to it, one per job run (same volume, so a rename), and the new file takes its place. A job cut short by a crash puts the old file back at the next start, but never over a file that changed since: then the old file comes back beside it as `name (before copy).ext`. A finished job deletes its held files. Without a journal the new file replaces the old one in one step.
- **Undo** covers rename, move, copy and Recycle Bin operations.
- **Keyboard and mouse guards:** F8 and Del act only when the file list has focus, repeated presses are debounced, and drag-and-drop between drives copies by default.

### Security

- **The app runs without admin rights.** Operations that need elevation run in a small separate helper process for that one operation, with the normal UAC prompt.
- **Starting programs:** files open with `ShellExecuteExW`, so Windows SmartScreen and Mark-of-the-Web checks apply. Programs downloaded from the internet get an extra confirmation.
- **No shell strings from file names:** processes start with argument lists and correct Windows quoting.
- **Untrusted file parsing** (previews of images, archives, PDFs) runs in a separate low-privilege worker process or in WASM, with size limits; extraction blocks path traversal (zip-slip).
- **Credentials** for remotes are kept in Windows Credential Manager, never in config files.
- **Plugins** are sandboxed and permission-checked; see [plugins.md](plugins.md).
- **Supply chain:** pinned dependencies, `cargo-deny` and `cargo-audit` in CI, minimal crate features.
- **No telemetry.** Diagnostic reports are created only on request and never leave the machine unless the user sends them.

## Global search

Navo's own index is the main path (D11, D23); it works like Everything.

- **`navo-indexd`**, a small Windows service, reads each NTFS volume's file table (MFT) at first start and then follows the USN change journal. It saves the last journal position, so a restart only catches up; if the journal wrapped, it rescans.
- **Memory layout:** one name buffer per volume, a parent index and a few fields (size, change time, attributes) per entry; paths are rebuilt from the parent chain. Budget about 80–100 MB for 2 million entries.
- **Search:** substring, wildcards and regex, run in parallel over the name buffer (SIMD); `nucleo` ranks fuzzy results in Ctrl+K. Target: under 50 ms on 2 million entries.
- **Filter syntax:** `ext:jpg`, `size:>10mb`, `dm:today`, `path:proiecte`, quotes and `regex:`; metadata providers (D24) can add their own keys.
- **The app talks to the service over a local pipe** restricted to the user's account. The service returns names only and never opens files. On machines with several users it can filter results by the user's access rights.
- **Fallback** for non-NTFS volumes, network drives, or when the service is not installed: a parallel walk saved to disk and kept current by watchers.
- **Everything** is an optional module for people who already run it.

## Copy engine

The user chooses how a copy runs (D22). The mode changes speed only: every mode keeps the plan, the temporary name, verification, the job journal and undo.

| Mode | How it copies | Good for |
| --- | --- | --- |
| **Auto** (default) | `CopyFile2` per file; parallel on SSD, NVMe and network, sequential on spinning disks; unbuffered above a size threshold; thresholds come from the copy benchmark | Everything |
| **Standard** | `CopyFile2`, one file at a time, like Explorer | Compatibility, odd file systems |
| **Parallel** | Like `robocopy /MT`: a chosen number of files at once (2–32) | Many small files, network shares |
| **Large files** | Unbuffered I/O with large buffers, reads and writes overlapped, like FastCopy | Big files between physical disks |

`CopyFile2` gives server-side copy on SMB (data stays on the NAS when source and target are on it), block cloning on ReFS and Dev Drive, and SMB compression where Windows supports them.

Options per job, with defaults in settings:

- **Verify:** none, size, fast hash (`xxh3`) or SHA-256.
- **Conflicts:** ask, skip, rename, overwrite older, overwrite all.
- **Keep:** timestamps and attributes (always), alternate data streams, permissions and owner (may need elevation).
- **Load:** normal or background I/O priority; an optional bandwidth limit for network targets.
- **When:** start now or add to the queue.

**Copy profiles** save a mode with its options under a name. **Rules** choose a profile by destination (for example, the NAS drive uses Parallel 4 with SHA-256). The F5 and F6 dialogs show the chosen profile and can change it for one job. A same-volume move is a rename.

Running `robocopy.exe` as an engine is not part of the core; it could come later as a module.

## Theming

Modelled on Zed (D16). Themes and icon themes are data packages, so they need no WASM host and ship before v1.0.

- **Colour themes** are TOML files with semantic tokens (surface, rail, line, text, muted, accent, selection, folder, file kinds, ok, warn, danger). One file holds a family with light and dark variants.
- **Mode:** `system`, `light` or `dark`; `system` follows the Windows setting. Settings name one light and one dark theme.
- **Live reload:** editing a theme file updates the window. The theme picker previews each theme while you move through the list.
- **Validation:** a theme with a missing token falls back to the built-in value; text and background pairs below 4.5:1 are reported, not silently accepted.
- **Icon themes** are separate: SVG files mapped by file name, extension and folder name, with open and closed folders. Icons are rendered once per size and cached. Files whose icon lives inside them (`.exe`, `.lnk`, `.ico`) keep the Windows shell icon.
- Fonts, size and density are settings, not part of a theme.

## Drives and network locations

- **Drive bar:** one button per drive letter above the panes, as in Total Commander; mapped network drives show a network icon, disconnected ones are dimmed. It can be turned off in Layout. The rail also lists drives with free space.
- **Alt+F1** opens the drive menu for the focused pane.
- **Enumeration:** `GetLogicalDrives` and `GetDriveTypeW`; mapped drives are resolved to their UNC path with `WNetGetConnectionW`; remembered but disconnected drives come from `WNetOpenEnumW(RESOURCE_REMEMBERED)`. Clicking a disconnected drive reconnects it with `WNetAddConnection2W`, and Windows asks for credentials when it needs them.
- **Changes** arrive through `WM_DEVICECHANGE`, plus a slow periodic check for network drives.
- **Never blocking:** labels, free space and listings of network drives are read in the background with a timeout (2 s for metadata). A share that does not answer shows "offline, showing cached".
- **UNC paths** (`\\server\share`) work wherever a path works: the path bar, new tabs, pins and workspaces.
- **Refresh** per network location: watcher, interval or manual, because SMB change notifications are unreliable.
- Browsing the network neighbourhood comes after v1, as a module.

## Diagnostics

From the Foundation phase, so problems found in testing come with evidence (D20). Nothing is sent anywhere: the user saves a report and decides what to do with it.

**What a report holds:**

| Part | Content |
| --- | --- |
| Build and system | Navo version and commit, Windows version, CPU count, RAM, GPU adapter class, display scale |
| Configuration | Enabled modules and their versions; settings that differ from the defaults, with paths redacted |
| Errors | Error kind, Windows error code, the operation and a backtrace of Navo's own functions |
| Panics and crashes | Message, thread, backtrace, the last events before the crash |
| Recent events | The in-memory ring of the last structured events (bounded, about 2 MB) |
| Memory | Process working set and private bytes, allocator statistics, every cache's size against its budget, task and channel counts per owner |
| Performance | Start-up spans, slow-frame histogram, listing times by drive type |

**How it stays free of personal data.** Redaction happens when an event is recorded, not when the report is written:

- Paths are logged only through a `RedactedPath` type. It keeps the shape and drops the names: `C:\<user>\<d:3fa2>\<d:91bc>\<f:07e1>.jpg`, with the drive type (local, network, removable), depth, length class, long-path and non-ASCII flags. Tokens are salted per report, so the same folder gets the same token inside one report and cannot be matched across reports.
- UNC servers and shares become `\\<server:1>\<share:1>`. User, computer and domain names are replaced.
- Windows error texts that embed paths are not stored; the error code and Navo's own message are.
- No file contents, clipboard data, search queries, file names typed by the user or credentials, ever.
- A test runs a session with canary values (a user name, paths, a server name, a search query) and fails if any of them appears in the logs or the report.

**Where it lives:** a rolling log in `%LOCALAPPDATA%\Navo\logs` (3 × 10 MB), already redacted. **Help → Save diagnostic report** writes `navo-report-<date>.zip` with a readable `report.json` and a summary of what it contains; the user can open it before sharing.

**Raw memory dumps** (`MiniDumpWriteDump`) contain whatever was in memory, including file names. They are off by default, can be turned on for one session, stay local, and are deleted after 7 days.

## Terminal

The terminal is a provider, chosen in settings:

| Provider | Behaviour |
| --- | --- |
| **Windows Terminal** (default) | `wt -w 0 nt -d "<folder>"` opens a tab in the existing window; also new window or split pane (`sp`), with an optional profile |
| External command | A template such as `wezterm start --cwd {folder}`, `alacritty --working-directory {folder}` or `cmd /k cd /d {folder}`; arguments are passed as a list, never a shell string |
| Embedded | ConPTY with `alacritty_terminal` in the drawer, following the focused pane; ships after v1 |

The command `terminal.open { where: Tab | Window | Split | Drawer }` works the same with every provider.

## Building and testing

Plans, tiers, test-driven atoms, size limits with their exceptions, the repository layout and the version policy are in [plans/README.md](../plans/README.md) (D26).


- Development on Linux: everything except the Windows-only crates builds, runs and tests there; the UI runs on Linux for layout work.
- Windows builds: GitHub Actions on `windows-latest` produces a portable `.exe` for every push to `main`, for testing at work. `cargo-xwin` builds locally when needed.
- Corporate machines may block unsigned executables; the build will be signed once a certificate is available, and until then it may need IT approval.
- Quality gate: `cargo fmt`, `clippy -D warnings`, tests, headless UI tests of the components, the soak test, the start-up benchmark (Windows CI), `cargo-deny`, `cargo-audit`.
