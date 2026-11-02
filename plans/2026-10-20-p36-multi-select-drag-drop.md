# P36 · Multi-select and drag and drop

Status: ready (owner answered Q1–Q4 on 2026-10-20) · Repo: `<repo>` · Branch: `p36-<atom>` per atom · Started: 2026-10-20

## Goal

The user can mark files from the keyboard and the mouse the way Windows and Total Commander users expect: Shift ranges that grow and shrink, Ctrl+A, unmark all, restore the last marks, mark by extension, Ctrl+click and Shift+click. The user can drag files inside Navo onto the other pane, a folder row, a tab, a breadcrumb or the drive pill, then drag files in from Explorer and the desktop, and drag files out to Explorer and other programs. Every drop into Navo runs through the planner, the job journal and the copy card, like F5/F6.

## What exists today (checked at 9d36589)

- **Marks.** Space and Insert toggle. `+`, `-` and `*` mark, unmark or invert by pattern. They push into the filter when a filter is active (`file_list/mod.rs:195-205`).
- **Core commands without keys.** The core already has `SelectionRange { to }` (marks only, from `Selection::anchor`), `SelectionAll` (files only, `folders = false`) and `SelectionClear`. None of them has a key.
- **Right-drag is incomplete.** `press.rs` cancels the menu prefetch when the pointer reaches another row. On release it toggles only the start row. D15's "right-drag selects a range" is not implemented.
- **Clicks.** A left click moves the cursor only. Modifiers are ignored. The empty area below the rows has only the right-button background menu.
- **The filter drops marks.** `replace_listing_near` → `keep_by_name` drops the marks of rows that the filter hides. Clearing the filter does not bring them back.
- **No job undo for the user.** The job journal and the A8 hold-aside exist, but "no undo is offered for finished jobs yet" (P30 log). Ctrl+Z for jobs comes with the D61/D65 work.
- **Same volume.** `volume_of` compares strings only. On Linux every path gives `/`, so a cross-device F6 or paste plans a `Rename` that fails with EXDEV. Sources stay intact, but this bug exists today. `ProviderOps::volume_serial` exists and is not used for this.
- **GPUI internal drag.** `on_drag` / `drag_over` / `on_drag_move` / `on_drop` / `can_drop` work on every platform (`side_sheet` uses them). Drags start on the left button only (`div.rs` near line 2984). GPUI does not cancel a drag on Esc.
- **GPUI drops from outside.** `vendor/gpui-pre-windows/src/window.rs:1144-1287` registers an `IDropTarget` that reads `CF_HDROP` and sends `FileDropEvent::{Entered, Pending, Submit, Exited}`. GPUI turns these into an `ExternalPaths` drag (`gpui-pre-0.3.9/src/window.rs:5656-5710`).
  - It always reports `DROPEFFECT_COPY`.
  - It ignores `grfKeyState`.
  - The synthesized mouse events carry `Modifiers::default()`.
  - It never sends `FileDropEvent::Ended`.
  - Drops also work on Linux, on both X11 and Wayland (`gpui-pre-linux`).
- **GPUI drag out.** gpui-pre 0.3.9 already has the API: `external_drag_payload` on `on_drag`. When the pointer leaves the viewport with the left button down, GPUI calls `start_external_drag` (`window.rs:5763-5792`). When the drag comes back over the window, `restore_platform_drag` restores the internal drag. macOS and Wayland implement `start_external_drag`. The Windows crate does not.
- **Mouse capture.** The Windows crate calls `SetCapture` on mouse down (`events.rs:476`), so moves outside the window still arrive.
- **COM and threads.** The GPUI UI thread is already STA: `OleInitialize` at `vendor/gpui-pre-windows/src/platform.rs:183`. `navo-shell` has a PIDL helper (`shell_menu/win/selection.rs`), `CF_HDROP` encode/decode (`clipboard/model.rs`) and a separate STA worker for the shell menu.
- **Explorer clipboard (P24).** `FileClipboardService` in `window_view/clipboard.rs` and `navo-app/src/clipboard.rs` (`path_location`, `without_verbatim`, `navo_fs::os_path`). `Browser::paste` is the closest model for a drop: an explicit source list, the target is the active folder, `asks: false`.

## Decisions

Final. The owner answered Q1–Q4 on 2026-10-20 (see the Log). Workers never reopen them; a worker that finds one wrong stops and reports. Items marked  wait for the owner's yes.

- **D1 — Three shippable parts, in this order.** Each part ends in a gate (`cargo xtask gate`, Windows CI, an Opus high review, merge, an alpha build).
  - Part K: keyboard and mouse multi-select.
  - Part D: drag and drop inside Navo.
  - Part X: Explorer in (X-in), then Explorer out (X-out).
  - The plan has 31 atoms. Under the about-15-atoms rule, the orchestrator may save the parts as `p36a-multi-select`, `p36b-drag-drop` and `p36c-explorer-drag`, with the decisions shared.
- **D2 — The Total Commander mark model stays (D15).** Marks are separate from the cursor; a plain click and plain arrows never change marks. Every new gesture adds or removes marks. Only an explicit unmark-all and the empty-space click (D7) clear them, and both save the marks first (D5).
- **D3 — Span.** One mechanism serves Shift+keys, Shift+click and right-drag.
  - The first extend saves the current marks as the *base* and the cursor row as the *anchor*. Every extend sets marks = base with every row in [anchor, cursor] set to `mark` (cleared in unmark mode, flipped from the base in toggle mode).
  - Moving back shrinks the span to the base.
  - The span ends at any other command, and when the listing is replaced.
  - Marks add to what exists (base ∪ span). This differs from Explorer, where Shift+click replaces the selection, and keeps marks safe.
- **D4 — Keys.** See the key table below. All new keys live in the `FileList` context, are rebindable in `keymap.toml`, and get `cmd-` labels in `commands.ftl`.
- **D5 — Restore last marks.** 
  - Scope: one saved set per window, on `AppState`, stored as names and bounded to 100,000 names. It is not saved in the session.
  - When it is saved: a non-empty mark set is cleared by unmark-all, by the empty-space click, by a finished job (`finish_job` → `SelectionClear`) or by leaving the folder.
  - What `/` does: it marks the saved names that exist in the *current* folder. So after F5, Tab and `/` mark the new copies in the target pane, a Total Commander habit.
  - It never clears marks. When nothing matches, the status line says "No saved marks match this folder".
- **D6 — Marks survive the filter.** 
  - Marks on rows that the filter hides are kept by name and come back when the filter changes or clears.
  - F5, F6, F8, Ctrl+C/X and drag act only on *visible* marked rows, so hidden files are never deleted by surprise.
  - The footer shows "· N marked not shown" (marks hidden by the filter or by show-hidden/show-system).
- **D7 — Mouse.**
  - Ctrl+click toggles the row and moves the cursor there. Shift+click (and Ctrl+Shift+click) extends the span from the cursor to the row.
  - Ctrl and Shift clicks act on mouse up (`on_click`), and mouse-down does not move the cursor while Ctrl or Shift is held. This way Ctrl+drag copies without toggling the row, and the Shift anchor stays the old cursor (the Windows behaviour).
  - A plain click works as today.
  - A plain left click on the empty area below the last row unmarks all, after saving the marks (D5). 
- **D8 — Right-drag stays Total Commander range marking.** 
  - D15 gets finished: right-drag paints rows in D3 span mode. It marks, or unmarks when the start row was marked, and shrinks when the pointer moves back. A release after a drag does not toggle.
  - Windows uses right-drag for a drop menu, but GPUI starts drags only on the left button. Right-button file drags would need our own drag machinery and could never be promoted to an OLE drag.
  - Navo shows the Windows drop menu when a *right-button drag from Explorer* drops on Navo (X-in, D16).
  - An Explorer-style right-drag setting comes later if the owner asks.
- **D9 — What a drag carries.**
  - If the pressed row is marked, the drag carries all visible marked rows. Otherwise it carries only the pressed row. Marks do not change.
  - The sources are frozen as `Location`s when the drag starts, so a watcher refresh during the drag cannot change them.
- **D10 — Targets and the target folder.**

  | Target | Target folder |
  | --- | --- |
  | Folder row, in either pane | That folder |
  | File row or list background | That pane's folder |
  | Tab | That tab's folder |
  | Breadcrumb segment | That segment's folder |
  | Drive pill | Root of the drive the pill shows |

  - Refused drops: the source's own folder ("Already in this folder"), and a folder into itself or below itself ("Cannot drop a folder into itself"). These are the `PasteRefusal` rules.
  - Refused targets show a muted badge, and a release does nothing.
- **D11 — Operation rule (Windows).** `navo_core::drop_op(same_volume, DropKeys) -> DropOp`:
  - Alt, or Ctrl+Shift → Link (a shortcut);
  - Ctrl → Copy;
  - Shift → Move;
  - no modifier → Move on the same volume, Copy otherwise.
  - Same volume means `volume_of` is equal, as for F6 and paste. Other providers count as another volume.
  - There is no `.exe` exception. Explorer links `.exe` files by default; Navo does not.
  - The planner checks the volume serial again before it plans a rename (V1).
- **D12 — Badge.** The GPUI drag view follows the pointer.
  - First line: the icon and "Move 3 items to Projects", "Copy 1 item to D:\" or "Create 3 shortcuts in Desktop".
  - Second line: "Ctrl copy · Shift move · Alt shortcut · Esc cancel". It follows the window's key-hints part (D58 rule 7).
  - Refused: the muted look plus the reason.
  - Text lives in `files.ftl` (D59). The badge reacts live to modifier changes.
  - Esc cancels the drag (`cx.stop_active_drag`, through a capture key listener that runs only while a drag is active).
