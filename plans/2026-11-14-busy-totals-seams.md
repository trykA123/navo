# Busy totals: close the retry and renderer seams

Status: T13/UI1 integrated locally after Linux checks and Sol review; UI2 integration pending. Base: origin/main 61508c910cd2651104e745fff813183a74bb2662, fetched 2026-11-14. Owned UI worktree: `/home/claud/.cache/weekend-worktrees/navo-ui`; source commits `2cb91e58` and `3d68b231` preserve the separate repair cuts.

## Goal and appetite

Two small repairs make T14 executable without changing its three-file scope. Finish these prerequisites before starting T14. No new product behavior or scope beyond the accepted Busy totals contract.

## Decisions

Use the existing public FolderTotalsSource trait for the cancellable retry API. Keep one policy implementation in measure_busy.rs. Preserve at least one second between Busy queries, off-thread execution, cancellation, the 500-folder cap and existing Unavailable fallback. TP11 remains unchanged. Tests do not count against the production line budget, per the owner.

## Alternatives

Opening private browser modules exposes implementation details. Duplicating the retry loop in preview duplicates timing/cancellation rules. Combining both repairs touches four files; split them instead.

## Risks and rollback

T13's renderer is presently unreachable and its retry helper is private. Calling waiting_for_totals makes its old dead-code expectation fail. Both changes must be checked before T14. T13's historical red/green reports lack surviving logs; rerun the candidate. Windows and visual proof remain T15, explicitly UNTESTED until exercised.

## Components and atoms

Source paths below are relative to `crates/navo-ui/src`. Current candidate lengths: `browser/measure.rs` 356; `browser/measure_busy.rs` 296; `window_view/preview/stats.rs` 334; `window_view/preview/totals.rs` 88. UI1 inserts beside the exported trait at measure.rs:24; UI2 branches at stats.rs:216 and removes the accessor expectation at totals.rs:45. Keep every file below 400 physical lines.

| Atom | Component | Files | After | Check | Status |
| --- | --- | --- | --- | --- | --- |
| UI1 | totals retry API | browser/measure.rs; browser/measure_busy.rs | copied T13 | measure_busy tests; cargo xtask check -p navo-ui | Linux checked; Sol pass |
| UI2 | preview renderer | window_view/preview/stats.rs; window_view/preview/totals.rs | UI1 | preview tests; cargo xtask check -p navo-ui | Linux checked; Sol pass |

UI1: add object-safe `fn wait(&self, folder: &Location, cancel: &CancelToken) -> TotalsState` with a default body to the already-exported FolderTotalsSource trait. Return Ready, Unavailable or Busy on cancellation. Delegate to sibling `ask<S: FolderTotalsSource + ?Sized>(source: &S, folder: &Location, cancel: &CancelToken, every: Duration) -> TotalsState`; do not expose browser internals. Existing row measurement uses the same method/policy. Keep test interval injection private; production uses at least one second. Budget 60 production lines.

UI2: call the existing waiting_stats renderer at the start of production stats when folder.waiting_for_totals() is true. Remove only the now-obsolete dead-code expectations from that renderer and accessor. The accessor remains false until T14. Ready/provisional behavior stays unchanged. Budget 15 production lines; tests may use the existing fixtures, without widening T14.

UI1's public method is non-generic and has no `Self: Sized` bound. A test must call it through `Arc<dyn FolderTotalsSource>`, proving preview can use the same API as row measurement.

Hydrogen builds at high effort; a fresh Sol 6.1 review checks each result. No commits, integration or deployment by workers. Before edits, count file lengths and use existing components. No new dependencies.

## Verification and done when

Every changed test has an assertion-red or meaningful restored mutation. UI1 proves Busy cannot walk or complete numerically, cancellation terminates waiting, and Ready returns real totals. UI2 proves the waiting card shows unknown and existing Ready/provisional rows retain their rendering. Run the candidate measure_busy and preview suites, required crate checks and final full gate before integration. Record commands, exits, final lines and platform/visual checks separately.

Cargo runs take the shared cargo lock, log under the owned target and read tails. UI target stays on /home's spacious filesystem. Never use a zero-test run as a pass.

## Log

- 2026-11-15 Europe/Bucharest: UI1 direct10, preview205, full UI crate4240 tests and all9steps passed; cancellation stress200/200. Fresh Sol direct10 passed. UI2 production +5/-5; forced waiting failed unknown versus9,000 in the existing real preview test, restored false; preview205 and all9steps/4240tests passed. Resumed fresh Sol review passed3 relevant cases after the original reviewer hit usage quota. Actual Busy transport remains T14; Windows/screenshots remain T15 and UNTESTED.

- 2026-11-14 Europe/Bucharest: Sol review found both missing seams in T13. Original atom was cut incorrectly; split its repair into UI1/UI2, retaining T14's three files. No implementation or platform pass claimed.
