# Speed checks that fail the build — shared spec and part A (instruction counts)

Status: part A ready (orchestrator, 2026-11-10, after plan check 3 and revision 4); parts B and C drafting · Repo: `navo` · Branch: one per atom from `main` · Started: 2026-11-10

This plan has three files. Each ships on its own:

- **This file:** the shared spec, the shared rules and part A, instruction counts. It is built first and must pass its plan check first.
- **`plans/2026-11-10-speed-checks-vm.md`:** part B, the Windows VM guard on every push.
- **`plans/2026-11-10-speed-checks-tracy.md`:** part C, the Tracy trace build for testers.

The parts are linked only by sequencing. No part needs another part's Rust code.

- C1 changes `Cargo.lock`, so it starts only after A9, the last lockfile writer in part A, has merged.
- C7 edits `release.yml`, and G-C2 uses A5's dry-run input, so C7 starts only after A5 has merged.

## What the owner gets (human spec)

Roadmap: owner order item 2, "Instruction-count benchmarks (gungraun) and Tracy profiling, so a slowdown is caught before it ships". Speed goals: back from the tray in 50 ms, warm start under 0.25 s, every key on screen within one frame, under 80 MB.

- **A release cannot ship slower code (part A).**
  - When a release starts, a check counts the CPU instructions of the hot paths. It compares them with the previous published release.
  - The hot paths are sort and filter in a big folder, Ctrl+K name search, the file diff with its cards and in-line changes, and the multi-rename preview.
  - The release stops when a path needs more than 5 % more instructions, or when a path cannot be measured.
  - A deliberate slowdown passes only through a line in `perf/speed.toml`, with a reason, in the same commit. That line stops counting after the next release.
- **Every push to `main` is checked on a real Windows (part B).**
  - After each VM CI run, the VM starts Navo six times on a fixed pair of folders.
  - Memory above the calibrated baseline fails the push. Start-up time fails it too, but only once calibration shows the VM is steady.
  - A missing reading, a stale report or a changed VM fails the check instead of passing it.
  - The push hook accepts only a VM log for that exact commit with the perf stage green. A local `cargo xtask check` no longer counts. The owner approved this on 2026-11-10.
- **A trace build for slow moments on the laptop (part C).**
  - The tester zip gains `navo-tracy.exe` and the Tracy tools.
  - The owner records a slow moment and keeps the trace on the laptop. He sends only a zone table, which holds fixed zone names and times.
  - The trace header says `navo@tester`, never the real login, computer name or full name (owner decision, 2026-11-10).
  - The normal Navo download never contains the profiler.
- **No new keys or screens.**

## Goal

A slowdown in platform-neutral hot code stops the release that would ship it. A memory or start-up regression on Windows stops the push that causes it. A slow moment on the laptop can be recorded and explained without guessing, and without leaking names.

## Shape

- **Appetite.** Two days of agent runs:
  - part A: 9 atoms;
  - part B: 7 atoms;
  - part C: 8 atoms.
  
  Orchestrator steps (`G-…`) are not atoms. Work left undone stops, and needs a new decision.
- **Today (checked at `4b10a0ed` = `v0.5.0`):**
  - **Workflows.** `bench.yml`, `bench-release.yml`, `checks.yml`, `release.yml`, `spike-compare.yml` and `windows-quick.yml`, all dispatch-only.
  - **Benches.** `bench.yml` runs one gungraun bench, `crates/navo-fs/benches/listing.rs`, with a 10 % soft limit, against `HEAD~1`. It has never run against a release.
  - **Windows perf guard.** `checks.yml:190-240` with `xtask perf-summary` (300 ms, 160 MB; unreadable memory counts as 0) runs only by hand.
  - **Tester zip.** `bench-release.yml` already ships `navo-bench.exe`, the tester `navo.exe` and `navo-index.exe`.
  - **Tracy.** The `tracy` feature exists, built on `tracing-tracy`, with frame marks and five listing spans. There is no trace build.
  - **VM CI.** The scripts live outside the repo, in `../winvm/tools/` (`ci.sh`, `ci.ps1`).
  - **Push hook.** The Claude `gate` hook accepts a VM log only with `=== ALL GREEN`, `=== ok clippy` and `=== ok nextest`. It also accepts a foreground `cargo xtask check`.
  - **Releases.** Origin has the product tags `v0.3.0` to `v0.5.0`, all published as pre-releases, plus `bench-*` tester pre-releases. Local tags are stale.
  - **Host.** Valgrind is not installed here.
- **Non-goals:**
  - benches in GPUI crates;
  - 2.6 M and 5 M entry scale (stays in `navo bench`);
  - key-to-frame and tray-summon guards (laptop `--trace` only);
  - index service memory (`plans/2026-11-04-index-ram.md`);
  - a trace of the index service process;
  - installing valgrind here.
