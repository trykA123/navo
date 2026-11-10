# Research picks: crates, tools, Windows speed, competitors

Status: atoms ready for R2, R5, R12a, R14a and R15; R12b and R14b wait for their measured gates; R6, R7 and R9 rejected; R10 and R11 already in place · Repo: `navo` · Base: `d2c931bc` · Started: 2026-11-10

Six researchers (2026-11-10) looked at alternatives to our crates, crates we could adopt instead of own code, Rust tooling, Windows file I/O, and competitor features. This plan keeps only picks with evidence. Every speed claim is measured with the speed-check benches before adoption; a pick that does not measure faster is dropped. Version bumps live in `2026-11-10-deps-latest.md`, not here.

## Picks

| Id | Pick | Evidence | Gate to adopt | Size | Priority |
| --- | --- | --- | --- | --- | --- |
| R1 | Natural sort for names (`file2` before `file10`) | `navo-fs/src/view/sort.rs` compares lowercase text only (checked by hand) | sort tests incl. digits, leading zeros, Unicode digits; bench on 5,000-file folder | small | high |
| R2 | Native crash reports: `crash-handler` + `minidumper` | Only a panic hook exists; access violations are not reported | a forced access violation in the tester build writes a minidump; no private data in it | medium | high |
| R3 | `cargo auditable` release exe + `cargo audit bin` in `release.yml` | cargo-auditable 0.7.7; dependency list < 4 kB in the exe | release dry run passes; audit runs on the shipped exe | small | high |
| R4 | One dependency bot (Renovate or Dependabot), weekly grouped PR, exact `=` pins | Owner rule: always on latest | PR goes through the deny gate and VM CI | small | high |
| R5 | `cargo shear` monthly; remove unused deps | Found `windows-core` declared but unused | clean run | small | medium |
| R6 | File watcher on `notify` 8.2 + `notify-debouncer-full` (replaces ~830 own lines) — **rejected (RP4)** | mature, maintained (2026-11-08) | our watch tests pass unchanged: rename pairing, overflow → rescan, 100/500 ms debounce, network share | medium | medium |
| R7 | Index service install/start via `windows-service` (keep our ACL code) — **rejected (RP5)** | windows-service 0.8.1 | service install/update test in the VM | small | low |
| R8 | Font viewer: `ttf-parser` → `skrifa` | ttf-parser is in maintenance mode and recommends fontations | font viewer tests; 3 call sites | small | medium |
| R9 | Fuzzy: benchmark `frizbee` against `nucleo-matcher` — **rejected (RP6)** | self-reported 4× (1 thread) / 10× (8 threads) on 1.4 M paths; nucleo-matcher unreleased since 2024 | ≥ 2× on the real 2.6 M index and the ranking tests pass | medium | medium |
| R10 | `flate2` on the `zlib-rs` backend — **already in place (RP7)** | zlib-rs claims; native gain unverified | faster on an archive corpus | small | low |
| R11 | Folder listing: `FindFirstFileExW` + `FindExInfoBasic` + `FIND_FIRST_EX_LARGE_FETCH` — **already in place** (`navo-fs/src/local/win_list.rs`; std `read_dir` is only the fallback) | Microsoft docs | faster on local 5,000-file and SMB 50,000-file folders vs today and vs Total Commander | small | high |
| R12 | Same-server SMB copy via `FSCTL_SRV_COPYCHUNK` | data stays on the server | copy test on a real share; falls back cleanly | medium | high (2 PB shares) |
| R13 | Block cloning for same-volume copies on ReFS / Dev Drive | Microsoft engineering post | copy test on a Dev Drive in the VM | small | low |
| R14 | SMB folders: first rows paint from the first 1 MB query; re-list on demand instead of change notifications (second half **rejected, RP9**) | MS-SMB2 credits; ReadDirectoryChangesExW limits on SMB | first-paint time on a 50,000-file share | medium | medium |
| R15 | Dev profile `debug = "line-tables-only"`; measure `sccache` across worktrees | Cargo docs; target dirs are 30–70 GB | target size and cold build time before/after | small | medium |

## Navo Compare (standalone) additions

- C-live — **Live working-copy compare** (Kaleidoscope 7, 2026-07): the diff and Stream update as files change. Our watcher and Stream make it cheap. Compare 2.
- C-syntax — Syntax-aware text diff with tree-sitter (WinMerge 2.16.60). Compare 3.
- C-table — Table compare with sheets (Beyond Compare 5). Compare 3, with the CSV/Excel item.
- C-patch — Open `.patch` / `.diff` files as a diff (Beyond Compare 5.2). Compare 2.

