# Speed checks — part C: a Tracy trace build for testers

Status: drafting, revision 5 · Repo: `navo`, plus a scan tool in `../winvm/tools/` (outside git) · Started: 2026-11-10

This plan ships on its own. The shared spec and the shared rules (builders, CHK, files, lockfile chain) are in `plans/2026-11-10-speed-checks.md`.

The links to that plan are sequencing only; no atom here needs part A's Rust code.

- C1 changes `Cargo.lock`, so it starts after A9, part A's last lockfile writer, has merged.
- C7 edits `release.yml`, and G-C2 uses A5's dry-run input, so C7 starts after A5 has merged.

## What the owner gets

The tester zip `navo-bench-tester.zip` gains:

- `navo-tracy.exe`;
- `tracy-capture.exe`, `tracy-csvexport.exe` and `tracy-profiler.exe`;
- Tracy's licence.

The normal Navo download never contains any of them.

How the owner records a slow moment (goes into the tester README):

1. Quit Navo from the tray, so the two copies do not compete for the summon key.
2. Unzip the tester zip into a folder outside Downloads.
3. In that folder, run `.\tracy-capture.exe -o navo.tracy -a 127.0.0.1`. It waits.
4. Start `.\navo-tracy.exe --profile .\tracy-profile`. It uses its own settings and never touches the real profile.
5. Do the slow thing, then close Navo. `tracy-capture` writes `navo.tracy` and stops.
6. Run `.\tracy-csvexport.exe navo.tracy > zones.csv` and send only `zones.csv`. It holds fixed zone names, empty source columns and times.
7. `navo.tracy` stays on the laptop. Open it with `tracy-profiler.exe` if you want to look yourself. Never commit or publish it.

What a trace holds:

- fixed zone names and their times;
- frame marks;
- thread ids and the names Windows gives threads;
- the exe name, `navo-tracy.exe`;
- CPU, RAM and OS version;
- the header line `User: navo@tester`.

By design it holds no file names, paths, field values, real login, computer name or full name. The design reasons are:

- zone names are fixed literals in our code;
- source fields are empty;
- the header values are fixed at build time.

What is checked is narrower: G-C1 scans one real capture of the exercised flows. That scan finds only `User: navo@tester`, no path-shaped string, and none of the forbidden words, in every ASCII, UTF-8 and UTF-16LE string of three or more characters.

What leaves the laptop is only `zones.csv`. Its columns are zone name, source file (empty), source line (0) and times.

## Goal

A slow moment on the laptop can be recorded and explained without guessing. Only the zone table leaves the laptop. The trace header names no person and no machine, and the release build carries no profiler code.

## Decisions

Final. Workers never reopen them.

- SC14 — **Generic host info: `navo@tester` (owner decision, 2026-11-10).**
  - **The hook.** The vendored Tracy client in `tracy-client-sys =0.30.1` has an upstream hook: with `TRACY_HAS_CUSTOM_USER_INFO` defined, it calls `PlatformGetUserLogin`, `PlatformGetUserFullName` and `PlatformGetHostname` (`TracySystem.cpp:348-376`, `TracyProfiler.cpp:599-608`).
  - **Our header.** `crates/navo-diag/tracy/navo_tracy_user.hpp` defines the macro and the three functions:
    - login `navo`;
    - full name `nullptr`, so no `(full name)` part is written;
    - host name `tester`.
  - **How it is built in.** The header is force-included into the client's single C++ file through `TRACY_CLIENT_SYS_CXXFLAGS`, which `tracy-client-sys/build.rs:109` reads:
    - MSVC: `-FI<abs path>`;
    - gcc and clang: `-include <abs path>`.
  - **Fail closed.**
    - **Build guard.** `crates/navo-diag/build.rs` fails any build with the `tracy` feature unless `TRACY_CLIENT_SYS_CXXFLAGS` holds a real force-include of the header: a whitespace token `-FI<path>` or `/FI<path>`, or a token `-include` followed by a token `<path>`, where `<path>` ends in `navo_tracy_user.hpp` and the file exists. A name inside a define (`-DX=navo_tracy_user.hpp`) does not count.
    - **No external client.** The same guard fails when `TRACY_CLIENT_LIB`, `TRACY_CLIENT_LIB_PATH` or `TRACY_CLIENT_STATIC` is set, because `tracy-client-sys/build.rs:121-136` then links a prebuilt client and never compiles the guarded source.
    - **Hook guard.** `bench-release.yml` fails when the resolved `tracy-client-sys` source no longer contains `TRACY_HAS_CUSTOM_USER_INFO` in both files.
    - **Re-proof on bumps.** Any `tracy-client` or `tracy-client-sys` version change repeats G-C1 before the next tester zip.
  - **Why not the other routes.** This beats a patched vendored copy, because there is no fork to maintain and the version stays pinned. It also beats scrubbing the header after capture, because the real values never reach the trace at all.
