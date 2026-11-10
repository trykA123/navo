# Speed checks — part B: the Windows VM guard on every push

Status: drafting, revision 5 · Repo: `navo`, plus the VM scripts in `../winvm/tools/` (outside git) · Started: 2026-11-10

This plan ships on its own. The shared spec, the shared rules (builders, CHK, files, lockfile chain) and part A are in `plans/2026-11-10-speed-checks.md`. No atom here changes `Cargo.lock`, and none needs part A's code.

## What the owner gets

- After each VM CI run, the VM starts Navo six times on a fixed pair of folders: one first launch and five warm ones.
- Memory above the calibrated baseline fails the push. Start-up time fails it only once calibration shows the VM is steady.
- A missing reading, a stale report, folders that did not open or a changed VM all fail the check.
- A push to `main` needs a VM log for that exact commit with `=== ok perf` and `=== ALL GREEN`. A local `cargo xtask check` no longer counts.

## Goal

A memory or start-up regression on Windows stops the push that causes it, and no path around the check remains.

## Decisions

Final. Workers never reopen them.

- SC9 — **A `perf` stage in `../winvm/tools/ci.ps1`.**
  - It runs `perf.ps1` through an interactive scheduled task, because the SSH session has no desktop.
  - **Perf build.** `--metrics` exists only with the `tester` feature, so the `perf` and `calibrate` stages build their own exe: `cargo build --release -p navo-app --features tester --target-dir target\perf`, with the same thin LTO and `debug = 0` as the `build` stage. The result is `target\perf\release\navo.exe`. The shipped release build is not changed.
  - `perf.ps1` launches that exe six times with `--metrics`, on a fixture generated fresh inside the run folder.
  - The stages required for `=== ALL GREEN` are held in one variable, `$Required`:
    - B5 keeps it at `clippy,test,build`;
    - B6 changes it to `clippy,test,build,perf`.
  - A run that leaves out a required stage prints `=== PARTIAL GREEN <stages>`, which the hook does not accept.
  - A failed stage prints `=== FAIL <stage>` and exits 1, with no green line.
- SC10 — **Fail closed in the VM.**
  - Every run writes into a new folder, `C:\winvm\perf\run-<sha8>-<yyyyMMddHHmmss>`. `perf.ps1` fails if the folder already exists.
  - The fixture is generated fresh for each run with `cargo run -q -p xtask -- perf-fixture <Run>\fixture`, an absolute path.
  - **Timeouts.** Each launch has 60 s, and `perf.ps1` kills a launch that runs over and records exit 124. The stage waits up to 420 s for `done.json` (6 × 60 s plus 60 s). On a timeout it ends the scheduled task, stops every `navo.exe` whose path is `target\perf\release\navo.exe`, and fails.
  - `perf.ps1` writes `done.json` last. It must be exactly `{"exits":[0,0,0,0,0,0],"finished":"<iso>"}` in shape: six integers, all zero. Anything else fails: a missing file after 420 s, malformed JSON, a list of another length, a non-integer or a non-zero value.
  - `perf-summary` fails when any of the six reports:
    - is missing;
    - has a `commit` that is not a prefix of the tested sha (at least 8 characters);
    - has absent or zero `idle_private_bytes`;
    - has `pane_entries` other than `[5000, 200]`, the fixture's counts from `xtask/src/perf/fixture.rs`.
  - **What the pane counts prove, and no more.** Both panes listed a folder with the fixture's shape: 5,000 entries on the left and 200 on the right. They do not prove the path itself. The path is covered another way: `perf.ps1` passes only the absolute `<Run>\fixture\A` and `<Run>\fixture\B`, after checking that both exist, and the folder was created in this run.
