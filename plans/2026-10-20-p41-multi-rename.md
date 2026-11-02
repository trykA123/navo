# P41 · Multi-rename

Status: ready (Q1–Q3 open, architect defaults apply until answered) · Repo: `navo` · Branch: `p41-<atom>` per atom · Started: 2026-10-20

## Goal

The user marks files, presses Ctrl+M and renames them all at once. They can use masks (`[N]`, `[E]`, `[C]`, dates, parent folder), search and replace (plain or regex) and case rules. A live table shows every old → new name and flags every conflict before anything happens. The batch runs as one job with progress and never overwrites anything. If one rename fails, every name goes back. Ctrl+Z undoes the whole batch in one step, even after a restart. Presets save a set of rules. Total Commander's Multi-Rename Tool is the reference for behaviour only. The look is Navo's own.

## What exists today (checked at d23c1e4)

- **Single rename.** F2 and Shift+F6 (`files.rename`) open the prompt (`window_view/prompt.rs`), which calls `Browser::rename` (`browser/naming/mod.rs`). It checks the name with `navo_core::check_name` and refuses a name already in the listing (it compares lower-case). It then runs straight on the I/O queue, with no plan, no journal and no undo. A case-only rename goes through a `.navo-rename` step name (`rename_case`).
- **Planner and job engine.** `OperationPlan` already has `Step::Rename { source, target }`. `OpKind` has only Copy, Move, Recycle and Delete, and is matched exhaustively in 8 files (`op_plan/{mod,risks}.rs`, `activity/text.rs`, `job_card/{mod,summary}.rs`, `browser/operate/mod.rs`, `window_view/operate/mod.rs`, `file_list/keys.rs`). `run_job` stops at the first failed step. `roll_back` undoes done renames in reverse (`job/undo.rs` `undo_rename`). Progress counts only `CopyFile` and `RemoveFile` steps (`job/run.rs` `counted`).
- **The lower layer already never overwrites.** `ProviderOps::rename` is `MoveFileExW` without `REPLACE_EXISTING` on Windows and `renameat2(NOREPLACE)` on Linux. The fake file system compares names case-sensitively only. It has fault injection (`Fault::Fail`, `Fault::Collide`).
- **Journal.** `JobJournal` lives in `navo.db` (`navo-store/src/journal`). On start, `recover()` (`navo-app/src/jobs/mod.rs`) rolls back unfinished jobs. Finished jobs are pruned to 50. There is no user-facing undo for any job (P36 log). Ctrl+Z is bound only in the settings sheet, and Ctrl+M is free.
- **Marks (P36).** `Browser::selected_sources()` returns the marked rows, or the cursor row when nothing is marked. A finished job on the active tab clears the marks.
- **Activity (P39 S2).** `Activity` turns `JobInfo` into Job, Done and Failure items, with texts in `files.ftl`. The strip view (P39 S3) is not built yet. P39 says a Done item gains "Undo Ctrl+Z" once job undo exists.
- **UI pieces.** `primitives::columns` (`ColumnSet`, `ColumnSpec`, `fit_name`; P38 A2), `VirtualList`, `TextField`, `IconButton::selected` (the toggle look of the viewer find bar), `Kbd`, the D46 menu panel, `prompt_dialog`, `confirm_dialog`, `dialog_focus`.
- **Text, data and crates.** Fluent files are listed in `navo-l10n/src/catalog.rs`. `navo.db` has migrations V1–V4. navo-ui uses `chrono`. `regex` 1.13 appears only as a transitive entry in `Cargo.lock`.
- **Decisions in force:**
  - D6/D61: one command surface. Rename goes through the planner and journal as an `OpKind::Rename` batch with swaps, cycles, case-only renames and one undo, and single rename moves onto the same path.
  - D7: plan before acting.
  - D15 and P36: marks.
  - D39: preview before change; undo offers only what it can do.
  - D58: calm UI, keys shown.
  - D59: new text goes in `.ftl` files.
  - D65: the delete safety net shares this journal and undo; it comes after the alpha.
  - D67: the strip shows the undo key.
  - D68: multi-rename is alpha item 5, after Ctrl+K.

## Decisions

Final. The owner asked for at least Total Commander parity plus the bonuses the architect chooses (2026-10-20), and left Q1–Q3 to the architect: the recommended answers apply. Workers never reopen them; a worker that finds one wrong stops and reports.

### Rules: how a new name is built

- **D1 — Order.** Each item goes through the name and extension masks, then search and replace, then case. Then the result is checked. This is Total Commander's order.
- **D2 — Name and extension.** The extension is the text after the last dot. A leading dot does not count, so `.gitignore` has no extension. Folders are split the same way. An empty extension adds no trailing dot.
- **D3 — Mask tokens.**
  - Name parts: `[N]` is the whole name, `[N3]` the 3rd character, `[N2-5]` characters 2 to 5, `[N2,5]` 5 characters from the 2nd, `[N2-]` from the 2nd to the end. A negative number counts from the end (`[N-3,3]`, `[N2--2]`). `[E…]` works the same way on the extension.
  - Folders: `[P]` is the parent folder name, `[G]` the grandparent.
  - Counter: `[C]`, or `[C10+5:3]` with its own start, step and digits. Partial forms (`[C10]`, `[C+5]`, `[C:3]`, `[C10:3]`) take the missing parts from the form's Start, Step and Digits.
  - `[F]` upper-cases the first letter of each word and leaves the other letters as they are.
  - Dates: `[Y] [y] [M] [D] [h] [i] [s]` come from the modified time in local time. `[Y]` has 4 digits, `[y]` and the others 2. `[d]` is the date as `YYYY-MM-DD`, `[t]` the time as `hh.mm.ss` (a colon is not allowed in names). Minutes are `[i]`, as in Total Commander.
  - Case from here: `[U]` upper, `[L]` lower, `[F]` first letter of each word upper, `[n]` back to the original. Each applies to the mask's output after it, until the next case token.
  - Fields: `[=group.field]` takes a value from a field provider (D21), for example `[=exif.date]`, `[=audio.artist]`, `[=file.size]`. A missing value gives empty text.
  - `[[` and `]]` are literal brackets.
  - Positions count characters from 1. A part outside the name gives empty text.
  - An unknown token shows an error under the field with its position, and no preview runs.
- **D4 — Counter.** Start (1), Step (1, at least 1) and Digits (1–10, zero-padded). Numbers follow the preview order, which is the pane's sort order. Excluded rows take no number.
- **D5 — Search and replace.**
  - One Find field and one Replace field, with four toggles: Regex, Match case, First only, Name only (leave out the extension). Without Name only, search works on the whole `name.ext`.
  - Plain mode replaces every match and ignores case unless Match case is on. `|` separates several pairs (`a|b` → `x|y`); this is safe because `|` cannot appear in a Windows name.
  - Regex mode uses the `regex` crate, with `$1` and `${name}` in Replace. An invalid regex shows an error under the field.