- SC15 — **Our own zone layer replaces `tracing-tracy`.**
  - **Why.** `tracing-tracy` sends each span's source file path.
  - **Zones.** A `tracing_subscriber::Layer` in `navo-diag` opens a zone on span enter with `Client::span_alloc(Some(name), "", "", 0, 0)`. The name is the span's static name; the function and file are empty, the line is 0, and there is no callstack.
  - **Zone stack.** Zones live on a thread-local stack and are popped on exit.
  - **Scope.** The layer sees spans only, at `DEBUG` or above. It ignores events and field values.
  - **`frame_mark`** checks `Client::running()` first.
  - **`tracy-client` features:** `enable`, `only-localhost` and `ondemand`. There is no `broadcast`, `sampling`, `code-transfer`, `system-tracing`, `context-switch-tracing` or `crash-handler`.
- SC16 — **Tracy only in the tester zip; proven on the packaged exes.**
  - **Build.** `bench-release.yml` builds `navo-tracy.exe` with `--features tester,tracy` in its own target dir, with SC14's flags.
  - **Positive control.** `cargo tree -e features` shows `tracy-client` for `tester,tracy`. The marker `TRACY_PORT` is present in the bytes of `navo-tracy.exe`; it is a `GetEnvVar` literal at `TracyProfiler.cpp:1585`.
  - **Negative checks.**
    - `cargo tree` for `tester` alone, and for the release build, does not show `tracy-client`.
    - The marker is absent from the tester `navo.exe` and from the release `navo.exe` and `navo-index.exe`.
    - `release.yml` fails when `dist/` holds any Tracy exe.
  - This extends D90.
