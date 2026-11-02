# P39 · Activity strip

Status: ready · Repo: `<repo>` · Branch: `p39-*` · Started: 2026-10-20

## Goal

The F-key bar is off by default (D67). Its row becomes the activity strip, direction 4 "one line, one thing" from the prototype of 2026-10-20: a row that appears only while something happens, shows the most urgent item, and fills as that item's progress.

## Decisions

- **D1 — Default off.** `window.view.fkey-bar` defaults to `false`. A user who set it keeps their value. Turning it on shows the bar as today, with the strip as a second row only while it has items.
- **D2 — Items and rank.** `ActivityItem` kinds, most urgent first: `Failure` (a job ended with errors), `Drag` (a drag is active; filled by P36 D4/D6), `Job` (copy, move, delete in progress), `Done` (the last finished file action, for 8 s, paused while hovered or focused), `Background` (long work such as indexing; no producer yet). Within one kind, the newest first.
- **D3 — The line.** One row of the existing F-key bar height. It shows the top item:
  - Job: icon, "Copying 128 files to D:\Backup", percent, "42 of 128 · 84 MB/s · 1 m 14 s left", Pause and Cancel buttons. The row background fills from the left with the accent at about 11 % as the progress; paused jobs fill in the faint colour.
  - Failure: warning tint, "2 files could not be copied", Show (Enter) and Dismiss (Del). Show opens today's job error view.
  - Done: "Moved 3 items to Projects". When job undo exists, it adds "Undo Ctrl+Z"; until then it has no button.
  - Drag: the drag key hints "Ctrl copy · Shift move · Alt shortcut · Esc cancel".
  - The other items wait as chips at the right ("Move 64 %", "Moved 3 items"); a click on a chip shows that item in the line until it ends. An "All N Alt+J" button shows when there are two or more items.
- **D4 — The drawer.** Alt+J with two or more items opens a drawer above the strip, over the lists (drawers over content, D58), listing every item with its full text, progress bar and actions. Esc closes it and returns focus to the pane. With one item, Alt+J focuses the line's first button.
- **D5 — Keys inside the strip.** ←/→ (and ↑/↓ in the drawer) move between buttons, Space pauses or resumes a job, Del cancels a job or dismisses an item, Enter acts, Esc returns to the active pane. These keys act only while the strip or drawer has focus. Alt+J is unbound today; Ctrl+J stays the job card.
- **D6 — Motion.** The row and items fade and rise 6 px in 180 ms and leave in 120 ms; progress fills linearly. With reduced motion, no travel. No flashing, no colour pulses.
- **D7 — It replaces the job pill.** The pill strip shown when the bar is hidden goes away; its job data feeds the strip.
- **D8 — Text** in `files.ftl` and `commands.ftl` (D59).

## Atoms

| Id | Atom | Role | Effort | Depends | Check | Status | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| S1 | `fkey-bar` default off | ui-builder (Sonnet) | medium | — | `cargo nextest run -p navo-store settings`, `-p navo-ui settings window_view` | ready | |
| S2 | `activity` model: items, rank, Done timer, from job infos | ui-builder (Opus) | medium | — | `cargo nextest run -p navo-ui activity` | ready | pure model, no drawing |
| S3 | strip line, chips, fill, Alt+J drawer, keys; replaces the pill | ui-builder (Opus) | medium | S1, S2 | `cargo nextest run -p navo-ui window_view::tests::activity`; gallery shots | done | job card hidden by default; Ctrl+J shows it; Show on a failure opens it |
| S4 | review | reviewer (Opus) | high | S3 | review notes; gate; Windows CI | blocked | |

Drag and Background items get producers later (P36 D4/D6; the search index).

## Done when

- With default settings the bottom row is gone when idle, shows a running copy with Pause and Cancel, and leaves after the job ends and its Done item expires.
- Alt+J reaches every item by keyboard.
- Gate and Windows CI pass.

## Log

- 2026-10-20 — Owner picked direction 4 of the activity-strip prototype; D67 amended.
- 2026-10-20 — S3 done: activity strip line, chips, fill, Alt+J drawer, strip keys in the registry; job pill removed from the window; gallery pages p39-strip, p39-strips, p39-drawer.
- 2026-10-20 — S4 review fixes: the primary item keeps its icon and buttons (chips capped at 45%, cut-off chips are no stops); Show closes the drawer before the card opens; focus returns to where it was (pane or viewer); strip state refreshes on job, timer, hover and focus events, not in render; Delete and Space do nothing on "All N"; Show/Dismiss keycaps only while the strip has focus; the band rings when its item has no buttons; a paused chip says Paused; drawer ↑/↓ scrolls the row into view; gallery pages p39-strip-6-1280, p39-strip-6-390, p39-drawer-6-390.