- **Alternatives that lost:**
  - **Gungraun in `cargo xtask gate` on this host.** It needs valgrind and Linux builds, which stopped on 2026-10-30.
  - **A committed instruction-count baseline.** Counts change with toolchain, valgrind and CPU dispatch, so base and head must run on one machine.
  - **Wall-clock benches on GitHub runners.** Their noise is far above 5 %.
  - **Base-vs-head wall-clock in the VM.** That needs two release builds per push.

## Shared rules for every atom in all three files

- **Builders.**
  - `api-builder` is DeepSeek (dshw) off-peak, and Terra high via crew in peak. It never works on `navo-ui` or `navo-app`.
  - `patcher` and `infra-builder` are Sonnet.
- **Files.** Each atom lists every file it touches, `Cargo.lock` included, with at most three. A worker who needs another file stops and reports.
- **Lockfile chain.**
  - In order: A6 → A8 → A9 (this file), then C1 (Tracy file).
  - Each waits for the previous one to merge. No other atom may change `Cargo.lock`; a worker whose build changes it stops.
- **CHK.**
  - Run `flock ~/.cache/rusty-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 CARGO_TARGET_DIR=<wt>/target <cmd> > <wt>/../<atom>.log 2>&1; echo $?`, then `tail -n 40` of the log.
  - Delete `<wt>/target` when the atom lands.
- **New files.** Every new file starts with a header comment, and every new function gets a doc comment. Tests sit in a `#[cfg(test)] mod tests` at the end of the file they test, unless the atom names a test file.

## Part A decisions

Final. Workers never reopen them; a worker who finds one wrong stops and reports.

- SC1 — **Instruction counts run on GitHub `ubuntu-latest`.**
  - Callgrind needs Linux. Only GPUI-free crates are built.
  - Base and head are measured on the same runner, in one job.
- SC2 — **The release waits for it (owner: yes, 2026-11-10).**
  - `release.yml` calls `bench.yml` as a reusable workflow, and `publish` needs `speed` to succeed. There is no skip input.
  - This supersedes D8 and Q1 of `plans/2026-10-29-speed-atoms.md`.
  - It is the one exception to "a Linux job never blocks a release". Linux is only the valgrind host, and the code measured runs the same on Windows.
- SC3 — **Base selection: `cargo xtask speed-base`, tested.**
  - **Inputs.** The workflow fetches tags with `git fetch --force --tags origin`. It writes `gh release list --exclude-drafts --limit 200 --json tagName,isDraft,isPrerelease,publishedAt` to a file.
  - **Filter first.** Keep releases whose tag matches `^v\d+\.\d+\.\d+$` and that are not drafts. Pre-releases count, since every Navo release is a pre-release. `bench-*` and every other tag are dropped before sorting.
  - **Then pick.** Keep tags whose commit is a strict ancestor of `HEAD_SHA`, and take the highest version. HEAD itself is never a base, so a rerun after publishing compares with the release before it.
  - **No eligible release** fails the run.
  - **Recovery.**
    - `recovery_base` and `recovery_reason` in `perf/speed.toml` name a commit to use instead. They arrive only through a reviewed commit, never through a workflow input.
    - That commit must be a strict ancestor of HEAD and a strict descendant of the selected release. Otherwise the run fails, so a stale recovery line fails the next release until it is removed.
  - **Noise mode** (dispatch only) runs HEAD twice and has no base.
- SC4 — **Fail closed: an ID manifest, compared with the base's copy.**
  - **Bench IDs.** `perf/speed.toml` lists every bench ID as `[[bench]] id, guarded`. A bench ID is `<package dir name>/<module_path>[.<id>]`, taken from the gungraun summary. The first `module_path` segment is the bench target.
  - **Base manifest.** The workflow reads the base's copy with `git show "$BASE_SHA:perf/speed.toml"`. An *established* ID is one that has `guarded = true` in the base copy.
  - **No base manifest.** A base without the file (only `v0.5.0`, which predates this plan) has no established IDs. The table says so in its first line.
  - **Guard mode fails when:**
    1. the head manifest is missing, empty or has an unknown key;
    2. an ID appears twice in the manifest, or two summaries give the same measured ID;
    3. a manifest ID has no head summary;
    4. a head summary has no manifest line;
    5. a bench target from `speed-targets` has no manifest ID, or a manifest ID names no target;
    6. a summary has no head `Ir`;
    7. a guarded ID has no base `Ir`;
    8. an established ID is missing from the head manifest or is set to `guarded = false`, unless `perf/speed-retired.toml` has `[[retired]] id, base, reason` for it and `base` resolves to this run's base commit. A missing `speed-retired.toml` means no retirements.
  - **New IDs.** A new ID enters only as a reviewed `guarded = false` line, after its noise run passed. With no base value it passes as "bootstrap". With a base value it is compared like a guarded ID and flagged "set guarded = true".
