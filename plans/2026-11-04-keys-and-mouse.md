# Keys and mouse: every shortcut changeable, user commands, conflict guard, right button, palette drag

Status: drafting (plan check fixes applied; owner answers pending) · Repo: `navo` · Branch: one branch per part (`keys-guard`, `keys-rest`, `user-commands`, `right-button`, `palette-drag`) from `main` · Started: 2026-11-04

Five owner decisions of 2026-11-04 in one plan. The atoms fall into five parts (K, U, M, P, D). Each part merges on its own, so the plan is bigger than the usual 15 atoms but every part stays small. Defaults marked "pending owner" are the recommended answers; builders use them unless the owner changes them.

## Goal

The user can change every Navo shortcut, and no built-in key does two things on one screen. The user can define their own commands (a program with file placeholders), put them on keys and find them in Ctrl+K. The user can choose how the right mouse button works, and can drag the in-window clipboard palette by its header.

## What exists for "every shortcut changeable" (checked in the code on 2026-11-04)

P20 (`plans/2026-10-16-p20-shortcuts.md`, done) built most of D49:

- The built-in keymap is data: `crates/navo-ui/src/commands/keymap.toml`, embedded and validated. It rejects unknown commands, bad keys, contexts that are not allowed and duplicates.
- Every bound action has exactly one command name (`commands/mod.rs`, `commands/tests.rs`). `snapshots/bound_keys.snap` and `bound_order.snap` pin the bound set and its order.
- The user `keymap.toml` lives next to `navo.toml`. It is read only and overlaid by (command, context). `keys = []` unbinds a key. A user key beats a built-in key in the same context, with a warning. An invalid file keeps the last valid keymap and shows the line. The navo-fs watcher reloads it live (`navo-app/src/keymap/`, `window_view/user_keymap.rs`).
- The Shortcuts page in Settings records keys and asks on a conflict (Same/Inner/Outer). It can Replace, reset with Ctrl+R and unbind with Backspace. It edits the file in place and keeps comments (`shortcuts/`, `navo-app/src/keymap/save.rs`).
- The F-key bar, the app menu and Ctrl+K read the active keymap (`keymap/shown.rs`).

Missing (this plan builds only these):

1. **Conflict view on the real screen.** `shortcuts/proposal.rs::reach` uses the logical `PARENTS` table only. It does not know that:
   - `FileList` sits inside `NavoPane` (`window_view/pane.rs:109`);
   - the preview's `Viewer` body sits inside `PreviewBrowse` while a file is loaded (`preview/sheet.rs:38`, `:104`; `preview/browse.rs:182`);
   - several contexts share one node: `FileList`+`Compare` (`file_list/keys.rs::context`), `Elf`+`Space`, `Map`+`Space`, `MultiRename`+`Dialog`.

   So some user conflicts go unreported.
2. **No guard test for the built-in keymap.** That is how the six real conflicts in the brief got in (K2–K5).
3. **Keys the user cannot change** because code matches them, not the keymap:
   - Tiling layout mode: `window_view/layout_mode.rs:52-90`, through `intercept_keystrokes`, including Ctrl+Shift+1–4 pane focus.
   - Workspace tray: F2 rename in the card's `on_key_down` (`workspace_tray/card.rs:213-214`). Alt+N on the New card is only a label (`card.rs:248`); `workspace.new` already exists with Alt+N in `FileList` (`commands/window/workspaces.rs:57`, `keymap.toml:377`).
   - Recently deleted: Enter restores and Delete purges (`recently_deleted/mod.rs:40-55`).
   - Small pop-up lists and forms: arrows, Enter, Esc and Tab (`layout_switcher.rs:326`, `workspace_tray/rename.rs:142`, `viewer/description/mod.rs:111`). See Q1.
4. **Labels that ignore the user keymap**:
   - Code:
     - viewer hints: `viewer/keys_bar.rs:5-8`, `viewer/pdf/keys.rs:29-38`, `viewer/memory/mod.rs:191-193`, `viewer/archive/body.rs:15`, `viewer/space/keys.rs:17`;
     - layout mode hints: `layout_mode.rs:36-49`;
     - sort labels: `shell_menu/background.rs:17-26` (Ctrl+F3–F6);
     - clip form: `clip_palette/edit_form_view.rs:169,327` (`alt-r`, `ctrl-enter`);
     - tray: `workspace_tray/card.rs:248`;
     - preview sheet: `preview/sheet.rs:97,134`.
   - Strings: `rename.ftl:59`, `files.ftl:282`, `app.ftl:59-60`, `viewer.ftl:103,143`.
   - `keymap/shown.rs::shown` formats one key string. Nothing looks up the live key of a command in a context, and `Keymap::has_key(command, key)` (`keymap/mod.rs:77`) ignores the context.
5. **`CompareView` is never set.** No `key_context("CompareView")` exists. It is reserved for the full-window file diff (P42), so its binds (F7, Shift+F7, Alt+↑/↓, Esc, Ctrl+Alt+←/→) are dead today. `PARENTS` puts it under `Compare`, but on screen it will be its own root.

The P20 log also notes "a global key still beats a context key on a child element". The guard (K2) treats a context-free key as live on every node, so that case counts as a conflict.

## Proposed key changes (owner approved 2026-11-04, Q2)

| Command | Today | Proposed | Why |
| --- | --- | --- | --- |
| `mark.colour_one`…`mark.colour_eight` | Ctrl+1…8 in every viewer | Same keys, text views only (`Paged`, new `Editor` context) | Only the text views handle marks (`editor/marking.rs:73`, `paged/marking.rs:66`). Tree depth and PDF 100 % keep Ctrl+1…9 |
| `mark.clear` | Ctrl+0 in every viewer | Ctrl+0, text views only | Image and PDF fit keep Ctrl+0 (D57) |
| `image.actual_size` | 1 | Ctrl+1 | Same as PDF (D57). Bare 1 stays "show as text" in every viewer |
| `image.fit` | 0, Ctrl+0 | unchanged | — |
| `viewer.back` | Esc in 11 inner contexts | Esc once, in `Viewer` | One Esc command. Each part steps back first, and the viewer closes when nothing is left |
| `viewer.close` | Esc in `Viewer` | no default key (bindable) | Esc already closes through `viewer.back` |
| `compare.enter`, compare Backspace/Esc | same node as `files.open` | unchanged keys | The `Compare` context moves to an inner node, so it wins by depth instead of by bind order |
| `multi_rename.paste_names` | Ctrl+Shift+V | unchanged, allowlisted | It pairs with Ctrl+Shift+C copy names and Ctrl+Shift+O load names. Win+Alt+V and the menu still open the history. Alternative: Alt+V |
| `tree/elf/map/trace.type_*` | 1, 3, 5, n, p in typing tables | unchanged, allowlisted | These tables type into their filter by design |
| `workspace.new` | Alt+N in `FileList` | also Alt+N in `WorkspaceTray` | The tray's New card works from a focused card |
| `CompareView` binds | dead | unchanged, modelled as their own root | P42 sets the context |

## Decisions

Final. A worker that finds one wrong stops and reports.

