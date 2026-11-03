# P17 · Context menu drawn by Navo

Status: ready · Repo: `<repo>` · Branch: `p17-mN` per atom · Started: —

## Goal

A long right-click, the Menu key or Shift+F10 opens the Windows context menu for the selection, drawn in Navo's own style (D46). Typing filters every command, including submenu items. Navo's actions sit on top. Shell extensions (7-Zip, Notepad++, Git, Defender) work as in Explorer.

## Decisions

- **D1 — Shape (D46).** Variant 1 filter list; section headers (Navo, then Windows groups) when the filter is empty; a "More" fold for rare verbs; cascading submenus; a "Classic menu" entry falls back to the native menu.
- **D2 — Opening (D15).** Long right-click (about 400 ms), the Menu key, Shift+F10. A short right-click still selects.
- **D3 — One STA thread owns the shell.** All COM calls (`IContextMenu`, `IContextMenu2/3`, `IExplorerCommand`, invoke) run on one dedicated single-threaded-apartment thread with a hidden message window. The UI never blocks on it; requests and results cross by channel.
- **D4 — A plain model in between.** The provider returns a `ShellMenu` tree: id, text, access key, icon as RGBA, state (enabled, checked, default), submenu (filled or lazy), group hint. The UI filters and draws only this model, so the component is tested headless with a fake.
- **D5 — Lazy submenus.** Build the menu with `CMF_SYNCCASCADEMENU` so "Send to" and similar fill during `WM_INITMENUPOPUP` (54–134 ms in M1); without it they fill on a shell thread while messages are pumped. An unfilled submenu is one item with blank text: treat it as Loading. Fill in the background when the menu opens, with a 250 ms budget per submenu; a slow handler is skipped quietly and its items appear when it answers. Extended verbs (Shift set) stay off for the default view.
- **D6 — Owner-drawn items.** Items that only draw themselves show their text if any; otherwise they are reachable through "Classic menu".
- **D7 — Windows 11 commands.** Packaged commands already appear in the classic `IContextMenu` (M1: Windows Terminal, with its CLSID as the verb), so there is no separate path. Explorer-only commands are unverified on the CI runner; the owner checks on a Windows PC.
- **D9 — Keep the menu object alive.** Item ids work only on the `IContextMenu` that produced them: the STA thread keeps it while Navo's menu is open and invokes by id, by canonical verb only when an item has no id. The STA thread is warmed at start (the first menu in a process takes 58–300 ms, later ones 10–30 ms).
- **D8 — Linux.** The provider returns only Navo's actions (D30).

## Components

| Component | Layer | Contract (in → out) | New / change | Test |
| --- | --- | --- | --- | --- |
| `ShellMenu` model + filter | navo-ui (or navo-core if shared) | `ShellMenu, query → rows` | new | unit, headless |
| `shell_menu` provider | navo-shell (`windows`) | `paths → ShellMenu`; `id → invoke`; `lazy id → items`; `classic(paths, point)` | new | Windows CI tests on temp files |
| `ContextMenu` component | navo-ui | model + keys → rendered menu, chosen action | new | headless UI tests, gallery page, screenshots |
| wiring | navo-ui, navo-app | long right-click, Menu key, Shift+F10 → menu; Navo actions | change | UI tests with the fake provider |

## Atoms