- SC5 — **Limit: `Ir` more than 5.0 % above base, per bench.**
  - Gungraun gets no limits. `cargo xtask speed-verdict` decides from `summary.json`, so a non-zero `cargo bench` exit always means a failure.
  - **Accepting a slowdown.** Add `[[accepted]] bench, base, max_percent, reason` to `perf/speed.toml`. The entry applies only when `base` resolves to this run's base commit and the diff is ≤ `max_percent`.
  - Other entries are listed as stale, as a warning. The orchestrator deletes them after each release.
- SC6 — **Bench set and noise.**
  - **Inputs.** Generated in memory with fixed values, outside the counted region. Setup asserts a non-empty result.
  - **Run time.** Each bench function takes ≤ 15 s under Callgrind. Both refs together take < 5 min of valgrind time. Benches set `RAYON_NUM_THREADS=1`.
  - **Determinism.** No measured path reads a clock.
  - **Noise rule.** A threshold counts only after a noise run passes: two runs of one commit differ by ≤ 0.5 % `Ir` per bench.

  | Target | Bench functions | Size |
  | --- | --- | --- |
  | `navo-fs/benches/listing.rs` (exists) | sort × 4 keys, filter `sensor`, `load_folder` | 100k rows, 50k entries |
  | `navo-index/benches/search.rs` | name `sensor`, fuzzy `snsr`, path fragment `huge-007/sensor`, extension `bin`, index `load` | 250k entries |
  | `navo-compare/benches/text.rs` | `diff_lines`, `card`, `intraline_unbounded` on 200 pairs | 20k-line pair, about 300 edits |
  | `navo-core/benches/rename.rs` | preview names, `check_rows` | 10k rows |

  16 bench functions. The start-up catalogue bench is follow-up F1.
- SC7 — **Benches live only in GPUI-free crates.** `speed-targets` fails on a bench under `navo-ui`, `navo-app` or `navo-ui-gallery`. Bench features come from the crate's `[[bench]] required-features`.
- SC8 — **Proof that the release blocks.**
  - `release.yml` gains a `dry-run` input.
  - In a dry run:
    - `check-tag` checks only the tag format;
    - `publish` is skipped;
    - `would-publish` runs, with the same `needs` and the same `if` except the dry-run term.
  - G-A3 proves both outcomes on branches.

## Risks and rollback

- R1 — **A false red blocks a release.**
  - Prevented by Callgrind counts, a same-runner base, and noise runs before every threshold (SC6).
  - Recovery: a recovery base through a reviewed commit (SC3).
- R2 — **A coordinated removal hides a regression.** Prevented by the base-manifest rule and reviewed retirements (SC4).
- No user data is written. Rollback: `git revert <sha>` per atom.

## Components

| Component | Paths | Contract | Test |
| --- | --- | --- | --- |
| `speed-config` | `xtask/src/speed/{mod,config}.rs`, `perf/speed.toml`, `perf/speed-retired.toml` | TOML → manifest, recovery, accepted, retired; tree → targets | in file |
| `speed-base` | `xtask/src/speed/base.rs` | releases JSON, head, config → base sha and label | in file |
| `speed-verdict` | `xtask/src/speed/{summary,verdict}.rs` | summaries, head and base config, base, mode → table, exit code | in files |
| `bench-workflow` | `.github/workflows/bench.yml` | mode → summaries, verdict | dispatch |
| `release-workflow` | `.github/workflows/release.yml` | tag, dry-run → release after `speed` | dry runs |
| `intraline-hook` | `crates/navo-compare/src/text/intraline.rs`, `intraline/tests.rs` | pair → `Intraline`, no clock (feature `test-hooks`) | `tests.rs` |
| `bench-targets` | `crates/{navo-fs,navo-index,navo-compare,navo-core}/benches/*.rs` and their `Cargo.toml` | fixed input → `Ir` | noise run |

## Part A atoms