- SC11 — **Calibration, and when the gate is active.**
  - **Baseline file.** `perf/vm-baseline.toml` holds `vm_fingerprint`, `private_bytes`, `warm_first_usable_ms`, `memory_guard`, `time_guard`, `memory_spread`, `time_spread`, `commit`, `measured` and `reason`.
  - **Fingerprint.** It is built from:
    - the OS build;
    - logical CPUs;
    - RAM in GB;
    - the GPU adapter name;
    - Defender real-time state (the EDR switch);
    - the text of `C:\winvm\snapshot-id.txt`. The orchestrator bumps this file after any VM change.
  - **Calibration.** Ten runs of one commit.
  - **Memory guard.** Compares the largest private bytes of the six launches with the median of the ten calibration maxima × 1.15. `memory_guard = true` only when those ten maxima spread by ≤ 5 %, measured as (max − min) / min.
  - **Time guard.** Compares the median warm first-usable with the median of the ten medians × 1.25. `time_guard = true` only when the ten medians spread by ≤ 10 %.
  - **The gate is active only when `memory_guard = true`.** The perf stage prints `=== perf: not calibrated`, then `=== FAIL perf (not calibrated)`, and exits 1 when:
    - the baseline is missing;
    - the baseline has `memory_guard = false`;
    - the fingerprint differs from the baseline's.
  - `perf-summary` signals this case with exit code 4.
  - **Modes of `perf-summary`**, all dispatched in `xtask/src/perf/mod.rs`, so `main.rs` does not change:
    - **No `--mode`:** today's two-argument behaviour, unchanged.
    - **`--mode fresh --commit <sha>`:** SC10's report checks only, with no baseline. GitHub's `checks.yml` uses this mode.
    - **`--mode vm --commit <sha> --done <file> --baseline <file> --fingerprint <text>`:** all four flags are required; a missing flag is a usage error (exit 2). The mode applies SC10 and SC11, and a missing baseline file gives exit 4.
    - **`--mode calibrate --commit <sha> --done <file> --fingerprint <text>`:** SC10's checks, then one `=== calibration …` line.
    - **`--mode aggregate <file>`:** ten calibration lines in, the baseline TOML out.
  - **Size.** Exe size keeps only the absolute 160 MiB guard (D44).
  - **Accepting a change.** Re-calibrate and commit the new baseline, with a `reason`.
- SC12 — **Push hook closed. Approved by the owner on 2026-11-10, done by the orchestrator, never by a worker.**
  - After B6 and G-B2, the `gate` hook accepts a push to `main` of this repo only when `~/.cache/winvm-ci/<sha8>.log` has all five of these lines:
    - `=== sha <the full 40-character pushed sha>`, which B5 makes `ci.ps1` print after its checkout check;
    - `=== ALL GREEN`;
    - `=== ok clippy`;
    - `=== ok nextest`;
    - `=== ok perf`.
  - A log whose `=== sha` line names another commit is refused, even when every other line is green.
  - A foreground `cargo xtask check` or `cargo xtask gate` no longer counts as lint or tests for this repo.
- SC13 — **Logs that cannot pass the gate.**
  - Partial runs write `~/.cache/winvm-ci/<sha8>.partial.log`.
  - Calibration runs write `~/.cache/winvm-ci/calibrate-<sha8>-<stamp>.log`.
  - Neither ever contains `=== ALL GREEN`, and neither overwrites `<sha8>.log`.

## Risks and rollback

- R1 — **VM noise or drift blocks pushes.**
  - Prevented by calibration, the time guard only after its noise check, and the fingerprint.
  - Recovery: re-calibrate (G-B1). There is no stage skip.
- R2 — **The scripts are not in git.**
  - Before each edit, B5 and B6 copy `ci.ps1` and `ci.sh` to `*.bak-<atom>`, for example `ci.ps1.bak-B5`.
  - Restore: `cp ../winvm/tools/ci.ps1.bak-B5 ../winvm/tools/ci.ps1`, and the same for `ci.sh`.
- R3 — **The memory spread is above 5 %.** The gate cannot activate. Part B stops after G-B1, and the orchestrator reports to the owner.

## Atoms