- **D13 — Drops run like F5/F6.**
  - `Browser::drop_into(DropRequest) -> Result<u64, DropRefusal>`. `paste` becomes a call to it with the active folder.
  - The job shows in the copy card with Pause and Cancel.
  - Confirmation follows `[files] drop-confirm = "never" | "moves" | "always"`. The default is `"never"`, as in Explorer (owner, Q2).
  - Protected locations always ask, as today.
  - Undo: drop jobs write the same journal as F5/F6, so they get undo when job Ctrl+Z lands. Before that, nothing is undone.
- **D14 — Shortcuts (Alt).** 
  - New `OpKind::Link`, through the planner and journal.
  - Windows: `<name> - Shortcut.lnk` (Explorer's name), written with `IShellLinkW` + `IPersistFile` on the job thread (`CoInitializeEx`).
  - Linux: a symlink with the source's name.
  - A conflict gets a number (`(2)`). Undo removes the created link.
  - This part ships after copy and move in Part D.
- **D15 — Optional extras.** 
  - Auto-scroll while dragging near the top or bottom 24 px of a list.
  - Spring-loaded targets:
    - a hover of 800 ms on a folder row opens that folder in its pane;
    - 500 ms on a tab activates the tab;
    - 800 ms on the drive pill opens the drive menu, whose rows become targets.
  - All of these are off when `reduce-motion` asks for that.
- **D16 — Drops from Explorer (X-in).**
  - The same targets accept `ExternalPaths` and `FileDrag` through one `DropPayload` enum.
  - The vendored drop target is patched:
    1. It stores `grfKeyState` per window.
    2. It dispatches the GPUI event first, then reads the drop-effect hint the app set, and returns that hint from `DragEnter` / `DragOver` (`DROPEFFECT_NONE` when no target accepts).
    3. `Drop` never returns `DROPEFFECT_MOVE`, so the source never deletes. Navo does every move itself through the planner. B0 confirms whether `NONE` or `COPY` is returned for a move.
    4. It exports `drag_key_state(hwnd)` and `set_drop_effect(hwnd, effect)`.
  - A right-button drag from Explorer opens Navo's drop menu: Copy here, Move here, Create shortcuts here, Cancel.
  - Virtual files (Outlook attachments, zip contents, phones) carry no `CF_HDROP` and are refused, as in P24 D4.
- **D17 — Drag out (X-out).**
  - The OLE work lives in `navo-shell::drag_out`:
    - `data_object(paths)` builds a shell data object with `SHCreateDataObject(parent PIDL, child PIDLs)`, reusing the `Pidl` type, which moves to `navo-shell/src/pidl.rs`;
    - `drag_files(hwnd, paths)` runs `SHDoDragDrop` with no drop source (the shell's default source and drag image) and allows COPY | MOVE | LINK.
  - The vendored crate gets a thin hook:
    - `can_start_external_drag()` returns true when a handler is registered;
    - `start_external_drag(payload)` stores the payload and posts `WM_USER + 11` to its own window;
    - the message handler calls the handler that navo-app registered, then sends `FileDropEvent::Ended`.
  - Threading:
    - Everything runs on GPUI's UI thread, which is STA and already initialized with OLE.
    - `SHDoDragDrop` is modal and pumps messages, so it starts from the posted message and never inside GPUI's event dispatch. Inside the dispatch it would re-enter borrowed App state.
    - The shell-menu STA worker is not used, because its thread does not own the mouse capture.
  - Effects:
    - Navo never deletes after a drag out. The target (Explorer) copies or moves and keeps its own undo.
    - When the drag ends, the source pane is refreshed for folders without a watcher.
    - A drag that comes back over Navo becomes internal again (`restore_platform_drag`).
- **D18 — Linux (secondary, D30).**
  - Works: keyboard and mouse marks, the internal drag, drops in from file managers on X11 and Wayland, drag out on Wayland (GPUI's own code), Link as a symlink.
  - Does not work:
    - modifiers during drops from other apps (no key state reaches Navo, so the default rule applies);
    - drag out on X11;
    - the Explorer drop menu.
  - V1 fixes the cross-device move. Before V1, the default on Linux is always Move.
- **D19 — Who builds what (the owner's ladder).**
  - Sonnet medium (`ui-builder`): well-specified UI.
  - Sol 6.1 xhigh (`api-builder`, codex): backend only, never UI.
  - Opus medium (`ui-builder` or `api-builder`): complex or cross-cutting UI, and the OLE layer.
  - Opus high (`reviewer`): reviews of every part that decides what is copied, moved or deleted.
  - This ladder supersedes the "no gpt-6.1-sol" line of D35 and the tier table in `plans/README.md`. The orchestrator records it as a decision if it has not done so already.

- **D20 — Left-drag lasso (owner, 2026-10-20: "putem sa ne folosim de click stanga pentru multi select cu mouse-ul").** A left-button drag that starts on empty space (below the last row, or right of a row's name text in the Name column) draws a lasso and marks the rows it crosses in D3 span mode (base ∪ span; Ctrl+lasso toggles, so it can unmark). A left drag that starts on a row's name or icon is a file drag (D9). A click without movement on the empty area keeps D7 (unmark all, after saving the marks). The lasso auto-scrolls near the list edges (D15). Atom K5b.

### Key table (Part K)

All keys are in the `FileList` context. Checked against `keymap.toml` and `typed_char`: Ctrl and Alt chords and keys without a `key_char` never reach the filter.

| Key | Command id → core | Does | Conflicts checked | Owner |
| --- | --- | --- | --- | --- |
| Shift+↑ / Shift+↓ | `files.extend_up` / `files.extend_down` → `SelectionExtend { to: Up/Down, mode: Mark }` | Extends from the anchor and shrinks when moving back (D3) | None in FileList; the Shift F-key layer only lights while held | — |
| Shift+PgUp / Shift+PgDn | `files.extend_page_up` / `files.extend_page_down` | As above, by a page | None | — |
| Shift+Home / Shift+End | `files.extend_first` / `files.extend_last` | As above, to the first or last row | None | — |
| Ctrl+A, Ctrl+Num+ | `files.mark_all` → `SelectionAll` (now **files and folders**) | Marks every visible row | `ctrl-a` is unused everywhere; `ctrl-+` only in Image/Pdf (other contexts) | — |
| Ctrl+Shift+A, Ctrl+Num− | `files.unmark_all` → `SelectionClear` (saves, D5) | Unmarks all | `ctrl-shift-a` is unused; `ctrl--` only in Pdf | — |
| `/` (Num /) | `files.restore_marks` → `SelectionRestore` | Marks the saved names here (D5) | Today `/` types into the filter, but names cannot contain `/`, so the filter loses nothing | yes (D5) |
| Alt+Num+ (`alt-+`) | `files.mark_extension` → `SelectionExtension { mark: true }` | Marks every file with the cursor file's extension (case-insensitive; no extension marks files without one) | `alt-left/right`, `alt-v`, `alt-m`, `alt-f1/f2` are the only Alt keys in FileList | — |
| Alt+Num− (`alt--`) | `files.unmark_extension` → `SelectionExtension { mark: false }` | Unmarks those files | As above | — |
| Ctrl+click | `PaneCursorMove` + `SelectionToggle`, on mouse up | Toggles one row (D7) | Today a click ignores modifiers | — |
| Shift+click, Ctrl+Shift+click | `SelectionExtend { to: Row(r) }`, on mouse up | Range from the cursor (D7) | As above | — |
| Click below the last row | `SelectionClear` (saves) | Unmarks all (D7) | Today: no left handler | yes |
| Right-drag over rows | `SelectionExtend { to: Row(r), mode: Unmark if the start row was marked, else Mark }` | Paints a range (D8) | Keeps the long-press menu and the prefetch cancel | yes (D8) |
| Esc during a drag | `cx.stop_active_drag` | Cancels the drag | A capture listener, active only while dragging; Esc otherwise keeps clear-filter → close-preview | — |
| Space, Insert, Num+, Num−, Num* | unchanged | — | — | — |
| (no key) marks survive the filter | browser behaviour | D6 | — | yes |

Checked by a test, not assumed: that `alt-+`, `alt--`, `ctrl-+`, `ctrl--` and `/` arrive from the numpad and the main keyboard through GPUI on Windows and Linux. A key that does not arrive is dropped from the defaults and logged; the command keeps working from Ctrl+K and the user keymap.

Later, not in P36:
- save and load marks to a file (Total Commander);
- mark files newer than, or missing from, the other pane (belongs to compare, D39/D40);

## Components

| Component | Layer | Contract (in → out) | New / change | Test |
| --- | --- | --- | --- | --- |
| `selection` span and extension | core lib | `Selection::extend(cursor, to, unmark)`, `end_span()`, `mark_extension(&Listing, ext, on)` | change | `navo-core/src/selection/tests/span.rs` (unit + proptest) |
| `command` text forms | core | `SelectionExtend { to: CursorMove, mode: SpanMode }`, `SelectionExtension { mark: bool }`, `SelectionRestore` ↔ text | change | command round-trip tests + insta |
| `apply::select` | core | commands → `AppState` (span end, `last_marks`, all with folders) | change | `apply/tests/select_span.rs` |
| `browser::filter` hidden marks | ui controller | refilter keeps hidden names; `hidden_marks(tab) -> u64` | change | `browser/filter/marks_tests.rs` |
| `file_list` keys | ui | actions → commands | change | `file_list/tests/input.rs` |
| `file_list` mouse | ui | modifier clicks, empty click, right-drag paint | change | `file_list/tests/input.rs`, `window_view/tests/shell_menu.rs` |
| status bar hidden count | ui | `StatusBarProps.hidden_marks` | change | `status_bar` tests |
| `op_plan::drop_op` | core lib | `(same_volume, DropKeys) → DropOp` | new | `op_plan/tests/drop.rs` |
| `Browser::drop_into` | ui controller | `DropRequest { kind, sources, target, confirm } → Result<u64, DropRefusal>` | new (paste uses it) | `browser/operate/tests/drop.rs` |
| `drop-confirm` setting | store + settings sheet | `[files] drop-confirm` | new | store settings tests, `settings` tests |
| `drag` | ui component | `FileDrag { pane, row }`, `DragState`, `DragBadge` props in | new | `drag/tests.rs`, gallery |
| `file_list` drag source and targets | ui | `on_drag_start(row) → Entity<DragBadge>`, `on_drop(DropEvent)` | change (new `file_list/drop.rs`) | `file_list/tests/drag.rs` |
| `window_view::drop` | ui view | session freeze, hover, `drop_into`, notices, Esc | new | `window_view/tests/drop.rs` |
| `pane_chrome` targets | ui | tab, crumb and pill accept `DropPayload` → `Location` | change | `pane_chrome/tests.rs` |
| planner same-volume check (V1) | ui controller | `plan_for` checks `volume_serial` before renaming | change | `browser/operate` tests |
| `OpKind::Link` plan | core | `Step::MakeLink { target_of, at }` | change | `op_plan` tests |
| `ProviderOps::make_link` | fs adapter | `(target_of, at) → Result<(), FsError>`, job step, undo | new | `navo-fs` job tests, Windows CI |
| vendored drop target | platform (vendor) | key state + effect hint, never MOVE | change | Windows build, owner check |
| `DropPlatform` bridge | ui contract, app adapter | `keys()`, `set_effect(Option<DropOp>)`, `locations(&[PathBuf])`, `paths(&[Location])` | new | `window_view/tests/drop_external.rs` (fake) |
| drop menu | ui | right-button external drop → 4 items | new | `window_view/tests/drop_external.rs` |
| `navo-shell::drag_out` | shell adapter | `data_object(&[PathBuf])`, `drag_files(hwnd, &[PathBuf])` | new | `navo-shell/tests/drag_out_windows.rs` |
| vendored drag-source hook | platform (vendor) | `set_external_drag_handler(fn)`, posted start, `Ended` | change | Windows build, owner check |
| drag-out wiring | ui + app | `external_drag_payload` from the session | change | `window_view/tests/drag_out.rs` (TestPlatform result) |

## Atoms

| Atom | Component | Role | Effort | After | Check | Status | Cost |
| --- | --- | --- | --- | --- | --- | --- | --- |
| K1 | `selection` span and extension | api-builder (Sol 6.1) | xhigh | — | `cargo nextest run -p navo-core selection` | done (e921926) | |
| K2a | `command` text forms | api-builder (Sol 6.1) | xhigh | K1 | `cargo nextest run -p navo-core command`; insta reviewed | done (e921926) | |
| K2b | `apply::select` | api-builder (Sol 6.1) | xhigh | K2a | `cargo nextest run -p navo-core apply` | done (e921926) | |
| K3 | `browser::filter` hidden marks | api-builder (Sol 6.1) | xhigh | — | `cargo nextest run -p navo-ui browser::filter` | done (`Event::MarksRestored`) | |
| K4a | `file_list` key actions | ui-builder (Sonnet) | medium | K2b | `cargo nextest run -p navo-ui file_list` | done | |
| K4b | bindings, registry, labels, key tests | ui-builder (Sonnet) | medium | K4a | `cargo nextest run -p navo-ui file_list commands`; `cargo xtask gate` (l10n) | done | |
| K5 | `file_list` mouse | ui-builder (Opus) | medium | K4b | `cargo nextest run -p navo-ui file_list window_view::tests::shell_menu` | done | |
| K5b | `file_list` left-drag lasso (D20) | ui-builder (Opus) | medium | K5 | `cargo nextest run -p navo-ui file_list::tests::lasso` | done | |
| K6 | status bar hidden count | ui-builder (Sonnet) | medium | K3, K4b | `cargo nextest run -p navo-ui status_bar window_view` | done (7ddacda) | |
| K7 | review Part K | reviewer (Opus) | high | K5, K6 | review notes; gate; Windows CI | done (0a44f5c) | |
| D1 | `op_plan::drop_op` | api-builder (Sol 6.1) | xhigh | — | `cargo nextest run -p navo-core op_plan::drop` | done (1fe41f4) | |
| D2 | `Browser::drop_into` | api-builder (Sol 6.1) | xhigh | D1 | `cargo nextest run -p navo-ui browser::operate` | done (39299b2) | |
| D3 | `drop-confirm` setting | ui-builder (Sonnet) | medium | — | `cargo nextest run -p navo-store settings`, `-p navo-ui settings` | done | |
| D4 | `drag` badge | ui-builder (Sonnet) | medium | D1 | `cargo nextest run -p navo-ui drag`; gallery shot | done | |
| D5 | `file_list` drag source and targets | ui-builder (Opus) | medium | D4, K5 | `cargo nextest run -p navo-ui file_list::tests::drag` | done | |
| D6 | `window_view::drop` | ui-builder (Opus) | medium | D2, D3, D5 | `cargo nextest run -p navo-ui window_view::tests::drop` | done | |
| D7a | tab and crumb targets | ui-builder (Sonnet) | medium | D6 | `cargo nextest run -p navo-ui pane_chrome window_view::tests::drop` | done | |
| D7b | drive pill target | ui-builder (Sonnet) | medium | D7a | same | done | |
| V1 | planner same-volume check | api-builder (Sol 6.1) | xhigh | D2 | `cargo nextest run -p navo-ui browser::operate`; Linux cross-device test | done (e4f06a1) | |
| D8 | auto-scroll, spring-loaded targets (optional) | ui-builder (Opus) | medium | D7b | `cargo nextest run -p navo-ui window_view::tests::drop` | done | |
| L1 | `OpKind::Link` plan | api-builder (Sol 6.1) | xhigh | D2 | `cargo nextest run -p navo-core op_plan` | done | |
| L2 | `ProviderOps::make_link` + job step + undo | api-builder (Sol 6.1) | xhigh | L1 | `cargo nextest run -p navo-fs job`; Windows CI `test-windows` | done | |
| L3 | Link in drop and badge | ui-builder (Sonnet) | medium | L2, D7b | `cargo nextest run -p navo-ui window_view::tests::drop drag` | done | |
| D9 | review Part D | reviewer (Opus) | high | D7b, V1, L3 (D8 if run) | review notes; gate; Windows CI | blocked | |
| X0 | OLE facts | researcher (Sonnet) | medium | — | answers with sources, in the Log | done (Log) | |
| X1 | vendored drop target | api-builder (Opus) | medium | X0 | `cargo build --target x86_64-pc-windows-msvc` in CI; unit test of the key-state mapping | done (Windows CI); owner check pending | |
| X2 | `DropPlatform` bridge, external drops | ui-builder (Opus) | medium | X1, D9 | `cargo test -p navo-ui drop --lib` | done (Linux); Windows pending | |
| X3 | drop menu | ui-builder (Sonnet) | medium | X2 | same | done (Linux); Windows pending | |
| X4 | `navo-shell::drag_out` | api-builder (Opus) | medium | X0 | `cargo nextest run -p navo-shell drag_out` (Windows CI) | done | |
| X5 | vendored drag-source hook | api-builder (Opus) | medium | X1, X4 | Windows CI build; owner check | done (Windows CI); owner check pending | |
| X6 | drag-out wiring | ui-builder (Opus) | medium | X5, X2 | `cargo test -p navo-ui drop --lib` | done (Linux); Windows pending | |
| X7 | review Part X | reviewer (Opus) | high | X3, X6 | review notes; gate; Windows CI; owner check list | blocked | |

Parallel lanes:
- K1 → K2a → K2b runs beside K3, D1 and D3.
- X0 can run at once.
- Part D waits for K5 only at D5, because both edit `file_list`.

### K1 — span and extension marks in `Selection`

- **Read:** `crates/navo-core/src/selection/mod.rs:1-175`, `selection/tests/mod.rs`, `selection/tests/bulk.rs` (style), `crates/navo-ui/src/file_list/kind.rs` (`split_name`: the extension rule to copy).
- **Change:**
  - Add a private `Span { anchor: u64, base: Vec<u64>, unmark: bool }` and `span: Option<Span>` to `Selection`.
  - `pub fn extend(&mut self, from_cursor: u64, to: u64, unmark: bool)`: it starts a span when none exists, saving the bits and taking `from_cursor` as the anchor. It then recomputes bits = base, and sets or clears [min(anchor, to), max(anchor, to)], clamped to `len`.
  - `pub fn end_span(&mut self)`.
  - `pub fn mark_extension(&mut self, listing: &Listing, ext: Option<&str>, on: bool)`: files only, last extension, case-insensitive. `None` means no extension.
  - `clear()` also ends the span.
  - `new()` starts with no span, so `set_listing` ends it.
- **Check:** `cargo nextest run -p navo-core selection`. New tests in `selection/tests/span.rs`:
  - grow, shrink, crossing the anchor, unmark mode;
  - the base is kept outside the span;
  - clamping at `len`;
  - extension case, no extension, folders untouched;
  - a proptest: for any sequence of `extend`, marks == base ∪ range.
  - Coverage floor 90 % holds.
- **Stop if:** `Selection` is used through `PartialEq` in a way that a new field breaks (session snapshot tests).

### K2a — command variants and text forms

- **Read:** `crates/navo-core/src/command/mod.rs:1-120`, `command/id.rs`, `command/parse.rs:100-180`, `command/print.rs:30-60`, the command tests and snapshots.
- **Change:**
  - Add `SelectionExtend { to: CursorMove, unmark: bool }`, `SelectionExtension { mark: bool }` and `SelectionRestore`.
  - Text forms: `selection.extend to=down unmark=false` (`to` takes the `CursorMove` keywords or a row number, like `PaneCursorMove`), `selection.extension mark=true`, `selection.restore`.
  - This is four files of one component, about 70 lines. The orchestrator accepts that overrun.
- **Check:** `cargo nextest run -p navo-core command`. Round-trip tests for every new form and for bad arguments; the insta snapshot shows only the new ids.
- **Stop if:** `CommandId` lists are generated or snapshotted elsewhere (navo-ui `command_registry.snap`). Report it; do not edit navo-ui.

### K2b — apply rules

- **Read:** `crates/navo-core/src/apply/mod.rs:66-110`, `apply/select.rs`, `state/mod.rs:18-23`, `state/tab.rs:205-260` (`keep_by_name`, `set_listing`, `leave`), `apply/tests/`.
- **Change:**
  - `extend`: calls `Selection::extend(cursor, target)`, where the target is resolved like `PaneCursorMove` (use the same helper), then sets the cursor and calls `reveal_cursor`.
  - At the top of `apply`, every command except `SelectionExtend` calls `end_span()` on the active tab.
  - `SelectionAll` marks folders too (`mark_all(&listing, true)`). Update its tests.
  - `SelectionExtension { mark }` uses the cursor entry's extension. On a folder it does nothing (`Ok(vec![])`).
  - `AppState.last_marks: Option<Vec<String>>`, capped at 100,000 names. It is saved by `SelectionClear` when marks exist and by `Tab::leave` (pass it through the existing navigation path).
  - `SelectionRestore` marks the matching names and returns `CommandError::NothingToRestore` when none match.
  - Not saved in the session.
- **Check:** `cargo nextest run -p navo-core apply`. Tests:
  - Shift sequence grow and shrink;
  - a toggle ends the span;
  - a listing reload ends it;
  - all includes folders;
  - extension on a file and on a folder;
  - restore after clear;
  - restore in another folder by name;
  - the cap holds.
- **Stop if:** session snapshot tests compare `AppState` and need the new field persisted.

### K3 — marks survive the filter (D6)

- **Read:** `crates/navo-ui/src/browser/filter/mod.rs:1-265`, `browser/filter/tests.rs:1-60`, `crates/navo-core/src/state/tab.rs:185-240`.
- **Change:**
  - `Browser` keeps `hidden_marks: HashMap<TabId, HashSet<String>>`.
  - Before a refilter event, collect the marked names that the new shown listing lacks and add them to the set.
  - After the event, mark the set's names that are now visible (`Command`-free: use the existing event path plus `SelectionAddPattern` is wrong; add a crate-internal `Browser::remark(tab, names)` through an `Event::MarksRestored { tab, names }` only if the core lacks one, otherwise stop and report).
  - Navigation and `SelectionClear` empty the set.
  - Add `pub fn hidden_marks(&self, tab) -> u64`.
- **Check:** `cargo nextest run -p navo-ui browser::filter`. In `marks_tests.rs`:
  - filter, then Ctrl+A-equivalent, then another filter, then clear: both groups are marked;
  - `selected_sources` never contains hidden rows;
  - navigating away empties the set.
- **Stop if:** restoring marks needs a new core event. That is a K2b follow-up; report it.

### K4a — key actions

- **Read:** `crates/navo-ui/src/file_list/keys.rs:1-198`, `file_list/mod.rs:161-235`.
- **Change:**
  - New actions: `ExtendUp`, `ExtendDown`, `ExtendPageUp`, `ExtendPageDown`, `ExtendFirst`, `ExtendLast`, `MarkAll`, `UnmarkAll`, `RestoreMarks`, `MarkExtension`, `UnmarkExtension`. They send the K2 commands.
  - `RestoreMarks` always restores. It does not take the `mark_key` filter path, because no name contains `/`.
- **Check:** `cargo nextest run -p navo-ui file_list` (existing tests stay green); it compiles with no unbound actions warned by `bind_namespace`.
- **Stop if:** an action name is already registered in another namespace.

### K4b — bindings, registry, labels, key tests

- **Read:** `crates/navo-ui/src/commands/keymap.toml:190-335`, `commands/window.rs:220-330`, `crates/navo-l10n/locales/en-US/commands.ftl:50-80`, `file_list/tests/input.rs:1-120`.
- **Change:**
  - The table's keys go into `keymap.toml` under `# FileList`.
  - Registry rows go into `commands/window.rs`, and `cmd-files-*` labels into `commands.ftl`.
  - Update `bound_keys.snap`, `bound_order.snap` and `command_registry.snap`. Only the new rows may appear.
- **Check:** `cargo nextest run -p navo-ui file_list commands`, then `cargo xtask gate`. Tests:
  - Shift+Down ×3 marks rows 0–3, and Shift+Up shrinks to 0–2;
  - Ctrl+A marks folders too;
  - Ctrl+Shift+A clears, and `/` brings the marks back;
  - Alt+`+` marks `.txt` files;
  - typed `a` and Shift+`A` still filter;
  - `/` with a filter active restores and does not type;
  - one test per numpad and main key in the table that proves it dispatches. A key that fails is removed from the defaults and noted in the Log.
- **Stop if:** a key cannot be parsed by the keymap parser.

### K5 — mouse

- **Read:** `crates/navo-ui/src/file_list/press.rs:1-163`, `file_list/mod.rs:262-300` and `368-418`, `window_view/tests/shell_menu.rs:200-300` and `760-800`, `file_list/tests/input.rs:40-120`.
- **Change:**
  - Row: with Ctrl or Shift held, mouse-down does not move the cursor. `on_click` (mouse up, no drag) does Ctrl → cursor plus toggle, and Shift → `SelectionExtend { to: Row(r) }`.
  - Empty click: a left mouse-down on the list area whose y is below the last row's bottom (from `list_bounds`, the scroll position and the row count) sends `SelectionClear`.
  - Right-drag:
    - `Press` records `start_marked` and `last_row`;
    - `drag_over` sends `SelectionExtend { to: Row(r), unmark: start_marked }` on every row change;
    - `right_up` toggles only when no drag happened.
  - `file_list/mod.rs` is near the 400-line gate, so move `render_row` and `render_rows` into `file_list/rows.rs` if needed.
- **Check:** `cargo nextest run -p navo-ui file_list window_view::tests::shell_menu`. Tests:
  - Ctrl+click toggles two rows;
  - Shift+click from the cursor marks the range;
  - an empty click clears and `/` restores;
  - right-drag 1→3 marks 1–3, back to 2 gives 1–2;
  - a right-drag started on a marked row unmarks;
  - the existing long-press and prefetch tests stay green.
- **Stop if:** GPUI's `on_click` fires after a drag started (then report; do not work around it with timers).

### K5b — left-drag lasso (D20)

- **Read:** D3, D7, D9, D20; `file_list/press.rs`, `file_list/mod.rs`.
- **Change:** a left drag that starts on empty space draws a lasso rectangle and applies D3 span marks to the rows it crosses (Ctrl toggles); a drag that starts on a name or icon stays a file drag; a click without movement on empty space keeps D7.
- **Check:** `cargo nextest run -p navo-ui file_list::tests::lasso`: lasso marks the crossed rows, shrinks when moved back, keeps earlier marks, Ctrl+lasso unmarks, a drag on a name does not lasso.
- **Stop if:** GPUI cannot tell a drag start on empty space inside a row from one on the name.

### K6 — hidden-mark count in the footer

- **Read:** `crates/navo-ui/src/status_bar.rs`, `status_bar/` tests, `window_view/pane.rs:80-110`.
- **Change:** `StatusBarProps.hidden_marks: u64`, filled from `browser.hidden_marks(tab)`. It renders as "· N hidden by filter" next to the marked totals; the text is in `files.ftl`.
- **Check:** `cargo nextest run -p navo-ui status_bar window_view`.
- **Stop if:** the footer layout has no slot next to the marked totals.

### K7 — review Part K

Opus high, on the K1–K6 diff. Focus:
- the span rules;
- `SelectionAll` with folders, against F8;
- hidden marks never reaching `selected_sources`;
- restore by name across folders;
- right-drag against the long-press menu.

Then: gate, Windows CI, merge, an alpha build, and the owner checks the key table on a Windows PC.

### D1 — drop operation rule

- **Read:** `crates/navo-core/src/op_plan/mod.rs:1-70`.
- **Change:** new `op_plan/drop.rs`. `DropKeys { ctrl, shift, alt }` (Default), `DropOp { Copy, Move, Link }` and `pub fn drop_op(same_volume: bool, keys: DropKeys) -> DropOp`, following D11. Re-export it.
- **Check:** `cargo nextest run -p navo-core op_plan::drop`: a table test of all 16 combinations.
- **Stop if:** none expected.

### D2 — `Browser::drop_into`

- **Read:** `crates/navo-ui/src/browser/operate/mod.rs:200-280`, `operate/paste.rs`, `operate/types.rs:100-128`, `operate/tests/` (rig).
- **Change:**
  - `pub struct DropRequest { kind: OpKind, sources: Vec<Location>, target: Location, confirm: DropConfirm }`.
  - `pub enum DropConfirm { Never, Moves, Always }`.
  - `pub fn drop_into(&mut self, request: DropRequest) -> Result<u64, PasteRefusal>`:
    - refusals as in `paste`;
    - `same_volume` from `volume_of`;
    - `asks` from `confirm` and `kind`;
    - `source_tab` is the tab that holds the sources' parent if open, else the active tab.
  - `paste` calls `drop_into` with the active folder and `Never`.
  - `pub fn same_volume(a, b) -> bool` for the badge.
- **Check:** `cargo nextest run -p navo-ui browser::operate`. Tests:
  - a drop into another folder starts a Move on the same fake volume;
  - Copy keeps the sources;
  - into itself and already-here are refused;
  - `Moves` asks for Move and not for Copy;
  - protected always asks;
  - paste tests stay green.
- **Stop if:** `OpRequest` needs a field the planner does not have.

### D3 — `drop-confirm` setting

- **Read:** `crates/navo-store/src/settings/model.rs:84-110`, `crates/navo-ui/src/settings/values.rs:200-240`, `settings/fields.rs` (a `Files` enum field such as `hidden-files`).
- **Change:**
  - `Files.drop_confirm: DropConfirm` (kebab-case, default `moves`).
  - A field row in the Files section with one sentence: "Ask before a dragged move runs".
  - Text goes in `settings.ftl`.
- **Check:** `cargo nextest run -p navo-store settings` and `-p navo-ui settings`. The default is written to `navo.toml`, and D56's ledger records it.
- **Stop if:** the settings schema needs a migration.

### D4 — drag badge

- **Read:** `crates/navo-ui/src/side_sheet/view.rs:20-55` (drag view pattern), `tokens/`, `crates/navo-ui/src/job_card/` (look), `crates/navo-l10n/locales/en-US/files.ftl`.
- **Change:**
  - New `crates/navo-ui/src/drag/`, with `mod.rs`, `badge.rs` and `tests.rs`.
  - `FileDrag { pane: PaneId, row: u64 }` (Clone).
  - `DragState { count, first_name, hover: Option<Hover { target_label, op: Option<DropOp>, refusal: Option<SharedString> }>, keys_line: bool }`.
  - `DragBadge` is an `Entity` view that renders from `Entity<DragState>` (D12) with theme tokens only.
  - Strings: `drag-move`, `drag-copy`, `drag-link`, `drag-keys`, `drag-refused-*`, with Fluent plurals.
  - A gallery story shows the states.
- **Check:** `cargo nextest run -p navo-ui drag`: label per op, count plural, refused look, keys line hidden when `keys_line` is false. Plus one gallery screenshot (`timeout 300`, fresh folder).
- **Stop if:** gpui-kit's theme lacks a token the badge needs.

### D5 — file list as drag source and drop target

- **Read:**
  - `crates/navo-ui/src/file_list/mod.rs:40-130` and `262-300`;
  - `gpui-pre-0.3.9/src/elements/div.rs:560-660` (in `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`);
  - `gpui-pre-0.3.9/src/interactive.rs:690-760`;
  - `side_sheet/mod.rs:215-230`.
- **Change:**
  - New `file_list/drop.rs`:
    - `pub enum DropPayload { Internal(FileDrag), External(ExternalPaths) }`;
    - `pub enum DropSpot { Row(u64), Folder }`;
    - `pub struct DropEvent { pane, spot, payload }`.
  - `FileList` gets `on_drag_start(Fn(&FileDrag, &mut Window, &mut App) -> Entity<DragBadge>)`, `on_drop(Fn(&DropEvent, …))` and `on_hover(Fn(Option<DropSpot>, …))`.
  - The row cell gets an id (`("cell", row)`) and `.on_drag(FileDrag { pane, row }, …)`, only when `on_drag_start` is set.
  - Folder rows get `drag_over` styling (accent outline from tokens), `on_drag_move` for hover, and `on_drop` for both payload types.
  - The list background accepts drops as `Folder`.
- **Check:** `cargo nextest run -p navo-ui file_list::tests::drag`. With a recording host:
  - a left drag from row 2 calls `on_drag_start` with row 2;
  - a drop on a folder row emits `Row(n)`;
  - a drop below the rows emits `Folder`;
  - a `FileDropEvent::Entered/Submit` over a row emits `External`;
  - with no `on_drag_start` there is no drag.
- **Stop if:** `on_drag` on the cell blocks the K5 click handlers.

### D6 — window drop glue

- **Read:** `crates/navo-ui/src/window_view/pane.rs:260-347`, `window_view/clipboard.rs:140-180`, `window_view/hints.rs:20-40` (modifier observation), `browser/operate/mod.rs:200-245`, `window_view/tests/clipboard.rs` (rig).
- **Change:**
  - New `window_view/drop.rs`:
    - `DragSession { sources: Vec<Location>, source_pane, state: Entity<DragState> }`, frozen in `on_drag_start` by D9 (`selected_sources` when the pressed row is marked, else that row);
    - hover → target `Location` (D10) → op from `drop_op(browser.same_volume, keys from window.modifiers())`, or a refusal → `DragState`;
    - `on_drop` → `browser.drop_into` with the `drop-confirm` setting;
    - a refusal → `show_notice`;
    - a capture Escape listener calls `stop_active_drag` and clears the session;
    - the badge re-renders on modifier change.
  - Wire `FileList` in `pane.rs`.
  - `pub(super) fn drop_on(&mut self, target: Location, payload, window, cx)` is reused by D7.
- **Check:** `cargo nextest run -p navo-ui window_view::tests::drop`, with `FakeFs` and two panes:
  - drag `home/a.txt` onto the right pane → Move job, file in `dest`;
  - Ctrl held → Copy;
  - 3 marked, drag a marked row → 3 sources;
  - drag an unmarked row → 1 source, marks unchanged;
  - onto a folder row → into that folder;
  - onto its own folder → no job plus a notice;
  - Esc → no job;
  - `drop-confirm = moves` → dialog for Move.
- **Stop if:** the badge constructor cannot reach `NavoWindow` without re-entering its update.

### D7a — tab and breadcrumb targets

- **Read:** `crates/navo-ui/src/pane_chrome/mod.rs:30-131`, `pane_chrome/path_bar.rs`, `pane_chrome/segments.rs`, `window_view/pane.rs:230-262`, `window_view/drop.rs` (from D6).
- **Change:**
  - `PaneTabs` and `PathBar` take `on_drop(Fn(&(usize | Location), &DropPayload, …))` and `on_hover`, with `drag_over` styling.
  - `pane.rs` maps tab i to that tab's location and a segment to its location, then calls `drop_on`.
- **Check:** `cargo nextest run -p navo-ui pane_chrome window_view::tests::drop`: a drop on tab 2 copies into tab 2's folder without activating it; a drop on the root crumb targets the root.
- **Stop if:** tab titles have no stable per-tab element id.

### D7b — drive pill target

- **Read:** `crates/navo-ui/src/pane_chrome/drive_pill.rs`, `pane_chrome/drives.rs` (`current_drive`), `window_view/drives.rs`.
- **Change:** the pill accepts drops; the target is the root of the drive the pill shows. The rest follows D7a.
- **Check:** same command. A drop on the pill targets `<drive root>`; a refusal when the sources are that root.
- **Stop if:** the pill has no drive location on Linux. Then it is not a target there; note that in the test.

### V1 — the planner checks the volume serial

- **Read:** `crates/navo-ui/src/browser/operate/work.rs:1-80`, `crates/navo-fs/src/provider/ops.rs`, `navo-fs/src/local/ops.rs:125-137`.
- **Change:**
  - `plan_for` takes `&dyn ProviderOps`.
  - When `request.same_volume` is true, compare `volume_serial` of the first source's parent and of the target. If both are `Some` and they differ, plan as another volume (copy plus delete).
  - This runs on the planner thread, never on the UI thread.
- **Check:** `cargo nextest run -p navo-ui browser::operate`:
  - a fake provider with two serials plans `CopyFile` steps;
  - equal serials plan `Rename`;
  - on Linux CI, a tmpfs (`/dev/shm`) to `$TMPDIR` move succeeds when the devices differ (skip when they are equal).
- **Stop if:** the fake provider cannot report serials. Add a setter to the fake only.

### D8 — auto-scroll and spring-loaded targets (only if the owner says yes to D15)

- **Read:** `file_list/drop.rs`, `window_view/drop.rs`, `primitives/` (ListScroll), `pane_chrome/drive_menu.rs`.
- **Change:**
  - A timer task owned by the session: it scrolls one row per 50 ms while the pointer is in the top or bottom 24 px.
  - Hover timers for D15 targets: they are cancelled by leaving the target, by Esc and by the drop.
  - Drive menu rows accept drops.
  - All of it is off with `reduce-motion`.
- **Check:** `cargo nextest run -p navo-ui window_view::tests::drop`, with a test clock: an 800 ms hover opens the folder; leaving at 700 ms does not.
- **Stop if:** the drive menu cannot open while a drag is active.

### L1 — `OpKind::Link` in the planner

- **Read:** `crates/navo-core/src/op_plan/{mod,build,names,risks}.rs`, `op_plan/tests/`.
- **Change:**
  - `OpKind::Link`; `Step::MakeLink { target_of: Location, at: Location }`.
  - Names: `<name> - Shortcut.lnk` on Windows targets, `<name>` on Unix, with `(2)` numbering through `names.rs`.
  - No walk into folders. Totals count links.
  - Add an arm to every `match` on `OpKind` the compiler reports, with labels "Create shortcuts".
  - The journal reads old plans unchanged.
- **Check:** `cargo nextest run -p navo-core op_plan`. Tests: names, conflicts, folders as one link, a journal round-trip of a Link plan, an old plan JSON fixture still parses.
- **Stop if:** a `match` sits outside navo-core and navo-ui (report the list).

### L2 — make links in the job engine

- **Read:** `crates/navo-fs/src/provider/ops.rs`, `navo-fs/src/job/{run,undo}.rs`, `navo-fs/src/fake/`, `navo-fs/src/local/{unix,ops}.rs`.
- **Change:**
  - `ProviderOps::make_link(&self, target_of, at) -> Result<(), FsError>` (required).
  - Fake: records a link entry.
  - Unix: `std::os::unix::fs::symlink`.
  - Windows: `IShellLinkW::SetPath` plus `IPersistFile::Save`, with `CoInitializeEx(COINIT_APARTMENTTHREADED)` on the job thread for the duration of the step.
  - The job step journals the created path. Undo and recovery remove it with `remove_file` (Windows) or `remove_link` (Unix).
- **Check:** `cargo nextest run -p navo-fs job`. Tests:
  - the fake creates and undoes the link;
  - a conflict fails the step without touching the existing file;
  - Unix temp-dir symlink test;
  - a Windows CI test resolves the `.lnk` target with `IShellLinkW::GetPath`.
- **Stop if:** `windows` crate features for `IShellLinkW` are missing in navo-fs (add them only to navo-fs).

### L3 — Link in drops and the badge

- **Read:** `window_view/drop.rs`, `drag/badge.rs`, `window_view/operate/mod.rs:1-60`.
- **Change:** `DropOp::Link` maps to `OpKind::Link`; the badge shows "Create N shortcuts in X"; `confirm_props` and the job card show it.
- **Check:** `cargo nextest run -p navo-ui window_view::tests::drop drag`: Alt and Ctrl+Shift drops create links on `FakeFs`.
- **Stop if:** none expected.

### D9 — review Part D

Opus high, on D1–L3. Focus:
- frozen sources;
- the refusal rules;
- volume rules and V1;
- `drop-confirm`;
- Link undo;
- no drop path that bypasses the planner.

Then: gate, Windows CI, merge, an alpha build, and an owner check (drag between panes, onto a tab, onto a crumb, with Ctrl, Shift and Alt).

### X0 — OLE facts (read-only)

The researcher answers with Microsoft sources:
1. For a drop from Explorer that the target moves itself, what `Drop` must return (`DROPEFFECT_NONE` or `COPY`), and whether to set `Performed DropEffect` / `Logical Performed DropEffect`, so Explorer never deletes the sources.
2. That a data object from `SHCreateDataObject` with file-system PIDLs renders `CF_HDROP` on `GetData`, and whether all children must share the parent.
3. That `SHDoDragDrop` with a null `IDropSource` supplies the drag image and the default source.
4. Known re-entrancy rules for calling `DoDragDrop` from a window procedure on an STA thread.

The answers go into the Log; X1, X4 and X5 follow them.

### X1 — vendored drop target

- **Read:** `vendor/gpui-pre-windows/src/window.rs:1140-1290` and `1510-1530`, `vendor/gpui-pre-windows/src/gpui_windows.rs`, `events.rs:20-32`, X0's answers.
- **Change (D16):**
  - In `WindowsWindowInner` state: `drop_effect_hint: Cell<Option<u32>>` and `drag_keys: Cell<u32>`.
  - `DragEnter` and `DragOver`: store `grfKeyState`, dispatch the GPUI event first, then write `*pdweffect = hint.unwrap_or(COPY)` (`NONE` when the app set "none"), then call the helper.
  - `Drop`: dispatch first, then return the hint mapped by X0's rule. It is never `DROPEFFECT_MOVE`.
  - Export `pub fn set_drop_effect(hwnd: isize, effect: Option<u32>)` and `pub fn drag_key_state(hwnd: isize) -> u32`, found through `GWLP_USERDATA`, the way the window procedure finds its inner state.
  - A header comment lists the Navo patches.
  - Add a line to `docs/dev-tools.md`.
- **Check:**
  - Windows CI builds.
  - A unit test maps `MK_CONTROL | MK_SHIFT | MK_ALT | MK_RBUTTON` to `DropKeys`.
  - Owner check: drag from Explorer, and the cursor shows "Move" on the same drive and "Copy" across drives.
- **Stop if:** the input callback cannot be run before `*pdweffect` is written.

### X2 — `DropPlatform` bridge and external drops

- **Read:** `crates/navo-app/src/clipboard.rs` (adapter pattern), `navo-app/src/gui.rs` (StartOptions wiring), `window_view/drop.rs`, `gpui-pre-0.3.9/src/window.rs:8780-8960` (tests that dispatch `FileDropEvent`).
- **Change:**
  - In navo-ui, `pub trait DropPlatform { fn keys(&self) -> DropKeys; fn right_button(&self) -> bool; fn set_effect(&self, op: Option<DropOp>); fn locations(&self, paths: &[PathBuf]) -> Result<Vec<Location>, String>; fn paths(&self, items: &[Location]) -> Vec<(PathBuf, bool)>; }`, given in `StartOptions`. The default `NoPlatform` uses the window modifiers.
  - In navo-app: on Windows it uses the X1 functions with the window's HWND (`shell_menu/targets.rs:56` shows how to get it), `startup::path_location` and `navo_fs::os_path` + `without_verbatim`. On Unix, keys are empty and `set_effect` does nothing.
  - `window_view/drop.rs` handles `DropPayload::External`: paths → Locations, hover → `set_effect`, keys from the platform.
- **Check:** `cargo nextest run -p navo-ui window_view::tests::drop_external`, with a fake platform:
  - `Entered` / `Pending` / `Submit` over the right pane → Copy job from the paths;
  - fake Shift → Move;
  - fake "same folder" → refused, and `set_effect(None)` was called;
  - a path that `locations` rejects → notice, no job.
- **Stop if:** `StartOptions` cannot carry another service without changing `navo-app` tests widely.

### X3 — drop menu for right-button drags from Explorer

- **Read:** `crates/navo-ui/src/context_menu/` (component), `window_view/drop.rs`.
- **Change:**
  - When `platform.right_button()` was true during the drag, a drop opens the context-menu component at the drop point with four items: Copy here, Move here, Create shortcuts here (when Link exists), Cancel.
  - The chosen item calls `drop_into`. Esc cancels.
  - Text goes in `files.ftl`.
- **Check:** `cargo nextest run -p navo-ui window_view::tests::drop_external`: fake right button → the menu opens; "Move here" → Move job; Esc → no job.
- **Stop if:** the context menu cannot open without a shell session (use its Navo-only section).

### X4 — `navo-shell::drag_out`

- **Read:** `crates/navo-shell/src/shell_menu/win/selection.rs:1-83`, `shell_menu/win/sta.rs:45-60`, `clipboard/{mod,model,windows}.rs`, `crates/navo-shell/Cargo.toml`, X0's answers.
- **Change:**
  - Move `Pidl` to `src/pidl.rs` (`pub(crate)`); `shell_menu` uses it from there.
  - New `drag_out/{mod,windows,unix}.rs`:
    - `pub fn data_object(paths: &[PathBuf]) -> Result<IDataObject, DragError>` (Windows only; same parent required, otherwise `DragError::MixedFolders`);
    - `pub fn drag_files(hwnd: isize, paths: &[PathBuf]) -> Result<DropOutcome, DragError>`, which calls `SHDoDragDrop(hwnd, data, None, COPY|MOVE|LINK)` and must run on the window's UI thread (documented on the fn);
    - Unix returns `Unsupported`.
- **Check:** `cargo nextest run -p navo-shell drag_out`. A Windows CI test with two temp files: the data object's `CF_HDROP` decodes (`decode_drop_files`) to the same paths, and `Shell IDList Array` is offered. Shell-menu tests stay green.
- **Stop if:** `SHCreateDataObject` is missing from the pinned `windows` 0.62.2 features.

### X5 — vendored drag-source hook

- **Read:** `vendor/gpui-pre-windows/src/window.rs` (platform window impl), `events.rs:82-170` (`handle_msg`), `gpui-pre-0.3.9/src/platform.rs:1320-1332`, `gpui-pre-0.3.9/src/window.rs:5763-5792`, X0's answers.
- **Change (D17):**
  - `pub fn set_external_drag_handler(handler: fn(isize, &[(PathBuf, bool)]) -> u32)` stored once.
  - `can_start_external_drag` is true when it is set.
  - `start_external_drag` stores the payload in the window state, posts `WM_GPUI_EXTERNAL_DRAG = WM_USER + 11` and returns true.
  - `handle_msg` handles it without holding any `state` borrow while the handler runs, then sends `FileDropEvent::Ended` through the input callback.
  - Add to the patch header and `docs/dev-tools.md`.
- **Check:** Windows CI builds; owner check: drag two files from Navo to the desktop (copy on another drive, move on the same drive, Ctrl, Shift), and drag out and back in to the other pane.
- **Stop if:** the modal loop re-enters the GPUI input callback while it is borrowed (report the stack; do not add global locks).

### X6 — drag-out wiring

- **Read:** `file_list/drop.rs`, `window_view/drop.rs`, `navo-app/src/gui.rs`, `gpui-pre-0.3.9/src/platform/test/window.rs` (`set_start_external_drag_result`).
- **Change:**
  - The rows add `.external_drag_payload::<FileDrag>(…)`, which returns `ExternalDragPayload::Files(FileDragPaths::new(platform.paths(&session.sources)))`. Non-local sources return `None`.
  - On `FileDropEvent::Ended` the session is cleared and the source tab runs `PaneRefresh` when it has no watcher.
  - navo-app registers `navo_shell::drag_files` with `set_external_drag_handler` at start on Windows.
- **Check:** `cargo nextest run -p navo-ui window_view::tests::drag_out`:
  - with the test platform accepting, dragging past the window edge hands the drag to the platform with the frozen paths;
  - re-entering restores it, and a drop on the other pane plans a job;
  - an archive or fake source offers no payload.
- **Stop if:** the test platform cannot report the payload.

### X7 — review Part X

Opus high, on X1–X6 and both vendor patches. Focus:
- `Drop` never returns MOVE;
- Navo never deletes after a drag out;
- the posted-message start, with no App borrow;
- PIDL lifetimes;
- `unsafe` comments.

Then: gate, Windows CI, merge, an alpha build, and the owner check list (D16, D17).

## Done when

- **Part K.**
  - Each key in the key table works in the file list on Windows and Linux.
  - Typing letters and Shift+letters still filters.
  - Marks survive the filter.
  - Right-drag paints and shrinks.
  - Ctrl+click and Shift+click work.
  - An empty click unmarks and `/` restores.
  - Checked by K4b, K5 and K6 tests, the gate, Windows CI, and the owner on the alpha build.
- **Part D.**
  - Dragging marked or single rows onto the other pane, a folder row, a tab, a crumb or the drive pill starts the right Copy, Move or Link job (D11), shown in the copy card and journaled.
  - The badge shows the count, the operation and the target, and follows Ctrl, Shift and Alt.
  - Refusals and Esc start no job.
  - Checked by the D6, D7 and L3 tests, Windows CI, and the owner check.
- **Part X.**
  - Files dragged from Explorer or the desktop drop into Navo with the Windows default and modifiers, the cursor shows the true effect, and Explorer never deletes anything.
  - A right-drag from Explorer opens the drop menu.
  - Files dragged from Navo drop into Explorer, the desktop and mail clients as copies, moves or shortcuts.
  - Linux matches D18.
  - Checked by X2, X3, X4 and X6 tests, Windows CI, and the owner's check list.
- After every part: `cargo xtask gate` and Windows CI are green, the Opus high review is approved, and the part is merged and released as an alpha.

## Log

- 2026-10-20 — Plan drafted at 9d36589 from reading the code.
  - Findings: right-drag marks only its start row; marks hidden by the filter are dropped; no user-facing job undo yet; `volume_of` makes every Linux path one volume.
  - GPUI: the Windows drop target always reports COPY and ignores modifiers. gpui-pre 0.3.9 already has the drag-out API (`external_drag_payload`, `restore_platform_drag`), but Windows lacks `start_external_drag`.
  - The UI thread is STA through GPUI's `OleInitialize`.
  - 31 atoms in three parts. The orchestrator may split them into p36a, p36b and p36c.

- 2026-10-20 — K4a and K4b done on `p36-k` (one commit, because an action without a registry row fails the command tests).
  - Keys: Shift+arrows/PgUp/PgDn/Home/End, Ctrl+A and Ctrl+Num+, Ctrl+Shift+A and Ctrl+Num−, `/`, Alt+Num+ and Alt+Num− are bound as in the key table. The held-modifier hint layers list them in the Files group.
  - "No saved marks match this folder" comes from `CommandError::NothingToRestore` and shows as an error notice on the status line, text `files-marks-restore-none` in `files.ftl`.
  - Key arrival, read from the GPUI platform code (`vendor/gpui-pre-windows` `keyboard.rs`/`events.rs`, `gpui-pre-linux` `keystroke_from_xkb`) and tested as parsed keystrokes:
    - Windows: the numpad `+`, `-` and `/` keys map through `MapVirtualKeyW` to the same key names as the main keyboard, so `ctrl-+`, `ctrl--`, `alt-+`, `alt--` and `/` arrive from both. On the main keyboard `+` needs Shift; GPUI clears Shift for symbol keys, so Ctrl+Shift+= is `ctrl-+`.
    - Linux: the main keyboard gives the same names. The numpad gives `add`, `subtract`, `divide` and `multiply` (`KP_*` keysyms), so Num keys do not match the `+`, `-`, `/` bindings. This already holds for the Num+, Num− and Num* mark keys. No default is dropped, because every key arrives on Windows. Aliases such as `ctrl-add` are an owner decision.
  - K3 is blocked: marks of rows hidden by the filter can come back only through a core event (for example `Event::MarksRestored { tab, names }` that marks the rows of the new listing by name and ends the span). The core has no public way to mark rows by name on a given tab. K2b follow-up first, then K3.
- 2026-10-20 — K3 done on `p36-k`, with the Linux numpad aliases.
  - Core: `Event::MarksRestored { tab, names }` adds the named rows of the tab's current listing to its marks and ends the span. The replay log prints it as `!marks-restored { tab: N, names: K }` followed by K quoted name lines.
  - Browser (`browser/filter/marks.rs`): marks of rows that a refilter hides are kept by name per tab and location. After the refilter, the names that are visible again are re-marked through the new event. Navigation and `SelectionClear` (Ctrl+Shift+A, Ctrl+Num−, and the clear after a finished job) empty the set; a reload drops names that no longer exist. `Browser::hidden_marks(tab) -> u64` returns the count for K6.
  - F5, F6, F8, Ctrl+C/X and drag read `selected_sources`, which only sees marks in the shown listing, so they never touch hidden rows.
  - `browser/mod.rs` was at the 400-line limit, so `open_row`, `go_up`, `open_at`, `open_tab_at` and `set_launcher` moved unchanged to `browser/open.rs`.
  - Linux aliases (owner decision): `add`, `subtract`, `multiply`, `divide`, `ctrl-add`, `ctrl-subtract`, `alt-add` and `alt-subtract` are bound beside `+`, `-`, `*`, `/`, `ctrl-+`, `ctrl--`, `alt-+` and `alt--` for mark_pattern, unmark_pattern, invert_marks, restore_marks, mark_all, unmark_all, mark_extension and unmark_extension. `ctrl-multiply` and the other numpad chords stay unbound. The bound-key snapshots gained exactly those 8 rows, and the FileList action count went from 54 to 62.

- 2026-10-20 — K5 and K5b done on `p36-k`.
  - Rows: Ctrl+click moves the cursor and toggles; Shift+click and Ctrl+Shift+click send `SelectionExtend { to: Row(r) }`. Both act in `on_click`; mouse-down with Ctrl or Shift only takes focus. GPUI fires `on_click` after a movement when no `on_drag` is set, so a click that follows a lasso is swallowed by a flag that the next mouse-down clears (no timers).
  - Empty click: a plain left click below the last row sends `SelectionClear` on mouse up, not on mouse down, so a lasso that starts there keeps the marks.
  - Right-drag: the first row change cancels the menu prefetch, puts the cursor back on the start row and starts the span; every row change sends `SelectionExtend { to: Row(r), unmark: start_marked }`. A release after a drag does not toggle.
  - Lasso: a left press on the gap right of the name text (a `flex_1` zone in the Name cell) or below the last row arms it; past 4 px it draws a rectangle (accent at 12 % alpha, 1 px accent border, no animation) and spans the crossed rows from the start row (or the last row, from below). Ctrl unmarks. Move and release are window listeners, so the lasso follows the pointer anywhere inside the window. A press on the icon or the name text does nothing new (reserved for D9).
  - Not done: auto-scroll near the list edges (needs a timer while the pointer rests; left to D8). A lasso started below the last row that crossed rows and comes back below them keeps the last row marked, because a span always contains its anchor.
  - `render_row` and `render_rows` moved to `file_list/rows.rs`. Gallery page "Mouse marks" (`p36-k5-mouse-marks`) drives real pointer events: Ctrl/Shift clicks, a right-drag range and a lasso in progress.

## Questions for the owner (answered 2026-10-20)

**Q1 — Right-drag.** Should right-drag stay Total Commander range marking (finished properly), with Windows' drop menu only for right-button drags that come from Explorer? Or should right-drag in Navo become the Windows drop menu?
*Recommended:* keep Total Commander marking (D8). GPUI drags only start on the left button, so Explorer-style right-drag out of Navo would need our own drag code and could never reach Explorer. The Explorer-to-Navo menu is cheap and covers the Windows habit. We can add a setting later.

**Q2 — Confirmation for drops.** `[files] drop-confirm`: `never` (like Explorer), `moves`, or `always` (like Total Commander's default)?
*Recommended:* `moves` until Ctrl+Z for jobs ships with the multi-rename and safety-net work, then `never` automatically through D56's default-follow. Copies and shortcuts never ask, and protected folders always ask.

**Q3 — Three selection habits that change today's behaviour.**
- (a) A click on the empty area below the files unmarks all.
- (b) Marks hidden by the type-to-filter are kept and come back. Operations still act only on what is visible.
- (c) `/` (Num /) restores the last cleared marks by name in any folder, so after F5 you can mark the copies in the target.

*Recommended:* yes to all three. Each one is recoverable or visible, and (b) and (c) make keyboard selection much stronger.

**Q4 — Shortcuts and extras in this plan.** Should Alt-drop (and Ctrl+Shift-drop) create shortcuts now (3 atoms: `.lnk` on Windows, symlink on Linux, with undo)? And should auto-scroll and spring-loaded folders, tabs and drives be in now (1 atom)?
*Recommended:* shortcuts yes, as the last part of Part D, since you asked for Alt. Auto-scroll and spring-loading also yes, but optional: they run after the core drag works and can slip to a later alpha without blocking Part X.

Answers (2026-10-20): Q1 yes, Total Commander right-drag marking. Q2 `never`, as in Explorer. Q3 yes to (a), (b) and (c). Q4 yes to shortcuts, auto-scroll and spring-loaded targets. Added D20, the left-drag lasso.

- 2026-10-20 — K7 review fixes on `p36-k`.
  - Core: `SpanMode { Mark, Unmark, Toggle }` replaces `unmark: bool` in `Selection::extend` and `SelectionExtend`; the text form is `selection.extend { to: down, mode: mark|unmark|toggle }` (printed-commands snapshot updated). Toggle = base XOR span; moving back restores the base. A span target past the last row (`Row(r)` with r ≥ len) holds no rows. `PaneScroll` no longer ends the span.
  - Lasso: Ctrl toggles (D20). The tracked y is clamped to the list viewport, so a drag past an edge marks only shown rows; the rectangle still follows the raw pointer. A lasso from below that comes back below sends `Row(len)` and shrinks to the base (D3).
  - Browser: unmark-all and the empty-space click save visible plus filter-hidden marks (D5). `/` under a filter puts saved names the filter hides into the hidden-marks set, and succeeds when only hidden names match.
  - Footer text is now "N marked not shown", since show-hidden/show-system also hide marks.
- 2026-10-20 — Table brought up to date: K6, K7, D1, D2 and V1 had merged without their rows changing.
- 2026-10-20 — X0 answers (Microsoft Learn and The Old New Thing; worth a Windows check):
  - Drop from Explorer that Navo moves itself: return `DROPEFFECT_NONE` from `Drop` and `SetData` `CFSTR_PERFORMEDDROPEFFECT = DROPEFFECT_NONE` (an optimized move). Explorer deletes the sources only when it sees `DROPEFFECT_MOVE`. `Logical Performed DropEffect` is read by the source; the target need not set it. Sources: learn.microsoft.com/windows/win32/shell/datascenarios, devblogs.microsoft.com/oldnewthing/20041208-00/?p=37093.
  - `SHCreateDataObject` takes one parent PIDL plus child PIDLs, so all children share one parent. It documents `CFSTR_SHELLIDLIST`, not `CF_HDROP`; the shell's object is believed to render `CF_HDROP` lazily (unconfirmed: X4 tests it with `GetData`). For several folders use `SHCreateDataObject` with a null parent and absolute PIDLs, or `GetUIObjectOf` (unconfirmed).
  - `SHDoDragDrop` with a null `IDropSource` creates the default source (Vista+) and a generic image when none is stored; a custom image goes in through `IDragSourceHelper`.
  - `DoDragDrop` is a modal loop on the STA thread; our window procedure and our own drop target can run nested inside it. Hold no `RefCell` borrow, lock or `&mut` app state across the call; re-read state after it; expect our own window as a target. Start it only from a real mouse message with the button down (not touch or pen).
- 2026-10-20 — D3 done on `p36-d3`: `[files] drop-confirm = never | moves | always`, default `never`, with a Settings row.
- 2026-10-20 — D4 done on `p36-d4`: `navo-ui::drag` badge (DragState, DragBadge, Refusal), `drag-*` strings, gallery page `p36-d4-drag-badge`.
- 2026-10-20 — L1 done: `OpKind::Link` and `Step::MakeLink { target_of, at }` in the planner (`op_plan/build/link.rs`); `<name> - Shortcut.lnk` on Windows-style targets, `<name>` elsewhere, always numbered on a clash, one link per top item, totals count links. Running or undoing a `MakeLink` fails with `FsErrorKind::NotSupportedYet` until L2; activity, job card and confirm texts have Link arms.
- 2026-10-20 — D5 done on `p36-d5`: `FileList::pane/on_drag_start/on_drop/on_hover`; the drag source is a `grip` around the icon and name text, not the whole cell, so gap presses still lasso; folder rows take drops as `Row(n)` with an accent ring, the list background as `Folder`; `on_hover` reports every drag move (caller dedupes); GPUI clears pending clicks on redraw during a drag, and a drag start also swallows the source row's click for a release before the redraw.
- 2026-10-20 — L2 done: `ProviderOps::make_link` (a provided method that refuses with `NotSupportedYet`, so the navo-ui test providers stay untouched; FakeFs and LocalFs implement it) makes a symlink on Unix and a `.lnk` on Windows and never replaces an existing name. Undo and recovery remove only a symlink (Unix, fake) or a `.lnk` file (Windows) at the step's path and skip anything else. Recovery treats a Link job's first step as unsure only while it is still `pending`, so a link that failed on a clash is never removed.
- 2026-10-20 — L2 review fixes: `ProviderOps::link_target` (provided, default `None`; Unix `read_link`, Windows shell link `GetPath`, FakeFs from its links, which `rename` now carries) lets undo and recovery remove a link only when it still points at the step's target. Windows links use the `\?\` form for the place, the temp folder and the no-replace move, and a failed save reports the final name. `DriveFs` forwards `make_link` and `link_target`.
- 2026-10-20 — D6 done on `p36-d6`: `window_view/drop.rs` freezes the sources at drag start (all marks when the pressed row is marked, else that row), focuses the source pane, and keeps it focused after the drop; hover reports are tracked per pane and only change the badge when the hover differs; the op comes from the modifiers on release; refusals and Alt drops ("Shortcuts from a drop come next", `drag-link-later`) show a notice and run nothing; external payloads are ignored. Escape is a keystroke interceptor owned by the drag session, because the Escape key binding wins over key-down listeners. `Values::drop_confirm` maps the setting; `browser::drop_refusal` is public for the badge. The window's three observers moved to `window_view/watches.rs` to keep `mod.rs` under 400 lines. The badge's keys line shows unless modifier hints are off.
- 2026-10-20 — D7a done on `p36-d7a`: `drag::Landing<T>` adds the accent ring, drop and hover handlers to any element for internal and external payloads; `Tabs`, `PaneTabs` and `PathBar` take `on_drop(&target, &payload)` and `on_hover(&target, inside)`. `window_view/drop.rs` maps tab i to that tab's folder and a crumb to its own location, then reuses `drop_on`, so the badge, refusals and Esc are the same; a tab drop never activates the tab. The hover target is now `Over::{Spot, Tab, Crumb}`.
- 2026-10-20 — D7b done on `p36-d7b`: `DrivePill` takes `Landing<Location>` (`on_drop`, `on_hover`) and is a target only while its drive is connected; the target is that drive's root. `Over::Drive(pane, root)` reuses `drop_on`, so badge, refusals and Esc match the other targets. Spring-loading the pill dropdown and drops on its rows stay in D8.
- 2026-10-20 — L3 done on `p36-l3`: `run_drop` maps `DropOp::Link` to `OpKind::Link` and goes through `drop_into` like Copy and Move (no confirm for links unless the setting is Always or the target is protected); the badge, confirm and activity texts already existed from L1; `drag-link-later` and its notice are gone. Alt and Ctrl+Shift drops of one and of marked rows make links on `FakeFs`; a shortcut into the source's own folder is refused like any drop.
- 2026-10-21 — X4 built on `p36-x4`, waits for Windows CI: `Pidl` moved to `navo-shell/src/pidl.rs` (parent through `ILClone` + `ILRemoveLastID`); `drag_out::{data_object, drag_files}` with `DragError` (`Empty`, `MixedFolders`, `Path`, `Shell`, `Unsupported`) and `DropOutcome` (`Copied`, `Moved`, `Linked`, `NoEffect`, which also covers an optimized move); the same-folder rule reuses the shell menu's `folder_key` and is checked before the platform, so Unix refuses mixed folders and returns `Unsupported` otherwise; `tests/drag_out_windows.rs` checks `CF_HDROP` and `Shell IDList Array` on Windows only.
- 2026-10-21 — X1 built on `p36-x1`, waits for Windows CI: the drop target stores `grfKeyState` (Drop keeps the buttons from the last DragOver, so a right-button drag still reads as one), dispatches first, returns the app's hint (default Copy) and `Drop` turns Move into None and sets `Performed DropEffect` = None; `set_drop_effect` and `drag_key_state` accept only this thread's GPUI windows. `navo_core::DropKeys::from_key_state` and `right_button_drag` map the key state.
- 2026-10-21 — X1 review (orchestrator): the drop target masks the app's hint with the effects the source allows (the incoming `*pdwEffect`); a hint the source does not allow gives None. X2 must refuse a Move the source does not allow, not only change the cursor.
- 2026-10-21 — X5 built, waits for Windows CI: the drag-source hook stores one handler, posts the payload to its window, takes it before the modal call without borrowing state or taking the input callback, then sends `FileDropEvent::Ended`; the Linux UI check passed (7 steps; 2172 tests passed, 1 skipped). Handler registration and Explorer checks remain for the next atom and Windows.
- 2026-10-21 — X1 and X5 pass Windows CI (run 39536709862): the vendored drop target and drag-source hook compile; key-state test green. Owner checks wait for X2 and X6.
- 2026-10-21 — Owner on 21c: dragging files from Navo into other apps (VS Code and others) does not work; Total Commander does it. Owner: this comes before the Follow bug. Order now: finish Ctrl+K (21d), then X6 drag-out wiring (X2 external drops with it if X6 needs the bridge), then X7 review.

- 2026-10-23 — X2/X3/X6 integration: `DropPlatform` carries native keys, allowed effects, path conversion and native source paths. Incoming hover freezes paths and uses the planner's conservative self/descendant rules without filesystem reads. Source restrictions and modal blocking apply before job planning. Windows records the allowed-effect mask before dispatch and refuses when no target sets a hint; incoming moves still return None to prevent source deletion.
- 2026-10-23 — Right-button drops use the existing context menu with Copy, Move, shortcuts and Cancel. Permissions and the target are frozen for the menu; Esc restores focus. Hovered tabs snapshot pane, index, tab id and location, so navigation or replacement refuses the release. Linux external drops are safely Copy-only until native modifiers are available; this tightens D18's earlier default-operation fallback.
- 2026-10-23 — Outgoing rows resolve the frozen selection into native paths, register the existing shell handler, retain the session while the platform owns it, and refresh unwatched source folders on Ended. A failed handoff cleans up on release. GPUI's test-window accessor is private; local tests observe the actual row resolver and UI lifecycle, but cannot force platform acceptance. Actual promotion, OLE reentry, Explorer effects, cancellation and source deletion behavior remain native Windows checks. No commit, merge, push or deployment.

- 2026-10-23 — Combined backend branch gate passed all 10 steps: 7,060 tests passed, 14 skipped, plus doc tests. Inspected the native drop-menu gallery in light and dark at 1280×720. Windows build and native OLE checks remain pending.
- 2026-10-26 — D8 done on `ui/alpha-drag-autoscroll`: `window_view/spring.rs` owns the timers in the drag session. A list reports its 24 px top and bottom bands for internal drags (`FileList::on_edge`); each 50 ms tick scrolls that pane one row through `PaneScroll` and drops a stale row hover, so the drop lands where the ring is. Hover-open: 800 ms on a folder row navigates its pane and the next drop in that pane lands in the opened folder until a new hover report; 500 ms on a tab activates it; 800 ms on the drive pill opens the menu without taking focus, and its rows take drops (`Over::Menu`). Leaving the target, Esc and the drop cancel timers; Esc and the drop close a menu the hover opened; reduce motion turns all of it off. Pane-targeted commands go through browser focus, so the scrolled pane gets browser focus; Esc and the drop restore the source pane.
- 2026-10-26 — D8 review fixes: a release away from any target now returns focus to the source pane (as Esc does); a scroll tick also clears the sprung-folder landing; auto-scroll stops once the platform drag is gone; an external drag that exits cancels a pending hover-open.