- **D6 — Case.** Name case: Unchanged, lower, UPPER, First letter, Title Case (a word starts after a space, `_`, `-` or `.`). Extension case: Unchanged, lower, UPPER. Unicode `to_lowercase`/`to_uppercase`, with no locale rules.

### Planning and safety

- **D7 — Plan the whole batch first.**
  - `navo_core::rename::plan_batch` marks every row as Changes, Unchanged, Excluded or Problem.
  - Problems: an invalid name (`check_name`), an empty name, two rows with the same new name, or a name that belongs to an item outside the batch. That check uses the full listing, so hidden and filtered items count.
  - Any problem blocks the whole batch: Rename stays off until the user fixes the rules or excludes rows.
- **D8 — Case folding.** On Windows, names compare case-insensitively (simple Unicode folding) for every location, shares included; this is the stricter, safe choice. On Linux they compare exactly. When names compare case-insensitively, a case-only rename runs as two steps through a temporary name. On Linux FAT, exFAT or SMB mounts under `Fold::Exact`, a case-only rename is refused by the no-replace rename and put back (safe).
- **D9 — Chains, swaps and cycles.**
  - Chains are ordered so each rename lands on a free name (b→c runs before a→b).
  - Each cycle (a→b, b→a or longer) is broken with one temporary name: a→t, b→a, t→b.
  - Temporary names are `.navo-rename-<8 hex>-<n>`, so they do not depend on the original name's length. The planner checks them against the listing and the batch.
  - The result is `OperationPlan { kind: OpKind::Rename, steps: [Step::Rename…], conflicts: [] }`, so the move-over path never runs.
- **D10 — One job, all or nothing (Q2).**
  - The batch runs as one journaled job on the volume's queue slot. Progress (renamed items of total, temporary steps not counted) shows in the job card and the activity model.
  - If a step fails (file in use, access denied, a name created after the preview), or the user cancels, the job puts back every done step in reverse. The Failure item names the file and the reason. If putting back also fails, the item lists the files still renamed, and the batch stays undoable.
  - A crash in the middle is put back on the next start by the existing `recover()`.
- **D11 — Undo the last batch.**
  - `files.undo` (Ctrl+Z in the file list) undoes the newest rename batch that has not been undone yet. It reads the batch from the journal, so it works after a restart, and it runs as a job with progress.
  - A step is undone only if the item still has its new name and the old name is free. Other steps are skipped and counted ("3 items had changed and were left as they are"). Any item left under a temporary name is listed.
  - If the strip's "Ctrl+Z undo" Done item for that batch is gone, Ctrl+Z asks first: "Undo the rename of 40 items in Projects?".
  - There is no redo. Undo for copy, move and delete stays with D65.
- **D12 — Single rename on the same path (Part C).** F2 plans a one-row batch with `plan_batch` and runs it as a quiet job, so Ctrl+Z undoes it too. The prompt, its errors and the cursor behave as today.

### The tool

- **D13 — Opening.**
  - `files.multi_rename` opens it: Ctrl+M in the file list (Total Commander's key), the context-menu Rename with 2 or more marks, and Ctrl+K. With Q1, F2 on 2 or more marks opens it too.
  - It takes the marked items, or the cursor item when nothing is marked. `..` is never included.
  - While the listing is still loading, it does not open and the status line says so.
- **D14 — Layout.**
  - A modal panel on the dialog scrim, centred, `min(window − 64, 1120)` px wide and `window − 96` tall.
  - Header: "Rename 40 items · Projects" and a Presets button.
  - Three rows of fields:
    1. Name mask and Extension mask, each with a `[ ]` token button.
    2. Find and Replace, with the Regex, Match case, First only and Name only toggles (`IconButton::selected`).
    3. Name case, Extension case, and the counter's Start, Step and Digits.
  - The preview table fills the rest.
  - Footer: the summary ("38 to rename · 2 unchanged · 1 problem"), the key-hint line (D58 rule 7), Cancel and Rename.
  - No tips, no banners. Motion is the same as dialogs today.
- **D15 — Preview table.**
  - Built on `primitives::columns` and `VirtualList`, with resizable Old name, New name and Status columns. Names are shortened with `fit_name`.
  - Row looks:
    - Unchanged: muted.
    - Excluded: muted and struck through.
    - Problem: error colour, with the reason in Status ("Already exists", "Same new name as row 12", "Cannot contain :").
    - Case-only and swap rows say "Case only" or "Swap", quietly.
  - A "Problems only" toggle filters the rows.
  - The preview recomputes on the background executor, 60 ms after the last change, with a generation number; stale results are dropped.
- **D16 — Keys.** All in the `MultiRename` context, rebindable, with `cmd-` labels.

  | Key | Action |
  | --- | --- |
  | Enter | Rename; with problems, move to the next problem instead |
  | Esc | Close the open menu, otherwise close the tool with no change |
  | Tab / Shift+Tab | Next / previous field |
  | Ctrl+Space | Token menu (in a mask field) |
  | Ctrl+↓ / Ctrl+↑ | Focus the preview / back to the last field |
  | PgUp / PgDn | Scroll the preview from any field |
  | Alt+↓ / Alt+↑ | Next / previous problem row |
  | Del (in the preview) | Exclude or include the row |
  | Ctrl+S / Ctrl+O | Save a preset / open the preset list |
  | Ctrl+R | Reset every field to its default |

  Enter with problems moves to the next problem after the cursor, wrapping to the first.

- **D17 — Menus.** The `[ ]` button and Ctrl+Space open the D46 menu panel. It lists each token with one line ("[N2-5] · characters 2 to 5 of the name"), filters as the user types, and inserts the token at the caret on Enter. The same panel lists the case choices and the presets.
- **D18 — Presets.**
  - A preset holds the rules only. It is stored as versioned JSON in `navo.db` V5: `rename_preset (name TEXT PRIMARY KEY, rules TEXT NOT NULL, saved_at INTEGER NOT NULL)`.
  - Ctrl+S asks for a name; an existing name asks before replacing.
  - In the list (Ctrl+O), Enter loads a preset and Del deletes it after a confirm.
  - Every run saves its rules as "Last used" (stored under the reserved name `""`).
  - The tool opens with the defaults (`[N]`, `[E]`, empty Find, Unchanged case) (Q3).
- **D19 — Text.** Tool texts go in a new `rename.ftl` (added to `catalog.rs`), labels in `commands.ftl`, activity texts in `files.ftl` (D59).

### Total Commander parity and bonuses (Part D)

- **D20 — Parity list.** Everything in Total Commander's Multi-Rename Tool that applies to Navo is in: masks with ranges, counter, dates and times, case tokens, field tokens (TC's content plugins), search and replace with regex and the extension switch, case lists, presets with last used, Edit names, Undo, the log file, and reordering the list to change the counter order.
- **D21 — Field providers.** `rename::fields` is a registry of providers, each a group of named fields read on demand for the items a mask uses (never read when no `[=…]` token is in the rules).
  - Built in: `file` (`size`, `size_kb`, `created`, `accessed`, `attrs`), `exif` (`date`, `camera`, `model`, `width`, `height`; kamadak-exif, already a dependency), `audio` (`artist`, `title`, `album`, `track`, `year`, `genre`; `lofty`, pinned).
  - Plugins add groups through the same trait later (D-modularity); the token menu lists every registered field.
  - Reads run on the background executor with the preview's generation number; a slow read shows "…" in the row until it lands.