| Atom | Component | Files (all) | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| B1 | start report | `crates/navo-ui/src/window_view/start_watch.rs`, `crates/navo-app/src/metrics/tests.rs` | patcher | medium | — | CHK `cargo xtask check -p navo-ui -p navo-app` | todo |
| B2 | metrics report | `crates/navo-app/src/metrics/report.rs`, `crates/navo-app/src/metrics/run.rs`, `crates/navo-app/src/metrics/snapshots/metrics_report.snap` | patcher | medium | B1 | CHK `cargo nextest run -p navo-app metrics`; CHK `cargo xtask check -p navo-app` | todo |
| B3 | `perf-guard` | `xtask/src/perf/fresh.rs` (new), `xtask/src/perf/mod.rs` | api-builder | high | B2 | CHK `cargo nextest run -p xtask perf`; CHK `cargo xtask check -p xtask` | todo |
| B3b | `perf-guard` caller | `.github/workflows/checks.yml` | patcher | medium | B3 | `actionlint .github/workflows/checks.yml` | todo |
| B4 | VM launcher | `../winvm/tools/perf.ps1` (new) | infra-builder | medium | B3b | a VM run by hand: six reports and a valid `done.json` | todo |
| B5 | VM stages | `../winvm/tools/ci.ps1`, `../winvm/tools/ci.sh` | infra-builder | medium | B4 | the four B5 checks below | todo |
| B6 | VM gate on | `../winvm/tools/ci.ps1`, `../winvm/tools/ci.sh` | patcher | medium | G-B1 | a full `ci.sh` run on `main` ends `=== ok perf` and `=== ALL GREEN` | todo |

### Rollout, step by step

1. **B1 → B2 → B3 → B3b merge.** `main` now writes `pane_entries`, and `perf-summary` has the new checks. The default VM run is unchanged and still ends `=== ALL GREEN` for `clippy,test,build`.
2. **B4, then B5.** These are VM scripts, with no repo commit. `$Required` stays `clippy,test,build`, so pushes go on exactly as today.
3. **G-B1, calibration.**
   1. Write `C:\winvm\snapshot-id.txt` = `2026-11-10a`.
   2. With the host otherwise idle, run `ci.sh --calibrate <repo> <sha>` ten times on the current `main` commit.
   3. Each run exits 0 and prints one `=== calibration …` line. Its log goes to `calibrate-<sha8>-<stamp>.log`, never `<sha8>.log`.
   4. Feed the ten lines to `cargo xtask perf-summary --mode aggregate <file>`. It prints the whole `perf/vm-baseline.toml`, with the guards decided by SC11.
   5. If `memory_guard = false`, stop (R3).
4. **Baseline commit.**
   1. The orchestrator commits `perf/vm-baseline.toml` with the spreads in `reason`.
   2. The push is proven by the normal VM run. `$Required` is still `clippy,test,build`, so it ends `=== ALL GREEN`.
   3. Then run `ci.sh <repo> <sha> clippy,test,build,perf` once by hand. It must show `=== ok perf`. Its log is a partial log under B5's rules.
5. **B6.** `$Required` and the default stages become `clippy,test,build,perf`. A full run on that same `main` commit ends `=== ok perf` and `=== ALL GREEN`.
6. **G-B2, regression proof.**
   1. On a throwaway branch that keeps 40 MB alive after start-up, the full run ends `=== FAIL perf` with no green line.
   2. With the baseline file removed on another throwaway branch, the run ends `=== perf: not calibrated`.
   3. Delete both branches.
7. **G-B3, hook change (orchestrator, owner-approved SC12).**
   1. Change the `gate` hook as SC12 says.
   2. Prove it with three pushes: one whose only log is partial is refused; one whose `<sha8>.log` is green but whose `=== sha` line names another commit is refused; one after a full green run passes.
   3. Log both.
8. **Whenever the fingerprint changes:** pushes fail with "not calibrated". Repeat steps 3 and 4. The baseline commit carries the new fingerprint, so its own full VM run passes `perf` and ends `=== ALL GREEN`; no hook exception is needed.