| Atom | Component | Files (all) | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | `speed-config` | `xtask/src/main.rs`, `xtask/src/speed/mod.rs`, `xtask/src/speed/config.rs` | api-builder | medium | — | CHK `cargo nextest run -p xtask speed`; CHK `cargo xtask check -p xtask` | done (e0c74537) |
| A2 | `speed-base` | `xtask/src/speed/base.rs`, `xtask/src/speed/mod.rs`, `perf/speed.toml` | api-builder | high | A1 | as A1 | todo |
| A3 | `speed-verdict` | `xtask/src/speed/summary.rs`, `xtask/src/speed/verdict.rs`, `xtask/src/speed/mod.rs` | api-builder | high | A2 | as A1 | todo |
| A4 | `bench-workflow` | `.github/workflows/bench.yml`, `crates/navo-fs/benches/listing.rs` | infra-builder | medium | A3 | `actionlint`; CHK `cargo bench -p navo-fs --bench listing --no-run`; G-A1 | todo |
| A5 | `release-workflow` | `.github/workflows/release.yml` | infra-builder | medium | G-A1 | `actionlint`; G-A3 | todo |
| A6 | `bench-targets` | `crates/navo-index/Cargo.toml`, `crates/navo-index/benches/search.rs`, `Cargo.lock` | api-builder | medium | G-A1 | BCHK navo-index search | todo |
| A7 | `intraline-hook` | `crates/navo-compare/src/text/intraline.rs`, `crates/navo-compare/src/text/intraline/tests.rs` | api-builder | medium | — | CHK `cargo nextest run -p navo-compare --features test-hooks intraline`; CHK `cargo xtask check -p navo-compare` | todo |
| A8 | `bench-targets` | `crates/navo-compare/Cargo.toml`, `crates/navo-compare/benches/text.rs`, `Cargo.lock` | api-builder | medium | A6, A7 | BCHK navo-compare text | todo |
| A9 | `bench-targets` | `crates/navo-core/Cargo.toml`, `crates/navo-core/benches/rename.rs`, `Cargo.lock` | api-builder | medium | A8 | BCHK navo-core rename | todo |

**BCHK, for bench atoms A6, A8 and A9.**
- **Worker.** Run CHK `cargo bench -p <crate> --bench <name> --no-run`, plus `--features` from `required-features`, and CHK `cargo xtask check -p <crate>`. That is the worker's whole check.
- **Noise qualification (orchestrator, after the worker).**
  1. Push the branch.
  2. Run `gh workflow run bench.yml --ref <branch> -f mode=noise`.
  3. Each new bench must take ≤ 15 s, with ≤ 0.5 % diff.
  4. In the merge commit, add the new IDs to `perf/speed.toml` as `guarded = false`. This is SC4's reviewed line.

**Orchestrator steps:**

- **G-A1** (after A4 merges):
  1. Dispatch `bench.yml` on `main` with `mode=noise`. It must be green, with every diff ≤ 0.5 %. The verdict must parse the real summaries; a parse error is a bug for an A3 fix atom.
  2. Dispatch `mode=guard` on `main`. It must be green, with base `v0.5.0`, and the "no base manifest" line. The table shows a base `Ir`, a head `Ir` and a diff for each of the six listing IDs; if any of the six lacks a base `Ir`, the step fails (SC4 rule 7) and part A stops until it is fixed.
  3. Log the run ids and the valgrind time.
- **G-A2** (after the first release that contains `perf/speed.toml`): every ID compared there with `guarded = false` becomes `guarded = true`, in one reviewed commit.
- **G-A3** (after A5): run `release.yml` with `dry-run: true` and `tag` = the workspace version.
  1. On a throwaway branch where `sort_order` does about 10 % more work: `speed` is red, and `publish` and `would-publish` are skipped.
  2. On the same branch plus an `[[accepted]]` entry: `would-publish` runs.
  3. On `main`: `would-publish` runs.
  4. Delete the branch. Log the run ids.

### A1 — config and bench targets

- **Read:** `xtask/src/main.rs`, `xtask/src/perf/mod.rs:1-40`, `xtask/Cargo.toml`, SC3–SC5 and SC7.
- **Change:**
  - **`main.rs`.** Add `speed-targets`, `speed-base` and `speed-verdict`, routed to `speed::targets_cmd`, `speed::base_cmd` and `speed::verdict_cmd`, each returning `ExitCode`. The last two print "not built yet" and exit 2.
  - **`config.rs`.** Parse with serde and `deny_unknown_fields`, using the existing `serde` and `toml` dependencies:
    - `load_config(path) -> Result<Config, String>`, where `Config { benches: Vec<BenchEntry { id, guarded: bool }>, recovery_base: Option<String>, recovery_reason: Option<String>, accepted: Vec<Accepted { bench, base, max_percent: f64, reason }> }`;
    - `load_retired(path) -> Result<Vec<Retired { id, base, reason }>, String>`, where a missing file gives an empty list and an unreadable file is an error.
  - **`mod.rs`.**
    - `speed-targets --config <file> [--discover]` walks `crates/*/benches/*.rs`, one level deep. It reads each crate's `[[bench]]` entries (`name`, `required-features`) and prints `<package> <bench> <features or ->` lines.
    - It fails on a GPUI crate (SC7) and on a bench file without a `[[bench]]` entry.
    - Without `--discover`, it also fails when the config has no `[[bench]]`.
- **Tests:**
  - a full config parses;
  - an unknown key is rejected;
  - a missing retired file gives an empty list;
  - targets are listed from a temp tree;
  - `navo-ui` is rejected;
  - a bench file without an entry is rejected.
