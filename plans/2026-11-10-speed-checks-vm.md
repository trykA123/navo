# Speed checks — part B: the Windows VM guard on every push

Status: ready, revision 7 · Base: `da6e3d8c` (local `main`; every path and line below checked there) · Repo: `navo`, plus the VM scripts in `/mnt/Sabrent/homelab/winvm/tools/` (outside git; written `$TOOLS` below) · Started: 2026-11-10

This plan ships on its own. The shared spec, the shared rules (builders, CHK, files, lockfile chain) and part A are in `plans/2026-11-10-speed-checks.md`. No atom here changes `Cargo.lock`, and none needs part A's code.

**Part A is merged** (A5 release dry run and `speed` job, A6/A8/A9 benches, W14 `Navo-Setup.exe` in `release.yml`, noise run 42890376477). No part B atom touches a part A file (`bench.yml`, `release.yml`, `perf/speed.toml`), so no B atom waits on part A.

## What the owner gets

- After each VM CI run, the VM starts Navo six times on a fixed pair of folders: one first launch and five warm ones.
- Memory above the calibrated baseline fails the push. Start-up time fails it only once calibration shows the VM is steady.
- A missing reading, a stale report, folders that did not open or a changed VM all fail the check.
- A push to `main` needs a VM log for that exact commit with `=== ok perf` and `=== ALL GREEN`. A local `cargo xtask check` no longer counts.

## Goal

A memory or start-up regression on Windows stops the push that causes it, and no path around the check remains.

## Decisions

Final. Workers never reopen them.

- SC9 — **A `perf` stage in `$TOOLS/ci.ps1`.**
  - `ci.ps1` builds the perf exe, makes the run folder and fixture, then starts `$TOOLS/perf.ps1` (copied to `C:\winvm\perf.ps1`) through an interactive scheduled task, because the SSH session has no desktop.
  - **Perf build.** `--metrics` exists only with the `tester` feature: `cargo build --release -p navo-app --features tester --target-dir target\perf`, with `CARGO_PROFILE_RELEASE_LTO=thin` and `CARGO_PROFILE_RELEASE_DEBUG=0` set by that step itself. The result is `target\perf\release\navo.exe`. The shipped build is not changed.
  - `$Required` holds the stages `=== ALL GREEN` needs: `clippy,test,build` after B5, `clippy,test,build,perf` after B6.
  - **Final line.** `-Mode calibrate` ends `=== CALIBRATION OK`. `-Mode normal` ends `=== ALL GREEN` when the stage set equals `$Required` (order-free), else `=== PARTIAL GREEN <stages>`. A failed stage prints `=== FAIL <stage> …` and exits 1 with no final line.
- SC10 — **Fail closed in the VM.**
  - **Run folder.** `ci.ps1` creates `C:\winvm\perf\run-<sha8>-<yyyyMMddHHmmss>` and fails if it exists. It generates the fixture with `cargo xtask perf-fixture <Run>\fixture` in its own environment (so `xtask` is not rebuilt with other flags).
  - **Launches.** `perf.ps1` launches with `Start-Process -PassThru` and `WaitForExit(60000)`: a bare `& navo.exe` returns at once, because release `navo.exe` is a GUI-subsystem exe (`crates/navo-app/src/main.rs:1`). Over 60 s it kills the process and records exit 124.
  - **Parent timeout.** `ci.ps1` waits up to 420 s (6 × 60 s + 60 s) for `done.json` or `error.txt`. On a timeout it ends the task, stops every `navo.exe` whose `Path` is the perf exe, and fails.
  - **`done.json`** is written last, through a temp file and a rename. It is exactly `{"exits":[0,0,0,0,0,0],"finished":"<iso>"}`: two keys, six integers, all zero. Anything else fails.
  - **Reports.** `perf-summary` fails when any of `first.json`, `warm-1.json` … `warm-5.json`:
    - is missing or is not JSON;
    - has a `commit` shorter than 8 characters or not a prefix of the tested sha (`NAVO_COMMIT` is 12 characters, `crates/navo-app/build.rs:68`);
    - has absent or zero `idle_private_bytes`;
    - has `pane_entries` absent or other than `[5000, 200]` (`xtask/src/perf/fixture.rs:4-11`).
  - **What the pane counts prove, and no more.** Both panes listed a folder of the fixture's shape. The path is covered another way: `perf.ps1` passes only `<Run>\fixture\A` and `<Run>\fixture\B`, which exist and were created in this run.