### B1 — pane counts in the start report

- **Read:** `crates/navo-ui/src/window_view/start_watch.rs` (all, 200 lines), `crates/navo-core/src/state/listing.rs:1-45`, and `crates/navo-app/src/metrics/tests.rs:165-190`.
- **Change:**
  - **`StartReport`** gains `pub pane_entries: Vec<u64>`.
  - **`into_report`** initialises it as `pane_entries: Vec::new()` in its `StartReport` literal.
  - **`report_start`**, after `into_report` returns `(report, on_report)`, makes `report` mutable and sets it before calling `on_report`:
    `report.pane_entries = self.panes().into_iter().map(|p| state.pane(p).map_or(0, |pane| pane.active().listing().map_or(0, |listing| listing.len()))).collect();`
    Here `state` is `self.browser.state()`. A missing pane or an unlisted tab gives 0, which `perf-summary` then rejects.
  - **`metrics/tests.rs`.** The `ui_report` literal adds `pane_entries: vec![5000, 200]`.
- **Check:** as in the table.
- **Stop if:** `listing.len()` counts a parent (`..`) row.

### B2 — `pane_entries` in the metrics JSON

- **Read:** `crates/navo-app/src/metrics/report.rs` (all, 88 lines), `crates/navo-app/src/metrics/run.rs`, and the snapshot file.
- **Change:**
  - **`Report`** gains `pane_entries: Vec<u64>`, serialised after `marks`.
  - **`Report::new`** leaves it empty. A new `with_panes(self, Vec<u64>) -> Self` sets it, so `tests.rs` needs no change.
  - **`run.rs`** calls `.with_panes(ui.pane_entries)`.
  - Update the snapshot with `cargo insta accept`, run under CHK.
- **Check:** as in the table. Also run CHK `cargo xtask check -p navo-bench`, because navo-bench reads these reports.
- **Stop if:** navo-bench rejects the new field.

### B3 — fail-closed `perf-summary` modes

- **Read:** `xtask/src/perf/mod.rs` (59 lines), `xtask/src/perf/report.rs` (134 lines), `xtask/src/perf/fixture.rs`, and SC10–SC11.
- **Change:**
  - **`fresh.rs`** holds pure functions, each with tests in the same file:
    - `check_done(json) -> Result<(), String>`: exactly six zero integers, per SC10.
    - `check_report(json, sha) -> Result<Fresh { private_bytes, warm_first_usable_ms }, String>`. It uses its own minimal serde struct for `commit`, `idle_private_bytes`, `pane_entries` and `first_usable_ms`, and applies SC10's rules. `EXPECTED_PANES = [5000, 200]`, with a comment pointing to `fixture.rs`.
    - `Baseline` parse with `deny_unknown_fields`.
    - `judge(first, warm, baseline: Option<Baseline>, fingerprint) -> Verdict { Pass(table), Fail(text), NotCalibrated(text) }`, per SC11.
    - `calibration_line(...)` and `aggregate(lines) -> Result<String, String>`. `aggregate` needs exactly 10 lines with one fingerprint and one commit, and prints the baseline TOML.
  - **`mod.rs`.** `summary(args)` parses the SC11 modes. Without `--mode` it runs today's code path unchanged.
- **Tests (in `fresh.rs`):**
  - `done.json` with 0, 5, 6 or 7 entries, a non-zero entry, a string entry, and malformed JSON;
  - a report with a wrong commit, absent memory, zero memory or wrong pane counts;
  - `fresh` mode passes with no baseline;
  - `vm` mode with a flag missing is a usage error, and with a missing baseline file, `memory_guard = false` or a fingerprint mismatch it gives `NotCalibrated`;
  - memory at × 1.15 passes and above it fails;
  - time above × 1.25 fails only with `time_guard`;
  - `aggregate` with 9 lines, mixed fingerprints, or a 6 % memory spread (gives `memory_guard = false`).

  The existing tests in `tests.rs` keep passing unchanged.
