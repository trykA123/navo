# P32 · Every string in Fluent files

Status: L1 done (reviewed, merge); L2–L6 moved after 1.0 by the owner (2026-10-20) · Repo: `<repo>` · Branch: `p32-lN` per atom · Started: 2026-10-19

## Goal

Every text a user reads in Navo comes from Fluent files (English only for now), so a translation or a wording change never touches Rust code, and a check stops new hard-coded text. Roadmap Alpha: "every string in Fluent files (English only)"; architecture names `fluent-bundle`. The 2026-10-15 audit found no Fluent files; on 2026-10-19 about 560 capitalised string literals sit in `navo-ui` (76 files under `window_view` alone) and `navo-app`, and command labels are `&'static str` in `commands/mod.rs`.

## Decisions

Final. Workers never reopen them; a worker that finds one wrong stops and reports.

- **D1 — Crate.** New crate `navo-l10n` (depends on nothing in the workspace) with `fluent-bundle` (concurrent bundle), `unic-langid` and `fluent-syntax` for the check. English files under `crates/navo-l10n/locales/en-US/`, one per area: `app.ftl`, `commands.ftl`, `files.ftl`, `jobs.ftl`, `preview.ftl`, `viewer.ftl`, `settings.ftl`, `errors.ftl`, `dialogs.ftl`. They are embedded with `include_str!` and parsed once into a process-wide bundle on first use. Unicode isolation marks are off.
- **D2 — API.** `t!("id")` and `t!("id", name = value, …)` return `String`; `tr(id)` and `tr_args(id, args)` are the function forms for ids built at run time. A missing id or a formatting error returns the id itself, logs once, and fails `debug_assert!` so tests catch it. Numbers that Navo formats itself (sizes in KB/MB/GB, dates, durations) stay Rust code and enter Fluent as ready strings.
- **D3 — Ids.** Kebab case with an area prefix: `files-new-folder`, `jobs-copy-title`, `error-access-denied`. Command labels use `cmd-` plus the command name with dots as dashes (`cmd-preview-recount`); `CommandSpec.label` becomes the id and every display site calls `tr`. Plurals use Fluent selectors on a `count` argument.
- **D4 — Scope.** In: every visible UI text in `navo-ui` and `navo-app` (window, sheets, dialogs, notices, menus, tooltips, status and F-key bar, accessibility names), the user-facing text for `FsErrorKind` and job errors, the settings sheet labels and help. Out: log lines, the diagnostic report body, CLI `--help` text, test strings, file-format names that are proper names (Intel HEX, ELF), and key names.
- **D5 — Check.** A gate step `l10n` in xtask: every `.ftl` parses without errors; every id used as a literal in `t!`/`tr` exists; every id in the files is used, except ids reached through the command registry (a `navo-ui` test checks each command's `cmd-` id exists); and a ratchet on capitalised multi-word string literals in non-test code of `navo-ui` and `navo-app`, whose allowed count lives in `xtask/l10n-allow.txt` and may only go down (0 when L6 is done, apart from listed proper names).

## Atoms

| Atom | What | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| L1 | D1–D3 crate, macro, command labels through `cmd-` ids, D5 gate step with today's count as the ratchet | api-builder (Codex sol high), then reviewer (Opus) | high | — | unit tests for lookup, arguments, plurals, missing id; command id test; gate passes with the new step | done, review pending |
| L2 | `window_view` part 1 (browser, panes, tabs, status, F-key bar, notices) | patcher (Sonnet medium) | medium | L1, P21 V9, P18 C4, P29 merged | ratchet count goes down; snapshot and UI tests unchanged in output; gate | after 1.0 |
| L3 | `window_view` part 2 (preview, viewer, shell menu, dialogs) | patcher (Sonnet medium) | medium | L2 | as L2 | after 1.0 |
| L4 | settings sheet, shortcuts page, app menu, job card, context menu, confirm dialog | patcher (Sonnet medium) | medium | L2, P22 S3 merged | as L2 | after 1.0 |
| L5 | error texts (`FsErrorKind`, job errors) and `navo-app` notices | patcher (Sonnet medium) | medium | L1 | as L2 | after 1.0 |
| L6 | Ratchet to 0, review of wording consistency against `docs` terms | reviewer (Opus) then patcher | medium | L2–L5 | ratchet 0 apart from an exception list of proper names and log lines (`l10n-allow.txt` holds only a number today); gate | after 1.0 |

## Done when

- `xtask l10n` passes with an allowed count of 0 apart from listed proper names, every UI text comes from `en-US/*.ftl`, and screenshots of the main window, settings and preview look unchanged.

## Log

- 2026-10-19 — Plan written. L2–L4 wait until the UI branches in flight (P21 V9, P18 C4, P29, P22 S3) are merged, because they touch the same files.
- 2026-10-20 — L1 built on `p32-l1`: crate `navo-l10n` (`t!`, `tr`, `tr_args`), 286 command labels now `cmd-` ids in `commands.ftl` and shown through `tr` at 8 sites, gate step `l10n` with the ratchet at 679 in `xtask/l10n-allow.txt`. Visible text and snapshots unchanged. Next: reviewer on the branch, then merge; L2–L4 stay blocked on the UI branches.
- 2026-10-20 — Opus review: merge. Owner moved L2–L6 after 1.0 (migration is churn with no visible value while English only). The exact ratchet stays: new UI text goes into `.ftl` files through `t!`, and a lower count updates `xtask/l10n-allow.txt`. Notes for later: `#[cfg(test)] mod fixtures` in `viewer/image` is still scanned (count high by one); strings in attributes such as `#[error(…)]` are not counted, which matters for L5.