- SC11 — **Calibration, and when the gate is active.**
  - **Fingerprint** (one string, built in `ci.ps1`, then every character outside `[A-Za-z0-9.;=+_-]` becomes `_`):
    `os=<CurrentBuild>.<UBR>;cpus=<n>;ram=<GB>;gpu=<names>;defender=<True|False|unknown>;snapshot=<id>`. The sources are in B5. A missing `C:\winvm\snapshot-id.txt` fails the stage; it never yields an empty value.
  - **One run's numbers.** `private_bytes` = the largest `idle_private_bytes` of the six reports. `warm_ms` = the median `first_usable_ms` of the five warm reports (the 3rd of 5 sorted).
  - **Medians of ten** are the mean of the 5th and 6th sorted values (integer division for bytes). **Spread** is a ratio, (max − min) / min, never a percent.
  - **Baseline file** `perf/vm-baseline.toml`, keys in this order, `deny_unknown_fields`:
    `vm_fingerprint` (string), `commit` (40-hex string), `measured` (string `YYYY-MM-DD`), `reason` (string), `private_bytes` (u64, median of ten), `warm_first_usable_ms` (f64, median of ten), `memory_spread` (f64), `time_spread` (f64), `memory_guard` (bool, `memory_spread <= 0.05`), `time_guard` (bool, `time_spread <= 0.10`).
  - **Verdict.** Memory fails when `private_bytes as f64 > baseline.private_bytes as f64 * 1.15`. Time fails only when `time_guard` and `warm_ms > baseline.warm_first_usable_ms * 1.25`.
  - **Not calibrated** (exit 4): the baseline file is missing, it has `memory_guard = false`, or its `vm_fingerprint` differs from `--fingerprint`. A baseline that does not parse is exit 1. The perf stage then prints `=== perf: not calibrated` and `=== FAIL perf (not calibrated)`.
  - **`perf-summary` CLI**, dispatched in `xtask/src/perf/mod.rs` and its child module `modes`, so `main.rs` does not change. Flags after the positionals, in any order; a missing, repeated or unknown flag, a missing value, an unknown mode or a `--commit` that is not 40 lowercase hex is a usage error (exit 2).

    | Mode | Arguments | stdout | appended to `<summary-file>` | Exit |
    | --- | --- | --- | --- | --- |
    | none | `<reports-dir> <summary-file>` | — | today's table | today's |
    | `fresh` | `<reports-dir> <summary-file> --mode fresh --commit <sha>` | `perf: fresh ok, private <N> bytes, warm <ms> ms` | today's table (SC10 checks first; today's guard still applies) | 0, 1, 2 |
    | `vm` | `<reports-dir> <summary-file> --mode vm --commit <sha> --done <file> --baseline <file> --fingerprint <text>` | `perf: <pass\|FAIL\|not calibrated>: <detail>` | the same line | 0, 1, 2, 4 |
    | `calibrate` | `<reports-dir> <summary-file> --mode calibrate --commit <sha> --done <file> --fingerprint <text>` | `=== calibration <json>` | the same line | 0, 1, 2 |
    | `aggregate` | `--mode aggregate <lines-file> --measured <YYYY-MM-DD> --reason <text>` | the baseline TOML | — | 0, 1, 2 |

  - **Calibration line.** `=== calibration ` then one compact JSON object with exactly the keys `commit` (the `--commit` sha), `fingerprint`, `private_bytes` (u64) and `warm_ms` (f64). `aggregate` reads its file, skips blank lines, requires every other line to be such a line, exactly 10 of them, one `commit` and one `fingerprint`.
  - **Size.** Exe size keeps only the absolute 160 MiB guard (D44).
  - **Accepting a change.** Re-calibrate and commit the new baseline, with a `reason`.
- SC12 — **Push hook closed. Approved by the owner on 2026-11-10, done by the orchestrator, never by a worker.**
  - After B6 and G-B2, the `gate` hook accepts a push to `main` of this repo only when `~/.cache/winvm-ci/<sha8>.log` has lines matching all five of `^=== sha <full 40-char pushed sha>$`, `^=== ALL GREEN$`, `^=== ok clippy( |$)`, `^=== ok nextest( |$)` and `^=== ok perf( |$)`. The `( |$)` keeps `=== ok perf` from matching another step name.
  - A log whose `=== sha` line names another commit is refused, even when every other line is green.
  - A foreground `cargo xtask check` or `cargo xtask gate` no longer counts as lint or tests for this repo.
