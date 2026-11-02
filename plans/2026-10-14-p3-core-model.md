# P3 · Core model

Status: done (G3 passed 2026-10-15) · Branch: `p3-core` · After: G2 for merging to `main`

## Goal

`navo-core` holds Navo's whole model as pure, tested Rust: typed commands that round-trip through text, the layout tree, windows, panes and tabs with history, Total Commander selection, and `apply()` that changes state only through commands. Recorded command streams replay to the same state.

## Decisions

- D6 (one command system), D15 (selection model), D21 (serialisable commands, record and replay, multi-window state). No I/O in this crate.
- `apply(state, command) -> Result<Effects, CommandError>`; effects are data (`ListFolder { pane, location }`, `SaveSession`), run later by the runtime.
- Command text form: `name { field: value }`, parsed and printed by one module; ids are kebab-case with a dot (`pane.open-tab`).
- Panes hold a `Location` placeholder type from this plan; P6 replaces its inside, not its name.
- Coverage floor `navo-core = 90` is added to `xtask/coverage-floors.toml` in C0.
- C0 adds the shared types before the Red atoms: `PaneId`, `TabId`, `WindowId` (`id/`), `Location` (`location/`, a non-empty string without NUL) and `Direction` (`direction/`).
- `layout.split` takes a `Direction` (the side where the new pane goes) instead of an axis, so the command text is unambiguous (`layout.split { direction: right }`).
- Briefs: [C1](2026-10-15/p3-c1-brief.md), [C3](2026-10-15/p3-c3-brief.md), [C5](2026-10-15/p3-c5-brief.md), [C7](2026-10-15/p3-c7-brief.md), [C9](2026-10-15/p3-c9-brief.md), [C11](2026-10-15/p3-c11-brief.md). C7 Red may start once C5 Red is committed (it needs the `Listing` type), in parallel with C6.

## Components

| Component | Layer | Contract | Test |
| --- | --- | --- | --- |
| `command` | lib | `Command` enum ↔ text; `CommandId` | unit + `insta` snapshots + proptest round-trip |
| `layout` | lib | split tree: split, close, focus by direction, swap, resize | proptest: tree stays valid, one focused leaf, never empty |
| `state` | lib | `AppState { windows, panes, tabs, history }`, ids as newtypes | unit |
| `selection` | lib | TC model: toggle, insert-and-move, range, by pattern, invert, clear | unit + proptest |
| `apply` | lib | `apply(&mut AppState, Command) -> Result<Effects, CommandError>` | unit per command + proptest: invalid commands never change state |
| `replay` | lib | `CommandLog` (versioned) → replay from an initial state | snapshot + proptest: replay(record(s)) == s |

## Atoms

| Atom | Component | Tier | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| C0 | shared types | T4 | orchestrator | high | — | `cargo xtask gate` | done |
| C1 | `command` Red | T2 | checker (Opus) | medium | C0 | tests compile, fail | done |
| C2 | `command` Green | T4 | orchestrator | high | C1 | `cargo nextest run -p navo-core command` | done |
| C3 | `layout` Red | T2 | checker (Opus) | medium | C0 | tests compile, fail | done |
| C4 | `layout` Green | T4 | orchestrator | high | C3 | `… layout` | done |
| C5 | `state` Red | T2 | checker (Opus) | medium | C2, C4 | tests compile, fail | done |
| C6 | `state` Green | T4 | orchestrator | high | C5 | `… state` | done |
| C7 | `selection` Red | T2 | checker (Opus) | medium | C6 | tests compile, fail | done |
| C8 | `selection` Green | T4 | orchestrator | high | C7 | `… selection` | done |
| C9 | `apply` Red | T2 | checker (Opus) | medium | C6, C8 | tests compile, fail | done |
| C10 | `apply` Green | T4 | orchestrator | high | C9 | `… apply` | done |
| C11 | `replay` Red | T2 | checker (Opus) | medium | C10 | tests compile, fail | done |
| C12 | `replay` Green | T4 | orchestrator | high | C11 | `… replay` | done |
| G3 | gate | — | orchestrator + reviewer | high | C1–C12 | `cargo xtask gate --coverage`; CI green; review | done |