- **Check:** as in the table.
- **Stop if:** release reports on Windows carry `commit` = `unknown`, or no `commit` at all.

### B3b — `checks.yml` uses `fresh` mode

- **Read:** `.github/workflows/checks.yml:190-240`.
- **Change:** the "Performance summary" step runs `cargo xtask perf-summary "$env:PERF_ROOT/reports" "$env:GITHUB_STEP_SUMMARY" --mode fresh --commit ${{ github.sha }}`.
- **Check:** `actionlint`. The orchestrator dispatches `checks.yml` once; its perf job must stay green.
- **Stop if:** the perf job's reports lack `pane_entries`, which means B2 is not on the branch.

### B4 — `perf.ps1` launcher

- **Read:** `../winvm/tools/ci.ps1` (49 lines), `../winvm/tester/run_interactive_task.ps1`, and `.github/workflows/checks.yml:217-232`.
- **Change:** a new `perf.ps1 -Run <dir> -Exe <path> -Repo <dir>`.
  - It fails unless all three paths are rooted (`[IO.Path]::IsPathRooted`), `<Exe>` and `<Repo>` exist, and `<Run>` does not exist.
  - It creates `<Run>`, then runs `cargo run -q -p xtask -- perf-fixture <Run>\fixture` in `<Repo>`, and checks that `<Run>\fixture\A` and `<Run>\fixture\B` exist.
  - It runs one first and five warm launches: `& $Exe --metrics <Run>\{first,warm-N}.json --left "<Run>\fixture\A" --right "<Run>\fixture\B" --profile <Run>\profiles\<name>`.
  - Each launch has a 60 s timeout. Over that, it kills the process and records exit 124.
  - Last, it writes `<Run>\done.json` as `{"exits":[…six codes…],"finished":"<iso>"}`.
- **Check:** first, in the VM, build `cargo build --release -p navo-app --features tester --target-dir target\perf` and confirm that `target\perf\release\navo.exe --help` lists `--metrics`. Then run `perf.ps1` by hand through an interactive task. Six reports and `done.json` appear, all exits are 0, and `pane_entries` is `[5000, 200]` in each report.
- **Stop if:** `navo.exe --metrics` does not exit by itself in the interactive session.

### B5 — `perf` and `calibrate` stages; partial runs

- **Read:** `../winvm/tools/ci.ps1`, `../winvm/tools/ci.sh` (28 lines), `../winvm/tools/perf.ps1`, `../winvm/tester/run_interactive_task.ps1`, and `xtask/src/perf/mod.rs` (mode shapes).
- **Change:**
  - **Backup.** Copy both scripts to `*.bak-B5` first.
  - **`ci.sh`:**
    - `ci.sh --calibrate <repo> <rev>` sets the stages to `build,calibrate` and the log to `calibrate-<sha8>-<stamp>.log`. It exits 0 only when the log has `^=== CALIBRATION OK`.
    - `ci.sh <repo> <rev> <stages>` with explicit stages writes `<sha8>.partial.log` when the stages differ from the full required set. It exits 0 when that log has `^=== PARTIAL GREEN`.
    - Without explicit stages, it writes `<sha8>.log` and exits 0 only on `^=== ALL GREEN`.
    - It also copies `perf.ps1` to `C:/winvm/perf.ps1`.
  - **`ci.ps1`:**
    - **New variables.** `$Required = 'clippy,test,build'`, and a `-Mode` parameter (`normal` | `calibrate`).
    - **Checkout line.** After the existing HEAD check, print `=== sha <git rev-parse HEAD>` (all 40 characters).
    - **Shared steps for `perf` and `calibrate`:**
      1. build the SC9 perf exe as a `Step perf-build`;
      2. compute the SC11 fingerprint;
      3. start `perf.ps1 -Run <new SC10 folder> -Exe <abs target\perf\release\navo.exe> -Repo C:\src\navo` through the interactive-task pattern from `run_interactive_task.ps1`;
      4. wait up to 420 s for `done.json`. On a timeout, end the task, stop the perf `navo.exe` processes, and fail.
    - **`perf`** runs `perf-summary <run> <run>\summary.md --mode vm --commit $Sha --done <run>\done.json --baseline perf\vm-baseline.toml --fingerprint …`. Exit 4 prints `=== perf: not calibrated`, then `=== FAIL perf (not calibrated)`, and exits 1.
    - **`calibrate`** runs `perf-summary <run> <run>\summary.md --mode calibrate …`, prints its line, and ends with `=== CALIBRATION OK`, never a green line.
    - **The final line** is `=== ALL GREEN` only in normal mode when the stage set that ran is exactly `$Required` (same order-free set rule as `ci.sh`) and every stage passed. Any other set, a superset included, ends `=== PARTIAL GREEN <stages>`.