- **Check:** as in the table. Also run CHK `cargo run -q -p xtask -- speed-targets --discover --config /dev/null`; it prints only `navo-fs listing -`.
- **Stop if:** a bench target in the tree is not described by its `Cargo.toml`.

### A2 — base selection

- **Read:** `xtask/src/speed/mod.rs`, `xtask/src/speed/config.rs`, SC3, and `crates/navo-fs/benches/listing.rs` (bench names only).
- **Change:**
  - **`base.rs`.**
    - Add a pure function: `choose_base(releases: &[Release { tag, is_draft, commit }], head: &str, recovery: Option<&str>, is_ancestor: impl Fn(&str, &str) -> bool) -> Result<Base { sha, label }, String>`. It follows SC3 in this order: tag filter, draft filter, strict-ancestor filter, highest version, then recovery.
    - Add the wrapper `speed-base --releases <json> --head <sha> --config <file>`. It resolves each eligible tag with `git rev-list -n 1 <tag>` and ancestry with `git merge-base --is-ancestor`. It prints `BASE_SHA=<sha>` and `BASE_LABEL=<label>`.
  - **New `perf/speed.toml`:**
    - a header comment explaining SC3–SC5;
    - the six listing IDs as `guarded = true`, named from `listing.rs` (G-A1 confirms the exact strings);
    - no recovery and no accepted entries.
- **Tests:**
  - the highest strict-ancestor product tag is chosen;
  - a newer `bench-2026-11-11` pre-release on an ancestor is ignored;
  - a pre-release product tag counts;
  - a draft is ignored;
  - a release on HEAD is skipped;
  - a non-ancestor is skipped;
  - `v1.2` and `v1.2.3-rc1` are ignored;
  - no eligible release is an error;
  - a valid recovery is accepted;
  - a recovery equal to HEAD, older than the release or not an ancestor is an error.
- **Stop if:** `gh release list --json` lacks `isDraft` or `tagName`.

### A3 — summaries and verdict

- **Read:**
  - `xtask/src/speed/{mod,config,base}.rs`;
  - in the cargo registry, `gungraun-runner-0.20.0/src/summary/model.rs:1-260` and `gungraun-runner-0.20.0/src/metrics/model.rs:80-130`, for the serde field names only.
- **Change:**
  - **`summary.rs`.**
    - Define minimal own serde structs for the summary JSON. Add no dependency: read only `package_dir`, `module_path`, `id`, and the Callgrind profile's total `Ir`, new and old.
    - `measured(json) -> Result<Measured { id, new_ir: f64, old_ir: Option<f64> }, String>`. A missing new `Ir` is an error.
    - `collect(dir)` reads every `summary.json` under `dir`. Zero files is an error.
    - Test it on an inline summary written from the model's serde attributes.
  - **`verdict.rs`.**
    - `verdict(measured, head: &Config, base: Option<&Config>, retired, base_sha, targets, mode, limit, noise_limit, resolve: impl Fn(&str) -> Option<String>) -> Report { lines, failed }`.
    - Guard mode follows SC4 (all eight rules) and SC5.
    - Noise mode fails when |diff| > `noise_limit` or a value is missing. Unlisted IDs are reported, not failed.
    - The output is a markdown table (ID, base `Ir`, head `Ir`, diff %), then the findings.
  - **`mod.rs`.** Wire `speed-verdict --summaries <dir> --config <head> --base-config <file|none> --retired <file> --targets <file> --base <sha> --label <text> --mode <guard|noise> --limit 5 --noise-limit 0.5 --out <file>`. It prints the report, appends it to `--out`, and exits 1 when failed.
- **Tests:**
  - each SC4 rule, one case each, including two summaries with the same measured ID;
  - a guarded ID with no base `Ir` when the base has no manifest fails;
  - an established ID removed, then the same with a matching retirement;
  - an established ID downgraded to `guarded = false`;
  - a retirement for another base still fails;
  - a regression at limit + 0.1;
  - an accepted entry with a matching base passes; one with another base is stale and regressed; one above `max_percent` is regressed;
  - a bootstrap ID without base passes; one with base is compared;
  - noise passes and fails.
- **Stop if:** the summary model has no Callgrind `Ir` with separate new and old values.

### A4 — `bench.yml` as a reusable guard; listing without limits

- **Read:**
  - `.github/workflows/bench.yml`;
  - `.github/workflows/release.yml:1-45`;
  - `.github/workflows/checks.yml` (pinned action lines only);
  - `xtask/src/speed/mod.rs` (command shapes);
  - `crates/navo-fs/benches/listing.rs`.