| Atom | Component | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| M1 | spike: provider on Windows | api-builder (Opus high) | high | — | Windows CI job prints the menu tree for a `.zip`, a folder and 3 files; reachable Windows 11 commands listed | done |
| M2 | `ShellMenu` model + filter | Red checker, Green orchestrator | medium | — | `cargo test` model tests | done |
| M3 | `shell_menu` provider (STA thread, lazy submenus, icons, invoke, classic) | api-builder (Opus high) | high | M1, M2 | Windows CI tests | done |
| M4 | `ContextMenu` component per D46 | ui-builder-high | high | M2 | headless UI tests, gallery screenshots both themes | done |
| M5 | wiring and Navo actions | ui-builder | medium | M3, M4 | UI tests with the fake provider; owner tries it on Windows | done |
| M7 | F3 and F4 context menu per kind (D51), on the M4 component | ui-builder | medium | M5 | UI tests per kind: right-click, Menu key, Shift+F10, each item dispatches its action | done |
| M7b | Missing viewer actions for D51 items: Find selection, Copy path:line, tree Copy value and Copy key, table Copy column, Markdown Copy and Copy link, image Copy image and Copy path; then add them to the viewer menu | ui-builder | medium | M7 | UI tests per action and per menu item | done (92f1000) |
| M8 | Background menu (owner, 2026-10-19: right-click on the empty part of a pane showed nothing). Right-click or the Menu key where no row is under the pointer (empty list area, below the last row, an empty folder) opens the folder's own menu: a Navo section (New folder F7, New file…, Paste, Refresh, Sort by ▸, Open terminal here, Copy folder path, Properties) above the Windows background menu of that folder (shell "New ▸" with Text Document and the other ShellNew types, "Open in Terminal", etc.) through the same provider and component as M3–M5. A new `files.new_file` command: an inline name field like New folder, creates an empty file, puts the cursor on it; its key is a free one checked against keymap.toml (Shift+F4 stays Edit with) | ui-builder (Opus medium) | medium | M5 | UI tests: right-click on the empty area and in an empty folder opens the background menu, nothing is marked or moved, New file creates and selects the file, Escape cancels; Windows CI: the background menu of a temp folder lists New ▸ Text Document; screenshots both themes | done |
| M6 | review | reviewer (Opus high) | high | M5, M7 | verdict | done |

## Done when

- A long right-click on a `.zip` shows 7-Zip's submenu in Navo's style; "Extract Here" works.
- Typing "send" shows the "Send to" targets; Enter on one runs it.
- "Classic menu" opens the native menu at the same place.
- The gate and Windows CI are green; the reviewer passes the branch; the owner confirms on a Windows PC.

## D72 — Native-first menu with drill-in (beta, D74)

Status: ready after P36 X6 · Branch: `p17-nN` per atom. The product is Navo (D78); code names (`NavoAction`, `navo_*`) stay until the rename plan. UI text in Inter, the context line and crumbs in JetBrains Mono, colours only from theme tokens.

### Decisions (D72 amends D1, D6 and M8's Navo section)

- **N-D1 — Explorer's list, our look.** The root page shows the shell items in Explorer's order with Explorer's separators. No section headers, no Navo section, no "More" fold. Shift held at open adds extended verbs (`CMF_EXTENDEDVERBS`), as in Explorer.
- **N-D2 — Filter stays.** The filter field from D46 stays at the top and keeps focus. On the root page it searches the whole tree, filled lazy submenus included; rows inside submenus show a crumb ("Send to › Desktop"). Inside a drilled page it searches that page and its descendants.
- **N-D3 — Drill-in, never cascades.** A submenu replaces the page in the same panel. A back row ("‹ Send to") sits under the filter. The panel keeps its top-left corner; the width is the widest page seen, clamped to the window; the height follows the page inside the window bounds. Pages slide 16 px with a fade (prototype variant 3); under reduced motion they swap at once.
- **N-D4 — Keys.** ↑↓ Home End PgUp PgDn move. Enter runs a command or drills into a submenu. → drills in. ← and Backspace go back, only while the filter is empty. Esc clears the filter, then goes back one page, then closes. Mouse: a click on a submenu row drills in; hover never opens anything; mouse button 4 goes back. Shift+F10 inside the menu opens the classic menu.
- **N-D5 — Lazy pages.** Drilling into a lazy submenu shows a quiet Loading row at once and asks `MenuSession::fill` (250 ms budget, D5). Failed shows Retry. No spinner flash: the Loading row appears only after 120 ms.
- **N-D6 — Owner-drawn items.** Items with `HBMMENU_CALLBACK` or `MFT_OWNERDRAW` get their icon by sending `WM_MEASUREITEM`/`WM_DRAWITEM` through `HandleMenuMsg2` into a 16 px memory DC on the STA thread, read back as RGBA. An item with no text after that is left out and counted; the footer then shows "N items only in the classic menu".
- **N-D7 — Fallback.** When `open` fails, or the tree has no named item, Navo calls the native `TrackPopupMenuEx` at the same point without asking (a new `ShellMenuService::classic_for(source, hwnd, point)` that queries and tracks in one STA request). A slow open (over 2 s, `HUNG_AFTER`) shows "Windows menu is slow" with Classic and Retry rows; the hung STA thread is replaced as today.
- **N-D8 — Threads.** No change to D3/D9: every COM call stays on the one STA thread with its message window, `InvokeCommand` uses `CMINVOKECOMMANDINFOEX` with `CMIC_MASK_UNICODE | NOASYNC | PTINVOKE`, by id first (with the folder as `lpDirectory`, so static verbs such as "Open Git Bash here" know their target) and by verb only for an item with no id. The UI only polls `Pending`.
- **N-D9 — Linux.** Keeps today's menu: the Navo actions as the root list (D8), with drill-in for Sort by.
- **N-D10 — Where Navo actions go.** Owner question Q1 below; until answered, they leave the list and stay reachable through the filter (a typed match shows them under the shell matches) and Ctrl+K.