- **Check:**
  1. `ci.sh <repo> <sha>` still ends `=== ALL GREEN`, and its log has `=== sha <the full sha>`.
  2. `ci.sh --calibrate <repo> <sha>` exits 0 with `=== CALIBRATION OK`, and its log has no `ALL GREEN`.
  3. `ci.sh <repo> <sha> build,perf` with no baseline committed ends `=== perf: not calibrated` and exits non-zero.
  4. `ls ~/.cache/winvm-ci/` shows that `<sha8>.log` was not overwritten by checks 2 and 3.
  5. `ci.sh <repo> <sha> clippy,test,build,calibrate` (a superset of `$Required`) ends `=== PARTIAL GREEN clippy,test,build,calibrate`, exits 0, writes only `<sha8>.partial.log`, and its log has no `ALL GREEN`.
- **Stop if:** the VM has no interactive session for the task.

### B6 — perf required

- **Read:** `../winvm/tools/ci.ps1` and `../winvm/tools/ci.sh` (B5 state).
- **Change:** copy both scripts to `*.bak-B6`. Set `$Required` and `ci.sh`'s default stages to `clippy,test,build,perf`.
- **Check:** as in the table, then rollout steps 6 and 7.
- **Stop if:** the run fails on `main`. Re-calibrate first.

## Done when

- A full VM run on `main` ends `=== ok perf` and `=== ALL GREEN`.
- A 40 MB regression ends `=== FAIL perf`, and a missing baseline ends `=== perf: not calibrated`.
- The hook refuses a push with a partial log and accepts one after a full run (G-B3).
- The orchestrator updates `docs/perf.md` inline: the VM guard and calibration.

## Log

- 2026-11-10 — Split out of the main plan in revision 3. It covers plan check 2 findings 1–4 and 9; see the main plan's Log.
- 2026-11-10 — Revision 4 after plan check 3 (part B, 7 findings):
  1. No new xtask command. Every mode sits under `perf-summary` in `perf/mod.rs`, and the `checks.yml` change is its own atom, B3b.
  2. A separate `tester` release build for perf and calibration, checked for `--metrics` before B4 (SC9).
  3. B1 gives the optional-pane handling, the empty start value and the later assignment.
  4. Explicit `fresh` and `vm` modes; `vm` requires the baseline and the fingerprint (SC11).
  5. The hook checks the full sha that the log records (SC12, G-B3).
  6. The absolute fixture path, generated per run; a 420 s parent timeout with cleanup (SC10, B4, B5).
  7. The pane-count proof is narrowed to the folders' shape; the paths are covered by the per-run absolute fixture (SC10).
- 2026-11-10 — Revision 5 after plan check 4 (part B, 2 findings):
  1. `ci.ps1` and `ci.sh` share one rule: `ALL GREEN` only when the stage set equals `$Required` exactly; a superset is partial. B5 check 5 proves it.
  2. B1 uses `|listing| listing.len()`, so `start_watch.rs` needs no `Listing` import.