- **D22 — Clean-up.** A fourth row of toggles after case: Trim spaces, Collapse repeats (spaces, `_`, `-`, `.`), Remove accents (`ă→a`, `ș→s`, `é→e`; Unicode decomposition, then marks dropped), Spaces to `_`. They run last, before the check.
- **D23 — Edit names.** Ctrl+E switches the preview to a text editor with one new name per line. Edits there override the rules for that row and show as "Edited" in Status; a line count that differs from the row count is a problem that blocks Rename. "Load names from file…" and "Paste names" fill the same lines; "Copy new names" copies them.
- **D24 — Order and folders.** Ctrl+Shift+↑/↓ in the preview moves a row, which changes the counter order; clicking a column head sorts the preview. "Counter per folder" restarts the counter in each folder when the items come from several folders (search results, flat views); collisions are always checked per folder.
- **D25 — Preview help.** The New name column highlights the characters that changed. With Find filled, the Old name column highlights the matches. A path longer than 260 characters is a warning, not a problem.
- **D26 — Undo history and log.** Ctrl+Z walks back through the rename batches, newest first, up to the last 20 kept in the journal; each one undoes only steps that still apply (D11). An optional log (Settings › Files › Rename log) appends `old → new` lines per batch to a text file in the profile folder.

## Components

| Component | Layer | Contract (in → out) | New / change | Test |
| --- | --- | --- | --- | --- |
| `navo-core::rename::mask` | lib | `&str → Mask`; `Mask::render(&Subject, counter) → String` | new | `rename/tests/mask.rs` |
| `navo-core::rename::rules` | lib | `RenameRules → Compiled`; `new_name(&Subject, index) → String` | new | `rename/tests/rules.rs` |
| `navo-core::rename::check` | lib | rows, existing, `Fold` → `Vec<RowState>` | new | `rename/tests/check.rs` |
| `navo-core::rename::order` | lib | `plan_batch(rows, existing, fold, seed) → BatchPlan` | new | `rename/tests/order.rs` (proptest) |
| `OpKind::Rename` | lib + ui arms | new enum variant | change | existing tests |
| `navo-fs::fake` fold | adapter (test) | `FakeFs::case_insensitive()` | change | `fake/tests/fold.rs` |
| `navo-fs::job::rename` | adapter | `run_rename(plan, ctx) → RenameOutcome` | new | `job/tests/rename.rs`, Windows `local/tests` |
| `navo-store::journal` undo | adapter | `last_undoable(kind)`, `mark_job_undone`, `JobState::Undone` | change | `journal/tests` |
| `navo-store::db` presets | adapter | V5; `save_preset`, `presets`, `delete_preset` | change | `db/tests/rename_presets.rs` |
| `commands` entries | lib (ui) | `files.multi_rename`, `files.undo`, `MultiRename` group | change | `commands/tests.rs`, snapshots |
| `multi_rename::form` | lib (ui) | `Form → RenameRules`; `Preview::compute(..) → Preview` | new | `multi_rename/tests/form.rs` |
| `browser` rename batch | service (ui) | `rename_batch(plan, first_new)`, `undo_rename(offer)`, `RenameHooks` | new | `browser/tests/rename_batch.rs` |
| `multi_rename::view::fields` | ui | props in, `FieldEvent` out | new | `multi_rename/tests/fields.rs`, gallery |
| `multi_rename::view::preview` | ui | rows and summary in, `PreviewEvent` out | new | `multi_rename/tests/preview.rs`, gallery |
| `window_view::multi_rename` | ui container | open, compute, keys, run, presets | new | `window_view/tests/multi_rename.rs` |
| `navo-app::rename_hooks` | wiring | journal and preset closures for `RenameHooks` | new | `navo-app/src/rename_hooks/tests.rs` |
| `window_view::undo` + activity text | ui container | Ctrl+Z → undo job; Done item "Ctrl+Z undo" | new | `window_view/tests/undo.rs`, `activity/tests.rs` |

## Atoms

Roles: Sonnet 5.5 medium or high for decided backend and UI work, Opus 5.5 medium for hard interaction, Opus 5.5 high for reviews. No Codex or Sol (D35). Each atom is test-first.