- SC13 — **Logs that cannot pass the gate.**
  - Runs with a stage set other than `$required` write `~/.cache/winvm-ci/<sha8>.partial.log`.
  - Calibration runs write `~/.cache/winvm-ci/calibrate-<sha8>-<yyyymmddHHMMSS>.log` (UTC).
  - Neither ever contains `=== ALL GREEN`, and neither overwrites `<sha8>.log`.

## Risks and rollback

- R1 — **VM noise or drift blocks pushes.** Prevented by calibration, the time guard only after its noise check, and the fingerprint. Recovery: re-calibrate (G-B1). There is no stage skip.
- R2 — **The scripts are not in git.** Before each edit, B5 and B6 copy `ci.ps1` and `ci.sh` to `*.bak-<atom>`. Restore: `cp $TOOLS/ci.ps1.bak-B5 $TOOLS/ci.ps1`, and the same for `ci.sh`.
- R3 — **The memory spread is above 5 %.** The gate cannot activate. Part B stops after G-B1, and the orchestrator reports to the owner.
- R4 — **A Windows update changes the fingerprint** (`UBR`). Pushes fail "not calibrated" until G-B1 repeats. This is intended: an update can move start-up numbers.

## Atoms

| Atom | Component | Files (all) | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| B1 | start report | `crates/navo-ui/src/window_view/start_watch.rs` (200 lines), `crates/navo-ui/src/window_view/tests/start_watch.rs` (127), `crates/navo-app/src/metrics/tests.rs` (226) | patcher | medium | — | CHK `cargo xtask check -p navo-ui -p navo-app` | done |
| B2 | metrics report | `crates/navo-app/src/metrics/report.rs` (88), `crates/navo-app/src/metrics/run.rs`, `crates/navo-app/src/metrics/tests.rs` | patcher | medium | B1 | CHK `cargo nextest run -p navo-app metrics`; CHK `cargo xtask check -p navo-app`; CHK `cargo xtask check -p navo-bench` | done |
| B3 | `perf-summary` fresh | `xtask/src/perf/mod.rs` (59), `xtask/src/perf/modes.rs` (new), `xtask/src/perf/modes/tests.rs` (new) | api-builder | high | B2 | CHK `cargo nextest run -p xtask perf`; CHK `cargo xtask check -p xtask` | done |
| B3c | `perf-summary` vm, calibrate, aggregate | `xtask/src/perf/modes.rs`, `xtask/src/perf/modes/calibrate.rs` (new), `xtask/src/perf/modes/calibrate/tests.rs` (new) | api-builder | high | B3 | as B3 | done |
| B3b | `checks.yml` caller | `.github/workflows/checks.yml` | patcher | medium | B3 | `actionlint .github/workflows/checks.yml`; one `checks.yml` dispatch | todo |
| B4 | VM launcher | `$TOOLS/perf.ps1` (new) | infra-builder | medium | B3b, B3c | the B4 VM run below | todo |
| B5 | VM stages | `$TOOLS/ci.ps1` (49), `$TOOLS/ci.sh` (28) | infra-builder | medium | B4 | the six B5 checks below | todo |
| B6 | VM gate on | `$TOOLS/ci.ps1`, `$TOOLS/ci.sh` | patcher | medium | G-B1 | a full `ci.sh` run on `main` ends `=== ok perf` and `=== ALL GREEN` | todo |

`modes/tests.rs` and `modes/calibrate/tests.rs` are exempt from the 400-line size check (`xtask/src/size/check.rs:24-29`).

### Rollout, step by step

1. **B1 → B2 → B3 → B3c, and B3b after B3, merge.** `main` writes `pane_entries`, and `perf-summary` has all modes. The VM run is unchanged.
2. **B4, then B5.** VM scripts, no repo commit. `$Required` stays `clippy,test,build`, so pushes go on as today. B5 writes `C:\winvm\snapshot-id.txt` = `2026-11-10a` before its checks.
3. **G-B1, calibration.**
   1. Check that `C:\winvm\snapshot-id.txt` holds `2026-11-10a`.
   2. With the host otherwise idle, run `$TOOLS/ci.sh --calibrate /mnt/Sabrent/homelab/navo <sha>` ten times on one `main` commit. Each exits 0 and prints one `=== calibration` line.
   3. `grep -h '^=== calibration ' ~/.cache/winvm-ci/calibrate-<sha8>-*.log > <scratch>/calibration.txt`, then `cargo xtask perf-summary --mode aggregate <scratch>/calibration.txt --measured <YYYY-MM-DD> --reason "<spreads, VM state>" > perf/vm-baseline.toml`.
   4. If `memory_guard = false`, stop (R3).