### Components

| Component | Layer | Contract (in → out) | New / change | Test |
| --- | --- | --- | --- | --- |
| `shell_menu::rows` | navo-core | `ShellMenu, page, query → rows` (no sections, crumbs, back row) | change | unit, headless |
| `context_menu` state + keys | navo-ui | page stack, keys → events | change | headless UI tests |
| `context_menu` panel | navo-ui | page + motion → element | change | UI tests, gallery screenshots |
| `shell_menu` provider | navo-shell (`windows`) | extended verbs, owner-drawn icons, `classic_for` | change | Windows CI tests |
| wiring | navo-ui | open, fallback, Linux path | change | UI tests with the fake provider |

### Atoms

| Atom | Component | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| N1 | `shell_menu::rows`: Explorer order, no headers or More fold, navo items only as filter matches, page-scoped filter | patcher (Sonnet) | medium | X6 | `cargo xtask check -p navo-core`; tests: root rows equal shell order with separators, a query finds a filled submenu item with its crumb, a query inside page `[3]` returns only its descendants, separators never lead, trail or double | ready |
| N2 | `context_menu` state + keys: page stack replaces cascade levels | senior (Opus medium) | medium | N1 | headless tests: → and Enter drill, ← and Backspace back only with an empty filter, Esc order clear → back → close, selection returns to the parent row on back, a lazy page asks once and fills in place, hover opens nothing | ready |
| N3 | `context_menu` panel: one panel, back row, width rule, slide and reduced motion | ui-builder (Sonnet) | medium | N2 | UI tests: back row present on a page and clickable, panel stays inside a 390 px wide window after three drills; gallery page with root, page, Loading and Failed in both themes, screenshots read | ready |
| N4 | provider: `CMF_EXTENDEDVERBS` on Shift, owner-drawn icons through a memory DC | senior (Opus medium) | medium | — | Windows CI: Shift menu of a `.txt` has `runas` or `copyaspath` beyond the plain menu; no item with `HBMMENU_CALLBACK` leaves the walk without an icon or a count | ready |
| N5 | provider: `classic_for` and the "no named item" signal | senior (Opus medium) | medium | N4 | Windows CI: `classic_for` on a temp file returns within the test's posted Esc (`None`), never panics on a dead STA; unit test for the empty-tree signal | ready |
| N6 | icon cache: process-wide `RenderImage` cache keyed by an RGBA hash, 256 entries | patcher (Sonnet) | medium | — | test: the same icon in two menus uploads once; the cache never passes 256 | ready |
| N7 | wiring: file and background menus without the Navo section, Shift at open, automatic fallback, Linux path | ui-builder (Sonnet) | medium | N3, N5 | UI tests with the fake provider: no Navo rows on Windows, Navo rows on the stub, an open error starts Classic, a slow open shows Classic and Retry; `run.rs` offset logic removed with its tests updated | ready |
| N8 | Windows CI known-verb tests: Send to drill path and invoke | senior (Opus medium) | medium | N7 | Windows CI: Send to fills, "Compressed (zipped) folder" invoked by id creates a `.zip` beside a temp file; background menu of a temp folder lists New ▸ Text Document | ready |
| N9 | review of N1–N8 | reviewer (Opus medium) | medium | N8 | verdict | ready |