| Atom | Component | Role | Effort | After | Check | Status | Cost |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **Part A — engine** | | | | | | | |
| A1 | `rename::mask` | api-builder (Sonnet) | high | — | `cargo nextest run -p navo-core rename::tests::mask` | done | |
| A2 | `rename::rules` | api-builder (Sonnet) | high | A1 | `cargo nextest run -p navo-core rename::tests::rules` | done | |
| A3 | `OpKind::Rename` + arms | api-builder (Sonnet) | medium | — | `cargo nextest run -p navo-core -p navo-ui op_plan activity job_card` | done | |
| A4 | `rename::check` | api-builder (Sonnet) | high | — | `cargo nextest run -p navo-core rename::tests::check` | done | |
| A5 | `rename::order` + `plan_batch` | api-builder (Sonnet) | high | A3, A4 | `cargo nextest run -p navo-core rename::tests::order` | done | |
| A6 | `fake` case-insensitive mode | api-builder (Sonnet) | medium | — | `cargo nextest run -p navo-fs fake::tests::fold` | done | |
| A7 | `job::rename` all or nothing | api-builder (Sonnet) | high | A5, A6 | `cargo nextest run -p navo-fs job::tests::rename`; Windows CI | done | |
| A8 | journal undo queries | api-builder (Sonnet) | high | — | `cargo nextest run -p navo-store journal` | done | |
| A9 | `db` V5 presets | api-builder (Sonnet) | medium | — | `cargo nextest run -p navo-store db` | done | |
| A10 | review Part A | reviewer (Opus) | high | A1–A9 | review notes; `cargo xtask gate`; Windows CI | done | review: ship with fixes; fixed in A10a and A10b |
| **Part B — the tool** | | | | | | | |
| B1 | commands, keys, `rename.ftl` | ui-builder (Sonnet) | medium | — | `cargo nextest run -p navo-ui commands`; gate l10n step | done | |
| B2 | `multi_rename::form` | ui-builder (Sonnet) | high | A2, A5 | `cargo nextest run -p navo-ui multi_rename::tests::form` | done | |
| B3 | `browser` rename batch + undo | api-builder (Sonnet) | high | A7 | `cargo nextest run -p navo-ui browser::tests::rename_batch` | done | |
| B4 | `view::fields` + token menu | ui-builder (Sonnet) | medium | B2 | `cargo nextest run -p navo-ui multi_rename::tests::fields`; gallery shots | done | |
| B5 | `view::preview` + summary | ui-builder (Opus) | medium | B2 | `cargo nextest run -p navo-ui multi_rename::tests::preview`; gallery shots | done | |
| B6 | `window_view::multi_rename` | ui-builder (Opus) | medium | B1, B3, B4, B5 | `cargo nextest run -p navo-ui window_view::tests::multi_rename` | done | |
| B7 | `navo-app::rename_hooks` | api-builder (Sonnet) | high | A8, A9, B3 | `cargo nextest run -p navo-app rename_hooks` | done | |
| B8 | Ctrl+Z flow + activity text | ui-builder (Sonnet) | medium | A3, B1, B3 | `cargo nextest run -p navo-ui activity window_view::tests::undo` | done | |
| B9 | review Part B | reviewer (Opus) | high | A10, B6–B8 | review notes; gate; Windows CI | blocked | |
| **Part C — single rename** | | | | | | | |
| C1 | F2 on the batch path | Codex | high | B9 | `cargo nextest run -p navo-ui browser::naming window_view::tests` | built on `backend/rename-undo-safety`; Windows verification pending | |
| C2 | final review + owner checks | reviewer (Opus) | high | C1, E7 | review notes; gate; Windows CI; owner list | blocked | |
| **Part D — parity and bonuses** | | | | | | | |
| E1 | `rename::fields` registry + file, exif | api-builder (Sonnet) | high | A1 | `cargo nextest run -p navo-core -p navo-ui rename::fields` | done | |
| E2 | audio provider (`lofty`) | api-builder (Sonnet) | medium | E1 | `cargo nextest run -p navo-ui rename::fields::audio` | done | |
| E3 | clean-up rules | api-builder (Sonnet) | medium | A2 | `cargo nextest run -p navo-core rename::tests::cleanup` | done | |
| E4 | Edit names, load, paste, copy | Codex Sol 6.1 | high | B6 | `cargo nextest run -p navo-ui window_view::tests::multi_rename` | built on backend branch; Linux gate passed, native Windows pending | |
| E5 | row order, column sort, counter per folder | Codex Sol 6.1 | high | B6 | `cargo nextest run -p navo-ui multi_rename window_view::tests::multi_rename` | built on backend branch; Linux gate passed, native Windows pending | |
| E6 | change and match highlights, long-path warning | ui-builder (Sonnet) | medium | B5 | `cargo nextest run -p navo-ui multi_rename::tests::preview`; gallery shots | done | |
| E7 | undo history + rename log | Codex Sol 6.1 | high | B7, B8 | `cargo nextest run -p navo-app rename_hooks -p navo-ui window_view::tests::undo` | built on backend branch; Linux gate passed, Windows checks pending | |
| E8 | review Part D | reviewer (Opus) | high | E1–E7 | review notes; gate; Windows CI | blocked | |

The plan has 29 atoms. The orchestrator may save it as `p41a-engine` (A1–A10) and `p41b-tool` (B1–C2), with these decisions shared.

### A1 — masks
- **Read:** D2–D4; `navo-core/src/name/mod.rs`, `lib.rs`, `selection/pattern.rs` (style).
- **Change:** `navo-core/src/rename/{mod,mask}.rs`.
  - `Mask::parse(&str) -> Result<Mask, MaskError { at, kind }>`.
  - `Token`: `Text`, `Name(Span)`, `Ext(Span)`, `Parent`, `Grand`, `Counter(Option<CounterSpec>)`, `Year`, `Month`, `Day`, `Hour`, `Minute`, `Second`.
  - `Span`: `All`, `At`, `Range`, `Len`, `From`, using signed positions.
  - `Mask::render(&Subject, counter: u64) -> String`, where `Subject { stem, ext, parent, grand, time: Option<LocalTime> }`. The caller converts the time; the core has no clock.
- **Check:** every D3 token, negative spans, out-of-range parts, multi-byte names, `[[`, the position of an unknown token, and an insta snapshot of the token table.
- **Stop if:** two D3 tokens parse ambiguously; report the pair.

### A2 — rules
- **Read:** D1, D4–D6; `rename/mask.rs`.
- **Change:** `rename/rules.rs`.
  - `RenameRules { version, name_mask, ext_mask, find: Find { text, replace, regex, match_case, first_only, name_only }, name_case, ext_case, counter: Counter { start, step, digits } }`, with serde.
  - `Compiled::new(&RenameRules) -> Result<Compiled, RuleError { field, message }>`.
  - `Compiled::new_name(&Subject, index) -> String`.
  - Add `regex` 1.13 to navo-core.
- **Also:** extend `rename/parse.rs` to accept the partial counter forms of D3 (A1 refuses them today with `BadCounter`).
- **Check:** D1 order, `|` pairs, regex groups, First only, Name only, Match case, each case mode on non-ASCII names, counter step and padding, an invalid regex, a JSON round trip.
- **Stop if:** `regex` fails the workspace lints or `cargo deny`.

### A3 — `OpKind::Rename`
- **Read:** the 8 files listed under "Planner and job engine".
- **Change:**
  - Add the variant.
  - `plan()` returns an empty plan for it, and `risks` adds none.
  - Verbs "Rename / Renaming / Renamed"; icon `Pencil`.
  - `activity-renaming` and `activity-rename-failed` in `files.ftl`.
  - `op_request` returns `None` for Rename.
  - This atom touches 8 files with one arm each. That is above the 3-file limit, accepted because the edits are mechanical.
- **Check:** the listed tests pass, and the snapshots change only by the new arm.
- **Stop if:** an arm needs behaviour beyond the text above.