- **Change:**
  - **`listing.rs`.** `callgrind()` returns `Callgrind::default()`, with no `soft_limits`. `config()` sets `RAYON_NUM_THREADS=1` through gungraun 0.20's per-config environment method. Bench IDs stay unchanged. Add a header comment.
  - **`bench.yml`, job level.**
    - Triggers: `workflow_dispatch` and `workflow_call`, input `mode` (`guard` | `noise`, default `guard`).
    - Remove the top-level `concurrency`.
    - `permissions: contents: read`.
    - Job `instructions`, `timeout-minutes: 40`.
    - `env: HEAD_SHA: ${{ github.sha }}`, `GH_TOKEN: ${{ github.token }}`.
    - Every `run` starts with `set -euo pipefail`.
  - **`bench.yml`, steps:**
    1. Check out with `fetch-depth: 0`. Fail unless `git rev-parse HEAD` equals `$HEAD_SHA`. Run `git fetch --force --tags origin`. Then the toolchain, rust-cache, valgrind and `gungraun-runner 0.20.0`, as today. Finally `rm -rf target/gungraun`.
    2. Targets: run `cargo xtask speed-targets --config perf/speed.toml` (add `--discover` in noise mode) into `$RUNNER_TEMP/targets.txt`.
    3. Base:
       All of this runs in one shell. Later steps see `$GITHUB_ENV` values; the same step does not.
       - Guard:
         1. Run `gh release list …` into `$RUNNER_TEMP/releases.json`.
         2. `base_out=$(cargo xtask speed-base --releases "$RUNNER_TEMP/releases.json" --head "$HEAD_SHA" --config perf/speed.toml)`.
         3. `BASE_SHA=$(printf '%s\n' "$base_out" | sed -n 's/^BASE_SHA=//p')`, and `BASE_LABEL` the same way.
         4. Validate in this shell: `BASE_SHA` matches `^[0-9a-f]{40}$`, differs from `$HEAD_SHA`, and `git merge-base --is-ancestor "$BASE_SHA" "$HEAD_SHA"` succeeds. Any failure exits 1.
         5. If `git cat-file -e "$BASE_SHA:perf/speed.toml"` succeeds, write `git show "$BASE_SHA:perf/speed.toml"` to `$RUNNER_TEMP/base-speed.toml` and set `BASE_CONFIG` to that path. Otherwise set `BASE_CONFIG=none`.
       - Noise: `BASE_SHA=$HEAD_SHA`, `BASE_LABEL=noise`, `BASE_CONFIG=none`.
       - Both modes end with `printf 'BASE_SHA=%s\nBASE_LABEL=%s\nBASE_CONFIG=%s\n' "$BASE_SHA" "$BASE_LABEL" "$BASE_CONFIG" >> "$GITHUB_ENV"`.
    4. Base pass:
       - Guard: `git checkout --force "$BASE_SHA"`. For each target whose bench file exists there, run `cargo bench -p <pkg> --bench <bench> [--features …] -- --save-baseline=base`. Then `git checkout --force "$HEAD_SHA"`.
       - Noise: the same, run on HEAD.
       - Print the elapsed time.
    5. Run `find target/gungraun -name summary.json -delete`. Head pass: every target with `-- --save-summary=pretty-json`, adding `--baseline=base` when step 4 measured it. Print the elapsed time.
    6. Run `cargo xtask speed-verdict` with every A3 flag. `--out` is `"$GITHUB_STEP_SUMMARY"`.
    7. Upload `target/gungraun/**/summary.json` as `speed-summaries`, with `if: always()` and the pinned upload action from `checks.yml`.
- **Check:** as in the table, then G-A1.
- **Stop if:** gungraun 0.20 has no per-config environment method, or the bench binary rejects `--save-summary` or `--baseline`.

### A5 — speed gate and dry run in `release.yml`

- **Read:** `.github/workflows/release.yml` (all of it), and `.github/workflows/bench.yml` (inputs only).
- **Change:**
  - **Input.** `dry-run` (boolean, default false). `check-tag` skips its "release exists" test when `dry-run` is set.
  - **`speed` job.** `needs: check-tag`, `uses: ./.github/workflows/bench.yml`, `with: { mode: guard }`.
  - **`publish`.** `needs` adds `speed`. Its `if` adds `needs.speed.result == 'success' && !inputs.dry-run`.
  - **`would-publish` job.** The same `needs`, and the same `if` with `inputs.dry-run` instead of `!inputs.dry-run`. It runs `echo "would publish $TAG"`.
- **Check:** as in the table, then G-A3.
- **Stop if:** `release.yml` cannot call a reusable workflow with its current permissions.

### A6 — Ctrl+K search bench (lockfile chain 1)

- **Read:**
  - `crates/navo-fs/benches/listing.rs` (the pattern);
  - `crates/navo-index/src/search.rs:1-120` and `crates/navo-index/src/search/tests.rs:80-95`;
  - `crates/navo-index/src/table/mod.rs:50-90,190-235` and `crates/navo-index/src/table/flags.rs:1-20`;
  - `crates/navo-index/src/file.rs:1-110`;
  - `crates/navo-index/Cargo.toml`.