## Navo candidates from competitors (check whether we have them)

Duplicate finder, checksums (XXH128, BLAKE3) with verify, repeat last command, per-folder remembered sort and view (Double Commander request open since 2022), formula search filters (Everything 1.5). No subscription for updates is a selling point against Directory Opus.

## Atoms for v0.7.0

Scope: R2, R5, R12/R13, R14 and R15. R1 and R8 are in build, R3 and R4 are done, R11 is in place. Decision ids use the plan-local prefix `RP`; global decisions stay `D…` in `docs/decisions.md`.

### Crate versions (crates.io, 2026-11-10)

`crash-handler` 0.8.1, `minidumper` 0.11.0, `minidump` 0.27.0, `minidump-synth` 0.27.0 (MIT, test only), `cargo-shear` 1.14.0, `sccache` 0.19.0. Checked and not adopted: `notify` 8.2.2 (9.0.0-rc.8 pre-release), `notify-debouncer-full` 0.7.0, `windows-service` 0.8.1, `frizbee` 0.13.0, `nucleo-matcher` 0.3.1 (kept), `flate2` 1.1.11 (DP1 bumps it).

### Decisions

- RP1 — **Order.** Atoms that change `Cargo.lock` (R2b, R5a) join the lockfile queue after the deps-latest chain (its D3), one at a time, each from the current `origin/main`. All other atoms run in parallel in their own worktrees; their cargo builds still take the shared `flock`, so only one build runs at a time. VM runs are serialized by the `ci.sh` lock.
- RP2 — **Native crash design (R2).** Out of process, as D88 picked.
  - Server: `navo.exe --crash-server <socket> <logs>`, a hidden standalone flag. It writes the dump through `minidumper` 0.11 and exits when its client disconnects, after one dump, or after 10 s with no client.
  - Client: `crash::install` (GUI start only, after the updater ran) starts a background thread. The thread removes a stale socket, spawns the server (`CREATE_NO_WINDOW`, all stdio null), connects (50 tries, 100 ms apart) and attaches `crash-handler` 0.8.1. The handler calls `request_dump` and returns `Handled(true)` on success, `Handled(false)` otherwise (Windows error reporting then runs as today).
  - Socket: `std::env::temp_dir()/navo-crash-<gui pid>.sock`. Over 100 bytes of path: no native reporter, one `warn` log line, the panic hook stays.
  - Rust panics stay with the panic hook: they unwind and never reach the unhandled-exception filter. `__fastfail` aborts cannot be caught by anyone; accepted.
  - The server writes a `crash-<stamp>-<server pid>-000.json` record in the panic hook's `Record` shape, through the hook's own writer, so `pending()`, the crash notice and the report zip pick it up unchanged.
