# P27 · Modifier hints drawer

Status: done (owner's check on the next alpha pending) · Repo: `<repo>` · Branch: `p27-m1` · Started: 2026-10-19

## Goal

While Ctrl, Shift or Alt is held in the file list, a drawer above the F-key bar shows every shortcut on that layer, not only the F-keys, and disappears on release. Owner's pick 2026-10-19: prototype `modifier-hints` direction A (`.crew/prototypes/modifier-hints/opus/index.html`, artifact UedLUEd54ika6jeYhSKN97). It can be turned off.

## Decisions

Final. Workers never reopen them; a worker that finds one wrong stops and reports.

- **D1 — When.** The drawer opens after the modifier set has been held unchanged for `appearance.modifier-hints-delay-ms` (default 400, clamped 150–2000) with no other key pressed. Any non-modifier key during the wait (a chord such as Ctrl+C) cancels it until all modifiers are released. Releasing every modifier hides it at once. Adding Shift to a held Ctrl switches to the Ctrl+Shift layer without a new delay. Only when focus is in the file list (pane) context; in the viewer, editor, dialogs, palette and sheets nothing opens and the F-key bar behaves as today.
- **D2 — Content.** Built from the live keymap (built-in plus the user's keymap.toml), so rebinds show at once: every binding in the window and file-list contexts whose keystroke uses exactly the held modifier set. One entry per command; several keys on one line (`Tab` `PgDn`). Groups and order: Clipboard, Tabs, Sort, Files, Pane, View, Navigation, Window, by a small table from command name to group; an unmapped command goes to "Other". On the Ctrl layer, a last muted column "+ Shift" lists the Ctrl+Shift entries. Labels are the commands' registry labels. A layer with no entries shows "No shortcuts use {combo} in the file list." Amended 2026-10-19 after the first build: a run of numbered commands of one family (Go to tab 1 … Go to tab 8) folds into one row ("Go to tab 1–8", keycap `1–8`), as in the prototype; a binding whose command has no handler in this build (for example Ctrl+K, Ctrl+N, Alt+F4 today) is left out.
- **D3 — Look (prototype A).** A raised card spanning the window width minus the side gutters, sitting just above the F-key bar, rounded top, the app's raised surface token and the up-shadow; header: modifier chip, "{n} shortcuts in the file list · add Shift for {m} more · release to hide", and a "Turn off" button with its `Ctrl+,` keycap on the right; body: one column per group with an uppercase group label and rows of keycaps plus label. Height follows the content. Fade and 12 px rise in ≤150 ms, none under reduced motion. Both themes AA. No new colours outside the token set.
- **D4 — Cursor row.** The drawer never moves layout. If the active pane's cursor row would be covered, the active list is drawn shifted up by whole rows so the cursor row sits above the drawer, and returns when the drawer closes; the scroll position and the passive pane do not change.
- **D5 — Setting.** `appearance.modifier-hints = "all" | "fkeys" | "off"`, default `"all"`; `"fkeys"` is today's behaviour (only the F-key labels switch); `"off"` keeps the F-key bar fixed too. `appearance.modifier-hints-delay-ms` as D1. Both in navo.toml and in the settings sheet's Appearance section, live like the other appearance rows. "Turn off" sets `"fkeys"` and opens the settings sheet on that row.

## Atoms

| Atom | What | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| M1 | Settings keys (D5), the pure layer model (D2), the hold state machine (D1), the drawer (D3, D4) | ui-builder (Opus medium) | medium | — | unit tests for the model and the state machine; UI tests: hold opens after the delay, a chord never opens it, release hides, Ctrl→Ctrl+Shift switches, off and fkeys settings, viewer focus opens nothing, a rebind shows; screenshots at 1280 in both themes for Ctrl, Shift, Alt and the cursor-row case; gate | done |

## Done when

- Holding Ctrl in the file list shows the drawer as in prototype A, in both themes; checked in screenshots and by the owner on the next alpha.
- Tests and the gate pass.

## Log

- 2026-10-19 — Owner picked direction A of the modifier-hints prototype; plan written.
- 2026-10-19 — M1 merged. Hold detected from the root modifiers listener plus `cx.intercept_keystrokes` for chords; a pure `Hold` state machine with token timers. First build followed D2 literally (Ctrl drawer ≈ 390 px with eight Go-to-tab rows, unhandled Ctrl+K/N listed); D2 amended, second pass folds numbered runs by label and drops commands without a listener (`is_action_available_in`), Ctrl drawer 222 px. Key names are kebab-case (`appearance.modifier-hints`). Open: Shift+F10 anchor fix has no test; arrow keys step the delay by 1 ms.