- **Change:**
  - **`Cargo.toml`.** Add the dev-dependency `gungraun = "=0.20.0"` and `[[bench]] name = "search" harness = false`.
  - **The table.** 250,000 entries: a root and 500 folders.
    - A folder is named `huge-NNN` when `NNN % 100 == 7`, so `huge-007` exists, and `dir-NNN` otherwise.
    - Files cycle through `sensor-NNNNNN.bin`, `object-NNNNNN.txt`, `report NNNNNN.pdf` and `résumé-NNNNNN.md`.
  - **Search benches.** They call `search(&table, terms, 200, &|| false)` with `[Term::Name("sensor")]`, `[Term::Name("snsr")]`, `[Term::Path("huge-007/sensor")]` and `[Term::Ext(&["bin".to_owned()])]`. Setup asserts that each query has hits.
  - **`load` bench.** It runs `file::load` on a file that setup saved into a `tempfile::TempDir` (an existing dev-dependency), kept alive in the input.
  - **Config.** The same as A4's listing.
- **Check:** BCHK.
- **Stop if:** `file::save` needs data the table cannot give, or a query returns no hits.

### A7 — intraline path without a clock

- **Read:** `crates/navo-compare/src/text/intraline.rs` (all, 224 lines), `crates/navo-compare/src/text/intraline/tests.rs:1-60`, and `crates/navo-compare/Cargo.toml` (`[features]`).
- **Change:**
  - **`intraline.rs`.**
    - Rename the private `intraline_with_deadline` to `refine(pair, deadline: Option<Instant>) -> Intraline`.
    - The `Instant::now()` checks and `config.deadline(…)` run only for `Some`. With `None`, the function never reads the clock.
    - `intraline(pair)` keeps its behaviour: `refine(pair, Some(Instant::now() + INTRALINE_DEADLINE))`. The clock is read at the same point as today, after the lossy and length checks.
    - Add `#[cfg(feature = "test-hooks")] pub fn intraline_unbounded(pair) -> Intraline { refine(pair, None) }`.
  - **`tests.rs`.** Build the 200 pairs of the A8 fixture, written out in the test: for each `i` with `i % 100 == 50`, the left line `i` against the same line plus ` // edited`. For each pair:
    - `intraline_unbounded(p)` equals `refine(p, Some(now + 1 h))`, so the production deadline branch runs without a flaky 5 ms budget;
    - neither result has a fallback.
    
    The existing deadline tests keep passing.
- **Check:** as in the table.
- **Stop if:** `similar` reads the clock even when no deadline is set.

### A8 — compare text bench (lockfile chain 2)

- **Read:**
  - `crates/navo-compare/src/text/side.rs:25-60,120-140`;
  - `crates/navo-compare/src/text/diff.rs:80-100`;
  - `crates/navo-compare/src/text/card.rs:10-60`;
  - `crates/navo-compare/src/text/diff/tests.rs:1-40`;
  - `crates/navo-compare/src/text/intraline.rs:75-90`;
  - `crates/navo-compare/Cargo.toml`.
- **Change:**
  - **`Cargo.toml`.** Add the gungraun dev-dependency and `[[bench]] name = "text" harness = false required-features = ["test-hooks"]`.
  - **Left side.** 20,000 `Line`s of the form `    value_<i> = compute(<i>, <i*7 % 13>);`, with LF endings, cumulative `starts` and `bytes: None`.
  - **Right side.** Three kinds of edits, chosen so that no two touch the same or neighbouring lines:
    - every line with `i % 100 == 50` gains ` // edited` (200 lines);
    - every line with `i % 250 == 125` is removed (80 lines; these never have `i % 100 == 50`);
    - a new line `    inserted_<i>();` follows every line with `i % 400 == 0` (50 lines).
  - **The 200 changed pairs** are not taken from the diff. They are built directly: for each `i % 100 == 50`, a `LinePair` of left line `i` and its edited right line, with each side's `starts`. All 200 survive, because no removal hits them.
  - **Benches:**
    - `diff_lines(…, &Options::default())`;
    - `card(&alignment, CardLimits::default())`, with the alignment built in setup;
    - `intraline_unbounded` on the 200 changed pairs. Setup asserts there are exactly 200, and that no pair has a fallback and every pair has at least one span.
  - **Config.** The same as A4's listing.
- **Check:** BCHK.
- **Stop if:** `diff_lines` does not report the 200 edited lines as changed rows. Setup checks this once.

### A9 — rename preview bench (lockfile chain 3)