- RP3 — **Native crash privacy.** The `.dmp` (`MiniDumpNormal`: stacks plus full module paths, so the user name) stays in `logs/`, newest 2 kept, and is never zipped or uploaded: `hook::is_crash` accepts only `.json`. The JSON record carries only the exception name, module base names and hex offsets (`navo.exe+0x1a2b`), and raw hex addresses outside modules. The R2e test proves no user name, profile path or drive path reaches it.
- RP4 — **R6 rejected.** `notify` 8.2.2 on Windows has a fixed 16 KiB buffer (ours is 64 KiB, the network-share maximum). It treats `ERROR_NOTIFY_ENUM_DIR` as an error that silently stops the watch, and a 0-byte completion is not an overflow; ours turns both into a rescan. `notify-debouncer-full` has no max-delay cap (ours: 100 ms quiet, 500 ms max), wakes every tick, and its file-id cache crawls the watched subtree, which D103 forbids on shares. We would also lose the live flag and the `raw_changes` count. Our ~830 lines stay.
- RP5 — **R7 rejected.** `windows-service` would replace only thin FFI wrappers in `navo-index-service/src/scm/windows.rs`. It would reopen the VM-proven install, update, elevation and rollback paths for no user-visible gain.
- RP6 — **R9 rejected; beta G18a and G18b closed.** `docs/spike/fuzzy-bench.md` measured the same versions we would adopt today (nucleo-matcher 0.3.1, frizbee 0.13.0; neither has released since). On 2 M paths, one thread, frizbee was 1.42× faster for `r`, 2.53× for `crt` and 2.50× for `cargotom`: below the 2× gate on short queries. It also lost a target nucleo finds (`testparse` → `parser_cases.rs`: 9/20 against 10/20). Our search already runs on all cores with an ASCII prefilter. Typo tolerance (frizbee with typos found 11/20) is a feature question for "smarter search" in the post-now order, not a speed pick. Revisit when frizbee releases 0.14 or nucleo-matcher stops building.
- RP7 — **R10 already in place.** `zip` 9.0.3's `deflate` feature turns on `flate2`'s `zlib-rs` feature. Feature unification therefore gives the shipped exe the zlib-rs backend (`Cargo.lock`: flate2 depends on `zlib-rs`). Per-crate tests of `navo-fs`, `navo-automotive` and `navo-ui` still build `flate2` on `miniz_oxide`; both backends are correct, so no atom.
- RP8 — **R12/R13: no own `FSCTL_SRV_COPYCHUNK` or `FSCTL_DUPLICATE_EXTENTS_TO_FILE` code.** `CopyFile2` (our copy engine, D88; `navo-fs/src/local/win_copy.rs`) already asks SMB for a server-side copy and ReFS/Dev Drive for block cloning. Own FSCTL code would bypass the progress callback, the destination registration on `STREAM_STARTED`, `FlushFileBuffers` and attribute copy. Those carry the journal and verified-copy guarantees. R12a only measures. R12b may only drop `COPY_FILE_NO_BUFFERING` for unowned copies; job copies (owned) never set it today. The journal (`journal_held` / `journal_placed`), registration, flush and the verify read of the target stay exactly as they are; verify still reads the target over the network by design.
- RP9 — **R14 second half rejected.** Our `ReadDirectoryChangesW` watcher works on SMB (64 KiB buffer, overflow → rescan); we do not use `ReadDirectoryChangesExW`. Re-listing on demand would show stale folders. The first-rows half is measured first (R14a, G-R14).
- RP10 — **R15.** `[profile.dev] debug = "line-tables-only"` goes in the root `Cargo.toml` (the test profile inherits it). The `CARGO_PROFILE_DEV_DEBUG` / `CARGO_PROFILE_TEST_DEBUG` lines in `checks.yml`, `release.yml`, `windows-quick.yml` and the VM's `ci.ps1` stay, because those also build commits from before R15a. `sccache` is measured only; adopting it is a machine setting outside the repo (the orchestrator's environment rules), never a repo file.
- RP11 — **R5.** `cargo shear` 1.14.0 is installed pinned with `--locked`. A reported dependency is removed only after `git grep` finds no use in source, `build.rs`, macros or `cfg`-gated code. A false positive goes into `[package.metadata.cargo-shear] ignored = [...]` in that crate, with a TOML comment giving the reason. The monthly update atom (T1 in `plans/README.md`) gains "run `cargo shear`"; the orchestrator adds that line when it merges R5a.

### Gates

