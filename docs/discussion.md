# Navo: product and design discussion

Record of the conversation that started the project, 13 October 2026. Working name in the prototypes: “Commander”. Project name: **Navo**.

## The idea

A Windows file manager in the spirit of Total Commander, built in Rust, with more than Total Commander offers:

- Shortcuts for everything: opening apps, copying paths, everything Total Commander can do.
- **More than two panes**, tiled like tmux or Hyprland.
- **Saved sets** (groups / workspaces) of panes and tabs.
- **Pins** for folders, tabs and files.
- **Terminal in the current folder**, fast.
- **Fast fuzzy search**, globally and inside a folder.
- Must be fast, portable, performant, good-looking and **extensible with plugins**.

## Feature ideas collected

Core:

- Tiling panes as a split tree (tmux/Hyprland), each pane holding tabs; focus and swap with the keyboard.
- Workspaces: a saved layout plus every pane's tabs, paths, sort and filters; Alt+1…9; session restore.
- Pins: pinned folders with Ctrl+1…9, pinned tabs, pinned files.
- Terminal here: Windows Terminal (`wt -d <path>`) and an embedded terminal that follows the focused pane (ConPTY).
- Fuzzy search: in-folder with `nucleo` + `ignore`; global through the Everything SDK when installed, otherwise our own NTFS MFT/USN index.

More:

- Command palette (Ctrl+K) over files, folders, commands, pins and workspaces.
- Frecency jump (zoxide-style).
- Copy path variants: Windows, quoted, forward slashes, WSL `/mnt/c/…`, UNC, relative to the other pane, file URL.
- Background copy queue: pause, resume, reorder, checksum verification, resume after crash, undo journal.
- Batch rename with regex and live preview.
- Preview pane: images, video frames, PDF, syntax-highlighted code, archives.
- Archives browsable as folders.
- Folder compare and sync, duplicate finder, disk-usage treemap (MFT scan).
- Git status columns, tags and colour labels, saved searches as virtual folders.
- Remotes: SFTP, S3, rclone backends.
- Global summon hotkey (quake mode), quick filter as you type.
- Plugins (WASM) and Lua scripting; Total Commander-compatible and Vim keymaps.

Hard parts to plan for: Windows shell integration (context menus, icons, thumbnails, Recycle Bin via COM), virtualised lists for 100k+ entries, drag and drop with Explorer, long paths (> 260), UAC elevation, slow network shares, file watching.

## Stack discussion

Options weighed for a Rust desktop UI:

| Option | Speed | Portable | Looks | Plugin UI | Maturity |
| --- | --- | --- | --- | --- | --- |
| GPUI (Zed) + gpui-component | Best (GPU, huge lists) | Single .exe | Excellent | Host draws declarative UI | Young, API churn, thin docs |
| Tauri + Svelte | Good (WebView2 overhead) | .exe + WebView2 (on Win 10/11) | Excellent, fastest to style | Easy (HTML) | Stable, large community |
| Slint | Very good | Single .exe | Good | Declarative | Stable, good docs |
| Iced | Good | Single .exe | Decent | Declarative | Stable-ish (COSMIC) |
| egui | Fast | Single .exe | Utilitarian | Easy | Stable |
| Dioxus | Webview | .exe + webview | Good | HTML | Native renderer experimental |

Recommendation at the time: **GPUI**, because Zed already proves the whole shape (fast lists, fuzzy picker, embedded `alacritty_terminal`, WASM extensions, splits, command palette). Risk: API churn and Windows maturity, so validate with a spike. Fallback: Tauri + Svelte. **Not decided yet**; see the architecture discussion.

Modularity, whichever UI:

- A UI-free core crate: file operations, copy queue, index/search, workspace/layout model, path utilities.
- Plugins as WASM components (`wasmtime` + WIT), capability-scoped (paths, network). Plugin kinds: commands/hotkeys, columns, previewers, virtual filesystems, search sources, context-menu actions.
- Lua (`mlua`) for small personal scripts.
- Settings and keymaps in TOML/KDL with live reload.

## Design rounds

The first-round prototypes are no longer in the repository. Shared online version: <https://claude.ai/artifact/3NSKQoaSK7ptjxMXjtZGg6> (private until shared).

**Round 1, four directions**

- A · Tiles: Hyprland/tmux tiling, waybar-style top bar with workspaces, terminal split, vim status line.
- B · Commander: Total Commander heir; left rail with pins, sets, drives, queue; dense columns (Name, Ext, Size, Date), `[folders]`, red marked files, command line, F-key bar.
- C · Studio: IDE layout; activity bar, workspace cards with layout thumbnails, editor-style panes, preview panel, bottom panel.
- D · Strip: niri/PaperWM scrolling columns with an inline preview column.