### A4 — row checks
- **Read:** D7, D8; `name/mod.rs`.
- **Change:** `rename/check.rs`.
  - `Fold::{Insensitive, Exact}` and `fold_key`.
  - `check_rows(&[Row { parent, old, new, excluded }], &Existing, Fold) -> Vec<RowState>`.
  - `RowState`: `Changes { case_only }`, `Unchanged`, `Excluded`, `Problem(Problem::{Invalid(NameError), Empty, Duplicate(row), Exists})`.
- **Check:** each problem; a case-differing duplicate is a problem under Insensitive and not under Exact; Exists only for names outside the batch; a target that is another row's old name is fine; a proptest that random names never panic.
- **Stop if:** `check_name` disagrees with D7; report it, do not change it.

### A5 — ordering and `plan_batch`
- **Read:** D9; `op_plan/mod.rs`; `rename/check.rs`.
- **Change:** `rename/order.rs` and `plan_batch(rows, existing, fold, seed) -> BatchPlan { states, plan: Option<OperationPlan> }`. `plan` is `None` while any problem exists. Build the dependency graph, order the chains, break each cycle with one temporary name, add the case-only steps, and set `totals.files` to the number of rows that change.
- **Check:** chain, swap, 3-cycle, a cycle mixed with case-only, a temporary name that collides, unchanged rows giving no steps. A proptest applies the steps to an in-memory folder model that folds names and refuses taken targets. It must show that every step lands on a free name, the end state is exactly the wanted names, and no other entry changes.
- **Stop if:** a valid batch cannot be ordered; report the case.

### A6 — case-insensitive fake
- **Read:** `navo-fs/src/fake/{mod,ops}.rs`, `fake/tests/mod.rs`.
- **Change:** a `FakeFs::case_insensitive()` builder. Lookups and collisions fold names, and an entry keeps the spelling of its newest rename.
- **Check:** `A.txt` collides with `a.txt`; a case-only rename keeps one entry; the default fake is unchanged.
- **Stop if:** `Tree` keys cannot fold without changing the default mode.

### A7 — all-or-nothing rename job
- **Read:** D10; `job/{mod,run,undo}.rs`, `job/tests/mod.rs`, `local/tests/ops.rs`.
- **Change:**
  - `job/rename.rs`: `run_rename(plan, ctx) -> RenameOutcome { Done, PutBack { seq, error }, Cancelled, Stuck { error, left } }`. It runs `run`; on a failure or a cancel it calls `roll_back` over the done steps.
  - `counted` counts a Rename step as one item when its target is not a temporary name.
- **Check:**
  - Fake fs: success; a fault at step k puts everything back; cancel puts everything back; a failure while putting back fills `left`; a case-only rename and a swap on the insensitive fake; `Fault::Collide` never overwrites.
  - Windows CI (`local::tests::rename_batch`): a case-only rename and a swap on NTFS.
- **Stop if:** `roll_back` needs a change for Rename plans.

### A8 — journal undo
- **Read:** `navo-store/src/journal/{mod,held}.rs`; `navo-app/src/jobs/mod.rs:74-160`.
- **Change:** `JobState::Undone`; `last_undoable(kind) -> Option<FinishedJob { id, plan, steps, created_at }>`; `mark_job_undone`; `prune` keeps the newest Done rename job.
- **Check:** the newest job wins; Undone and RolledBack jobs are left out; the result survives a reopen and a prune.
- **Stop if:** a schema change is needed.

### A9 — presets
- **Read:** `db/migrations.rs`, `db/frecency.rs` (style), `db/tests/`.
- **Change:** V5 per D18; `db/rename_presets.rs` with `save_preset`, `presets` (sorted, "Last used" separate) and `delete_preset`.
- **Check:** V4 → V5 keeps every row; saving the same name replaces it; a crash between statements leaves V4 or V5, never half.
- **Stop if:** the P35 quarantine code needs a change.

### A10, B9, C2 — reviews
- **Read:** the part's diff and D1–D19. C2 also gets the owner check list, run on a Windows PC:
  - 300 files numbered with `[N]_[C3]`;
  - two names swapped;
  - a case-only rename;
  - a batch with a file open in Word, which must be put back;
  - Ctrl+Z after a restart;
  - 1,000 renames, with the time logged.
- **Check:** one verdict; gate; Windows CI.

### B1 — commands and text
- **Read:** `commands/{mod,window,contexts}.rs`, `commands/tests.rs`, the snapshots, `keymap.toml:270-300`, `commands.ftl`, `navo-l10n/src/catalog.rs`.
- **Change:**
  - Add `file_list::MultiRename` (`files.multi_rename`, Ctrl+M) and `file_list::UndoLast` (`files.undo`, Ctrl+Z).
  - Add a `multi_rename` group with the D16 keys in the `MultiRename` context (parent `Dialog`).
  - Labels go in `commands.ftl`; every D14–D18 text goes in `rename.ftl`.
  - Update the snapshots.
- **Check:** command tests, the snapshots reviewed, and the gate's l10n step.
- **Stop if:** Ctrl+M or Ctrl+Z is already bound in a parent context of `FileList`.

### B2 — form and preview model
- **Read:** D3–D7, D13–D18; `navo-core/src/rename/*`; `command_center/source.rs` (generation pattern).
- **Change:** `multi_rename/{mod,form}.rs`.
  - `Form` holds the fields, toggles, cases, counter, excluded rows and the problems-only flag.
  - `Form::rules()`.
  - `Preview::compute(&Compiled, &[Item], &Existing, Fold, generation) -> Preview`, which is `Send`.
  - `Item::from_entry` uses chrono `Local`.
  - `next_problem` and `previous_problem`; the defaults; `from_preset`.
- **Check:** the defaults give all-Unchanged rows; an excluded row takes no number; the problems-only filter; problem navigation wraps; an ignored release test computes 50,000 items in under 300 ms.
- **Stop if:** a test needs a GPUI context.

### B3 — browser batch and undo
- **Read:** `browser/operate/{mod,types,work}.rs`, `browser/naming/mod.rs`, `browser/mod.rs` (`Message`), A7's `run_rename`.
- **Change:** `browser/rename_batch.rs`.
  - `rename_batch(plan, first_new)` runs without a confirm. When Done it clears the marks and puts the cursor on `first_new`; when put back it keeps the marks.
  - The `RenameHooks { last, undo_log, presets, save, delete }` type.
  - `undo_rename(offer)` runs `roll_back` as a job.
  - `work.rs` branches on `OpKind::Rename`.
- **Check:** on the fake fs, 5 items with a swap rename correctly; a fault puts every name back; undo restores the names; undo skips an item renamed since the batch.
- **Stop if:** `JobUpdate` must change shape; report the shape needed.