4. **Baseline commit.** The orchestrator commits `perf/vm-baseline.toml`. Its normal VM run still ends `=== ALL GREEN`. Then `ci.sh <repo> <sha> clippy,test,build,perf` by hand must show `=== ok perf` (a partial log).
5. **B6.** `$Required` and `ci.sh`'s `required` become `clippy,test,build,perf`. A full run on that `main` commit ends `=== ok perf` and `=== ALL GREEN`.
6. **G-B2, regression proof.** On a throwaway branch that keeps 40 MB alive after start-up, the full run ends `=== FAIL perf` with no green line. With the baseline removed on another throwaway branch, the run ends `=== perf: not calibrated`. Delete both branches.
7. **G-B3, hook change (orchestrator, SC12).** Change the `gate` hook. Prove it with three pushes: a partial-only log is refused; a green `<sha8>.log` whose `=== sha` names another commit is refused; a full green run passes. Log both.
8. **When the fingerprint changes:** pushes fail "not calibrated". Repeat steps 3 and 4.

### B1 — pane counts in the start report

- **Read:** `crates/navo-ui/src/window_view/start_watch.rs` (all), `crates/navo-ui/src/window_view/tests/start_watch.rs:67-88`, `crates/navo-ui/src/window_view/tests.rs:103-120` (`two_panes`, `fake`), `crates/navo-core/src/state/listing.rs:15-30`, `crates/navo-app/src/metrics/tests.rs:173-180` (`ui_report`).
- **Change:**
  - **`StartReport`** (`start_watch.rs:8`) gains `pub pane_entries: Vec<u64>`, with a doc comment.
  - **`into_report`** (`:89`) sets `pane_entries: Vec::new()`.
  - **`report_start`** (`:173`) binds `let Some((mut report, on_report)) = …`, then before `on_report` sets:
    `let state = self.browser.state(); report.pane_entries = self.panes().into_iter().map(|p| state.pane(p).map_or(0, |pane| pane.active().listing().map_or(0, |listing| listing.len()))).collect();`
    A missing pane or an unlisted tab gives 0, which `perf-summary` rejects.
  - **`tests/start_watch.rs`.** In `the_watch_reports_once_after_the_listed_frame_and_the_idle_wait`, assert `report.pane_entries == vec![2, 2]`: both panes show `home`, which holds `docs` and `a.txt`.
  - **`metrics/tests.rs`.** The `ui_report` literal adds `pane_entries: vec![5000, 200]`.
- **Stop if:** the new assert sees 3 per pane (a parent row is counted).

### B2 — `pane_entries` in the metrics JSON

- **Read:** `crates/navo-app/src/metrics/report.rs` (all), `crates/navo-app/src/metrics/run.rs:37-66`, `crates/navo-app/src/metrics/tests.rs:173-215`.
- **Change:**
  - **`Report`** gains `pane_entries: Vec<u64>` after `marks`, with `#[serde(skip_serializing_if = "Vec::is_empty")]`, so the snapshot `metrics_report.snap` does not change. `perf-summary` treats a missing field as wrong counts.
  - **`Report::new`** leaves it empty. A new `pub(super) fn with_panes(mut self, panes: Vec<u64>) -> Self` sets it.
  - **`run.rs`** `finish`: `Report::new(…).with_panes(ui.pane_entries)`.
  - **`tests.rs`.** `a_finished_run_writes_the_report_and_exits_cleanly` adds `assert_eq!(value["pane_entries"], serde_json::json!([5000, 200]));`.
- **Stop if:** `cargo insta` reports a changed snapshot, or navo-bench rejects the field.

### B3 — `modes`: parser, SC10 checks, `fresh` mode