- D1 — **Screen focus model.** New `commands/focus_paths.rs` owns `screen_reach(mine, theirs) -> Option<Reach>`. `Reach` moves there from `shortcuts/proposal.rs` and is re-exported.
  - The model is `PARENTS`, plus these screen facts: `FileList`→`NavoPane`, `PaneWaiting`→`NavoPane`.
  - `Viewer` has two possible parents: the window root (F3 viewer), or `PreviewBrowse` (the preview body, only while a file is loaded). `PreviewBrowse` is never inside `Viewer`.
  - These contexts share one node: [`FileList`,`Compare`] (until K3), [`Elf`,`Space`], [`Map`,`Space`], [`MultiRename`,`Dialog`].
  - `CompareView` is a root.
  - A context-free key (`global`) is live on every node.
  - A slot like `"ClipPalette > Input"` means `Input` under `ClipPalette`.
  - `PARENTS` itself does not change, because it decides where a user may bind a command.
- D2 — **The guard** is a test file, `commands/keymap/guard_tests.rs`, over `Keymap::builtin()`.
  - It fails when one key has two commands whose contexts are on the same node, or one is an ancestor of the other, unless one of three allowances applies:
    - (a) `YIELDS`: outer commands meant to be taken over by a focused part: `viewer.line_up`, `viewer.line_down`, `viewer.scroll_left`, `viewer.scroll_right`, `lister.find_next`, `lister.previous_image`, `find.selection`, `viewer.copy_path_line`, `editor.focus_preview`, `settings.row_down`, `settings.edit_value` and `settings.close`.
    - (b) `TYPING` contexts (`Tree`, `ElfTable`, `MapTable`, `CanTrace`) may shadow single-character keys and Backspace.
    - (c) `ALLOW` pairs: `Viewer` inside `PreviewBrowse` for every key; `ClipPalette` over global Ctrl+K; `multi_rename.paste_names` over `clipboard.open` for Ctrl+Shift+V.
  - Same-node conflicts never get an allowance.
  - The `!Viewer` predicate of `clipboard.open` (`bind.rs::bindings_of`) counts.
- D3 — **`KNOWN` list.** K2 lists today's real conflicts in `KNOWN`, and the test passes with them. K3, K4 and K5a each delete their own entries. The guard also checks that every `KNOWN`, `ALLOW`, `YIELDS` and `TYPING` entry is still used, so the lists cannot go stale.
- D4 — **Viewer Esc.** Esc binds once, to `viewer.back` in `Viewer`.
  - Every part's `Back` handler stays where it is and runs first, because an action bubbles from the focused node.
  - The viewer root handler (`viewer/view_menu.rs:160`, `close_drawer`) closes the viewer when no drawer is open, instead of calling `cx.propagate()`.
  - The archive viewer handles `Back` instead of `CloseViewer` (`viewer/archive/draw.rs:194` today calls `back` on `CloseViewer`). Esc leaves a nested archive first, then clears the filter, then closes. A user key for `viewer.close` closes at once.
  - The preview sheet handles `viewer.back` as it handled `viewer.close`.
  - List-focus rule (from branch `fix-preview-focus`): with the preview open and focus in the file list, the first Esc closes the preview and keeps the filter; the second Esc clears the filter. K5b starts only after that branch has merged.
- D5 — **Fixed keys** (owner approved 2026-11-04, Q1). The digits 1–9 that pick a numbered layout in tiling layout mode stay fixed. So do the arrows, Enter-as-confirm, Esc-as-cancel and Tab inside small pop-up lists and forms. The verbs become commands: split, close, swap, resize, even, save, remove, Tab target, pane focus, done, rename, new, restore and purge.
- D6 — **Key lookups.** K6a adds three lookups on `Keymap`. All label atoms use them:
  - `keys_in(command, context) -> impl Iterator<Item = &str>`: the keys of a command in one slot, after the user overlay;
  - `has_key_in(command, context, &Keystroke) -> bool`;
  - `shown_in(command, context) -> Option<String>`: the first key, formatted by `shown`.

  Hint bars, footers and key chips use `shown_in` on `active(cx)`. A command with no key hides its hint. Help prose in Settings (`settings.ftl`) keeps the built-in key names.