### B4 — fields view
- **Read:** D14, D16, D17; `primitives/{text_field,button,kbd}.rs`, `window_view/viewer/search/bar.rs`, `context_menu/panel.rs`, `multi_rename/form.rs`.
- **Change:** `multi_rename/view/fields.rs`, presentational only, and the token table. Add a gallery page `multi_rename_page.rs` showing the fields in every state, including errors.
- **Check:** the tests pass; gallery shots at 1100 and 1600 px, in light and dark, are looked at.
- **Stop if:** a dropdown primitive would be needed beyond the D46 panel.

### B5 — preview view
- **Read:** D15; `primitives/columns/*`, `primitives/virtual_list.rs`, the ELF table on columns (P38 A3), `multi_rename/form.rs`.
- **Change:** `multi_rename/view/{preview,summary}.rs`: the table, the row states, the summary and the footer. Rename is disabled while problems exist. Events: `RowToggled`, `Run`, `Cancel`.
- **Check:** the tests pass; gallery shots of every row state and of 50,000 rows scrolling.
- **Stop if:** `ColumnSet` cannot draw struck-through text.

### B6 — container
- **Read:** `window_view/{mod,prompt,pane}.rs`, `window_view/operate/mod.rs:72-120`, `dialog_focus.rs`, `multi_rename/*`.
- **Change:** `window_view/multi_rename.rs`.
  - Open per D13.
  - Pass the full listing as `Existing`, with the platform's `Fold`.
  - Compute the preview in the background with the D15 debounce.
  - Handle the D16 keys.
  - Run builds the plan with `plan_batch`, calls `rename_batch`, closes the tool and returns focus to the list.
  - Presets go through `RenameHooks`.
- **Check:**
  - Ctrl+M with 3 marks shows 3 rows.
  - `[N]_[C2]` updates the preview.
  - Enter renames, and the cursor lands on the first new name.
  - Enter with a conflict renames nothing and moves to the conflict.
  - Esc changes nothing.
  - Del excludes a row.
  - A preset saved with Ctrl+S loads with Ctrl+O.
- **Stop if:** `window_view/mod.rs` would grow by more than 10 lines.

### B7 — app wiring
- **Read:** `navo-app/src/jobs/mod.rs`, `navo-app/src/gui.rs` (where `FileHooks` attach), `navo-app/src/files/memory.rs`, `RenameHooks`.
- **Change:** `navo-app/src/rename_hooks.rs`. `last` decodes `last_undoable("rename")`; `undo_log` marks the steps and then the job undone; the preset closures work over `SharedDb`. Attach them in `gui.rs`.
- **Check:** with an in-memory db: a journaled rename gives an offer; undo marks it Undone and the offer is gone; the offer is still there after a reopen.
- **Stop if:** `navo-ui`'s public API needs more than `RenameHooks`.

### B8 — Ctrl+Z and activity
- **Read:** D11; `activity/{mod,text}.rs`, `window_view/jobs.rs`, `confirm_dialog`.
- **Change:**
  - `window_view/undo.rs` per D11: run at once while the Done item is shown, otherwise confirm first. With nothing to undo, the status line says "Nothing to undo".
  - Done items get the detail "Ctrl+Z undo", with the key from the active keymap.
  - Texts for a failed batch and for an undo job.
- **Check:** `activity` and `window_view::tests::undo` pass.
- **Stop if:** the Done item's lifetime is not visible from the window.

### C1 — F2 on the batch path
- **Read:** D12; `browser/naming/{mod,tests}.rs`, `window_view/prompt.rs`, `browser/rename_batch.rs`.
- **Change:** `Browser::rename` keeps its signature and its `NameProblem` errors, plans a one-row batch and calls `rename_batch`. Remove `rename_case` and `CASE_STEP`. New folder and new file stay as they are.
- **Check:** the existing naming tests pass unchanged. New tests: F2 then Ctrl+Z restores the name; a case-only F2 rename works on the insensitive fake.
- **Stop if:** an existing test's expectation must change.

Parallel lanes:
- Start at once: A1, A3, A4, A6, A8, A9, B1.
- Part A: A1 → A2; A3 and A4 → A5; A5 and A6 → A7. A10 runs after A1–A9.
- Part B: B2 waits for A2 and A5; B4 and B5 then run in parallel. B3 waits for A7. B7 waits for A8, A9 and B3. B8 waits for A3, B1 and B3. B6 waits for B1 and B3–B5.
- Shared files: A3 and B8 both touch `activity/text.rs`, so they run in sequence.

## Done when

- Ctrl+M on marked files opens the tool. Masks, find and replace, and case update the preview live, and every conflict is flagged before Rename can run (B6 tests; owner check).
- Swaps, cycles and case-only renames work on NTFS (A7 Windows CI; owner check).
- A failure or a cancel leaves the folder exactly as before. A crash in the middle is put back on the next start (A7, B3 tests).
- Ctrl+Z undoes the last batch and the last F2 rename, also after a restart (B7, B8, C1 tests; owner check).
- Presets save, load and delete (B6 tests).
- Speed is logged, with no gate (D63): 1,000 renames on a Windows PC (log the time), and a 50,000-item preview recomputing in under 300 ms (B2 ignored release test).
- `cargo xtask gate` and Windows CI pass for each part, and each part ships in an alpha build.

## Open questions for the owner

Answered 2026-10-20: the owner left Q1–Q3 to the architect. Q1 F2 with 2 or more marks opens multi-rename; Q2 all or nothing; Q3 the tool opens with the defaults, "Last used" first in the preset list.

## Log

- 2026-10-20 — Plan written from D6, D7, D39, D58–D68 and the code at d23c1e4. Found:
  - F2 rename has no journal and no undo.
  - `Step::Rename` and its rollback exist, but there is no `OpKind::Rename`, and progress does not count renames.
  - No job has a user-facing undo yet.
  - The fake file system compares names case-sensitively only.
  - Ctrl+M is free.