- **Read:** `xtask/src/perf/mod.rs` (all), `xtask/src/perf/report.rs:1-30`, `xtask/src/perf/fixture.rs:1-12`, `xtask/src/bench_walk/mod.rs:278-283` (the temp-dir test pattern), and SC10–SC11.
- **Change:**
  - **`mod.rs`.** Add `mod modes;`. `summary` starts with `if args.iter().any(|arg| arg == "--mode") { return modes::run(args); }`; the rest is unchanged.
  - **`modes.rs`** (header comment, doc comment on each item):
    - `pub(super) fn run(args: &[String]) -> ExitCode`: parse, run, map errors to exit codes (usage 2 with the mode's usage line on stderr, failure 1 with `perf: <error>` on stderr).
    - `enum Mode { Fresh { root: PathBuf, output: PathBuf, commit: String } }` and `fn parse(args: &[String]) -> Result<Mode, String>` with SC11's flag rules. Only `fresh` exists here; other names are "unknown mode".
    - `pub(super) const EXPECTED_PANES: [u64; 2] = [5000, 200];` with a comment naming `fixture.rs`.
    - `pub(super) fn check_report(json: &str, sha: &str) -> Result<Fresh, String>`; `pub(super) struct Fresh { pub(super) private_bytes: u64, pub(super) first_usable_ms: f64 }`. A private serde struct reads `commit`, `idle_private_bytes`, `pane_entries`, `first_usable_ms` (other fields ignored).
    - `pub(super) fn load_run(root: &Path, sha: &str) -> Result<Run, String>`; `pub(super) struct Run { pub(super) private_bytes: u64, pub(super) warm_ms: f64 }` per SC11. Errors name the file.
    - `fresh`: `load_run`, print the stdout line, then `super::summarize(root, output)`.
- **Tests (`modes/tests.rs`, temp dirs as in `bench_walk`):**
  - `parse`: missing value, unknown flag, repeated flag, unknown mode, short or upper-case `--commit` → usage; a valid `fresh` line parses.
  - `check_report`: wrong commit, 7-character commit, absent memory, zero memory, `[5000, 199]`, absent `pane_entries` fail; a good report passes.
  - `run` on a folder with six good reports gives `ExitCode::SUCCESS` and the summary file contains `## Navo Windows performance`; with `warm-3.json` deleted it gives `ExitCode::FAILURE`.
  - The existing `perf/tests.rs` passes unchanged.
- **Stop if:** release reports on Windows carry `commit` = `unknown`.

### B3c — `vm`, `calibrate` and `aggregate` modes

- **Read:** `xtask/src/perf/modes.rs` (B3 state), SC11.
- **Change:**
  - **`modes.rs`.** Add `pub(super) fn check_done(json: &str) -> Result<(), String>` per SC10 (moved from B3: `fresh` never calls it, so B3 would fail `dead_code`), with its tests in `modes/tests.rs` (0, 5 and 7 entries, a `1`, a `"0"`, a `0.5`, a third key, no `finished`, malformed JSON all fail; six zeros pass). Add `mod calibrate;`, the `Vm`, `Calibrate` and `Aggregate` variants with SC11's flags, and their runners. `vm`: `check_done(--done)`, `load_run`, read `--baseline` (missing file → `None`), `judge`, print and append the line, exit per SC11. `calibrate`: `check_done`, `load_run`, print and append `calibration_line`. `aggregate`: print `aggregate(...)`.
  - **`modes/calibrate.rs`:**
    - `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] #[serde(deny_unknown_fields)] pub(super) struct Baseline` with SC11's fields and types, in order.
    - `pub(super) enum Verdict { Pass(String), Fail(String), NotCalibrated(String) }`.
    - `pub(super) fn judge(run: &Run, baseline: Option<&Baseline>, fingerprint: &str) -> Verdict`.
    - `pub(super) fn calibration_line(commit: &str, fingerprint: &str, run: &Run) -> String`.
    - `pub(super) fn aggregate(lines: &str, measured: &str, reason: &str) -> Result<String, String>`; output via `toml::to_string`.
    - Private `median_u64(&mut [u64]) -> u64` and `median_f64(&mut [f64]) -> f64` per SC11.
- **Tests (`modes/calibrate/tests.rs`):**
  - memory at exactly × 1.15 passes, one byte more fails; time above × 1.25 fails only with `time_guard`;
  - `None`, `memory_guard = false` and another fingerprint each give `NotCalibrated`;
  - a baseline with an extra key does not parse;
  - `calibration_line` → `aggregate` round trip; 9 lines, mixed fingerprints and mixed commits fail; a 6 % memory spread gives `memory_guard = false`, 4 % gives `true`; 11 % time spread gives `time_guard = false`; the output parses back into `Baseline`;
  - through `super::super::run`: `vm` without `--fingerprint` → `ExitCode::from(2)`; `vm` with a missing baseline file → `ExitCode::from(4)`.
- **Stop if:** `toml::to_string` reorders or renames the fields.

### B3b — `checks.yml` uses `fresh` mode

- **Read:** `.github/workflows/checks.yml:200-248`.
- **Change:** line 247 becomes `cargo xtask perf-summary "$env:PERF_ROOT/reports" "$env:GITHUB_STEP_SUMMARY" --mode fresh --commit ${{ github.sha }}`.
- **Check:** `actionlint`. The orchestrator dispatches `checks.yml` once on the branch; `perf-windows` stays green.
- **Stop if:** the perf job's reports lack `pane_entries` (B2 not on the branch).

### B4 — `perf.ps1` launcher

- **Read:** `$TOOLS/ci.ps1` (all), `/mnt/Sabrent/homelab/winvm/tester/run_interactive_task.ps1` (4 lines), `.github/workflows/checks.yml:224-236`.
- **Change:** a new `$TOOLS/perf.ps1`, header comment first:
  - `param([Parameter(Mandatory)][string]$Run, [Parameter(Mandatory)][string]$Exe)`, `$ErrorActionPreference = 'Stop'`. The whole body sits in `try { … } catch { [IO.File]::WriteAllText("$Run\error.txt", $_.ToString()); exit 1 }`.
  - It fails unless both paths are rooted (`[IO.Path]::IsPathRooted`), `$Exe` exists, `$Run\fixture\A` and `$Run\fixture\B` exist, and `$Run\done.json` does not.
  - Six launches, names `first`, `warm-1` … `warm-5`, in order:
    ```powershell
    $p = Start-Process -FilePath $Exe -PassThru -ArgumentList @('--metrics', "$Run\$name.json", '--left', "$Run\fixture\A", '--right', "$Run\fixture\B", '--profile', "$Run\profiles\$name")
    $null = $p.Handle  # keeps ExitCode readable after exit
    if ($p.WaitForExit(60000)) { $code = $p.ExitCode } else { Stop-Process -Id $p.Id -Force; $code = 124 }
    ```
  - Last: `$json = [ordered]@{ exits = $codes; finished = (Get-Date).ToUniversalTime().ToString('o') } | ConvertTo-Json -Compress`, `[IO.File]::WriteAllText("$Run\done.json.tmp", $json)` (UTF-8, no BOM), then `Move-Item "$Run\done.json.tmp" "$Run\done.json"`.
- **Check (in the VM, `C:\src\navo` at a `main` commit with B1–B3c, via `/mnt/Sabrent/homelab/winvm/winvm.sh ssh`):**
  1. Build the SC9 perf exe (background run, poll); `& target\perf\release\navo.exe --help | Out-String` contains `--metrics`.
  2. `cargo xtask perf-fixture C:\winvm\perf\manual-b4\fixture`; copy `perf.ps1` to `C:\winvm\perf.ps1`.
  3. Start it through `schtasks.exe /create /tn navo-perf /tr "<powershell line as in run_interactive_task.ps1> -Run C:\winvm\perf\manual-b4 -Exe C:\src\navo\target\perf\release\navo.exe" /sc once /st 23:59 /it /f`, then `/run`.
  4. Within 420 s: six reports, `done.json` with six zeros, no `error.txt`, and `pane_entries` `[5000,200]` in each report.
- **Stop if:** `navo.exe --metrics` does not exit by itself in the interactive session.

### B5 — `perf` and `calibrate` stages; partial runs

- **Read:** `$TOOLS/ci.ps1`, `$TOOLS/ci.sh`, `$TOOLS/perf.ps1`, SC9–SC13.
- **Change:**
  - **Backup** both scripts to `*.bak-B5`. **Write** `C:\winvm\snapshot-id.txt` = `2026-11-10a` if absent.
  - **`ci.sh`** (usage `ci.sh [--calibrate] <repo dir> <rev> [stages]`):
    - `required=clippy,test,build` near the top. A helper `set_of() { tr ',' '\n' <<<"$1" | sort -u | paste -sd, -; }`.
    - `--calibrate` (then exactly two arguments): stages `build,calibrate`, mode `calibrate`, log `calibrate-${sha:0:8}-$(date -u +%Y%m%d%H%M%S).log`, success iff `^=== CALIBRATION OK`.
    - A third argument whose `set_of` differs from `required`'s: log `${sha:0:8}.partial.log`, success iff `^=== PARTIAL GREEN`.
    - Otherwise: stages `${3:-$required}`, log `${sha:0:8}.log`, success iff `^=== ALL GREEN`.
    - It also copies `$here/perf.ps1` to `C:/winvm/perf.ps1`, and passes `-Mode $mode` to `ci.ps1`.
  - **`ci.ps1`:**
    - `param(… [ValidateSet('normal','calibrate')][string]$Mode = 'normal')`; `$Required = 'clippy,test,build'`. A stage other than `clippy`, `test`, `build`, `perf`, `calibrate` prints `=== FAIL usage (unknown stage <x>)` and exits 1. `-Mode calibrate` without `calibrate` in the stages fails the same way.
    - After the HEAD check: `Write-Output "=== sha $(git rev-parse HEAD)"`.
    - **`function Start-PerfRun([string]$Stage)`**, used by both stages, sets `$script:PerfRun` and `$script:PerfFp` and is called without assignment (`Step` and `schtasks.exe` write to the output stream, so a captured return value would swallow the step logs); `perf` and `calibrate` below read `$run = $script:PerfRun; $fp = $script:PerfFp`:
      1. set the two release env vars; `Step build-perf { cmd /c "cargo build --release -p navo-app --features tester --target-dir target\perf 2>&1" }`;
      2. no `C:\winvm\snapshot-id.txt` → `=== FAIL $Stage (no C:\winvm\snapshot-id.txt)`, exit 1;
      3. fingerprint, then `-replace '[^A-Za-z0-9.;=+_-]', '_'`:
         ```powershell
         $cv = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
         $gpu = ((Get-CimInstance Win32_VideoController).Name | Sort-Object) -join '+'
         $ram = [math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory / 1GB)
         try { $def = [string](Get-MpComputerStatus).RealTimeProtectionEnabled } catch { $def = 'unknown' }
         $id = (Get-Content -LiteralPath C:\winvm\snapshot-id.txt -Raw).Trim()
         $fp = "os=$($cv.CurrentBuild).$($cv.UBR);cpus=$([Environment]::ProcessorCount);ram=$ram;gpu=$gpu;defender=$def;snapshot=$id"
         ```
      4. the SC10 run folder (exists → `=== FAIL $Stage (run folder exists)`); `Step fixture-perf { cmd /c "cargo xtask perf-fixture $run\fixture 2>&1" }`;
      5. `$tr` = the `run_interactive_task.ps1` PowerShell line with `-File C:\winvm\perf.ps1 -Run $run -Exe <abs perf exe>`; over 261 characters fails. `schtasks.exe /create /tn navo-perf /tr $tr /sc once /st 23:59 /it /f` and `schtasks.exe /run /tn navo-perf`, called directly (as `git` is), each followed by a `$LASTEXITCODE` check;
      6. poll every 2 s up to 420 s for `$run\done.json` or `$run\error.txt`. `error.txt` → `=== FAIL $Stage (perf.ps1: <text>)`. Timeout → `schtasks.exe /end /tn navo-perf`, stop the perf `navo.exe` processes, `=== FAIL $Stage (timeout 420 s)`. Both exit 1.
    - **`perf`** (after `build`): `Write-Output '=== perf'`, then `cmd /c "cargo xtask perf-summary $run $run\summary.md --mode vm --commit $Sha --done $run\done.json --baseline perf\vm-baseline.toml --fingerprint $fp 2>&1"`. Exit 4 → `=== perf: not calibrated`, `=== FAIL perf (not calibrated)`, exit 1. Other non-zero → `=== FAIL perf (<code>)`, exit 1. Zero → `=== ok perf`.
    - **`calibrate`** (after `perf`): the same with `--mode calibrate --commit $Sha --done $run\done.json --fingerprint $fp`; zero → `=== ok calibrate`.
    - **Final line** per SC9, replacing today's unconditional `=== ALL GREEN`:
      `if ($Mode -eq 'calibrate') { '=== CALIBRATION OK' } elseif (-not (Compare-Object ($want | Sort-Object -Unique) ($Required.Split(',') | Sort-Object -Unique))) { '=== ALL GREEN' } else { "=== PARTIAL GREEN $Stages" }`.
- **Check** (`<sha>` = a `main` commit with B1–B3c; `$TOOLS/ci.sh … > /mnt/Sabrent/homelab/winvm/B5-<n>.log 2>&1`, background, poll):
  1. `ci.sh <repo> <sha>` ends `=== ALL GREEN`; its log has `=== sha <full sha>`.
  2. `ci.sh --calibrate <repo> <sha>` exits 0, prints one `=== calibration {` line and `=== CALIBRATION OK`; its log has no `GREEN`.
  3. `ci.sh <repo> <sha> build,perf` (no baseline on `main`) ends `=== perf: not calibrated`, exits non-zero, writes `<sha8>.partial.log`.
  4. With `snapshot-id.txt` renamed away, the same ends `=== FAIL perf (no C:\winvm\snapshot-id.txt)`; rename it back.
  5. `stat -c %Y ~/.cache/winvm-ci/<sha8>.log` is the same before and after checks 2–4.
  6. `ci.sh <repo> <sha> clippy,test,build,calibrate` ends `=== PARTIAL GREEN clippy,test,build,calibrate`, exits 0, writes only `<sha8>.partial.log`, and has no `ALL GREEN`.
- **Stop if:** the VM has no interactive session for the task.

### B6 — perf required

- **Read:** `$TOOLS/ci.ps1` and `$TOOLS/ci.sh` (B5 state).
- **Change:** copy both to `*.bak-B6`. Set `$Required`, the `-Stages` default and `ci.sh`'s `required` to `clippy,test,build,perf`.
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
- 2026-11-10 — Revision 6 after plan check 5 (Gemini argon, part B, 6 findings, all accepted after checking the code at `da6e3d8c`):
  1. CLI, outputs and contracts decided: SC11's mode table (arguments, stdout, summary file, exit codes), the calibration line as `=== calibration <json>`, `Baseline` field types, ratio spreads, the even-count median, and `--measured`/`--reason` on `aggregate`.
  2. `perf.ps1` uses `Start-Process -PassThru` + `WaitForExit(60000)` (release `navo.exe` is GUI-subsystem, so `&` does not wait). The fixture moves to `ci.ps1`, which has the cargo env; `perf.ps1` runs no cargo.
  3. B5 writes `snapshot-id.txt` before its checks, and a missing file fails cleanly (new check 4). The final line depends on `-Mode`, not on stage names, which removes the clash with check 6. `perf-summary` runs through `cmd /c "… 2>&1"` like every `Step`; `build-perf` sets the release env itself. The fingerprint expression is written out.
  4. Every `../winvm` path is now `/mnt/Sabrent/homelab/winvm/...` (`$TOOLS`).
  5. B3 split into B3 (`modes.rs`: parser, SC10 checks, `fresh`) and B3c (`modes/calibrate.rs`: baseline, verdict, aggregate); tests sit in `tests.rs` files, which the size check exempts.
  6. B1 asserts `pane_entries == [2, 2]` in the navo-ui watch test; B2 asserts `[5000, 200]` in the run JSON test. `skip_serializing_if` keeps the snapshot unchanged, so B2 stays at three files.
  - Also: SC12's hook patterns end in `( |$)`, and the perf steps are named `build-perf` and `fixture-perf`, so no step line starts with `=== ok perf` by accident. Base moved to `da6e3d8c`; part A's merged atoms touch no part B file.
- 2026-11-10 — Gemini re-check of revision 6: `check_done` and its tests move from B3 to B3c (no `fresh` caller → `dead_code`); `Start-PerfRun` sets `$script:PerfRun` / `$script:PerfFp` instead of returning values. Revision 7.
- 2026-11-11 — B1, B2, B3 merged (B3 review APPROVE; two minor test gaps left: unsorted warm fixture, missing --commit and extra positional cases).
- 2026-11-11 — B3c merged (qwen3.8-flash, one fix round after a Sonnet review asked for run-level tests of the fail-closed paths; the new tests also caught `finished: null`). 45 perf tests pass. Real `done.json`, reports and the ten-run calibration stay UNTESTED until B4 and B5. `modes.rs` has 385 code lines, so the next addition goes in a new file.