- **G-R12/R13** (from R12a's `offload.json`; a case counts as "offloaded" when the test process read less than 1 % of the file size):
  - every case offloaded on a target → that pick is done ("in place"), numbers in the Log, no code;
  - `large_unowned` not offloaded while `small_unowned` or `large_job` is offloaded on the same target → R12b;
  - no case offloaded on SMB → R12 moves to Rejected (RP8); no case offloaded on the Dev Drive → R13 moves to Rejected, with the VM's Windows build number.
- **G-R14** (owner run of R14a's bench on a real share with at least 50,000 files): median `list_ms` ≥ 500 ms **and** median `first_batch_ms` ≤ 0.25 × median `provider_ms` → R14b. Otherwise R14 moves to Rejected with the numbers.
- **G-R15a:** keep the profile when the cold `navo-fs` test build's target dir shrinks by ≥ 15 % or its wall time by ≥ 10 %; otherwise revert and move the dev-profile half to Rejected.
- **G-R15b:** the second worktree's cold build with `sccache` takes ≤ 0.5 × the first worktree's → the orchestrator adopts it in its environment rules (cache bounded at 20 GB); otherwise drop it.

### Atoms

CHK means: `flock ~/.cache/rusty-cargo.lock nice -n 10 env CARGO_BUILD_JOBS=8 CARGO_TARGET_DIR=<wt>/target <cmd> > <wt>/../<atom>.log 2>&1; echo $?`, then `tail -n 40` of the log. VM means: commit on the atom branch, then `../winvm/tools/ci.sh <wt> <sha> clippy,test` ends with `=== ALL GREEN`. Windows-only code never compiles on Linux, so every atom with `cfg(windows)` code has VM as its check.

| Id | Name | Files | Role | Effort | Deps | Check | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| R2a | `crash-record-writer` | `crates/navo-app/src/crash/hook.rs` (154) | patcher | medium | — | CHK `cargo nextest run -p navo-app crash`; CHK `cargo xtask check -p navo-app` | todo |
| R2b | `native-summary` | `crates/navo-app/Cargo.toml` (97), `Cargo.lock`, `crates/navo-app/src/crash/native_summary.rs` (new); one `mod` line in `crash/mod.rs` (111) | api-builder | high | R2a, lockfile slot | CHK `cargo nextest run -p navo-app native_summary`; CHK `cargo xtask check -p navo-app`; `cargo deny check` | todo |
| R2c | `native-server` | `crates/navo-app/src/crash/native_server.rs` (new), `crates/navo-app/src/crash/mod.rs` | senior | high | R2b | CHK `cargo nextest run -p navo-app native_server`; CHK `cargo xtask check -p navo-app`; VM | todo |
| R2d | `crash-server-flag` | `crates/navo-app/src/cli.rs` (268), `crates/navo-app/src/cli/tests.rs`, `crates/navo-app/src/main.rs` (403 raw, 381 code) | api-builder | medium | R2c | CHK `cargo nextest run -p navo-app cli`; CHK `cargo xtask check -p navo-app`; VM | todo |
| R2e | `native-crash-test` (red) | `crates/navo-app/tests/crash_test.rs` (61) | api-builder | medium | R2d | VM shows the new tests red, all else green | todo |
| R2f | `native-client` | `crates/navo-app/src/crash/native_client.rs` (new), `crates/navo-app/src/crash/mod.rs`, `crates/navo-app/src/main.rs` | senior | high | R2e (same branch) | VM `=== ALL GREEN`, incl. R2e's tests | todo |
| R5a | `shear` | the `Cargo.toml` of at most 2 crates `cargo shear` names, `Cargo.lock` | patcher | medium | DP1, lockfile slot | `cargo shear` clean; CHK `cargo xtask check -p <each touched crate>` | todo |
| R12a | `copy-offload-probe` | `crates/navo-fs/src/local/tests/copy_offload.rs` (new), `crates/navo-fs/src/local/tests/mod.rs` (157), `scripts/offload-probe.ps1` (new) | senior | high | — | VM; then the VM probe run below writes `offload.json` | todo |
| R12b | `offload-buffering` | `crates/navo-fs/src/local/win_copy.rs` (328 raw, 294 code), `crates/navo-fs/src/local/tests/copy_offload.rs` | senior | high | G-R12/R13 | VM; VM probe run shows `large_unowned` offloaded | gated |
| R14a | `first-batch-bench` | `crates/navo-app/src/bench/listing.rs` (78), `crates/navo-app/src/bench/report/mod.rs` (127), `crates/navo-app/src/bench/folder.rs` (246) | api-builder | medium | — | CHK `cargo nextest run -p navo-app --features tester bench::folder`; CHK `cargo xtask check -p navo-app` | todo |
| R14b | `first-rows` | split after G-R14 (see brief) | api-builder | high | G-R14 | per split atom | gated |
| R15a | `dev-line-tables` | `Cargo.toml` (42) | patcher | medium | — | measurement below; CHK `cargo xtask check -p navo-fs` | todo |
| R15b | `sccache-measure` | none (measurement only) | infra-builder | medium | R15a | measurement below | todo |

**Order (RP1).** Wave 1 in parallel: R2a, R12a, R14a, R15a. Then R15b. The lockfile queue, after deps-latest: R2b, then R5a. Chain: R2b → R2c → R2d → R2e + R2f (one branch, merged together). VM runs (R2c, R2d, R2f, R12a) take the VM lock in turn. R12b and R14b start only when their gates pass.

#### R2a — Shared crash record writer

- **Change:** Extract the write half of `Writer::write` (`hook.rs:80-92`) into `pub(super) fn write_record(logs: &Path, record: &Record) -> io::Result<PathBuf>`. It names the file `crash-<UTC %Y%m%d-%H%M%S>-<pid>-<seq:03>.json`, writes it with `navo_store::replace_atomic`, prunes to `KEEP_FILES` through `files()`, and returns the written path. `Record` and its fields become `pub(super)`. `Writer::write` builds the record and calls `write_record`. Behaviour is unchanged.
- **Tests:** none new (pure extraction); every existing test in `crash/tests.rs` and `tests/crash_test.rs` passes unchanged.
- **Stop if:** an existing crash test needs a change.

#### R2b — Native crash summary from a minidump

- **Dependencies** (`crates/navo-app/Cargo.toml`, all `=`-pinned):
  - Under `[target.'cfg(windows)'.dependencies]`: add `minidump`, `minidumper` and `crash-handler`, and add the `Win32_System_Diagnostics_Debug` feature to `windows-sys`.
  - Under `[dev-dependencies]`: add `minidump` and `minidump-synth`.
  - `crash/mod.rs` gets `#[cfg(any(windows, test))] mod native_summary;`.
- **Contract** (`native_summary.rs`):
  - `pub(super) struct NativeCrash { pub(super) reason: String, pub(super) at: Option<String>, pub(super) frames: Vec<String> }`
  - `pub(super) fn summarize(dump: &[u8]) -> Result<NativeCrash, minidump::Error>`
  - `pub(super) fn record(crash: &NativeCrash) -> super::hook::Record`
- **Rules:**
  - Module name: the text after the last `\` or `/` of `code_file()`. Frame text: `{name}+0x{offset:x}`, where offset = address − module base. An address outside every module is `0x{addr:x}`.
  - `reason`: `get_crash_reason(os, cpu)` from the system-info stream, as text. Without that stream: `exception 0x{code:08x}`.
  - `at`: the frame text of the crash address.
  - `frames`: a stack scan of the crashing thread, which is the exception stream's thread id. Read aligned `u64` words from the context stack pointer through at most 64 KiB of that thread's stack memory. Keep a word only when it falls inside a module. The instruction pointer comes first. At most 32 frames.
  - `record`:
    - `thread` "native";
    - `location` = `at`;
    - `message` = `native crash: {reason}`;
    - `backtrace` = `stack scan (may include stale frames)` plus one frame per line;
    - `events` empty;
    - `version` and `commit` as the hook sets them.
- **Red tests** (build the dumps with `minidump-synth`, as `minidump` 0.27's own tests do in `src/minidump.rs`, `read_synth_dump`):
  1. An access violation in a module `C:\Users\someone\AppData\Local\navo\current\navo.exe`. The reason names an access violation, `at == "navo.exe+0x1234"`, and the serialized record contains neither `someone` nor `:\`.
  2. A stack with 40 in-module words and noise gives 32 frames, each `<base name>+0x…`.
  3. Garbage bytes return `Err`, never a panic.
- **Stop if:** `minidump-synth` cannot build an AMD64 thread context with stack memory. Then ship tests 1 and 3; R2e proves the stack scan.

#### R2c — Crash server

- **Contract** (`native_server.rs`):
  - `#[cfg(any(windows, test))] pub(super) struct DumpSink` has three methods:
    - `new(logs: &Path) -> io::Result<Self>` runs `create_dir_all`;
    - `create(&self) -> io::Result<(File, PathBuf)>` opens `logs/crash-<UTC stamp>-<pid>.dmp` with `create_new`;
    - `finish(&self, dump: &Path) -> io::Result<PathBuf>` reads at most 64 MiB, runs `summarize` + `record` + `hook::write_record`, and keeps the 2 newest `crash-*.dmp`. When the dump is unreadable it still writes a record, with message `native crash: unreadable minidump` and an empty backtrace.
  - `#[cfg(windows)] pub(crate) fn serve(socket: &Path, logs: &Path) -> ExitCode` uses `Server::with_name(SocketName::Path(socket))`. Its `ServerHandler`:
    - `create_minidump_file` calls `DumpSink::create`;
    - `on_minidump_created` calls `finish` and returns `Exit` (`Exit` on an error too);
    - `on_client_connected` sets a flag;
    - `on_client_disconnected(0)` returns `Exit`;
    - `on_message` ignores the message.
  - Serve loop and exit: a watchdog thread sets the shutdown flag after 10 s when no client ever connected. It calls `run(handler, &shutdown, None)`. On exit, `serve` removes the socket file (`NotFound` ignored). It returns 0, or 1 on a server error.
  - `crash/mod.rs`: `mod native_server` (cfg as above). It re-exports `pub(crate) fn serve`, and a `#[cfg(not(windows))]` `serve` that returns `ExitCode::from(2)`.
- **Red tests** (sink, run on Linux):
  - `finish` writes one record that `hook::files` lists, and leaves 2 of 4 dumps (the newest);
  - an unreadable dump still writes a record;
  - `create` never overwrites.
- **Stop if:** `minidumper` 0.11 fails to build for `x86_64-pc-windows-msvc` with `+crt-static` in the VM.

#### R2d — Hidden `--crash-server` flag

- **Contract:**
  - `Command::CrashServer { socket: PathBuf, logs: PathBuf }`, parsed from `--crash-server <socket> <logs>` with the existing `value` helper.
  - The flag is added to both `STANDALONE` arrays (sizes 9 and 5) and left out of `USAGE`. `uses_console` returns false for it.
  - `execute` (`main.rs:99-101`) returns `crash::serve(&socket, &logs)` right after the `PdfHelper` arm. `execute_command` gets the same arm.
- **Red tests** (`cli/tests.rs`):
  - the flag with both values parses;
  - a missing value is a usage error;
  - combined with `--left` it is an error.
- **Stop if:** `main.rs` would pass 400 code lines; move `crash_test` into `crash/` first and report.

#### R2e — Native crash integration tests (red)

- **Tests** (`#[cfg(windows)]` in `tests/crash_test.rs`):
  1. `native_crash_test_writes_a_redacted_record` runs `navo --crash-test <root>` with `NAVO_CRASH_TEST=native` and `NAVO_LOG=info`. It checks:
     - the exit code is none of 0, 2, 3 or 101;
     - within 10 s exactly one `crash-*.json` appears in `<root>/data/logs`, with `message` starting `native crash: ` and naming an access violation;
     - `location` starts with `navo.exe+0x`;
     - the first backtrace line is `stack scan (may include stale frames)`;
     - no string field contains `:\`, `%USERNAME%` (case-insensitive, when it has ≥ 3 characters) or `%USERPROFILE%`;
     - exactly one `crash-*.dmp` exists;
     - within 5 s `temp_dir()/navo-crash-<child pid>.sock` is gone.
  2. `panic_crash_leaves_no_crash_server` runs the existing `NAVO_CRASH_TEST=1` probe and checks that its socket is gone within 5 s.
  - The existing guard test stays as it is (`native` is a new accepted value, not one of its refused values).
- **Check:** VM. The two new tests fail, and everything else is green. Quote the red lines.

#### R2f — Crash client

- **Contract** (`native_client.rs`, `#[cfg(windows)]`):
  - `pub(super) fn attach(logs: &Path)` never blocks: it does everything of RP2 on a new thread.
  - It keeps the `CrashHandler` in a `static NATIVE: OnceLock<CrashHandler>`.
  - `make_crash_event` is `unsafe`. Its `// SAFETY:` comment says the closure only sends on the socket that is already connected.
  - Failures log one `warn` line (`native crash reporter off reason=…`) and leave the panic hook in place.
  - `pub(crate) fn ready(timeout: Duration) -> bool` polls `NATIVE` every 10 ms.
  - `pub(crate) fn raise_access_violation() -> !` calls `RaiseException(EXCEPTION_ACCESS_VIOLATION, 0, 0, null)`. It is well defined, unlike a null write.
- **`crash/mod.rs`:** `install` calls `native_client::attach(&data.join("logs"))` on Windows when `data` is `Some`. It re-exports `ready` and `raise_access_violation`.
- **`main.rs` `crash_test`:** the guard accepts `1` or `native`. For `native`, after `crash::install` it does one of two things:
  - when `crash::ready(5 s)` fails: `eprintln!("navo: native crash reporter did not attach")` and exit 3;
  - otherwise: `raise_access_violation()`.
- **Check:** VM `=== ALL GREEN`, including both R2e tests.
- **Stop if:**
  - the startup perf guard (`checks.yml`, 300 ms) regresses by more than 10 ms in the VM;
  - a `navo.exe --crash-server` process outlives its GUI by more than 2 s;
  - a name-based `navo.exe` check in `navo-index-service` (`elevation_result.rs`, `install_check.rs`) then blocks an update.

#### R5a — Remove unused dependencies

- **Steps:**
  1. Run `cargo install cargo-shear --version 1.14.0 --locked`.
  2. Run `cargo shear` (no `--fix`). Save the output in the atom log.
  3. Act on each finding as RP11 says.
  4. Run `cargo shear` again until it is clean.
- **Stop if:** more than 2 crates are named. Fix the first 2 and report the rest as the input of R5b.

#### R12a — Copy offload probe (measure only)

- **Test** (`copy_offload.rs`, `#[cfg(windows)]`):
  - Every test is `#[ignore = "requires NAVO_TEST_SMB_DIR or NAVO_TEST_DEVDRIVE_DIR"]`. A target whose env var is unset is skipped.
  - Each target gets a `tempfile::tempdir_in` folder and a deterministic xorshift source file.
  - Three cases:
    - `small_unowned`: 64 MiB through `LocalFs::new().copy_file`;
    - `large_unowned`: 300 MiB, the same call, which crosses `unbuffered_from` (256 MiB, `local/mod.rs:94`) and so sets `COPY_FILE_NO_BUFFERING`;
    - `large_job`: 300 MiB through the job path, following `copy_plan` and `run_job` in `copy_windows.rs`.
  - For each case, `GetProcessIoCounters` gives `ReadTransferCount` and `WriteTransferCount` before and after the copy only. That is `Win32_System_Threading`, already a `navo-fs` feature.
  - Asserts: the content is equal (compared after the measured window), and the last progress value equals the size. Offload itself is never asserted.
  - When `NAVO_TEST_OFFLOAD_OUT` is set, each case appends one JSON line: `{target, case, size, read_bytes, write_bytes, ms, offloaded}`.
- **Script** (`offload-probe.ps1`; elevated; idempotent; ends with an explicit `exit`):
  - It creates `C:\navo-offload\share` and `New-SmbShare -Name navo-offload -FullAccess admin`.
  - It creates a 20 GB expandable `C:\navo-offload\devdrive.vhdx` with `diskpart`, as `R:`, then `Format-Volume -DevDrive`. When Dev Drive is unavailable it prints `devdrive: unavailable` and goes on with SMB only.
  - It sets `NAVO_TEST_SMB_DIR=\\localhost\navo-offload`, `NAVO_TEST_DEVDRIVE_DIR=R:\` and `NAVO_TEST_OFFLOAD_OUT=C:\winvm\offload.json`. It prints the Windows build number.
  - It runs `cargo nextest run -p navo-fs --run-ignored only -E "test(copy_offload)" --no-fail-fast`.
- **VM probe run:**
  1. VM.
  2. `flock ~/.cache/winvm-ci/.vm.lock ../winvm/winvm.sh ssh "powershell -NoProfile -ExecutionPolicy Bypass -File C:\src\navo\scripts\offload-probe.ps1"`.
  3. `scp` back `offload.json`, then paste its lines into the Log.
- **Stop if:** a copy fails or its content differs. That is a data-safety bug: report it and change nothing.

#### R12b — Keep offload for large unowned copies (gated)

- **Change:** add `fn offload_likely(source: &Path, target: &Path) -> bool`. It is true in two cases:
  - both paths name the same SMB server (`\\server\` or `\\?\UNC\server\`, case-insensitive);
  - `GetVolumePathNameW` gives the same volume and that volume's `GetVolumeInformationW` flags include `FILE_SUPPORTS_BLOCK_REFCOUNTING`.
- Any API error gives false (today's behaviour). `copy` (`win_copy.rs:146`) sets `COPY_FILE_NO_BUFFERING` only when `!offload_likely(..)`. Nothing else changes (RP8).
- **Red test:** the probe run before the change shows `large_unowned` not offloaded. The run after shows it offloaded, with content and progress asserts still green.
- **Stop if:** `win_copy.rs` would pass 330 code lines; put `offload_likely` in a new `local/win_offload.rs`.

#### R14a — First-batch time in the folder bench

- **Change** (`listing::run`): after the Browser pass, add a provider pass.
  - It calls `LocalFs::new().list(folder, &mut sink, &CancelToken::new())`. The sink is a closure; `provider/mod.rs:125` implements `ListSink` for closures.
  - It records `first_batch_ms` (elapsed time when the first non-empty `EntryBatch` arrives) and `provider_ms` (total).
  - Both become fields of `report::Listing` and `folder::Run`.
  - The text report is unchanged.
- **Red test:** extend `a_folder_run_writes_numbers_only_and_leaves_the_folder_alone`. Each run in the JSON has `first_batch_ms` ≤ `provider_ms`, and both are > 0.
- **Owner run (G-R14):** the tester build: `navo bench --folder \\server\share\<folder with ≥ 50,000 files> --out first-batch.json`.

#### R14b — First rows on slow folders (gated; split when G-R14 passes)

- **Fixed design:** three parts.
  - `navo-fs/src/snapshot/mod.rs` (59) gains `load_folder_streaming`. It calls `on_first(&[FsEntry])` once, with the rows collected so far, when the load is still running 200 ms after start.
  - `navo-ui/src/browser/cached/mod.rs` is 431 raw and 397 code lines, so first a pure move of `Fetch` into `cached/fetch.rs`.
  - Then `cached/first_rows.rs` shows those rows through the same path as `early.rs` (`accept_cached_sorted`). Actions on them behave as on cached rows today. The full listing replaces them as it does now.
- This gives three atoms: `snapshot-streaming`, `fetch-move` and `first-rows`.

#### R15a — Dev profile with line tables

- **Measure:**
  1. In the atom worktree, at the base: `rm -rf <wt>/target`, CHK `/usr/bin/time -f %e cargo nextest run -p navo-fs --no-run`, `/usr/bin/du -sb <wt>/target`.
  2. Add `[profile.dev]` `debug = "line-tables-only"` above `[profile.release]`.
  3. Wipe the target and repeat step 1.
  4. Apply G-R15a, and paste the 4 numbers in the report.
- **Stop if:** the build or any `navo-fs` test fails with the profile.

#### R15b — sccache across worktrees (measurement only)

- **Measure:**
  1. `cargo install sccache --version 0.19.0 --locked --root ~/.cache/sccache-tool`.
  2. Make two fresh worktrees at the same `origin/main` sha.
  3. In each, run CHK `/usr/bin/time -f %e cargo nextest run -p navo-fs --no-run` with `RUSTC_WRAPPER=~/.cache/sccache-tool/bin/sccache`, `SCCACHE_DIR=~/.cache/sccache` and `SCCACHE_CACHE_SIZE=20G`. First worktree 1, then worktree 2.
  4. Record both times and `sccache --show-stats`, and apply G-R15b.
  5. Remove both worktrees and their targets.

## Rejected

panic = "abort" (we catch panics), opt-level "s"/"z" (size is not a goal), cranelift (no unwinding on Windows), BOLT (ELF only), `sonic-rs` (needs `target-cpu=native`), `mupdf` (AGPL), `hayro` for now (no text, links or outline), `gxhash` (no fallback without AES-NI), Turso (pre-1.0), `compio`/IoRing (pre-1.0, Windows 11 only), `git2` (C, no proven gain over gix).

From this plan:

- R6 `notify` + `notify-debouncer-full`: loses overflow → rescan, the 64 KiB share buffer and the 500 ms cap, and crawls trees (RP4).
- R7 `windows-service`: no user gain; it reopens VM-proven service paths (RP5).
- R9 `frizbee`: below 2× on short queries and one ranking target worse in our own spike (RP6). Beta G18a/G18b are closed by this.
- R14 re-list on demand: our watcher works on SMB (RP9).
- Own `FSCTL_SRV_COPYCHUNK` / `DUPLICATE_EXTENTS` code: it would bypass the journal and verify guarantees (RP8).

## Log

- 2026-11-10 — Written from six researcher reports. Next: owner picks the order; R1, R3, R4 and R11 are small and high value.
- 2026-11-10 — Atoms for v0.7.0 written (base `d2c931bc`). Decision ids are plan-local `RP1`–`RP11`, so they do not collide with the global `D…` series or other plans' local prefixes. The atoms are R2a–R2f, R5a, R12a–b, R14a–b and R15a–b. R6, R7 and R9 are rejected, R10 is in place, and beta G18a/G18b are superseded by RP6. Next: wave 1 (R2a, R12a, R14a, R15a); R2b and R5a take lockfile slots after deps-latest.
- 2026-11-10 — R11 measured in the VM (median of 10, ms; local NTFS, cache purged): std `read_dir` 51.1 / 495 (5k / 50k files), Navo's `win_list` 7.5 / 57.6, `FileIdExtdDirectoryInfo` 64 KiB 7.2 / 57.1, 1 MiB 7.1 / 64.3. Loopback SMB 50k warm: std 80.5, Navo 28.1, 64 KiB 29.3, 1 MiB 39.0. Best variant ≤ 7 % faster, under the 20 % bar: listing stays as is. Real SMB untested. The bench is kept off main (branch `worktree-agent-a0fdf18c12ee51782`).
- 2026-11-10 — R12a (VM, Windows 26H2): copy offload already works. Test-process bytes read/written were 0 in all six cases — SMB loopback 64 MiB 563 ms, 300 MiB unowned 2101 ms, 300 MiB job 1405 ms; Dev Drive 64 MiB 14 ms, 300 MiB unowned 63 ms, 300 MiB job 9 ms. Plain NTFS control read and wrote the full size. G-R12/R13: R12 and R13 are in place; R12b does not start. Untested: a separate remote SMB server.
- 2026-11-10 — R8 (skrifa 0.49), R2a and R14a merged. DP1 kept `windows-core`: navo-bench uses it (`guard/shell_sink.rs`); R5a decides with `cargo shear`.
- 2026-11-10 — R1 merged: natural order through a precomputed byte key per row (`NaturalArena`). Release timing, median of 5: 5,000 rows 1.07 → 0.73 ms, 100,000 rows 17.9 → 23.0 ms (1.29×, accepted by the orchestrator: +5 ms on 100k for correct digit order). CI's gungraun guard measures it.
