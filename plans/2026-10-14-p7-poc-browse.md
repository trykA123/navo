# P7 · PoC 1: Browse

Status: ready (G2 and G6 passed 2026-10-15) · Branch: `p7-browse` · After: G6

## Goal

**PoC 1.** Navo opens a design B window with two panes that browse real folders on Linux and Windows: tabs, path bar, virtual file list, keyboard navigation, Total Commander selection, light and dark themes. Closing and reopening restores the layout and the visible rows before any database opens.

## Decisions

- D25 (component layers on `gpui-kit`), D16 (themes), D15 (selection), D17 (session first frame).
- The app never imports `gpui-kit` outside `navo-ui/src/primitives/`.
- From G3 review (2026-10-15): window focus, tab activation and scrolling become commands (`window.open`, `window.close`, `window.focus`, `pane.activate-tab`, `pane.scroll`) so replay reproduces them; this removes the targeted `expect(dead_code)` on their state mutators. Cursor moves keep the cursor visible by adjusting `scroll` in `apply`, using the pane's page rows.
- Theme tokens come from variant 1 of the spike (D27), cleaned into `navo-ui/src/tokens/`.
- Coverage floor `navo-ui = 60` added in U1.
- D36 (owner, 2026-10-15): a custom title bar merged with the header row replaces the native Windows caption: `fe_` icon, title, the header's search, and themed minimise, maximise and close buttons. Windows keeps snap layouts on maximise hover, drag to move, double-click to maximise and resize edges.
- Accessibility follow-up from G2: Narrator read nothing on the spike list. Find why (the AccessKit tree is built only when a screen reader connects) and record it; the fix may come after PoC 1, since the owner does not need a screen reader now.

## Components

| Component | Layer | Contract | Test |
| --- | --- | --- | --- |
| `tokens` | ui | `Theme { colors, type, spacing, radii }`, light and dark, mapped onto the `gpui-kit` theme | unit: every token defined in both themes; contrast ≥ 4.5:1 for text pairs |
| `primitives` | ui | `Button`, `IconButton`, `Kbd`, `Tabs`, `VirtualList`, `Splitter`, `Tooltip` | headless tests per primitive |
| `file_list` | ui | rows + cursor + selection → render; key and mouse input → commands | headless: navigation, Space, Insert, right-click select, long right-click event |
| `pane_chrome` | ui | `PaneTabs`, `PathBar` | headless |
| `fkey_bar`, `status_bar` | ui | labels from the keymap; counts and sizes | headless |
| `pane_view` | ui (container) | owns listing tasks, generation counter, streams batches into `file_list` | integration with `fake_fs` |
| `title_bar` | ui | custom caption: icon, title, header search slot, window buttons; hit-test areas for drag, caption buttons and resize | headless: buttons send minimise/maximise/close; screenshot both themes |
| `window_view` | ui (container) | title bar, two panes in a split, rail placeholder, F-key bar | headless + screenshot |
| `gallery` | ui (bin) | every component in every state, both themes | runs; screenshots reviewed |
| `app_wiring` | app | start-up: session → first frame → open databases → list folders; save session on exit | start-up timing test |

## Atoms

| Atom | Component | Tier | Role | Effort | After | Check |
| --- | --- | --- | --- | --- | --- | --- |
| U1 | `tokens` Red | T2 | checker | medium | G6 | tests compile, fail |
| U2 | `tokens` Green | T2 | ui-builder | medium | U1 | `cargo nextest run -p navo-ui tokens` |
| U3 | `primitives` | T2 | ui-builder | high | U2 | headless tests pass |
| U4 | `gallery` | T2 | ui-builder | medium | U3 | gallery runs; screenshots in both themes |
| U5 | `file_list` Red | T2 | checker | high | U3 | tests compile, fail |
| U6 | `file_list` Green | T4 | orchestrator | high | U5 | headless tests pass; 100k rows smooth |
| U7 | `pane_chrome`, `fkey_bar`, `status_bar` | T2 | ui-builder | medium | U3 | headless tests pass |
| U8 | `pane_view` | T4 | orchestrator | high | U6, U7 | integration with `fake_fs` passes; stale batches dropped |
| U8b | `title_bar` | T2 | ui-builder-high | high | U3 | headless tests; screenshots both themes; on Windows CI exe: snap layouts on maximise hover, drag, double-click maximise |
| U9 | `window_view` | T2 | ui-builder | high | U8, U8b | headless + screenshots at 1100 and 1440 px, both themes |
| U10 | `app_wiring` | T3 | api-builder | xhigh | U9 | start-up test: first frame from session only |
| U11 | PoC check on Windows | — | orchestrator | — | U10 | CI `.exe`; owner browses a Windows PC |
| G7 | gate | — | orchestrator + reviewer | high | U1–U11 | gate, CI on both OS, review, **PoC 1 demo** |

U5–U6 and U7 run in parallel after U3; U4 runs in parallel with them.

## Done when

- The owner opens `navo.exe` on a Windows PC, browses with the keyboard, selects with Space and Insert, closes it, and on reopening sees the same layout immediately.
- Gallery screenshots in both themes match design B.
- `navo-ui` coverage ≥ 60 %.

## Log

- 2026-10-15 — G2 and G6 passed; P7 starts on `p7-browse`. The owner wants a testable PoC today and Windows first, so the atoms run as fewer, larger ones: U0 (orchestrator) puts `gpui-kit` 0.7.3 and the low-power GPU patch (`vendor/gpui-pre-windows`) in the workspace; UA = U1–U4 (tokens, primitives, gallery), UB = D36 title bar on gpui-kit's `TitleBar`, UC = U5–U7 (file list, pane chrome, F-key and status bars), all on `ui-builder-high` (Opus high). Core (orchestrator): window, tab and scroll commands, the cursor keeps its row visible, `Location::parent`, `AppState::tab`, `list_missing`. `navo-fs`: `load_folder` (folders first, then names without case). `navo-store`: `StoreDirs` (`%APPDATA%\Navo`, `%LOCALAPPDATA%\Navo`). `navo-ui`: `Browser` controller and the window (U9 + U10, orchestrator). `navo` without arguments opens the window; release builds use the GUI subsystem and CLI modes attach to the parent console. CI: Linux jobs only on manual dispatch.
- Decisions taken on the way: core `Entry` stays name + folder flag; display details stay row-aligned in the pane; fonts are Archivo and JetBrains Mono with their OFL files; the close callback is registered once per window (re-registering on render panicked on Wayland).
- 2026-10-15 — G7 review (Opus high): FIX, then PASS after fixes. Fixed with tests: Tab is a FileList action (gpui-kit Root bound tab), the list re-applies the core scroll when rows arrive, Insert/range/resize keep the cursor visible, no stale rows or errors while a folder loads, link rows open, every close path saves the session (should_close registered once; registering on render panicked on Wayland), the theme follows the system appearance, headless NavoWindow tests. Deferred, recorded here: the first frame does not yet show the session's cached rows (D17 / Goal; disk listing follows at once); superseded listings are not cancelled; Space does not size folders (MeasureFolder); the stale-answer browser test needs a gated fake; a symlink to a file is treated as openable and Enter leads to a "not a folder" location (resolve link targets in LocalFs); the Linux close button is checked by hand only.