- D7 — **User commands file.** The file is `user-commands.toml`, next to `navo.toml` (owner approved 2026-11-04, Q3). It is read only, reloads live like `keymap.toml`, and an invalid file keeps the last valid list and reports the line. Each entry is a `[[command]]` table. "Edit user commands" (`user_commands.edit`) opens the file in F4 and writes a commented template only when the file is absent.

  | Field | Meaning | Default |
  | --- | --- | --- |
  | `name` | shown in Ctrl+K and in the status line; unique | required |
  | `program` | a path or a name found on PATH with PATHEXT (the editor's `resolve`); no placeholders | required |
  | `args` | one string, split by Windows rules | `""` |
  | `start-in` | a folder; `{folder}` and `{other-folder}` allowed | `"{folder}"` |
  | `console` | `"show"` or `"hide"` | `"show"` |
  | `key` | one key, like `"ctrl-alt-e"`, bound in `FileList` | none |

- D8 — **Placeholders** (plain words; TC equivalents in brackets):
  - `{file}`: the full path of the cursor item (%P%N).
  - `{name}`: its name (%N).
  - `{stem}`: the name without the extension (%O).
  - `{ext}`: the extension (%E).
  - `{folder}`: the current folder (%P).
  - `{other-folder}`: the other pane's folder (%T). "Other" means the copy-target pane.
  - `{other-file}`: the other pane's cursor item (%T%M).
  - `{marked}`: one argument per marked item. It must be a whole argument, and it falls back to the cursor item when nothing is marked (%S).
  - `{marked-list}`: the path of a list file (%L).
  - `{{` and `}}` write literal braces.
  - Paths use `Location::as_str()`, never the `\\?\` form.
  - An unknown placeholder, or `{marked}` inside other text, is a load error with its line.
- D9 — **Quoting.**
  - `args` is split by `terminal::split` before substitution. So a value with spaces or quotes stays one argument and can never add an argument.
  - Each argument goes to `std::process::Command::arg`, which quotes for the MSVCRT rules.
  - `raw_arg` is never used.
  - When `program` is `wt`, every `;` in a value becomes `\;`, as `git/launch/custom.rs:23` does, so a path cannot start a second Terminal command.
- D10 — **Shell rule** (decided by the orchestrator for safety; Q4 is withdrawn).
  - A command line contains a shell when its `program`, or any word of its `args`, is a shell. "Shell" means `terminal::is_shell_program` (cmd, powershell, pwsh, sh, bash, wsl, wscript, cscript, mshta, conhost and others, plus `.bat`/`.cmd`), or a word ending in `.ps1`.
  - Such a command may contain no placeholder at all; `start-in` counts. The load gives a line error, so the command never appears.
  - This matches the Git tool rule (`git/launch/custom.rs:28-33`), but is stricter: a placeholder before the shell word is refused too.
  - Commands with no placeholder may still run a shell, because the user wrote the whole line.
  - Navo never builds a shell string.
- D11 — **Limits and list files.**
  - A command line over 32,000 UTF-16 units is refused. The message suggests `{marked-list}`; Navo never switches silently.
  - `{marked-list}` is a UTF-8 file without BOM, with one full path per line and CRLF line ends.
  - It lives in `<StoreDirs.data>/user-commands/`. The folder is created when needed.
  - The file is created with `tempfile::Builder::new().prefix("list-").suffix(".txt").tempfile_in(dir)` (create-new, random name), then kept with `keep()`. Two commands never share a file.
  - The file is deleted when the spawn fails, and when the wait thread sees the program exit.
  - After the spawn, Navo writes the child's process id to a sidecar `<list>.pid`.
  - Navo sweeps once at start, before any command runs. The sweep deletes a list only when:
    - it is older than one day;
    - and its `.pid` is missing, or names a process that no longer runs.

    A reused pid only keeps a file longer.
- D12 — **Refusals.** These are status-line messages; nothing runs:
  - no cursor item, or the cursor is on `..`;
  - a path that is not local (an archive or a virtual folder);
  - a start folder that is missing;
  - a program that is not found;
  - a `.bat`/`.cmd` argument that std refuses;
  - the D11 length limit.
- D13 — **Start.**
  - `console = "show"` uses `CREATE_NEW_CONSOLE | CREATE_NEW_PROCESS_GROUP` (0x10 | 0x200).
  - `"hide"` uses `CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP` (0x0800_0000 | 0x200).
  - Stdio is null. A wait thread reports a non-zero exit of a hidden command in the status line.
- D14 — **User command keys.**
  - `RunUserCommand { name }` is a data action. It is registered as a normal command spec, `user_commands.run`, with its action name, hidden from the palette and absent from `keymap.toml`. So `for_action` knows it, and `rebind_app_keys` (`window_view/keys.rs:44-56`) drops its bindings with the other registered ones.
  - The keys come from a global list that the window sets from the news. `bind_app_keys` binds them last, in `FileList`, so they win ties and are rebuilt on every keymap or user-command reload.
  - A removed command loses its key. A key moved to another command runs only the new one.
  - A key that a FileList or global command already uses gives a load warning.
  - A key on a modifier alone, a bare letter or digit, or one the OS reserves is a load error.
- D15 — **Right button setting.** The setting is `selection.right-button = "total-commander" | "windows"`, in Settings → Selection. It applies on the next press.
  - The default stays `"total-commander"`: today's behaviour, as decided in D15 of decisions.md (owner approved 2026-11-04, Q5).
  - Total Commander mode is today's code: a short press marks, a right drag paints marks, and a hold of 400 ms (`LONG_PRESS`) opens the menu.
  - Windows mode: right button down moves the cursor and prefetches the menu. Right button up over the list opens the item menu at the press point. The target rule is today's long press (`press.rs::adds_pressed_row`, owner approved 2026-11-04, Q7). A right drag does nothing (owner approved 2026-11-04, Q6).
  - Right click on the background opens the background menu at once in both modes. Space and Insert do not change.
- D16 — **Palette drag.**
  - In `Place::Window`, a left drag on the header moves the palette.
  - A pure `clamp_offset(offset, panel, viewport, margin) -> Point` places the panel inside the window, with `t.spacing.md` margin, one axis at a time:
    - when the panel fits on an axis, the offset is clamped so the whole panel stays inside;
    - when it does not fit, the panel is pinned to the margin at the top or left edge on that axis, and the rest overflows at the bottom or right.
  - The offset lives in the window state for the run and is not saved. It is clamped again on each render, so a smaller window pulls the palette back in.
  - There is no animation while dragging.
  - Element handlers on the layer carry the drag. No window callback is registered in render.
- D17 — **New decisions** for `docs/decisions.md`, recorded by the orchestrator on merge:
  - D109: keymap guard and key changes.
  - D110: user commands.
  - The right-button mode extends D15 (an option, same default); no new decision is needed unless the owner changes the default.
- D18 — **Check form.**
  - Each check runs as `timeout 600 flock /tmp/navo-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 <cmd> > ../<atom>.log 2>&1; s=$?; tail -n 40 ../<atom>.log; exit $s`.
  - The atom passes only when the exit status is 0.
  - `<cmd>` is the nextest filter named in the atom, then `cargo xtask check -p <crate>` in the same form.
  - Accepted snapshot changes are listed in the report.
  - `cfg(windows)` code (U3) also gets a VM build and test before the push.

## Risks and rollback

- R1 — A user command runs the wrong program or files. Prevented by D9, D10 and D12:
  - no shell string is built;
  - values never add arguments;
  - no placeholder may appear in a line that contains a shell;
  - refusal is the default whenever a rule fails.
  - U2 and U3 run at `high`, and a `reviewer-senior` and a `reviewer-cross` check the branch.
- R2 — In Windows mode, a right click acts on marks the user forgot. Accepted: Windows mode is opt-in, the rule is the same as today's long press, and the menu header shows the count (Q7).
- R3 — The Esc change closes a viewer where a step back was expected. Prevented: K5a and K5b test Esc in Image, PDF, Tree, Paged, the archive, the find bar and the preview.
- R4 — Moving the `Compare` context breaks compare keys. Prevented: K3 runs the compare key tests unchanged, plus a test with a user keymap.
- R5 — A list file is deleted while a program still reads it. Prevented: D11 deletes only after the exit, and the sweep keeps files of running processes.
- Rollback: one squash commit per part on `main`; `git revert <sha>`. User files are only read, so they need no rollback. List files are Navo's own temporary files.

## Components

| Component | Crate / path | Contract (in → out) | New / change | Test |
| --- | --- | --- | --- | --- |
| `focus-paths` | navo-ui `commands/focus_paths.rs` | `(context, context) → Option<Reach>` | new | unit, in file |
| `keymap-guard` | navo-ui `commands/keymap/guard_tests.rs` | `Keymap::builtin() → conflicts` | new | itself |
| `shortcut-proposal` | navo-ui `shortcuts/proposal.rs` | key + slot → conflict prompt | change | `shortcuts/tests.rs` |
| `compare-context` | navo-ui `file_list/keys.rs`, `file_list/mod.rs`, `compare_list/list.rs` | key context nodes | change | `window_view/tests/compare.rs` |
| `builtin-keymap` | navo-ui `commands/keymap.toml`, `commands/*.rs` | data | change | snapshots, guard |
| `viewer-back` | navo-ui `window_view/viewer/view_menu.rs`, `viewer/archive/draw.rs`, `preview/` | `Back` → step back or close | change | viewer and preview tests |
| `key-lookups` | navo-ui `commands/keymap/mod.rs`, `keymap/shown.rs` | command + context → keys, match, label | change | unit |
| `layout-mode-keys`, `tray-keys`, `deleted-keys` | navo-ui `layout_mode.rs`, `workspace_tray/`, `recently_deleted/` | key → verb via keymap | change | their tests |
| `shown-keys` | navo-ui labels, `navo-l10n/locales/en-US/*.ftl` | keymap → label | change | label tests |
| `user-command-model` | navo-app `user_commands/model.rs` | TOML → `Vec<UserCommand>` or line errors | new | unit |
| `user-command-args` | navo-app `user_commands/expand.rs` | `UserCommand, Invocation → Plan \| Refusal` | new | unit, many |
| `user-command-start` | navo-app `user_commands/start.rs` | `Plan → spawn, list file, on_exit` | new | unit + VM |
| `user-command-hooks` | navo-ui `window_view/user_commands.rs` | hooks, news, run | new | UI |
| `user-command-app` | navo-app `user_commands/mod.rs`, `gui.rs` | load, watch, wire | new | unit |
| `user-command-keys` | navo-ui `commands/`, `window_view/keys.rs`, `window_view/user_commands.rs` | key → `RunUserCommand` | change | UI |
| `command-center-target` | navo-ui `command_center/source.rs`, `command_center/view/row.rs`, `command_center/view/footer.rs` | `Target::User` → icon, footer keys | change | view tests |
| `user-command-source` | navo-ui `window_view/command_center/` | commands → Ctrl+K rows, run | new | UI |
| `right-button-setting` | navo-store `settings/model.rs`, navo-ui `settings/` | TOML → `RightButton` | new | store + values |
| `right-press` | navo-ui `file_list/press.rs` | mouse → cursor, marks, menu | change | `file_list/tests/mouse.rs` |
| `palette-drag` | navo-ui `clip_palette/`, `window_view/clipboard_palette.rs` | drag → clamped offset | change | unit + one UI |

## Atoms

| Atom | Component | Role | Effort | After | Check | Status | Cost |
| --- | --- | --- | --- | --- | --- | --- | --- |
| K1 | `focus-paths` + `shortcut-proposal` | ui-builder | high | — | `nextest -p navo-ui focus_paths shortcuts` | ready | |
| K2 | `keymap-guard` | ui-builder | high | K1 | `nextest -p navo-ui guard` | blocked | |
| K3 | `compare-context` | ui-builder | high | K2 | `nextest -p navo-ui compare` | blocked | |
| K4 | `builtin-keymap` (marks, image) | ui-builder | medium | K2 | `nextest -p navo-ui commands viewer` | blocked (Q2 pending) | |
| K5a | `viewer-back` (keymap, viewer root, archive) | ui-builder | high | K2 | `nextest -p navo-ui viewer archive shortcuts` | blocked (Q2 pending) | |
| K5b | `viewer-back` (preview) | ui-builder | high | K5a, `fix-preview-focus` merged | `nextest -p navo-ui preview` | blocked | |
| K6a | `key-lookups` | ui-builder | medium | K2 | `nextest -p navo-ui keymap` | blocked | |
| K6b | `layout-mode-keys` | ui-builder | medium | K6a | `nextest -p navo-ui layout_mode commands` | blocked (Q1 pending) | |
| K7a | `tray-keys` | ui-builder | medium | K6a | `nextest -p navo-ui workspace_tray commands` | blocked (Q1 pending) | |
| K7b | `deleted-keys` | ui-builder | medium | K7a | `nextest -p navo-ui recently_deleted commands` | blocked (Q1 pending) | |
| K8a | `shown-keys` (keys bar, PDF, memory) | ui-builder | medium | K5a, K6a | `nextest -p navo-ui keys_bar pdf memory` | blocked | |
| K8b | `shown-keys` (archive, space) | ui-builder | medium | K8a | `nextest -p navo-ui archive space` | blocked | |
| K8c | `shown-keys` (sort menu, clip form) | ui-builder | medium | K6a | `nextest -p navo-ui shell_menu clip_palette` | blocked | |
| K9a | `shown-keys` (files, app strings) | ui-builder | medium | K8c | `nextest -p navo-ui command_center pane_chrome` | blocked | |
| K9b | `shown-keys` (rename, viewer strings) | ui-builder | medium | K8b | `nextest -p navo-ui multi_rename elf guide` | blocked | |
| KR | review of part K | reviewer | high | all K | verdict | blocked | |
| U1 | `user-command-model` | api-builder | medium | — | `nextest -p navo-app user_commands::model` | ready | |
| U2 | `user-command-args` | api-builder | high | U1 | `nextest -p navo-app user_commands::expand` | blocked | |
| U3 | `user-command-start` | api-builder | high | U2 | `nextest -p navo-app user_commands::start` + VM | blocked | |
| U4 | `user-command-hooks` | ui-builder | medium | U1 | `nextest -p navo-ui user_commands` | blocked | |
| U5 | `user-command-app` | api-builder | medium | U3, U4 | `nextest -p navo-app user_commands` | blocked | |
| U6 | `user-command-keys` | ui-builder | high | U4, K2 | `nextest -p navo-ui user_commands commands` | blocked | |
| U7a | `command-center-target` | ui-builder | medium | — | `nextest -p navo-ui command_center` | ready | |
| U7b | `user-command-source` | ui-builder | medium | U6, U7a | `nextest -p navo-ui command_center user_commands` | blocked | |
| UR | review of part U | reviewer-senior, then reviewer-cross | medium / high | all U | verdict | blocked | |
| M1 | `right-button-setting` | ui-builder | medium | — | `nextest -p navo-store -p navo-ui settings` | ready (Q5 default) | |
| M2 | `right-press` | ui-builder | high | M1 | `nextest -p navo-ui mouse shell_menu` | ready after M1 (Q6, Q7 defaults) | |
| P1 | `palette-drag` | ui-builder | medium | — | `nextest -p navo-ui clip_palette clipboard_palette` | ready | |
| D1 | user guide | documenter | medium | K, U, M, P | `rg -n "Right-click" docs/user-guide.md` | blocked | |

Run order:
- Part K: K1 → K2 → (K3, K4, K5a, K6a in any order) → K5b after `fix-preview-focus` merges → K6b, K7a → K7b → K8a–K8c → K9a, K9b.
- Part U: U1 → U2 → U3, with U4 next to U2; U5 → U6 → U7b. U7a runs at any time.
- Part M: M1 → M2.
- P1 runs at any time.

Each worker runs one gate at a time.

### K1 — Screen focus model; the Shortcuts page uses it

- **Read:** `commands/contexts.rs` (PARENTS, `is_within`), `shortcuts/proposal.rs` (`Reach`, `reach`), `file_list/keys.rs` (`context`), `window_view/pane.rs:100-115`, `window_view/preview/sheet.rs:30-45,95-110`, `window_view/preview/browse.rs:179-190`.
- **Change:**
  - Create `commands/focus_paths.rs` as in D1, with `pub(crate) fn screen_reach` and `pub(crate) fn same_node(a, b) -> bool`.
  - Move `Reach` there and re-export it from `shortcuts`.
  - `proposal.rs::reach` calls `screen_reach`.
- **Tests (in file):**
  - `FileList` under `NavoPane` is Inner.
  - `Viewer` under `PreviewBrowse` is Inner, and `PreviewBrowse` under `Viewer` is never reported.
  - `Compare` with `FileList` is Same.
  - `Elf` with `Space` is Same.
  - `CompareView` with `Compare` is None.
  - Global with any context is Same.
  - `"ClipPalette > Input"` parses.
  - A page test: recording Ctrl+K in `ClipPalette` against global asks.
- **Check:** `cargo nextest run -p navo-ui focus_paths shortcuts`, then `cargo xtask check -p navo-ui`, both in the D18 form.
- **Stop if:** a context in `keymap.toml` is set on screen in a way D1 does not describe.

### K2 — Guard test with today's conflicts listed

- **Read:** `commands/keymap/mod.rs`, `commands/bind.rs` (`bindings_of`), `focus_paths.rs` (K1), `snapshots/bound_keys.snap`.
- **Change:**
  - Create `commands/keymap/guard_tests.rs` (wired with `#[cfg(test)] mod guard_tests;`) with `YIELDS`, `TYPING`, `ALLOW` and `KNOWN` from D2 and D3.
  - K2 may extract `pub(crate) fn predicate_of(bind)` from `bind.rs` so the `!Viewer` case is shared, with no behaviour change.
  - `KNOWN` holds these entries:
    - the Compare trio (Enter, Backspace, Esc) against FileList;
    - `mark.*` Ctrl+0…8 against Image, PDF and Tree;
    - Image `1` against `lister.show_text`;
    - `viewer.back` against `viewer.close` (Esc) in the 11 contexts.
- **Tests:**
  - No conflict outside the allowances and `KNOWN`.
  - Every list entry is used.
  - A planted same-node pair and a planted ancestor pair fail.
  - Every context in the keymap is known to `focus_paths`.
- **Check:** `cargo nextest run -p navo-ui guard`, then xtask check (D18).
- **Stop if:** the first run finds a conflict that is neither in this list nor covered by D2. Report it with the command, key and both contexts. Never allowlist it on your own.

### K3 — Compare context on its own node

- **Read:** `file_list/keys.rs`, `file_list/mod.rs:385-395`, `compare_list/list.rs:160-170`, `window_view/compare_keys.rs`.
- **Change:**
  - The list root keeps `FileList` only.
  - In compare mode, a child that holds focus gets `Compare`. Focus stays on the same handle; only the context node changes.
  - Remove the three Compare entries from `KNOWN`, and add `ALLOW` Compare over FileList for all keys.
- **Tests:** the compare key tests stay unchanged. A new test sets a user keymap with `files.open = ["enter"]` in `FileList`, enters compare mode, and checks that Enter still runs `compare.enter`.
- **Check:** `cargo nextest run -p navo-ui compare`, then xtask check (D18).
- **Stop if:** the focus handle must move to another element.

### K4 — Text marks scoped; Image 100 % on Ctrl+1

- **Read:** `keymap.toml:1299-1335`, `:1394-1402`, `editor/view.rs:30-50`, `window_view/viewer/acts.rs:38-55`, `commands/tests.rs:384`.
- **Change:**
  - The `mark.*` binds get `context = "Paged"` and `context = "Editor"`.
  - Add `.key_context("Editor")` on the F4 editor root, and add `("Editor", VIEWER)` to `PARENTS`.
  - `image.actual_size` becomes `ctrl-1`.
  - Rename and update `pdf_keys_shadow_only_the_viewer_marks_and_escape` to the new keys.
  - Delete the mark and Image entries from `KNOWN`.
- **Tests:** Ctrl+3 marks in Paged and in the editor. Ctrl+1 in Image shows 100 %, and Ctrl+1 in Tree folds to depth 1.
- **Check:** `cargo nextest run -p navo-ui commands viewer`, then xtask check (D18); the snapshot diff holds only these rows.
- **Stop if:** `Paged` or `Editor` is not on the focus path when the text has focus.

### K5a — One Esc command: keymap, viewer root, archive

- **Read:** the `keymap.toml` Esc entries (`rg -n escape`), `viewer/view_menu.rs:150-170,280-300`, `viewer/pdf/view.rs:151-161`, `viewer/archive/draw.rs:185-200`, `viewer/archive/mod.rs:175-195`, `shortcuts/tests.rs:210-235`.
- **Change:**
  - In `keymap.toml`, bind Esc to `viewer.back` once in `Viewer`. Remove the 11 inner Esc entries and the default key of `viewer.close`.
  - In `view_menu.rs`, `close_drawer` closes the viewer when no drawer is open.
  - In `archive/draw.rs`, the `CloseViewer` handler becomes a `Back` handler that calls `back`. `CloseViewer` then reaches the viewer root and closes.
  - Delete the Esc entries from `KNOWN`.
- **Tests:**
  - Each of these Esc presses steps back first, then closes:
    - PDF with the find bar open;
    - Image zoomed;
    - Tree with the filter;
    - Paged with go-to open;
    - archive: inside a nested archive, then with a filter, then closed.
  - A user key for `viewer.close` closes an archive with a filter at once.
- **Check:** `cargo nextest run -p navo-ui viewer archive shortcuts`, then xtask check (D18).
- **Stop if:** a part's `Back` handler sits on an element that is not an ancestor of its focused node. Also stop if more than these three source files need changes.

### K5b — One Esc command: preview

- **Read:** `window_view/preview/sheet.rs:90-140`, `preview/browse.rs:90-110` (`escape_preview`), the merged `fix-preview-focus` tests in `preview/tests/browse.rs` and `preview/tests/files.rs`.
- **Change:**
  - The preview sheet handles `viewer.back` where it handled `viewer.close`.
  - The sheet's `.close("escape", ..)` and `.hint(&["escape"], ..)` read `shown_in("viewer.back", "Viewer")` (needs K6a).
  - Keep the D4 list-focus rule.
- **Tests:**
  - The `fix-preview-focus` sweep test passes unchanged: one Esc closes the preview for every file kind.
  - With the preview open, focus in the file list and a filter typed, the first Esc closes the preview and keeps the filter, and the second Esc clears the filter.
  - Esc in a viewer opened with F3 from the preview returns to the preview.
- **Check:** `cargo nextest run -p navo-ui preview`, then xtask check (D18).
- **Stop if:** `fix-preview-focus` is not merged, or its tests fail before the change.

### K6a — Key lookups by context

- **Read:** `commands/keymap/mod.rs:60-90` (`keys_of`, `has_key`), `commands/keymap/shown.rs`, `commands/keymap/parse.rs` (`normal`, `stroke`).
- **Change:** Add `keys_in`, `has_key_in` and `shown_in` (D6). `has_key` keeps its signature and behaviour.
- **Tests:**
  - A key bound in two contexts is found only in its own.
  - A user override replaces the built-in key in `keys_in`.
  - `has_key_in` matches a `Keystroke` with modifiers, and a shifted digit on Linux the way `bind.rs` aliases it.
  - `shown_in` of an unbound command is `None`.
- **Check:** `cargo nextest run -p navo-ui keymap`, then xtask check (D18).
- **Stop if:** the overlay does not keep the slot per key.

### K6b — Tiling layout mode keys are commands

- **Read:** `window_view/layout_mode.rs` (all), `commands/mod.rs` (GROUPS, `CommandSpec` with `action: None`), K6a's lookups.
- **Change:**
  - Add a `LayoutMode` context and `layout_mode.*` commands with no action for each verb:
    - split right (v);
    - split below (s);
    - close (x);
    - swap: Shift+←/→/↑/↓, one command per direction;
    - resize: Ctrl+←/→/↑/↓, one command per direction;
    - even (=);
    - save (Ctrl+S);
    - remove (Delete);
    - Tab target;
    - pane focus 1–4 (Ctrl+Shift+1–4, one command each);
    - done (Esc).
  - `mode_key` matches each keystroke with `active(cx).has_key_in(command, "LayoutMode", keystroke)`. The digits 1–9 stay fixed (D5).
  - The `HINTS` table (`layout_mode.rs:36-49`) shows `shown_in` for each verb, and hides a verb with no key.
- **Tests:** the existing layout-mode tests pass unchanged. A user keymap moves "split right" to `b`, and `b` splits and the hint shows B. Ctrl+Shift+2 still focuses pane 2.
- **Check:** `cargo nextest run -p navo-ui layout_mode commands`, then xtask check (D18).
- **Stop if:** a verb needs more than one command per direction or per pane.

### K7a — Workspace tray keys are commands

- **Read:** `workspace_tray/card.rs:200-250`, `workspace_tray/mod.rs:40-60`, `commands/window/workspaces.rs`.
- **Change:**
  - Add a `WorkspaceTray` context on the card frame that tracks focus (`card.rs:212`).
  - Add a `workspace.rename_card` action bound to F2 in `WorkspaceTray`. It replaces the `on_key_down` check at `card.rs:213-214`.
  - Reuse `workspace.new`: add a second bind, Alt+N, in `WorkspaceTray`. No new command.
  - The `Kbd` chip shows `shown_in("workspace.new", "WorkspaceTray")`.
- **Tests:** F2 renames and Alt+N creates from a focused card, as before. A user key for `workspace.new` in `WorkspaceTray` works, and the chip shows it.
- **Check:** `cargo nextest run -p navo-ui workspace_tray commands`, then xtask check (D18).
- **Stop if:** the tray card has no focus handle to carry the context.

### K7b — Recently deleted Enter and Delete are commands

- **Read:** `recently_deleted/mod.rs:30-60`.
- **Change:** Add `deleted.restore` (Enter) and `deleted.purge` (Delete) in a `RecentlyDeleted` context. `event_for_key` keeps only the arrows and Esc (D5).
- **Tests:** Restore and purge by key as before. Purge still asks first.
- **Check:** `cargo nextest run -p navo-ui recently_deleted commands`, then xtask check (D18).
- **Stop if:** purge would run without its confirm.

### K8a — Keys bar, PDF and memory hints read the keymap

- **Read:** `viewer/keys_bar.rs`, `viewer/pdf/keys.rs:20-40`, `viewer/memory/mod.rs:185-195`, K6a's lookups.
- **Change:**
  - Replace each literal key with `shown_in(command, context)` on `active(cx)`. A command with no key hides its hint.
  - `CLOSE` uses `viewer.back`, and `VIEW` uses the view menu command.
  - `NEXT_PREVIOUS` joins the keys of its commands.
- **Tests:** For each file, with a user key, the hint shows the user key.
- **Check:** `cargo nextest run -p navo-ui keys_bar pdf memory`, then xtask check (D18).
- **Stop if:** a label has no command behind it.

### K8b — Archive and Space hints read the keymap

- **Read:** `viewer/archive/body.rs:10-20`, `viewer/space/keys.rs:10-20`.
- **Change:** As K8a, for the archive's `BACK` and the Space view's `NEXT_PART`.
- **Tests:** As K8a.
- **Check:** `cargo nextest run -p navo-ui archive space`, then xtask check (D18).
- **Stop if:** as K8a.

### K8c — Sort menu and clip form labels read the keymap

- **Read:** `shell_menu/background.rs:10-30`, `clip_palette/edit_form_view.rs:160-175,320-335`.
- **Change:** These labels come from `shown_in`: the sort labels (`files.sort_*`), `clip_palette.toggle_reveal` and `clip_palette.save_form`.
- **Tests:** As K8a.
- **Check:** `cargo nextest run -p navo-ui shell_menu clip_palette`, then xtask check (D18).
- **Stop if:** as K8a.

### K9a — String keys in files.ftl and app.ftl

- **Read:** `files.ftl:282`, `app.ftl:59-60`, and their callers (`rg -n "command-center-content-footer|pane-strip-target|pane-target-here" crates/navo-ui`).
- **Change:** These strings take `{ $open }`-style variables, filled from `shown_in`. The drag modifier strings stay as they are, because modifiers are not commands.
- **Tests:** The footer test shows a user key.
- **Check:** `cargo nextest run -p navo-ui command_center pane_chrome`, then `cargo xtask check -p navo-ui -p navo-l10n` (D18).
- **Stop if:** more than four files change.

### K9b — String keys in rename.ftl and viewer.ftl

- **Read:** `rename.ftl:59`, `viewer.ftl:103,143`, and their callers.
- **Change:** As K9a, for `rename-hints`, `elf-hidden-note` and `guide-hint`.
- **Tests:** The rename hint test shows a user key.
- **Check:** `cargo nextest run -p navo-ui multi_rename elf guide`, then xtask check for navo-ui and navo-l10n (D18).
- **Stop if:** as K9a.

### U1 — User command file model

- **Read:** D7, D8, D10 and D14 above; `navo-app/src/keymap/mod.rs` (load pattern); `navo-app/src/terminal/mod.rs:74-135` (`program_identity`, `is_shell_program`, `split`); `navo-ui/src/commands/keymap/parse.rs` (key parsing, line errors).
- **Change:**
  - Create `navo-app/src/user_commands/model.rs` with `UserCommand { name, program, args: Vec<Token>, start_in: Vec<Token>, console: Console, key: Option<String> }`.
  - Add `parse(text) -> Result<Vec<UserCommand>, Vec<LineError>>`.
  - `Token` is a split argument with its literal parts and its placeholders.
  - The D10 shell rule is a load error.
  - Add `mod user_commands;` in `navo-app/src/lib.rs`.
- **Tests (many):**
  - Each load error in D8, D10 and D14 gives its line.
  - D10 rejects a placeholder in each of these lines:
    - `program = "cmd"`;
    - `program = "wt"` with `args = "cmd /c {file}"`;
    - `program = "wt"` with `args = "{folder} pwsh"`;
    - `program = "tool.exe"` with `args = "run.ps1 {file}"`;
    - `program = "C:\\x\\go.bat"`;
    - `start-in = "{folder}"` with `program = "bash"`.
  - D10 accepts `cmd /c build.cmd` with no placeholder.
  - `{{` and `}}` give literal braces.
  - `{marked}` inside other text is an error.
  - A duplicate name is an error.
  - A placeholder in `program` is an error.
  - A bad key and a reserved key are errors.
  - The defaults apply.
  - CRLF and BOM files parse.
- **Check:** `cargo nextest run -p navo-app user_commands::model`, then `cargo xtask check -p navo-app` (D18).
- **Stop if:** `terminal::split` or `is_shell_program` is not visible from this module.

### U2 — Expand, quote and refuse

- **Read:** U1's `model.rs`, `terminal/mod.rs` (`split`), `git/launch/custom.rs:15-35` (`wt` escaping), `editor/mod.rs:60-105` (`resolve`), D8, D9, D11 and D12.
- **Change:**
  - Create `user_commands/expand.rs` with `Invocation { file, marked, folder, other_folder, other_file }` as `Location`s.
  - Add `plan(&UserCommand, &Invocation, &Search) -> Result<Plan, Refusal>`.
  - `Plan` has `{ program: PathBuf, args: Vec<String>, start_in: PathBuf, console, list: Option<Vec<String>> }`.
  - `Refusal` is a typed enum with a `Display` message.
  - As a second guard, `plan` re-checks D10 and refuses.
- **Tests (many, table-driven):**
  - A value with spaces, quotes, `&`, `%` or a trailing backslash stays one argument.
  - `{marked}` gives N arguments and falls back to the cursor item.
  - `{marked-list}` gives a list and its path argument.
  - With `wt`, a path with `;` arrives as `\;`.
  - The line-length refusal triggers at the limit.
  - `..`, a non-local path and a missing start folder are refused.
  - `{other-file}` with an empty other pane is refused.
  - A command that reached `plan` with a shell and a placeholder is refused.
- **Check:** `cargo nextest run -p navo-app user_commands::expand`, then xtask check (D18).
- **Stop if:** any path needs the shell or `raw_arg`.

### U3 — Start the process and keep the list file

- **Read:** `git/launch/start.rs` (spawn, wait thread, `windows_flags`), U2's `Plan`, D11 and D13.
- **Change:**
  - Create `user_commands/start.rs` with `start(Plan, list_dir, on_exit: Box<dyn FnOnce(Option<i32>) + Send>) -> Result<(), String>`.
  - It creates the list file as D11 says, before the spawn. It deletes the file when the spawn fails. After the spawn it writes the `.pid` sidecar, and it deletes both after the wait.
  - Add `sweep(dir, now, is_running: impl Fn(u32) -> bool)`. The real `is_running` uses `OpenProcess` + `GetExitCodeProcess` on Windows and is `|_| true` elsewhere.
  - Add `creation_flags(console) -> u32`.
- **Tests:**
  - The list file has CRLF lines, no BOM and full paths.
  - Two lists started at once get two files.
  - The list is gone after exit, and after a failed spawn.
  - The sweep keeps:
    - fresh files;
    - old files whose pid runs.

    It deletes old files whose pid is gone or whose sidecar is missing.
  - The flags for show and hide.
  - On Windows (VM): `cmd /c exit 3` hidden reports 3.
- **Check:** `cargo nextest run -p navo-app user_commands::start`, then xtask check (D18). Then the VM run of the same filter.
- **Stop if:** the wait thread would outlive a closed app in a way that holds the profile lock.

### U4 — UI hooks and news

- **Read:** `window_view/user_keymap.rs` (the pattern), `window_view/hand_off.rs:130-150` (`OpenHooks`).
- **Change:**
  - Create `window_view/user_commands.rs` with:
    - `UserCommandInfo { name, key }`;
    - `UserCommandNews::{Applied(Vec<UserCommandInfo>, Vec<String>), Invalid(String), Exited { name, code }, Refused { name, message }}`;
    - `UserCommandHooks { run, file, news }`;
    - `listen_user_commands`.
  - Add `user_commands: Option<UserCommandHooks>` to `OpenHooks`, so no `StartOptions` call site changes.
  - Notices use the status line as the keymap does.
- **Tests:**
  - Applied replaces the list.
  - Invalid keeps the old list and shows the line.
  - Exited with a non-zero code shows a notice.
  - Refused shows its message.
- **Check:** `cargo nextest run -p navo-ui user_commands`, then xtask check (D18).
- **Stop if:** a subscription would need to be made in render.

### U5 — App load, watch and wiring

- **Read:** `navo-app/src/keymap/mod.rs` (`hooks`, `watch`), `gui.rs:140-150`, `navo-store/src/dirs/mod.rs:115-125`.
- **Change:**
  - Create `user_commands/mod.rs` with `hooks(dirs) -> (UserCommandHooks, Option<Watch>)`. It loads the file, watches it and sweeps at start, before it returns.
  - `run` calls `expand::plan`, then `start::start` on a worker thread, and sends refusals and exits back as news.
  - Add `StoreDirs::user_commands_file()`.
  - Wire it in `gui.rs` through `OpenHooks`.
- **Tests:**
  - A valid file gives Applied.
  - A broken edit gives Invalid and keeps the list.
  - "Edit" writes the template only when the file is absent.
- **Check:** `cargo nextest run -p navo-app user_commands`, then `cargo xtask check -p navo-app -p navo-store` (D18).
- **Stop if:** `gui.rs` needs more than the wiring lines.

### U6 — Keys and the run action

- **Read:** `commands/mod.rs:100-140` (`all`, `for_action`, `ARGUMENT_COMMANDS`), `commands/bind.rs` (`bind_user_keys`), `window_view/keys.rs:44-70` (`rebind_app_keys`, `bind_app_keys`), `window_view/shell_menu/targets.rs:14`, U4's file.
- **Change:**
  - Add the data action `RunUserCommand { name }`, derived with gpui-kit `Action` and not usable from JSON.
  - Register it as the spec `user_commands.run` in a command group (D14), hidden from the palette. Add `user_commands.edit` as a normal command.
  - Add a global `UserKeys(Vec<(key, name)>)`. The window sets it on Applied news, then calls `rebind_app_keys` with the active keymap. `bind_app_keys` binds `UserKeys` last, in `FileList`.
  - The handler builds `Invocation` from the active pane and calls `run`:
    - the cursor item;
    - the marked items, by the `targets.rs` rule;
    - the copy-target pane, its folder and its cursor.
- **Tests:**
  - A key runs a fake `run` with the right `Invocation`.
  - After a keymap reload, each key is bound exactly once.
  - After a user-command reload that removes a command, its key does nothing.
  - After a reload that moves a key to another command, the key runs only the new one.
  - The guard (K2) and `every_navo_action_has_a_command` stay green.
  - Running on `..` gives a refusal notice.
- **Check:** `cargo nextest run -p navo-ui user_commands commands`, then xtask check (D18).
- **Stop if:** the registry needs a `keymap.toml` entry for the spec.

### U7a — Ctrl+K target for user commands

- **Read:** `navo-ui/src/command_center/source.rs:15-40`, `command_center/view/row.rs:125-180`, `command_center/view/footer.rs:115-135`.
- **Change:**
  - Add `Target::User(SharedString)`.
  - In `row.rs` (`:159`, `:174`), it gets the command icon, as `Action`.
  - In `footer.rs:123`, it gets `RUN, NEXT_GROUP, CLOSE`.
- **Tests:** A view test with a `Target::User` hit shows the command icon and the run footer.
- **Check:** `cargo nextest run -p navo-ui command_center`, then xtask check (D18); the snapshots are reviewed.
- **Stop if:** another exhaustive `match` on `Target` exists outside these files and `window_view/command_center/run.rs`.

### U7b — User commands in Ctrl+K

- **Read:** `window_view/command_center/mod.rs:185-230` (`file_sources`), `window_view/command_center/run.rs:60-95`, `command_center/commands_source.rs`, U4's file.
- **Change:**
  - Add a `UserCommandsSource` with rows that show the name and the key chip.
  - Add it in `file_sources`.
  - `run.rs` dispatches `RunUserCommand` for `Target::User`.
  - "Edit user commands" shows as a normal command.
- **Tests:** A user command appears and runs from Ctrl+K. A renamed command updates after news.
- **Check:** `cargo nextest run -p navo-ui command_center user_commands`, then xtask check (D18).
- **Stop if:** the source needs more than these three files and one new file.

### M1 — Right button setting

- **Read:** `navo-store/src/settings/model.rs:196-210`, `navo-ui/src/settings/fields.rs:110-125`, `settings/general_fields.rs:110-125` (`files.drop-confirm` pattern), `settings/values.rs:15-25,290-300`.
- **Change:**
  - Add `RightButton { TotalCommander, Windows }` (kebab-case) to `SelectionSettings`, with the default `TotalCommander` (D15).
  - Add `Field::RightButton` in `Section::Selection` as `Control::Choices`, and a `values.right_button()` getter.
  - Add the Fluent labels in `settings.ftl`.
- **Tests:** The store round trip. A missing key gives `TotalCommander`. The values getter.
- **Check:** `cargo nextest run -p navo-store -p navo-ui settings`, then `cargo xtask check -p navo-store -p navo-ui` (D18).
- **Stop if:** the field needs a new `Control` kind.

### M2 — Right press follows the setting

- **Read:** `file_list/press.rs` (all), `file_list/rows.rs:35-50`, `window_view/settings_save.rs:139-148`, `file_list/tests/mouse.rs`, `window_view/tests/shell_menu/background.rs`.
- **Change:**
  - Add `browser.set_right_button(..)` in `steer`, passed through to `ListContext` props.
  - `Held` keeps `at` and `extended`.
  - Windows mode: `right_down` sets no timer and does no drag paint. `right_up` over the list runs the long-press body (cursor, `adds_pressed_row`, `ContextMenu`) at the press point.
  - Total Commander mode stays byte-for-byte today's code.
- **Tests:**
  - Windows mode:
    - a click opens the menu on release;
    - an unmarked row, with other rows marked, joins the marks;
    - a drag marks nothing.
  - Total Commander mode: all of today's mouse tests pass, run under that mode.
  - The background menu is the same in both modes.
- **Check:** `cargo nextest run -p navo-ui mouse shell_menu`, then xtask check (D18).
- **Stop if:** the menu target differs from today's long press in any case.

### P1 — Drag the in-window palette

- **Read:** `clip_palette/chrome.rs:55-80`, `clip_palette/view.rs:40-115,355-420`, `window_view/clipboard_palette.rs:325-345`.
- **Change:**
  - Add the pure `clamp_offset` from D16, in `clip_palette/view.rs` or a small `place.rs`.
  - For `Place::Window`:
    - the header's left mouse down starts a drag with the start pointer and offset;
    - while dragging, the full-window layer's `on_mouse_move` updates the clamped offset;
    - the layer's `on_mouse_up` ends the drag.
  - `clipboard_layer` applies the clamped offset to the centred panel on every render.
  - The offset lives on `NavoWindow` and survives close and reopen in the same run.
  - `Place::Anywhere` keeps `WindowControlArea::Drag`.
- **Tests:**
  - `clamp_offset` covers:
    - the four edges and zero;
    - a panel wider than the window: pinned left at the margin, height still clamped;
    - a panel taller than the window: pinned top;
    - a window that shrank after a drag.
  - One UI test: a drag moves the palette, and the palette reopens at the same place.
  - Screenshots at 390 px and desktop, in both themes.
- **Check:** `cargo nextest run -p navo-ui clip_palette clipboard_palette`, then xtask check (D18).
- **Stop if:** a drag needs a window-level callback.

### D1 — User guide

- **Read:** `docs/user-guide.md` (the mouse section near line 194, the keys section, the clipboard section) and the merged decisions.
- **Change:** Describe these:
  - the right-button setting;
  - user commands: the file, the placeholders, the shell rule and the refusals, with one example such as a diff tool with `{file} {other-file}`;
  - the key changes from the table;
  - the palette drag.
- **Check:** Every key in the text matches `keymap.toml`, and no machine paths appear.

## Owner answers (2026-11-04): every recommendation accepted

- Q1 — Should arrows, Enter-as-confirm, Esc-as-cancel and Tab stay fixed inside small pop-up lists and forms, and should the digits in tiling layout mode stay fixed too? **Recommended: yes.** Every other key becomes changeable.
- Q2 — Is the key table above approved? **Recommended: yes as written.**
- Q3 — Should user commands be defined in `user-commands.toml` now, with Ctrl+K "Edit user commands", and a form page in Settings later? **Recommended: the file now.**
- Q4 — Withdrawn: the orchestrator decided the shell rule (D10).
- Q5 — Which default for the right button? **Recommended: Total Commander stays the default**, as D15 decided and as today's code does; Windows is the option. This needs no new decision unless you want Windows as the default.
- Q6 — In Windows mode, should a right drag mark nothing, or still paint marks? **Recommended: nothing.**
- Q7 — In Windows mode, a right click on an unmarked row while other rows are marked: should the row join the marks, as a long press does today (the menu shows the count)? **Recommended: yes, the same rule in both modes.** The alternative is Explorer's rule, where only the clicked row is the target.

## Done when

- The guard test passes with an empty `KNOWN` list (K2–K5a).
- A user key works and shows on its label for:
  - any command in the table;
  - every verb in layout mode;
  - the tray;
  - Recently deleted.

  (K6–K9.)
- Esc steps back and then closes in every viewer, the archive and the preview, as K5a and K5b test.
- In the VM:
  - a user command with `{marked}` runs a program on the marked files;
  - a command that puts a placeholder after `cmd` (or `wt cmd`) is rejected at load with its line;
  - a list file is gone after the program exits.

  (U.)
- The right-button setting switches between the modes without a restart, and Total Commander stays the default (M).
- The palette drags, stays inside the window, and reopens in place (P).
- The KR and UR reviews pass, the Windows CI is green, and the user guide is updated.

## Log

- 2026-11-04 — Plan written. Research: what exists for feature 1, today's conflicts (script over `bound_keys.snap`), `CompareView` never set, and the right press is already the Total Commander model.
- 2026-11-04 — Plan check (Terra xhigh) applied. Changes:
  - shell rule over the whole line, decided by the orchestrator; Q4 withdrawn;
  - archive Esc;
  - preview nesting reversed;
  - the preview list-focus rule; K5 split into K5a and K5b;
  - user-command keys registered and rebuilt on reload;
  - context-aware key lookups as K6a;
  - `workspace.new` reused;
  - the Ctrl+K target split into U7a and U7b;
  - checks keep the exit status;
  - list-file lifetime;
  - the fallback for a palette larger than the window;
  - Q5 default is Total Commander.

  Added by Turgon:
  - `wt` escapes `;` in values (D9);
  - three more literal hints (`keys_bar.rs`, `archive/body.rs`, `space/keys.rs`, now K8a and K8b);
  - the preview sheet's literal Esc (K5b).
- 2026-11-04 — U1–U7b built; UR review (Opus senior, Sol cross): SHIP WITH FIXES. Two decided deviations, from the earlier security fix rounds:
  - D9: Windows Terminal (`wt`) commands refuse every placeholder in their arguments (load error pointing to `start-in` and `wt -d .`) instead of escaping `;`.
  - D11: list files live in a private, checked `%LOCALAPPDATA%\Navo\lists` folder (Unix: `$XDG_RUNTIME_DIR`), not `<StoreDirs.data>/user-commands/`; a portable install writes these short-lived files outside its folder.
  - Fix rounds: UF-A (navo-app: value shell check, missing program at run, resolved shell at load, `-` names, exit reports, stale pending sidecars, waiter thread first, test release) and UF-B (navo-ui: D14 key-clash warnings, run notices clear, open Ctrl+K refresh).