- **Read:**
  - `crates/navo-core/src/rename/rules.rs:1-140`;
  - `crates/navo-core/src/rename/mask.rs:110-140`;
  - `crates/navo-core/src/rename/check.rs:1-140`;
  - `crates/navo-core/Cargo.toml`.
- **Change:**
  - **`Cargo.toml`.** Add the dev-dependency and `[[bench]] name = "rename" harness = false`.
  - **Input.** 10,000 names, alternating `IMG_NNNNN.JPG` and `scan NNNNN.pdf`.
  - **Rules.** Name mask `[N]_[C]`, find `IMG` and replace with `photo`, all other fields default. Compiled in setup.
  - **Bench 1** produces every new name.
  - **Bench 2** runs `check_rows` on those rows against the old names plus 100 colliding names. Setup asserts that exactly those 100 collide.
- **Check:** BCHK. After merge, part C's C1 may start.
- **Stop if:** compiling the rules fails for this mask.

## Done when

- `bench.yml` in guard mode on `main`:
  - lists 16 bench IDs;
  - has no missing ID;
  - spends < 5 min in valgrind.
- G-A3 shows a red `speed` with `publish` and `would-publish` skipped, and a green path once the slowdown is accepted.
- The next real release shows `speed` green, with base = the previous product release.
- The orchestrator updates `docs/perf.md` inline: the instruction-count section and the release rule.

## Owner answers

- Q1 — The release waits for a Linux runner job. **Yes (owner, 2026-11-10).** See SC2.
- Push hook (part B, SC12): **approved (owner, 2026-11-10).**
- Tracy header (part C, SC14): **generic `navo@tester` (owner, 2026-11-10).**

No open questions remain.

## Follow-ups

- F1 — A start-up catalogue bench (`navo-l10n`, `Catalog::build(&FILES)`) after part A ships. It is one atom on the lockfile chain.

## Log

- 2026-11-10 — Plan drafted by the architect (Opus 5.5) at `4b10a0ed`.
- 2026-11-10 — Revision 2 after plan check 1 (not ready, 11 findings). It added:
  - a fail-closed VM guard;
  - full-stage `ALL GREEN`;
  - an own Tracy layer;
  - the ID manifest;
  - published-release bases;
  - noise before thresholds;
  - the `index.answer` drop;
  - atom order;
  - the `huge-007` fix;
  - the release dry run;
  - three parts.
- 2026-11-10 — Revision 3 after plan check 2 (not ready, 13 findings):
  1. **Hook bypass:** the owner approved closing it. The orchestrator changes the hook (VM plan, SC12, G-B3).
  2. **Unenforced VM guard and empty `done.json`:** the gate is active only with a calibrated memory guard. `done.json` must hold exactly six zero exits (VM plan, SC10–SC11).
  3. **Relative fixture paths:** absolute paths, and both panes checked by entry counts (VM plan, B1–B4).
  4. **Calibration and rollout blocked by `ci.sh`:** a separate `--calibrate` mode, partial-run exit rules, and a written step order (VM plan, B5, G-B1, B6).
  5. **`bench-*` tags as base:** product tags only, filtered before sorting, and tested (SC3, A2).
  6. **Coordinated removal:** the base-manifest comparison plus `speed-retired.toml` (SC4, A3).
  7. **A1 noise cycle:** the worker check is compile only. Noise qualification comes after A4 (BCHK).
  8. **Undefined `HEAD_SHA`:** taken from `github.sha` and asserted (A4).
  9. **`checks.yml` broken by B1:** the old interface is kept, and `checks.yml` changes in the same atom (VM plan, B3).
  10. **Clock in intraline:** a clock-free function, the production deadline kept, and an equality test (A7).
  11. **Tracy privacy:**
      - the header carries `navo@tester` through Tracy's custom-user-info hook;
      - a tested string scan runs over the decompressed real trace;
      - the CSV limits are stated;
      - the trace stays on the laptop (Tracy plan).
  12. **Briefs and files:**
      - every file listed, `Cargo.lock` included;
      - the catalogue bench moved out of part A (F1);
      - zone sites found now, so there is no scout and no worker split (Tracy plan).
  13. **Coupling and size:** three files, sequencing-only links (C1 after A9, C7 after A5), and no open question.
  
  To do after acceptance: mark D8 and Q1 in `plans/2026-10-29-speed-atoms.md` superseded, and add decisions for SC2, SC12 and SC14 to `docs/decisions.md`.
- 2026-11-10 — Revision 4 after plan check 3. Part A fixes:
  1. `BASE_SHA` is computed, validated and used in one shell, then persisted, in both modes (A4 step 3).
  2. G-A1 fails if any of the six listing IDs lacks a base `Ir`, and A3 tests that case.
  3. The A8 fixture guarantees 200 surviving changed pairs, built directly by index.
  4. Duplicate measured IDs fail (SC4 rule 2), with a test.
  5. C7 depends on A5.