N1, N4 and N6 run in parallel. N2 → N3 and N4 → N5 are serial. Each atom touches at most three files plus its tests.

### Atom notes

- **N1 read:** `crates/navo-core/src/shell_menu/rows.rs`, `filter.rs`, `mod.rs` (`Row`, `Section`, `ItemRow`). Delete `Section` and `Row::More`; add `Row::Back { title }`; `rows(menu, page: &[usize], query)`. Stop if a caller outside `navo-ui::context_menu` uses `Section`.
- **N2 read:** `crates/navo-ui/src/context_menu/state.rs`, `keys.rs`, `tests/submenu.rs`, `tests/keys.rs`. `Level` becomes a `Page { path, selection }` stack; `ContextMenuEvent::Fill` stays as is. Stop if a viewer menu (D51) or `drop_menu.rs` depends on cascade levels beyond `open_submenu`.
- **N3 read:** `crates/navo-ui/src/context_menu/panel.rs`, `motion.rs`, `place.rs`, `mod.rs` (`submenu_spot`, `frames`). Remove the per-depth frames. No timers or focus changes in render; the slide reads `Instant` like `grow_done`.
- **N4 read:** `crates/navo-shell/src/shell_menu/win/loaded.rs`, `read.rs`, `icon.rs`, `sta.rs` (`forward_menu_message`). `MenuSource` gains `extended: bool`.
- **N5 read:** `crates/navo-shell/src/shell_menu/mod.rs`, `win/invoke.rs` (`classic`), `win/worker.rs`, `stub.rs` (returns `MenuError::Unsupported`).
- **N7 read:** `crates/navo-ui/src/window_view/shell_menu/mod.rs`, `run.rs`, `background.rs`, `tests/shell_menu.rs`. Alt+M, Menu key and Shift+F10 bindings stay (`keymap.toml` unchanged).

### Done when

- The menu of a `.zip`, a folder, three files and empty space matches Explorer's classic menu, item for item, on a Windows PC.
- Send to and Open with drill in and back with the keyboard and the mouse; typing "desk" on the root page finds Send to › Desktop.
- An enumeration failure opens the native menu at the same place; the gate and Windows CI are green; N9 passes.

### Owner check on Windows

1. Right-click a `.zip`: same items as Explorer's "Show more options", 7-Zip and Git included, with their icons.
2. Shift+right-click: "Copy as path" and "Open PowerShell window here" appear.
3. Send to ▸ Desktop with the mouse, then with ↓ → Enter; Backspace and Esc go back; Esc on the root closes.
4. Type "send" and "zip": the right rows, with crumbs; Enter runs them.
5. Open with ▸ Choose another app; a slow extension (Defender scan) does not freeze the window.
6. Right-click empty space: New ▸ Text Document creates a file; Navo selects it.
7. Near the right and bottom window edges, three drills deep, the panel stays inside the window.
8. With reduced motion on in Windows, pages switch without sliding.

## Log