**Round 2, the bottom area as a dock** (feedback: liked the bottom terminal in A and C, wanted more than a terminal there)

- E · Console: A with a split console; terminal left, switchable views right.
- F · Shelf: C with a shelf of live widgets that expand.
- G · Inspector: A with a context-aware inspector (photos, code, folders, programs, archives) plus a one-line command bar.
- H · Blocks: C with a Warp-style block timeline of every action.

Dock views: Terminal, Transfers, Search, Git, Clipboard, Notes, Disk, Activity, Preview.

**Round 3, calmer, laptop first** (feedback: too crowded; liked E and A; handle density with add/remove; must look good in a small window)

- I · Calm tiles, J · Calm console, K · Focus (one pane plus a pane stack), L · Side dock (dock as a right column for short screens).
- Principles: quiet start, add/remove views and panes, columns that drop as panes narrow, focus by lift instead of borders.

## Decision: B, Commander

Chosen direction, with these changes (built into the prototype):

- **Resizable sidebar**: drag 160–420 px with a live width badge, collapse to an icon rail, double-click to reset, lock the width, keyboard resize; remembered.
- **Copy paths from tabs**: copy icon on tabs and the path bar; tab context menu (copy path, WSL, quoted, file URL, copy all tab paths in the panel, duplicate, open in other panel, close).
- **New tab from a path**: “+” menu with new tab here, new tab from the clipboard path, recent folders, type or paste a path.
- **Drawer instead of a fixed terminal** (F9, resizable height). Views are added and removed: Transfers, Clipboard, Search by default; also Compare (panel diff with sync), Selection (marked files and actions), Git, Notes, Disk, Activity, Preview, Terminal.
- Panels drop Ext, then Date, then Size as they narrow, so three panels fit at 1100×700.

## Open questions

- Global index: Everything SDK dependency vs own MFT/USN indexer (admin rights). Proposed: Everything first.
- Distribution: portable .exe only, or also an installer with shell integration (“Open in Navo”, default folder handler).
- Code signing for running builds on a corporate machine.

Answered since: UI framework is GPUI (spike pending), Windows testing happens at work, plugins are defined in [plugins.md](plugins.md), and the terminal can be Windows Terminal. See [decisions.md](decisions.md).

## Why Rust, compared with Total Commander

Total Commander is a native Windows app written in Pascal (Delphi; Lazarus/Free Pascal for 64-bit). It is already small and fast, so Rust is not chosen for raw speed alone. What Rust adds:

- **Memory safety without a garbage collector.** No use-after-free, buffer overflows or data races in safe code. That matters most where a file manager handles untrusted input: archive formats, image and document previews, file names and paths.
- **Safe parallelism.** The compiler rejects data races, so listing, icons, thumbnails, search, hashing and copy jobs can run on all cores without the bugs that usually come with threads. The interface stays responsive while heavy work runs.
- **Resources that clean themselves up.** File handles, watchers, tasks and processes are released when their owner goes away (`Drop`). Cancelling or closing something cancels its work. This is the main defence against leaks.
- **Errors that cannot be ignored.** File operations return `Result`; a failed step must be handled, which suits copy, move and delete code where a silent error means data loss.
- **Types that encode rules.** Path types for long and UNC paths, enums for job states, plans that must be confirmed before they run: invalid states fail to compile instead of failing at a user's desk.
- **A safe plugin model.** Total Commander's plugins are native DLLs in its own process: powerful, but a buggy plugin can crash it, and every plugin has full access. Rust's mature WebAssembly runtime (wasmtime is written in Rust) makes sandboxed plugins with permissions practical.
- **The ecosystem:** Microsoft's own `windows` crate for the Win32 and COM APIs, `nucleo` (Helix's fuzzy matcher), `ignore` (ripgrep's walker), `alacritty_terminal`, `gix` for git, `serde`, `tokio`, GPUI from Zed.
- **Tooling:** Cargo, built-in tests, Clippy, fuzzing for parsers, property tests, cross-compiling from Linux, and reproducible builds with a lockfile.
- **Portability:** one self-contained executable, and a core that also runs on Linux and macOS if Navo ever goes there.

Limits to keep in mind:

- Rust does not make I/O faster by itself; speed comes from parallel work, caching and doing less.
- Windows shell integration (context menus, thumbnails, drag and drop) is COM and needs `unsafe` code, so it stays small, isolated and reviewed.
- Total Commander has decades of edge cases behind it; Navo has to earn them with tests.
- Compile times are longer, and GPUI is younger than Delphi's VCL.
- An option worth keeping open: running Total Commander's content and lister plugins in a separate helper process, so existing plugins could work without sharing Navo's memory.
