<img src="assets/brand/navo.svg" alt="Navo" width="96" height="96">

# Navo

A keyboard-first file manager for Windows. Two panes, every action on a key, and no waiting.

[![Navo showreel: a still from the product film](design/showreel/poster.png)](design/showreel/navo-showreel.mp4)

The still links to a two-and-a-half-minute product film. It shows the planned product, not only what is built today. The [film note](#about-the-film-the-name-and-the-crates) at the end says which scenes are real.

## Why you would switch

- **You keep your hands on the keys.** Two dense panes, F-keys and Total Commander selection work the way your fingers expect. Nothing makes you wait.
- **You find anything in one box.** Press Ctrl+K and type. Commands, recent folders, open tabs and files appear together, and new files show up within a second.
- **You can see what changed.** Compare two folders, open a pair as a side-by-side diff, and read every difference in one scroll.

Navo does not copy Total Commander's look. It looks calm and modern, and drawers open only when you ask for them.

## Your first five minutes

1. Download Navo from the [releases page](https://github.com/trykA123/navo/releases). Unzip the portable zip and run `navo.exe`, or run the installer.
2. Press Tab to move between the two panes. Press Enter to open a folder and Backspace to go up. Open one folder in each pane.
3. Move the cursor to a file and press F5. Navo copies it to the other pane, with progress and a journal. F6 moves it. Ctrl+Z undoes the last operation.
4. Press Ctrl+K and type a few letters of a file or folder. Press Enter to open the top hit.
5. Open two folders with similar content, one in each pane, and press Ctrl+Shift+D. Navo marks every difference. Press Enter on a file pair to read its diff, and Esc to come back.

The [user guide](docs/user-guide.md) lists the first ten keys to learn.

## A tour

### Two panes, all on keys

Each pane holds any number of tabs, with history and session restore. The list is virtualised, so large folders stay smooth. You sort, filter by typing, mark with keys, mouse or lasso, and drag files between panes. Multi-rename shows a preview and one Ctrl+Z undoes it.

### Ctrl+K and the index

One box finds commands, recent folders, open tabs and files. An optional background service keeps a whole-disk index of NTFS drives, so a file you just made turns up within a second. Alt+F7 searches inside files.

### The F3 viewer

F3 opens the file under the cursor in a view that fits it: text and code with colours, hex, or a rich view. The rich view reads ELF images and linker MAP files (memory regions, an address-ordered strip and a treemap of modules), HEX and S-record files, CAN traces, PDF manuals, JSON, YAML, XML and Markdown. On a folder, F3 previews its size and file count.

### Compare and the Stream

Ctrl+Shift+D compares the two panes. A narrow column between them names each result in a word: changed, only on one side, "3 inside". Enter on a file pair opens a full-window diff with colour washes, the words that changed inside a line, syntax colours and a change map. Long lines scroll sideways, or wrap with Alt+Z. Alt+F opens the Stream: every difference under the folder, one card per file. You can copy a change to the other side, and Ctrl+Z puts the file back.

### The installer

`Navo-Setup.exe` is a guided installer for one user. It has five pages, never asks for administrator rights and works offline. It checks the folder you pick, updates an existing install in place, and Windows Settings removes it. See [docs/features/install.md](docs/features/install.md). It is built and tested, and comes with v0.7.0.

## What Navo is

Navo is for people who move files all day and want to do it from the keyboard. It keeps the behaviour that makes Total Commander fast: two dense panes, F-keys, Total Commander selection, and no waiting.

It is also built for engineers who work with build output, which is why the F3 viewer reads ELF, MAP, HEX, S-record and CAN files.

Navo ships for Windows as a portable zip with `navo.exe` and as a Velopack installer. Both are on the [releases page](https://github.com/trykA123/navo/releases). A guided installer with a choice of folder is built but not released yet; see [docs/features/install.md](docs/features/install.md). It also builds and runs on Linux, which is where most development happens.

## The ideas behind it

Navo started from a short list of things Total Commander does not do. Some are built, some are designed and planned. The table says which.

| Idea | What it means | State on `main` |
| --- | --- | --- |
| One command system | Every action is a typed command. Keys, menus, Ctrl+K, drag and drop and, later, plugins all send the same commands through the same checks. | Built |
| Ctrl+K | One box finds commands, recent folders, open tabs and files. An optional background service keeps a whole-disk index of NTFS drives, so new files show up within a second. | Built |
| Safety model | Every file operation is planned before it runs. Copy, move, rename and delete run as jobs with progress, cancel and a crash-safe journal. Multi-rename is undone with one Ctrl+Z. Invalid settings files are reported and never overwritten. | Built |
| Drawer | Panels that open on demand: the folder preview (F3 on a folder), the job list (Alt+J) and modifier hints. Terminal, compare and clipboard views come later. | Partly built |
| Rich viewers | F3 has simple and rich modes. The rich ELF and MAP view shows memory regions, an address-ordered strip and a treemap of modules. | Built, finishing in the alpha |
| Tiled panes | More than two panes, split like tmux or Hyprland. Today there are two panes with tabs and history. | Planned |
| Saved panel sets | Workspaces on Alt+1 to Alt+9: a saved layout with every pane's tabs, paths, sort and filters. | Planned for the beta |
| Pins | Pinned folders on Ctrl+1 to Ctrl+9, pinned tabs and pinned files. | Planned |
| Plugins | Data packages (themes, keymaps) and sandboxed WebAssembly extensions with explicit permissions. | Designed; planned for v2.0 |

## Status

Navo is in its alpha (v0.5). It is usable, but it is not finished, and it is not yet safe to trust with files you have no other copy of.

- The foundation phase (v0.1) closed on 15 October 2026.
- The alpha is now limited to four features: multi-select with drag and drop, the rich ELF and MAP views, Ctrl+K, and multi-rename. Everything else waits for the beta.
- The [roadmap page](design/roadmap.html) counted 215 of 247 alpha items done on 21 October 2026.
- Each green build of `main` becomes a Windows pre-release on the [releases page](https://github.com/trykA123/rusty/releases). [CHANGELOG.md](CHANGELOG.md) lists what each one changed.

What works today, in short: two panes with tabs, history and session restore; a virtualised file list with sorting and a quick filter; copy, move, rename, new folder, Recycle Bin and Shift+Del with a journal; marking with keys, mouse and lasso; drag and drop between panes and onto tabs, crumbs and drives; copy and paste with Explorer; multi-rename; Ctrl+K with the file index; folder and file compare with the Stream; the preview drawer; the activity strip; settings; light and dark themes.

Known gap: drag and drop with other apps is not finished. Dragging files from Navo into Explorer or an editor does not work yet.

### Roadmap

| Phase | Goal | State |
| --- | --- | --- |
| Foundation, v0.1 | Prove the stack and build the quality gate and CI | Done |
| Alpha, v0.5 | A file manager the owner uses every day on Windows | In progress |
| Beta, v0.9 | Safe, configurable and pleasant: modules, workspaces, undo for every job, themes, installer | Next |
| v1.0 | Replaces Total Commander for daily work: find by content, archives as folders, compare and sync, checksums | Planned |
| v1.1 and v1.2 | Preview pane and thumbnails; Git, duplicates, disk usage, tray and summon hotkey | Planned |
| v2.0 | Open platform: WebAssembly plugins, plugin SDK, embedded terminal, remote file systems | Planned |

The full list, with each item's kind and exit criteria, is in [docs/roadmap.md](docs/roadmap.md).

## How it works

Navo is a Cargo workspace. The UI is drawn with GPUI, the GPU-accelerated framework from the Zed editor, through `gpui-kit`. There is no async runtime in the core.

State has one owner and moves in one direction. `AppState` in `navo-core` is plain data. It changes only through `apply(command)`, a pure function that returns effects. The runtime carries out those effects (list a folder, start a job, save the session) and feeds the results back as events.

```mermaid
flowchart LR
    input["Keys, menus, Ctrl+K,<br/>drag and drop"] --> cmd["Typed command"]
    cmd --> core["navo-core<br/>validate, plan, apply"]
    core -->|effects| run["Runtime"]
    run --> fs["navo-fs<br/>listing, jobs, journal"]
    run --> store["navo-store<br/>settings, session, SQLite"]
    run --> index["navo-index<br/>fuzzy search, disk index"]
    run --> shell["navo-shell<br/>Windows shell"]
    fs & store & index & shell -->|events| core
    core --> ui["navo-ui<br/>draws state"]
```

The main crates:

| Crate | Job |
| --- | --- |
| `navo-core` | Model and logic with no I/O: panes, tabs, layout, selection, sorting, commands, operation plans |
| `navo-fs` | Listing, watching, and the job engine for copy, move, rename, recycle and delete |
| `navo-store` | Settings in TOML, the session snapshot, the `navo.db` and `cache.db` SQLite databases, migrations |
| `navo-index` | Fuzzy matching with `nucleo`, folder walks, and the NTFS index behind Ctrl+K |
| `navo-shell` | Windows shell: icons, context menu, Recycle Bin, clipboard, drag and drop |
| `navo-ui` | The component library and every view, on design tokens |
| `navo-automotive`, `navo-pdf`, `navo-structured`, `navo-syntax` | Readers and models behind the F3 viewer |
| `navo-cache`, `navo-diag`, `navo-l10n` | Bounded caches, redacted diagnostics, Fluent translations |
| `navo-app` | The `navo` binary: wiring, single instance, crash handling, the index service |

`navo-plugins` and `navo-term` are placeholders for later phases. [docs/architecture.md](docs/architecture.md) covers state, storage, caching, start-up and safety in depth.

## Build and run

You need Rust 1.99.0, which `rust-toolchain.toml` pins, and the tools listed in [docs/dev-tools.md](docs/dev-tools.md).

```sh
cargo run -p navo-app --release     # the app, navo
cargo run -p navo-ui-gallery        # the component gallery
```

`navo-ui-gallery --screenshot <dir> --pages <stem,stem,...>` shoots only the listed gallery pages.

On Linux, `scripts/install-local.sh` builds an `x86-64-v3` binary (as CachyOS does) into `target/x86-64-v3/`. It installs it as `~/.local/bin/navo`, with its icons and a `.desktop` entry under `~/.local/share`.

The app icon and wordmark live in `assets/brand/`. After editing an SVG there, run `scripts/build-icons.sh` to rebuild the PNG files and `navo.ico`.

## Test and the quality gate

`cargo xtask gate` runs the whole quality gate. `cargo xtask gate --coverage` adds the coverage floors.

Workers run `cargo xtask check -p <crate> [-p <crate>...]` on the crates they changed. It runs fmt, clippy, taplo, typos, size, l10n, colors and the nextest run for those crates only. The full gate runs on `main` after merging.

The gate runs, in order:

1. `cargo fmt` check
2. clippy
3. taplo (TOML check)
4. typos
5. size limits
6. l10n: the Fluent files parse, every literal message id exists and is used, and the count of hard-coded texts (capitalised literals of two or more words in non-test code of `navo-ui` and `navo-app`) matches `xtask/l10n-allow.txt`
7. colors: no colour is built outside the theme token layer (`rgb(`, `hsla(`, `Hsla {`, `black()`, SVG `fill="#`), except the files listed with a reason in `xtask/colors-allow.txt`
8. cargo-deny
9. cargo-audit
10. nextest
11. doc tests
12. coverage (only with `--coverage`)

Other subcommands: `cargo xtask size`, `cargo xtask l10n [--list]`, `cargo xtask colors`, `cargo xtask coverage`, `cargo xtask binsize <path>`, `cargo xtask bench-copy` and `cargo xtask perf-fixture`.

CI is Windows-first. Every push runs clippy, nextest and the doc tests on Windows, and builds `navo.exe` with its PDB and `pdfium.dll` as an artifact. Pushes to `main` also run a start-up and memory benchmark, described in [docs/perf.md](docs/perf.md). The full gate on Linux runs only when started by hand. The binary size and copy speed checks are informational for now.

## Contributing

Changes merge locally as one squash commit per branch; pull requests are not used yet. If you want to help, open an issue first.

- Each feature has a plain-language spec in [docs/features/](docs/features/). The owner approves it before work starts.
- Plans in [plans/](plans/README.md) cut each feature into small atoms, and each plan ends in a gate.
- Code follows the limits the gate enforces: no `unwrap`, `expect` or `panic`, size limits per file and function, and tests beside each module.
- Text follows [docs/writing.md](docs/writing.md): short sentences, plain words, no internal ids in anything a person reads.

## Documentation

- [Discussion](docs/discussion.md): how the project started, and why Rust and GPUI
- [Architecture](docs/architecture.md): crates, state, storage, caching, start-up, safety
- [Plugins](docs/plugins.md): what a plugin can do, permissions, lifecycle
- [Decisions](docs/decisions.md): every product and design decision, dated
- [Roadmap](docs/roadmap.md), and its visual version in [`design/roadmap.html`](design/roadmap.html)
- [Feature specs](docs/features/): Ctrl+K, multi-rename, multi-select and drag and drop, rich ELF and MAP, compare
- [Performance](docs/perf.md): how start-up and memory are measured, and the baseline
- Chosen designs: the prototypes in [`design/prototypes/`](design/prototypes/) open in a browser
- [Showreel sources](design/showreel/README.md)

## Licence

Navo is licensed under either the MIT licence ([LICENSE-MIT](LICENSE-MIT)) or the Apache licence 2.0 ([LICENSE-APACHE](LICENSE-APACHE)), at your choice.

## About the film, the name and the crates

The film is drawn from the feature specs and design prototypes, with invented file names. It is not a recording of the app. Many of its scenes are on the roadmap and not built yet: tiled panes, saved sets of panes, pins, find by content, archives as folders, the recently deleted view, the drawer with transfers, clipboard and a terminal, compare, CAN traces decoded with a DBC file, plugins and colour themes. Each of those scenes carries a "Planned" tag with its phase. The [status](#status) section says what works today.

Navo is the product and codebase name. Crates use the `navo-` prefix.

Navo was called Ferrite before. An existing Ferrite profile moves to Navo on first start, and the old folder is renamed to `Ferrite.migrated-to-navo`.