- 2026-10-16 — Plan written after the owner picked the designer's recommendation (D46). Prototype: https://claude.ai/artifact/3yRKtiJ1xaYjnW45Dqz6xL
- 2026-10-16 — M2 (8373238) in `navo_core::shell_menu`; review PASS WITH FIXES (disabled parents, selection kept by path, empty text skipped, `set_submenu`, Classic menu action) fixed in dbbd4f2; merged. M1 spike running on Windows CI.
- 2026-10-16 — M1 spike done (cc04f67, run 38851289858): 38–70 items, 36–48 with verbs; 7-Zip submenu complete; invoke by id works (Extract Here, copy → CF_HDROP); icons are 32-bit bitmaps, converted to RGBA; 0 owner-drawn; Send to needs `CMF_SYNCCASCADEMENU`. D5 and D7 updated, D9 added; merged.
- 2026-10-16 — M4 (88d5579, review fixes 306d3f3) and M3 (499874b, review fixes 3a75452; Windows run 38855214205 green) merged. M3 timings on the runner: open 350–780 ms, Send to fill ~436 ms (the fill blocks past the 250 ms budget and still returns Filled; revisit in M5 with an async fill). M5 contract: the UI polls `Pending` with `wait_timeout`, never blocks; restore focus via `ContextMenuInput.restore`; call `context_menu::bind_keys` in init.
- 2026-10-16 — M7 added: the owner asked for a context menu in the F3 viewer (D51).
- 2026-10-16 — M7 (06cc311) built: viewer menu per kind on existing actions only. Deferred to M7b because no action exists yet: Find selection, Copy path:line, tree Copy value and key, table Copy column, Markdown Copy and Copy link, image Copy image and Copy path. A Markdown file with no link under the pointer has no menu until M7b. Review PASS WITH FIXES; fixes running.
- 2026-10-16 — M7 review fixes (f06bf3a) merged: stale Markdown link cleared in capture phase, build_action errors assert and show a header note, a re-entrant update panic on window-handled items fixed with window.defer, 16 viewer menu tests. M5 was merged earlier (d798384).
- 2026-10-16 — M7b merged (92f1000): Find selection, Copy path:line, tree Copy value (refuses shortened values) and Copy key, table Copy column, Markdown Copy and Copy link, image Copy image (PNG only on Windows) and Copy path. Released in alpha-2026-10-16k with M7 and L4. Open: Ctrl+C image inside the preview drawer is unchecked. Next: M6 review of M5 and M7.
- 2026-10-16 — M6 review PASS WITH FIXES; fixes merged (p17-m6-fixes, CI 38937246695 green): the Windows menu refuses names Win32 would rewrite (trailing dot or space, device names) so a shell verb can never hit another file; viewer right-click targets the clicked tree row or table cell; Retry resends a failed submenu; dropped prefetch opens are cancelled before the shell builds them; Linux shows only working Navo actions; long-press marks the pressed row; tree Copy value walk is bounded by the cap. Deferred: Find selection and Copy path:line in the paged large-file view.
- 2026-10-17 — Status closed: M1 (merged 164bd28; decisions D5, D7 came from it) was done but still marked running.
- 2026-10-19 — Owner: right-click on the empty part of a pane shows no menu, so there is no way to create a file from the mouse. M8 added (background menu and a New file command).
- 2026-10-21 — Owner checked M8 on a Windows PC (Windows): right-click on empty space works. M8 done.
- 2026-10-21 — D72: owner confirmed the native-first menu, restyled, no Navo section, and asked to keep the current filter field. Needs a plan section (atoms) before work starts; it comes after P36 X6 drag-out.
- 2026-10-26 — D72 section planned (N1–N9): most of the shell side exists from M3 (lazy fill, invoke by verb then id, classic, hung STA replacement), so the work is the drill-in UI, the list without sections, extended verbs, owner-drawn icons and the automatic fallback. Q1 (Navo actions) and Q2 (which Explorer menu) asked.
- 2026-10-26 — Owner answered the D72 questions: Navo's own actions stay out of the list (reachable by typing in the filter and from Ctrl+K; on the background menu Sort by, Refresh and New file sit where Explorer shows them); the menu mirrors the full classic Windows menu, Shift adds the extended verbs.
