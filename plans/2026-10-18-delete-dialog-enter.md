# Delete-dialog keyboard regression

Status: done (pending merge) · Repo: `<repo>` · Branch: `fix-delete-dialog-enter` · Started: 2026-10-18

## Goal

Enter activates the focused button in the real delete dialog. Cancelling keeps every file intact; confirming performs only the requested operation.

## Decisions

- D1 — Reproduce the owner's alpha-2026-10-17c report through the real window key bindings before changing production code.
- D2 — Cover Enter on both Cancel and confirmation, including a selected folder. Check shared prompt-dialog routing if the cause affects it.
- D3 — Preserve the existing keyboard contract and typed-confirmation protection. Tests use only an isolated fake filesystem.

## Components

| Component | Layer | Contract | Change | Test |
| --- | --- | --- | --- | --- |
| Delete dialog | UI | Focused button + Enter → cancel or confirm | Reproduce and fix routing | Headless GPUI window tests |
| Shared dialog keys | UI | Key binding + focused control → action | Change only if the reproduction proves it necessary | Dialog and prompt regressions |

## Atoms

| Atom | Component | Role | Effort | After | Check | Status | Cost |
| --- | --- | --- | --- | --- | --- | --- | --- |
| R1 | Delete dialog | checker, Opus | medium | — | New UI regression compiles and fails for the reported behaviour | done | |
| G1 | Proven routing cause | orchestrator | high | R1, orchestrator accepts tests | New tests pass without changing their contract | done | |
| Q1 | Full branch | reviewer, Opus | high | G1 | Review, unpiped gate, merge gate, Windows CI | blocked | |

### R1 — Reproduce Enter on each delete-dialog button

- **Read:** `docs/bugs.md`; `confirm_dialog/`, `prompt_dialog/`, `dialog_focus.rs`, `commands/bind.rs`, and the window tests under `crates/navo-ui/src/`.
- **Change:** regression tests only; exercise the production binding setup and use the existing fake filesystem.
- **Check:** targeted `cargo nextest run -p navo-ui` filters; record exact failures and logs outside the repository.
- **Stop if:** the report cannot be reproduced, production code needs editing, or a missing decision changes keyboard behaviour.

### G1 — Fix the proven cause

- **Change:** the smallest shared correction established by the failing tests; leave the Red test contract intact.
- **Check:** the new regressions and affected existing dialog, prompt and keymap tests.
- **Stop if:** the test contract is wrong or the fix extends beyond the proven regression.

## Done when

- Enter activates both focused delete-dialog buttons and Cancel leaves the isolated files intact.
- The tests fail before the fix and pass afterwards.
- The reviewer approves, the branch and merge gates pass, and Windows CI succeeds.

## Log

- 2026-10-18 — Read the full previous-session handoff and stopped-branch notes. Main is clean with seven local documentation commits. Created an isolated branch; implementation remains Claude-only as requested.
- 2026-10-18 — R1: six window tests (Enter and Space on each button, Delete and Shift+Delete, production bindings) pass on Linux; the Windows key path and binding order were ruled out. Leads: focus on the kit button inside a stop, or Shift held.
- 2026-10-18 — Reproduced: after the window's tab order (`focus_next`, used by gpui-kit's Tab) moves focus from a stop onto the kit `Button` inside it, Enter does nothing because `activate`/`press` match only the stop handle (`is_focused`). Test `enter_after_the_window_tab_order_moved_focus_still_presses` failed, then passed with `contains_focused` in both the confirm and prompt dialogs. Shift+Enter is still not bound; ask the owner whether Shift was held.