- SC17 — **Zones: the app process only, fixed sites.**
  - **Existing:** `listing.sort`, `listing.filter`, `listing.view_job`, `listing.load`, `status.totals`, and frame marks.
  - **New:**
    - `index.search` (`navo-index/src/search.rs:65`, `search_filtered_with`);
    - `l10n.catalog` (`navo-l10n/src/catalog.rs:82`, `build_on`);
    - `compare.diff` (`navo-app/src/compare/diff/load.rs:140`, `alignment_of`);
    - `compare.card` (`navo-app/src/compare/diff/stream/worker.rs:77-81`: the `card_plan` expression in `build_card`);
    - `compare.intraline` (`navo-app/src/compare/diff/stream/worker.rs:82`: the `to_card` call, which computes every row's in-line spans).
    - Both are block-scoped in `worker.rs`, for example `let plan = { let _zone = tracing::debug_span!("compare.card").entered(); … };`. `convert.rs` (381 lines) is not touched.
    - `rename.preview` (`navo-ui/src/multi_rename/preview.rs:134`, `rows`).
  - **Form.** `let _zone = tracing::debug_span!("<name>").entered();` with no fields: the function's first line, or the first line of the block for the two `worker.rs` zones.
  - **Deferred.** Viewer search and the index service are follow-ups F1 and F2.
- SC18 — **Privacy proof by a real capture.**
  - **The scan tool.** `../winvm/tools/tracy_scan.py` decompresses a `.tracy` file and lists its strings. It fails on:
    - any `User: ` line other than `User: navo@tester`;
    - a drive path `[A-Za-z]:[\\/]`;
    - a backslash path segment `\\[^\\\s]+\\`;
    - `\.rs\b`;
    - `/(home|Users|mnt|srv)/`;
    - any `--forbid` word, matched as a whole word and ignoring case.
  - **Strings scanned.** Runs of three or more printable characters in ASCII and UTF-8, and runs of three or more UTF-16LE code units, all from the decompressed data.
  - **Positive control.** The same scan of a real capture of a copy of `navo-tracy.exe` renamed `zz-canary-7f3e.exe` must fail on `--forbid zz-canary-7f3e`. Tracy records the exe name, so this proves the decoder reaches real trace strings.
    
    Its regexes are proven by its own tests.
  - **What the CSV proves.** The export proves which zone names ran, that their `src_file` is empty and `src_line` is 0, and their times. It cannot prove anything about the header, thread names, the program name, frame names or any other string. Those are covered by the scan, and the owner sends only the CSV.
  - **What the scan proves, and no more.** For the captured trace only, there are no path-shaped strings and no forbidden words in the scanned encodings, and the header says `navo@tester`. It does not prove anything for flows that were not exercised, for numeric fields, or for other encodings. Those rest on the design reasons above.

## Risks and rollback

- R1 — **The trace leaks names.** Prevented by SC14, SC15 and the SC18 scan on a real capture (G-C1).
- R2 — **Profiler code ships.** Prevented by SC16's checks on the packaged exes, with a positive control.
- R3 — **A Tracy bump drops the hook.** Prevented by SC14's source grep and the G-C1 repeat.
- Rollback: `git revert <sha>` per atom. The scan tool is outside git; delete it.

## Atoms

| Atom | Component | Files (all) | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| C1 | profiling | `crates/navo-diag/Cargo.toml`, `crates/navo-diag/src/profiling.rs`, `Cargo.lock` | api-builder | high | part A's A9 merged (lockfile only) | CHK `cargo nextest run -p navo-diag --features tracy profiling`; CHK `cargo xtask check -p navo-diag` | todo |
| C2 | host info | `crates/navo-diag/build.rs` (new), `crates/navo-diag/tracy/navo_tracy_user.hpp` (new), `crates/navo-diag/tracy/flags.rs` (new), `crates/navo-diag/tests/tracy_flags.rs` (new) | patcher | medium | C1 | the six C2 checks below | todo |
| C3 | zones | `crates/navo-index/src/search.rs`, `crates/navo-l10n/src/catalog.rs` | patcher | medium | — | CHK `cargo xtask check -p navo-index -p navo-l10n` | todo |
| C4 | zones | `crates/navo-app/src/compare/diff/load.rs`, `crates/navo-app/src/compare/diff/stream/worker.rs` | patcher | medium | — | CHK `cargo xtask check -p navo-app` | todo |
| C5 | zones | `crates/navo-ui/src/multi_rename/preview.rs` | patcher | medium | — | CHK `cargo xtask check -p navo-ui` | todo |
| C6 | tester zip | `.github/workflows/bench-release.yml`, `crates/navo-bench/README.txt` | infra-builder | medium | C2 | `actionlint`; the C6 branch dispatch below | todo |
| C7 | release check | `.github/workflows/release.yml` | patcher | medium | C6, part A's A5 merged | `actionlint`; G-C2 | todo |
| C8 | scan tool | `../winvm/tools/tracy_scan.py`, `../winvm/tools/tracy_scan_test.py`, `../winvm/tools/tracy-requirements.txt` (all new) | infra-builder | medium | — | `.venv-tracy/bin/python -I tracy_scan_test.py` passes | todo |

**Orchestrator steps:**

- **G-C1** (after C3–C6 and C8):
  1. Dispatch `bench-release.yml` with a test tag. The zip lists `navo-tracy.exe` and the three Tracy tools, and the workflow's hook, feature and marker checks pass.
  2. In the VM, regenerate the perf fixture and add an empty file `zz-canary-7f3e.txt` to `fixture\A`.
  3. Run `tracy-capture` and `navo-tracy.exe` from the zip, with the fixture as the left folder. Sort, filter, type in Ctrl+K, open a compare of two text files, and open a multi-rename. Close Navo.
  4. Run `tracy-csvexport navo.tracy > zones.csv`. Every exercised SC17 name is present, with an empty `src_file` and `src_line` 0.
  5. Copy `navo.tracy` to the host scratch folder. Run `tracy_scan.py navo.tracy --forbid <VM USERNAME> --forbid <VM COMPUTERNAME> --forbid zz-canary-7f3e`. It passes, and lists `User: navo@tester`.
  6. **Positive control.** Copy `navo-tracy.exe` to `zz-canary-7f3e.exe` and capture it for a few seconds, the same way. The scan of that trace with `--forbid zz-canary-7f3e` must fail and name the canary. If it passes, the scanner is broken and part C stops.
  7. Delete both traces from both machines. Delete the test pre-release and its tag. Log the result.
- **G-C2** (after C7): a `release.yml` dry run (part A's SC8) on `main` passes the marker and `dist` checks.

### C1 — own zone layer

- **Read:**
  - `crates/navo-diag/src/profiling.rs` (all) and `crates/navo-diag/Cargo.toml`;
  - `crates/navo-app/src/logging/mod.rs`, the `tracy_layer` call site only;
  - in the cargo registry, `tracy-client-0.19.0/src/span.rs:100-140`, `src/state.rs:30-50` (`Client::running`) and `src/frame.rs:165-180` (`frame_mark`).
- **Change:**
  - **`Cargo.toml`.** Drop `tracing-tracy`; the `tracy` feature becomes `["dep:tracy-client"]`. Set `tracy-client` features to `["enable", "only-localhost", "ondemand"]` with `default-features = false`.
  - **`ZoneLayer`**, under `cfg(feature = "tracy")`, implements `Layer<S>` with `on_enter` and `on_exit` only:
    - `on_enter`: when `Client::running()` returns a client, push `client.span_alloc(Some(meta.name()), "", "", 0, 0)` onto a `thread_local!` `RefCell<Vec<tracy_client::Span>>`;
    - `on_exit`: pop.
  - **Filter.** `filter_fn(|m| m.is_span() && *m.level() <= Level::DEBUG)`.
  - **`tracy_layer()`** keeps its signature. It starts the client and returns `Some(ZoneLayer.with_filter(…))`.
  - **`frame_mark`** calls `tracy_client::frame_mark()` only when `Client::running()` is `Some`.
  - **Allocation arguments.** A private `zone_args(meta) -> (Option<&'static str>, &'static str, &'static str, u32, u16)` returns them, so a test can see them.
- **Tests** (feature on, no client running):
  - entering and exiting a span with a `path` field does not panic;
  - the filter rejects an event and a `TRACE` span;
  - `zone_args` gives an empty file, an empty function, line 0 and depth 0;
  - `frame_mark` does not panic.
  
  For this check only, C1 runs before C2's guard exists.
- **Stop if:** `span_alloc` needs a static source location, or a `Span` cannot be kept in a thread-local.

### C2 — `navo@tester` host info

- **Read:**
  - `crates/navo-diag/Cargo.toml`;
  - in the cargo registry: `tracy-client-sys-0.30.1/build.rs:85-115`, `tracy-client-sys-0.30.1/tracy/common/TracySystem.cpp:340-380` and `tracy-client-sys-0.30.1/tracy/client/TracyProfiler.cpp:590-612`.
- **Change:**
  - **`navo_tracy_user.hpp`.** A header comment, then:
    - `#pragma once`;
    - `#include <cstddef>` and `#include <cstring>`;
    - `#define TRACY_HAS_CUSTOM_USER_INFO 1`;
    - three `static inline` global functions: `PlatformGetUserLogin()` returns `"navo"`, `PlatformGetUserFullName()` returns `nullptr`, and `PlatformGetHostname(char* buf, size_t size)` copies `"tester"` with `strncpy(buf, "tester", size - 1)`.
  - **`build.rs`.**
    - It prints `cargo:rerun-if-env-changed=TRACY_CLIENT_SYS_CXXFLAGS`.
    - It also prints `cargo:rerun-if-env-changed=` for `TRACY_CLIENT_LIB`, `TRACY_CLIENT_LIB_PATH` and `TRACY_CLIENT_STATIC`.
    - When `CARGO_FEATURE_TRACY` is set, it panics unless the flags hold a real force-include as SC14's build guard defines (token parse, path exists), and it panics when any of the three external-client variables is set. Each message names the variable and the two flag forms from SC14.
    - The token parse is a small pure function. Build scripts do not run `#[cfg(test)]`, so it lives in `crates/navo-diag/tracy/flags.rs`, included by `build.rs` with `include!` and by a test in `crates/navo-diag/tests/tracy_flags.rs`.
- **Check:** CHK, three runs:
  1. `cargo check -p navo-diag --features tracy` fails with the message.
  2. The same with `TRACY_CLIENT_SYS_CXXFLAGS="-include $PWD/crates/navo-diag/tracy/navo_tracy_user.hpp"` builds.
  3. CHK `cargo nextest run -p navo-diag --features tracy profiling` passes with that variable set.
  4. With `TRACY_CLIENT_SYS_CXXFLAGS="-DX=navo_tracy_user.hpp"` the build fails with the message.
  5. With the valid flags plus `TRACY_CLIENT_LIB=tracy` the build fails with the external-client message.
  6. CHK `cargo nextest run -p navo-diag tracy_flags` passes (accepts both MSVC forms and `-include <path>`; rejects a define, a missing file and a bare `-include`).
- **Stop if:** the client fails to compile with the header force-included.

### C3 — backend zones

- **Read:** `crates/navo-index/src/search.rs:60-80` and `crates/navo-l10n/src/catalog.rs:76-90`.
- **Change:** add SC17's zone line as the first line of `search_filtered_with` (`index.search`) and of `Catalog::build_on` (`l10n.catalog`).
- **Check:** as in the table.
- **Stop if:** either function is not at the line SC17 gives.

### C4 — compare zones

- **Read:** `crates/navo-app/src/compare/diff/load.rs:135-155` and `crates/navo-app/src/compare/diff/stream/worker.rs:60-103`.
- **Change:**
  - `compare.diff`: the zone line as the first line of `alignment_of` in `load.rs`.
  - `compare.card`: wrap the `card_plan` expression in `build_card` (`worker.rs:77-81`) in a block whose first line is the zone.
  - `compare.intraline`: wrap the `to_card(&loaded, &plan)` call (`worker.rs:82`) the same way.

  `convert.rs` is not touched.
- **Check:** as in the table.
- **Stop if:** `worker.rs:77-82` no longer holds the `card_plan` expression and the `to_card` call.

### C5 — rename zone

- **Read:** `crates/navo-ui/src/multi_rename/preview.rs:125-180`.
- **Change:** add SC17's zone line as the first line of `rows` (`rename.preview`).
- **Check:** as in the table.
- **Stop if:** `rows` runs inside a GPUI render callback. Zones go in the work, not in render.

### C6 — trace build in the tester zip

- **Read:**
  - `.github/workflows/bench-release.yml` (all);
  - `crates/navo-bench/README.txt`;
  - in the cargo registry, `tracy-client-sys-0.30.1/README.mkd` (the version table).
- **Change:**
  - **Hook guard.** After checkout, find the `tracy-client-sys` source with `cargo metadata --format-version 1`. Fail unless both `tracy/common/TracySystem.cpp` and `tracy/client/TracyProfiler.cpp` contain `TRACY_HAS_CUSTOM_USER_INFO`.
  - **Build.** Run `cargo build --release --locked -p navo-app --features tester,tracy --target-dir target/tracy`, with `TRACY_CLIENT_SYS_CXXFLAGS: -FI${{ github.workspace }}\crates\navo-diag\tracy\navo_tracy_user.hpp` in that step's `env`. Stage it as `bench/navo-tracy.exe`.
  - **Tools.**
    - Download the Windows asset of the Tracy release that the version table names (expected v0.14.1).
    - Check a SHA256 pinned in the workflow.
    - Stage `tracy-capture.exe`, `tracy-csvexport.exe`, `tracy-profiler.exe` and Tracy's licence.
  - **Checks (SC16).**
    - `cargo tree -p navo-app -e features --features tester,tracy` contains `tracy-client`. The same with `--features tester` does not.
    - The bytes of `navo-tracy.exe` contain `TRACY_PORT`. The tester `navo.exe` does not.
  - **README.** Add the seven owner steps, and the "What a trace holds" paragraph, from this plan.
  - **No build paths in the zip.** The `navo-tracy.exe` build step also sets `RUSTFLAGS: --remap-path-prefix=${{ github.workspace }}=navo --remap-path-prefix=${{ env.CARGO_HOME }}=cargo` and adds `-d1trimfile:${{ github.workspace }}\` to `TRACY_CLIENT_SYS_CXXFLAGS`, so line tables, panic locations and C++ `__FILE__` strings hold no runner paths.
  - **Artifact scan.** Before upload, scan every staged `.exe` and `.dll` in the zip folder with the same string rules as C8 (ASCII, UTF-8, UTF-16LE, 3+ chars). Fail on `runneradmin`, `D:\a\`, `C:\Users\`, `\.cargo\registry` or `${{ github.workspace }}`. The scan runs on `navo-tracy.exe` and the tester `navo.exe`; a hit in Tracy's own prebuilt tools is listed but does not fail (they are upstream binaries).
- **Check:**
  1. `actionlint`.
  2. **Branch dispatch (C6's own check, needs only C2):** dispatch `bench-release.yml` on the C6 branch with a test tag. The hook guard, SC16 feature and marker checks and the artifact scan pass; the zip lists `navo-tracy.exe` and the three Tracy tools. Delete the test release.
  3. G-C1 runs later, on `main`, after C3–C6 and C8 have merged; it is not C6's check.
- **Stop if:** the version table names another Tracy version, or the asset lacks one of the three tools.

### C7 — Tracy checks in the release

- **Read:** `.github/workflows/release.yml`, the `build-windows` job only.
- **Change:** after the build, add three checks to `build-windows`:
  - fail when `cargo tree -p navo-app -p navo-index-service -e features --prefix none` contains `tracy-client`;
  - next to the `navo-bench.exe` staging check, fail when `dist/` holds `navo-tracy.exe`, `tracy-capture.exe`, `tracy-csvexport.exe` or `tracy-profiler.exe`;
  - fail when the bytes of `dist/navo.exe` or `dist/navo-index.exe` contain `TRACY_PORT`.
- **Check:** `actionlint`, then G-C2.
- **Stop if:** `build-windows` has no staging step for `dist/`.

### C8 — trace scan tool

- **Read:** from the Tracy source tarball of the pinned tag, unpacked into the scratch folder:
  - `server/TracyFileRead.hpp`;
  - `server/TracyFileHeader.hpp`;
  - `server/TracyFileWrite.hpp`.
- **Change:**
  - **`tracy-requirements.txt`.** Pins `lz4` and `zstandard` with `==` and `--hash`. Install them into `../winvm/tools/.venv-tracy` with `python -m venv` and `pip install --require-hashes -r`.
  - **`tracy_scan.py <file> [--forbid WORD]…`** does four things:
    1. checks the file header;
    2. decompresses every block as those headers define it;
    3. extracts strings per SC18: ASCII and UTF-8 runs of three or more printable characters, and UTF-16LE runs of three or more;
    4. applies SC18's rules.
    
    It prints the `User:` line and each finding, and exits 1 on any finding or decode error.
  - **`tracy_scan_test.py`.**
    - It builds tiny synthetic `.tracy` files with the same writer layout, for both compressions.
    - These must fail, each in ASCII and again in UTF-16LE: `C:\Users\x\a.txt`, `crates\navo-ui\src\a.rs`, `src/view/sort.rs`, `/home/x/main.rs`, `a.rs`, `User: admin@DESKTOP-1`, `User: navo@tester (Admin Name)`, and the forbidden word `zz-canary-7f3e`.
    - These must pass: `listing.sort`, `compare.intraline`, `User: navo@tester`, `rayon-worker-3`, `navo-tracy.exe`, and 1 MiB of seeded random bytes.
    - A truncated block must give a decode error.
- **Check:** as in the table.
- **Stop if:** the file layout cannot be decoded with `lz4.block` and `zstandard`.

## Done when

- The tester zip holds the trace build and the three Tracy tools.
- The real VM capture passes the scan with `User: navo@tester`, and its CSV has the zones with empty source columns (G-C1).
- The release exes carry no marker, and the release refuses Tracy exes in `dist/` (G-C2).
- The orchestrator updates `docs/perf.md` inline: the capture steps and what a trace holds.

## Follow-ups

- F1 — A viewer text search zone, once the search worker has one function that does the work.
- F2 — A trace build of `navo-index-service` with the same layer and host info, captured separately (zone `index.answer`).

## Log

- 2026-11-10 — Split out of the main plan in revision 3.
  - It covers plan check 2 findings 11 and 12 for part C.
  - It records the owner's Q2 decision: generic `navo@tester`, decided, not pending.
  - The zone sites were found during planning, so there is no scout atom and no split for the worker to decide.
- 2026-11-10 — Revision 4 after plan check 3 (part C, 4 findings):
  1. C7 depends on A5; both links are stated.
  2. The guarantee is narrowed to what the capture and scan prove. The scan covers ASCII, UTF-8 and UTF-16LE strings of three or more characters.
  3. A positive-control capture: an exe renamed to the canary must fail the scan.
  4. C3–C5 run at medium effort. C4's zones go in `worker.rs` and `load.rs`, not in `convert.rs`.
- 2026-11-10 — Revision 5 after plan check 4 (part C, 3 findings):
  1. The build guard parses the force-include token and checks the file exists; a define with the name fails. It also rejects `TRACY_CLIENT_LIB`, `TRACY_CLIENT_LIB_PATH` and `TRACY_CLIENT_STATIC`. C2 checks 4–6 prove both bypasses fail.
  2. C6 has its own branch dispatch check; G-C1 runs on `main` after integration.
  3. The tester build remaps workspace and cargo-home paths (Rust and MSVC), and C6 scans the staged binaries for runner paths before upload.