C1–C2 and C3–C4 run in parallel. Red atoms run on `checker` with Opus `medium` (D34). P3 merges to `main` only after G2 passes.

### Briefs (outline)

- **C1/C2:** `Command` covers the PoC set: `pane.open-tab`, `pane.close-tab`, `pane.navigate`, `pane.back`, `pane.forward`, `pane.cursor-move`, `selection.*`, `layout.split`, `layout.close`, `layout.focus`, `app.save-session`. Parsing errors carry the position. Unknown fields are errors.
- **C3/C4:** binary split tree with ratios; closing a leaf merges its sibling; focus moves by direction using the geometry of the ratios; the last leaf cannot close.
- **C5/C6:** ids are newtypes over `u64` from one counter; history is bounded (100 per tab); tabs keep cursor and scroll per location.
- **C7/C8:** selection works on row indices of the current listing: Space toggles the cursor row, Insert toggles and moves down, range from the anchor, pattern with `*` and `?` (case-insensitive), invert, clear. Space on a folder also emits a "measure folder" effect.
- **C9/C10:** one handler per command, each ≤ 30 lines; validation before any change, so a failed command leaves the state untouched (proptest).
- **C11/C12:** the log stores the initial state version and commands with relative times; a log from a newer version is refused with a clear error.

## Done when

- Every PoC command is typed, round-trips through text and changes state only through `apply()`.
- Replaying a recorded log gives the same state, checked by proptest.
- `navo-core` coverage ≥ 90 %.

## Log

- 2026-10-15 — The owner asked to keep working past the current plan while G2 waits for the Windows numbers. `navo-core` does not depend on the UI framework, so P3 starts on `p3-core` now and merges only after G2. C0 done (orchestrator): ids, `Location`, `Direction`, `thiserror` 2.0.21, `proptest` 1.12.0 and `insta` 1.49.0, floor `navo-core = 90`; gate green. C1 and C3 start in parallel (checker, Sonnet, high), each in its own worktree.
- 2026-10-15 — C1 and C3 done (checker, Opus medium): 96 command tests and 47 layout tests, all failing on `todo!()`. Orchestrator review: C3's resize rule changed so that resizing towards an outer edge moves the far border (tmux-like) instead of failing; C1's `WrongType` expectations fixed as "string", "integer", "keyword", "keyword or integer". C2 and C4 done (orchestrator, Opus high): hand-written lexer, parser and printer; split tree with permille ratios and geometric neighbours. 151 `navo-core` tests pass; `cargo xtask gate --coverage` green (command 96 %, layout 98 %). C5 starts.
- 2026-10-15 — C5 and C7 done (checker, Opus medium): 107 state tests, 98 selection tests. C6 done (orchestrator): windows and the app hold their focused pane and window inline, so readers need no `unwrap` or `expect` (D31); the mutators `expect(dead_code)` until C10. C8 done: bitset selection (1M rows in 125 KB), linear wildcard matcher, `Tab` owns its `Selection`. 359 tests pass. C9 starts; C11 brief written.
- 2026-10-15 — C9 and C11 done (checker, Opus medium): 99 apply tests, 20 replay tests. C10 done (orchestrator): one handler per command, page rows on the pane, events applied only to a tab still at that location; window, tab-activate and scroll mutators keep a targeted `expect(dead_code)` until their commands come in P7. C12 done: replay log with a 10,000-record window and a line-numbered text form that reuses the command lexer; 16 extra error-path tests. 494 `navo-core` tests pass; coverage 97 %. At the owner's request all branches were merged into `main` (P3 up to C10, P4 W1–W4, P2 spike, the D27 variants). G3 review starts.
- 2026-10-15 — G3 passed. `reviewer` (Opus, high): approve with notes; gate green (595 tests, `navo-core` coverage 98.9 %). Fixed now: pattern matching folds case per character (final sigma), and `CommandLog` counts dropped records and writes a `dropped <n>` line so a trimmed log is never mistaken for a complete one. Recorded for later plans: request numbers on listings and keeping marks on refresh (P6), window, tab and scroll commands and cursor-follows-scroll (P7). The session-restore counter is validated in P4 W6.