- 2026-10-20 — Owner: "at least like Total Commander, with the bonus features you choose". Added D20–D26 and Part D (E1–E8); minutes moved to `[i]`, case tokens `[U] [L] [F] [n]`, `[y] [d] [t]` and field tokens added to D3. A1 and A4 started.
- 2026-10-20 — A1 done: `rename::{Mask, Token, Span, CaseMode, Subject, split_name}` with the Total Commander token set; 61 tests. A4 done: `rename::check` (`Fold`, `fold_key`, `check_rows`, `RowState`, `Problem`); an excluded row's old name counts as taken; `case_only` only under Insensitive. Architect: partial counters allowed (A2), `[F]` keeps the rest of each word.
- 2026-10-20 — A6 done: `FakeFs::case_insensitive()` folds names in the tree; a case-only rename keeps one entry with the new spelling.
- 2026-10-20 — A3 done: `OpKind::Rename` with arms in core, job card, activity text, confirm text and `op_request`; `activity-renaming`, `activity-renamed` and `activity-rename-failed` added.
- 2026-10-20 — A5 done: `rename::{plan_batch, BatchPlan}` orders chains from the free end, breaks each cycle and case-only rename with one `.navo-rename-<seed hex>-<n>` name, and gives `plan: None` while a row has a problem; a proptest runs the steps on a folding folder model.
- 2026-10-20 — A9 done: `navo.db` V5 adds `rename_preset`; `Db::save_preset`, `presets` (`RenamePresets { named, last_used }`) and `delete_preset` in `db/rename_presets.rs`; rules are an opaque JSON string and time is passed in.
- 2026-10-20 — A2 done: `rename::{RenameRules, Find, Counter, NameCase, ExtCase, Compiled, RuleError, RuleField}` and `regex` =1.13.1 in navo-core. `Token::Counter` now holds `CounterParts` (partial `[C10]`, `[C+5]`, `[C:3]`, `[C10:3]` accepted). Chosen: a single Replace part applies to every `|` pair, missing parts delete; pairs run in order; First letter and Title Case lower-case the rest of each word.
- 2026-10-20 — A7 done: `run_rename` (and `RenameOutcome { Done, PutBack, Cancelled, Stuck }`) runs `run`, then `roll_back` over the steps it saw finish; `Stuck.left` lists the targets whose put-back failed; `counted` counts a Rename step unless its target is a `.navo-rename-` name; `local::tests::rename_batch` is Windows-only.
- 2026-10-20 — E3 done: `rename::Cleanup { trim, collapse, remove_accents, spaces_to_underscore }` in `RenameRules.cleanup` (serde default), applied to stem and extension after case; `unicode-normalization` =0.1.26 added for NFD.
- 2026-10-20 — A8 done: `JobState::Undone`; `JobJournal::last_undoable(kind) -> Option<FinishedJob { id, plan, steps, created_at }>` returns the newest Done job of that kind; `mark_job_undone`; `prune` also keeps the newest 20 Done `rename` jobs. No schema change.
- 2026-10-20 — A10 review (Opus high): ship with fixes. MUST: put-back results were journaled as forward steps (recovery could swap names back); a crash between a rename and its journal write left a temporary name. SHOULD: stuck batches not undoable; `counted` broke same-volume move progress (fixed on main); name re-split after an empty Find; Remove accents changed Hangul. NICE: simple case folding (D8), FAT/exFAT/SMB under Fold::Exact refuse case-only renames (safe, put back), blank preset names. Fix rounds A10a (journal, Opus medium) and A10b (rules, Sonnet medium).
- 2026-10-20 — E1 done: `rename::{FieldProvider, FieldInfo, FieldRegistry, FieldRef, ItemFields, Values, groups_needed}` in navo-core (pure; `Values::insert` swaps forbidden file-name characters for `_`); `navo_ui::rename::fields::{builtin, FileFields, ExifFields}` in navo-ui next to the image viewer's kamadak-exif. Chosen: `size_kb` rounds up; `attrs` is letters R H S A (Linux: R and dot-name H); `exif.camera` is the Make; label ids `rename-field-<group>-<field>` are defined by B1 in `rename.ftl`.
- 2026-10-20 — B2 done: `multi_rename::{Form, Item, Preview}`; `Item.excluded` is set by `Form::mark_excluded` and takes no counter number; the preview runs `plan_batch` and is `Send`; field providers (E1) plug in at `Item::subject` (`fields: None` for now); 50,000 items compute in 228 ms (release, ignored test).
- 2026-10-20 — A10b done: rules keep the mask's stem/extension split unless Find changed the text; Remove accents recomposes (NFC) so Hangul and CJK compatibility names stay unchanged; `fold_key` uses simple one-to-one folding (Greek with ypogegrammeni now folds equal).
- 2026-10-20 — A10a done: `JobLog::undone(seq, outcome)` carries every roll-back result; `JournalLog` stores it with `mark_undone` and a failed put-back leaves the step Done, not undone, and finishes the job `RestorePending`. `Journaled.unsure` names a step that may have run unjournaled (the failed step in `run_rename`, the first step not Done when `recover()` meets a rename job); it is undone only when its source is gone and its target is present. A rename job whose recovery fails stays `RestorePending` and is retried on every start; `prune` never deletes `RestorePending` jobs. Chosen for D10: `last_undoable` also returns a `RestorePending` batch, so Ctrl+Z can finish it. `Db::save_preset` refuses blank names (`DbError::BlankPresetName`); "Last used" is written by `save_last_used`.
- 2026-10-20 — E2 done: `AudioFields` registered in `builtin()` with lofty =0.25.4; reads the primary tag or first available tag, with audio properties and cover art disabled. Track is numeric with at least two digits; year comes from the recording date. Missing tags and read failures give no values. Invented MP3/ID3v2 and FLAC/Vorbis fixtures cover all fields, safe names and track padding; all 19 field-provider tests pass.
- 2026-10-20 — B1 done: `files.multi_rename` (Ctrl+M) and `files.undo` (Ctrl+Z) in `FileList`; 13 `multi_rename.*` commands in `MultiRename` plus `multi_rename.toggle_row` (Del) in a new `MultiRenamePreview` context (parent `MultiRename`, so Del cannot clash with text fields); Tab/Shift+Tab come from `Dialog`; `rename.ftl` holds every D14–D18 text and the `rename-field-*` labels; `multi_rename::texts` lists the ids not yet used so the l10n gate passes until B4–B6 use them (remove the lists then); handlers are not wired (B6/B7); F2 with 2+ marks is C1.
- 2026-10-20 — B3 done: `Browser::{rename_batch(plan, first_new), undo_rename(UndoOffer), set_rename_hooks, rename_hooks}`; `RenameHooks { last, undo_log, presets, save, delete }` over `UndoOffer { job, plan, done, created_at }` (`done` = steps still to undo) and `RenamePresetList`. `JobView` gains `undo`, `report: RenameReport { left, skipped }`. Every `RenameOutcome` is finished on the log, so a stuck put-back is journaled `RestorePending` and `last` still offers it. Undo checks each step against the folder right then (new name present, old name free), skips and counts the rest, and lists items left under a temporary name. `navo_fs::is_rename_temporary` exported. `JobUpdate::Done` now carries `Ended { progress, outcome, report }` (private to `browser`).
- 2026-10-20 — B3 review fixes: undo journals each skipped step as undone, and a failed metadata check as a failed step (the batch stays `RestorePending` and can be retried). The undo log contract (`UndoLogFactory`) marks the batch `RestorePending` when it opens and `Undone` on `finish(Done)`; `jobs::undo_log(journal, job)` does this for B7, so `recover()` finishes an undo cut short by a crash. A rename batch waits while any rename batch or undo runs, and an undo waits while a batch runs. Undo jobs cannot be cancelled or paused and keep the marks. Undo lists every temporary name still present, journaled or not. Open point for E7: undo does not check that the item at the new name is the same file (no file identity stamp per step).
- 2026-10-20 — B3 merged after a senior review and two cross-model reviews: undo and start-up recovery stop at the first failed step and stay RestorePending (also on a lost journal write); skips are journaled; a pending batch is always offered; overlapping rename jobs are refused; undo cannot be cancelled. Open for B7/B8: set `UndoOffer.restore_pending` from the journal, show `still_pending` at start as a visible notice (.ftl), keep marks through an undo; file identity per step stays open for E7.
- 2026-10-20 — B4 done: `multi_rename::view::fields` (`Fields`, `FieldInputs`, `FieldErrors`, `FieldsEvent`, `FieldMenu`, token table `TOKENS`/`matching`, case texts) is presentational; errors arrive as ready texts and the menu is a styled list, not the D46 `ContextMenu` (B6 decides how it opens and where it sits). Built-in `[=field]` tokens are not in the table yet. Gallery page `p41-b4-fields` (1280 px window only); icons Regex, CaseSensitive and Replace added to `NavoIcons`; the used ids left `multi_rename::texts`.
- 2026-10-20 — B4 review (orchestrator), for B6: the token menu is a styled list (`FieldMenu`), B6 decides how it opens; map `RuleError` to the error texts; add the `[=field]` tokens to the menu; the `[ ]` button renders as "[]".
- 2026-10-20 — B5 done: `multi_rename::view::{PreviewView, PreviewEvent, Shown}` draw the preview on `ColumnSet` and `VirtualList` with row numbers, an include toggle, strikethrough for excluded rows and a footer whose Rename button is off while problems exist. Swap is found in the view, since `RowState` has no swap. Gallery pages `p41-b5-preview` and `p41-b5-preview-large`.
- 2026-10-20 — B6 done: `window_view::multi_rename` opens from Ctrl+M, F2 or the context-menu Rename with 2+ marks, and Ctrl+K (refused with a status line while the folder loads); the preview runs on the background executor 60 ms after the last change with a generation number, and Enter right after typing waits for the fresh preview; the token, case and preset menus are `FieldMenu` lists under their field (presets under the header button) on the `ContextMenu` keys; `[=field]` tokens are listed and their values are read in the background (`Item.fields`); presets and "Last used" (saved as name `""`) go through `RenameHooks`, with no-op hooks until B7 attaches them. Gallery `p41-b6-tool`, `p41-b6-tool-menu`.
- 2026-10-21 — B6 fix round after the mid review: a pending Enter runs only if the form is unchanged when its preview lands (typing, Del or a preset load cancels it); Enter re-reads the folder first and, when names changed, refreshes the snapshot, re-runs the preview and asks for a second Enter; Ctrl+K, the app and window menus, settings and the jobs key stay shut while the tool is open; "Nothing to rename." and "folder changed" status lines (`rename-nothing`, `rename-folder-changed`); `rename-busy` shows only while a rename runs; the `[ ]` buttons are a Brackets icon button, because the text glyph showed sub-pixel colour fringes (orange or blue by position) in the screenshots; Enter with problems moves to the next problem after the cursor (D16).
- 2026-10-21 — B7 done: `navo-app::rename_hooks` builds `RenameHooks` from the journal and `SharedDb`, and `gui.rs` attaches it through a new `StartOptions.rename` field; `last` decodes `last_undoable("rename")` (`done` = Done steps not yet undone) and fills `restore_pending` from a new `FinishedJob.restore_pending`; `LastRename` now returns `Result<Option<UndoOffer>, String>` so B8 can show a read error; the preset name `""` is the last-used slot (never listed, not deletable) and a blank named preset is refused.
- 2026-10-21 — B8 done: Ctrl+Z (`files.undo`) in `window_view::undo` undoes the newest batch from `RenameHooks.last`; it runs at once while the strip still shows that batch's Done item (`Activity::shows_batch`, matched by item count; the item is retired when the undo starts) and otherwise asks through the normal confirm dialog (`Browser::ask_undo` puts an undo on `PendingOp.undo`, so every modal guard already applies); a RestorePending batch always asks to finish it; no batch says "Nothing to undo" and a `last` Err shows on the status line. Done items of a batch say "Ctrl+Z undo" (key from the active keymap); an undo job has its own title, chip and detail ("3 items had changed and were left as they are", items left under a temporary name), and a failed batch lists the files still renamed. A start-up notice says a rename undo did not finish. Marks: an undo does not touch them and the rename already cleared them, so they stay cleared; re-marking is not decided in D11 or the Log.
- 2026-10-21 — E6 done: the New name column marks the changed middle of each name (common prefix and suffix cut off), the Old name column marks Find matches through `Compiled::match_ranges` (same matcher as the rules; with name-only on, only the name part), both with the Ctrl+K highlight style (accent text, medium weight); a target path over 260 characters shows "Path is longer than 260 characters" with a warning icon in Status and does not block Rename. No warning kind existed in the row check, so the warning lives in the view line, not in `RowState`. Gallery `p41-b5-preview` sets Find to IMG.
- 2026-10-23 — E7 identity work is isolated on `backend/rename-undo-safety`, not merged. Provider identities are journaled before each rename and checked during undo and recovery. Replacement files and legacy unstamped steps are skipped; identity errors stop the operation. See [the implementation plan](2026-10-23-rename-undo-identity.md). The optional rename log and C1 single-file rename routing remain open.
- 2026-10-23 — C1 is implemented on the isolated backend branch. Single-file and folder rename use plan_batch and the journaled runner; undo survives reopening the journal. The headless F2/Ctrl+Z regression failed first, then passed. Kept conservative case-insensitive validation, pane errors, cursor and filter behavior; added a busy refusal. Two temporary-name fixtures and their waits now use the shared engine. The optional rename log and native Windows checks remain open.
- 2026-10-23 — E7 logging is implemented on the backend branch. The off-by-default `files.rename-log` setting is live; completed forward jobs append to the profile's rename.log on the worker. Chains, swaps and case-only staging collapse to original-to-final paths. Paths are quoted; concurrent appends hold a file lock. Failed and undo jobs do not log. A write failure leaves the rename done and undo available, with an activity warning. Both new behavior regressions failed first, then passed with the adapter and settings tests.
