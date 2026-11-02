# Windows rename log continuation

Status: done (Linux gate and review; native Windows rerun pending) · Repo: `navo-alpha-validation` · Branch: `backend/alpha-validation` · Started: 2026-10-23

## Goal

Continue the alpha backend plan from `backend/rename-undo-safety` at `1d59fb75`.
Fix the four Windows rename-log failures while preserving append behavior and batch serialization.

## Decisions

- D1 — Use a new worktree based on the backend branch; leave existing worktrees intact.
- D2 — The main agent implements and checks everything. One Sol 6.1 xhigh agent reviews the final changes.
- D3 — Keep the current alpha scope. Native Explorer/OLE checks and Windows measurements require Windows.
- D4 — Open the rename log with read and append access, as required for Windows file locking. Keep the file lock through append and sync; never truncate existing history.
- D5 — Make no dependency, UI, merge, push or deployment changes.

## Components

| Component | Layer | Contract (in → out) | New / change | Test |
| --- | --- | --- | --- | --- |
| Rename log | adapter | completed batch → locked profile log append | change | existing Windows failure regressions, history and concurrent batches |

## Atoms

| Atom | Component | Role | Effort | After | Check | Status | Cost |
| --- | --- | --- | --- | --- | --- | --- | --- |
| W1 | Rename log | self | high | — | Windows CI failure evidence; rename-log tests | done (Linux) | |
| W2 | Rename log | self | high | W1 | `cargo xtask gate` | done (Linux) | |
| W3 | Rename log | Sol 6.1 reviewer | xhigh | W2 | read-only review of continuation diff | done; no actionable findings | |

### W1 — Restore Windows log append

- **Read:** `crates/navo-app/src/rename_log.rs`, `crates/navo-app/src/rename_log/tests.rs`, original continuation plan and Windows CI run 39945045244.
- **Change:** add read access to the append handle; verify existing history survives and distinct concurrent batches remain intact.
- **Check:** all rename-log tests pass. The four existing writer tests already fail on Windows before this fix.
- **Stop if:** another application component or dependency must change.

### W2 — Check the continuation

- **Read:** `xtask/src/gate/steps.rs`.
- **Change:** record actual gate results and native verification limits.
- **Check:** the full repository gate passes.
- **Stop if:** an unrelated baseline failure requires a separate scope decision.

### W3 — Review

- **Read:** the complete diff from `1d59fb75` and the surrounding log integration.
- **Change:** review only; the main agent addresses actionable findings.
- **Check:** no unresolved review findings.
- **Stop if:** a merge, push or deployment would be required.

## Done when

- Log creation, reopening, escaping and concurrent batches pass available checks.
- The full Linux gate passes and Sol 6.1 xhigh completes review.
- Native Windows rerun and manual checks remain explicitly pending unless actually executed.
- Changes stay on the isolated backend continuation branch.

## Log

- 2026-10-23 18:20 EEST — Located the recent backend commits and continuation plan. Latest Windows CI 39945045244 repeats four `Access is denied` rename-log failures. The Rust `File::lock` documentation explicitly disallows append-only handles on Windows: https://doc.rust-lang.org/std/fs/struct.File.html#method.lock.
- 2026-10-23 18:26 EEST — `CARGO_BUILD_JOBS=8 nice -n 10 cargo nextest run -p navo-app -E 'test(rename_log::tests)'` passed all eight log tests, including the stronger history and distinct-concurrent-batch assertions. Local system Rust is 1.99.1; the repository and Windows CI pin 1.99.0. No toolchain or dependency files changed. Full gate started.
- 2026-10-23 18:30 EEST — `CARGO_BUILD_JOBS=8 nice -n 10 cargo xtask gate` passed all 10 steps in 222.0 seconds: 7,060 tests passed, 14 skipped, plus documentation tests. The audit used the repository's existing eight warning allowances; no allowances or lint rules changed. Started the single read-only Sol 6.1 xhigh review. Native Windows verification remains pending.
- 2026-10-23 18:31 EEST — Sol 6.1 xhigh completed the read-only review with no actionable findings. It confirmed append preservation and the Windows locking requirement; no builds or tests ran in the review agent. Verification logs and JUnit output are retained at a local verification log, a local verification log and a local verification report. Native Windows rerun, Explorer/OLE checks and Windows measurements remain open. No post-alpha work was started.
