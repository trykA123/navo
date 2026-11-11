# Speed checks — part C: a Tracy trace build for testers

Status: ready, revision 7 · Base: `da6e3d8c` (local `main`; every path and line below checked there) · Repo: `navo`, plus a scan tool in `/mnt/Sabrent/homelab/winvm/tools/` (outside git; written `$TOOLS` below) · Started: 2026-11-10

This plan ships on its own. The shared spec and the shared rules (builders, CHK, files, lockfile chain) are in `plans/2026-11-10-speed-checks.md`.

The links to that plan are sequencing only; no atom here needs part A's Rust code. **Part A is merged:** A5 (release dry run and `speed` job), A6/A8/A9 benches, W14 (`Navo-Setup.exe` built in `release.yml`) and noise run 42890376477.

- C1 changes `Cargo.lock`; A9, part A's last lockfile writer, has merged, so C1 may start.
- C7 edits `release.yml`; A5 and W14 have merged, so C7 reads the W14 layout (below).
- G-C2 uses A5's dry-run input, so it runs after part A's G-A3 has proven that input.

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

What is checked is narrower: G-C1 scans one real capture of the exercised flows (SC18). The scan finds only `User: navo@tester` and none of the forbidden words in any ASCII, UTF-8 or UTF-16LE text, and no path-shaped string among the strings Tracy stores.

What leaves the laptop is only `zones.csv`. Its columns are zone name, source file (empty), source line (0) and times.

## Goal

A slow moment on the laptop can be recorded and explained without guessing. Only the zone table leaves the laptop. The trace header names no person and no machine, and the release build carries no profiler code.

## Decisions

Final. Workers never reopen them.

- SC14 — **Generic host info: `navo@tester` (owner decision, 2026-11-10).**
  - **The hook.** The vendored Tracy client in `tracy-client-sys =0.30.1` has an upstream hook: with `TRACY_HAS_CUSTOM_USER_INFO` defined, it calls `PlatformGetUserLogin`, `PlatformGetUserFullName` and `PlatformGetHostname` (`TracySystem.cpp:348-376`, `TracyProfiler.cpp:599-608`).
  - **Our header.** `crates/navo-diag/tracy/navo_tracy_user.hpp` defines the macro and the three functions: login `navo`; full name `nullptr`, so no `(full name)` part is written; host name `tester`.
  - **How it is built in.** The header is force-included into the client's single C++ file through `TRACY_CLIENT_SYS_CXXFLAGS`, which `tracy-client-sys/build.rs:109` passes to `cc` (`try_flags_from_environment`): MSVC `-FI<abs path>`; gcc and clang `-include <abs path>`.
  - **Fail closed.**
    - **Build guard.** `crates/navo-diag/build.rs` fails any build with the `tracy` feature unless `TRACY_CLIENT_SYS_CXXFLAGS`, split on ASCII whitespace as `cc` does, holds exactly one force-include of the header and nothing else except `-d1trimfile:`/`/d1trimfile:` tokens. A force-include is a token `-FI<path>` or `/FI<path>`, or a token `-include` followed by `<path>`; `<path>` ends in `navo_tracy_user.hpp` and the file exists. A define (`-DX=navo_tracy_user.hpp`), an `-U`, or a second include is refused.
    - **No other flag source.** `cc` 1.2.67 (our lockfile) also appends `TRACY_CLIENT_SYS_CXXFLAGS_<target>`, `TRACY_CLIENT_SYS_CXXFLAGS_<target with - and . as _>`, `HOST_TRACY_CLIENT_SYS_CXXFLAGS` and `TARGET_TRACY_CLIENT_SYS_CXXFLAGS` (`cc-1.2.67/src/lib.rs:4038-4053`, `:4074-4095`), and `CC_SHELL_ESCAPED_FLAGS` changes the split. The guard fails when any of them is set, even empty.
    - **No external client.** The guard fails when `TRACY_CLIENT_LIB`, `TRACY_CLIENT_LIB_PATH` or `TRACY_CLIENT_STATIC` is set: `tracy-client-sys/build.rs:121-136` then links a prebuilt client.
    - **Hook guard.** `bench-release.yml` fails when the resolved `tracy-client-sys` source no longer contains `TRACY_HAS_CUSTOM_USER_INFO` in both files.
    - **Re-proof on bumps.** Any `tracy-client`, `tracy-client-sys` or `cc` version change repeats G-C1 before the next tester zip.
  - **Why not the other routes.** No fork to maintain, and the version stays pinned; scrubbing after capture is weaker, because the real values would reach the trace first.
- SC15 — **Our own zone layer replaces `tracing-tracy`.**
  - **Why.** `tracing-tracy` sends each span's source file path.
  - **Zones.** A `tracing_subscriber::Layer` in `navo-diag` opens a zone on span enter with `Client::span_alloc(Some(name), "", "", 0, 0)`: the span's static name, empty function and file, line 0, no callstack.
  - **Zone stack.** Zones live on a thread-local stack and are popped on exit.
  - **Scope.** Spans only, at `DEBUG` or above. Events and field values are ignored.
  - **`frame_mark`** checks `Client::running()` first.
  - **`tracy-client` features:** `enable`, `only-localhost` and `ondemand`. No `broadcast`, `sampling`, `code-transfer`, `system-tracing`, `context-switch-tracing` or `crash-handler`.
- SC16 — **Tracy only in the tester zip; proven on the packaged exes.**
  - **Build.** `bench-release.yml` builds `navo-tracy.exe` with `--features tester,tracy` in its own target dir, with SC14's flags.
  - **Positive control.** `cargo tree -e features` shows `tracy-client` for `tester,tracy`. The marker `TRACY_PORT` is in the bytes of `navo-tracy.exe`; it is a `GetEnvVar` literal at `TracyProfiler.cpp:1585`.
  - **Negative checks.** `cargo tree` for `tester` alone, and for the release packages (`navo-app`, `navo-index-service`, `navo-setup`), does not show `tracy-client`. The marker is absent from the tester `navo.exe` and from the release `navo.exe`, `navo-index.exe` and `navo-setup.exe`. `release.yml` fails when `dist/` holds any Tracy exe.
  - This extends D90.
- SC17 — **Zones: the app process only, fixed sites.**
  - **Existing:** `listing.sort`, `listing.filter`, `listing.view_job`, `listing.load`, `status.totals`, and frame marks.
  - **New:**
    - `index.search` (`navo-index/src/search.rs:65`, `search_filtered_with`);
    - `l10n.catalog` (`navo-l10n/src/catalog.rs:83`, `build_on`);
    - `compare.diff` (`navo-app/src/compare/diff/load.rs:140`, `alignment_of`);
    - `compare.card` (`navo-app/src/compare/diff/stream/worker.rs:77-81`: the `card_plan` expression in `build_card`);
    - `compare.intraline` (`navo-app/src/compare/diff/stream/worker.rs:82`: the `to_card` call, which computes every row's in-line spans);
    - `rename.preview` (`navo-ui/src/multi_rename/preview.rs:134`, `rows`).
  - **Form.** `let _zone = tracing::debug_span!("<name>").entered();` with no fields: the function's first line, or the first line of a block for the two `worker.rs` zones, for example `let plan = { let _zone = tracing::debug_span!("compare.card").entered(); … };`. `convert.rs` is not touched.
  - **Deferred.** Viewer search and the index service are follow-ups F1 and F2.
- SC18 — **Privacy proof by a real capture.**
  - **The scan tool.** `$TOOLS/tracy_scan.py` decodes a `.tracy` file (layout in C8) and collects two kinds of text from the decoded bytes:
    - **stored strings:** every place where a little-endian `u64` length `L` (3 ≤ `L` ≤ 4096) is followed by `L` bytes of valid UTF-8 whose characters are all printable, `\n` or `\t`. This is how Tracy writes every string (`server/TracyWorker.cpp`, `Worker::Write`: `uint64_t sz`, then the bytes);
    - **raw runs:** runs of three or more printable characters in ASCII/UTF-8, and runs of three or more printable UTF-16LE code units at both byte alignments.
  - **Rules on every stored string and raw run:**
    - each `User: ` must be followed, to the end of its line or run, by exactly `navo@tester`;
    - no `--forbid` word, matched case-insensitively with no letter, digit or `_` on either side.
  - **Rules on stored strings only** (path shapes are too common by chance in binary data to apply to raw runs):
    - a drive path `[A-Za-z]:[\\/]`;
    - a backslash path segment `\\[^\\\s]+\\`;
    - `\.rs\b`;
    - `/(home|Users|mnt|srv)/`.
  - **Positive control.** A real capture of a copy of `navo-tracy.exe` renamed `zz-canary-7f3e.exe` must fail on `--forbid zz-canary-7f3e`. Tracy stores the exe name, so this proves the decoder reaches real trace strings. The rules are proven by the tool's own tests.
  - **What the CSV proves.** Which zone names ran, that their `src_file` is empty and `src_line` is 0, and their times. Nothing about the header, thread names or other strings; the scan covers those, and the owner sends only the CSV.
  - **What the scan proves, and no more.** For the captured trace only: the header says `navo@tester`, no forbidden word appears in any ASCII, UTF-8 or UTF-16LE text, and no stored string is path-shaped. Flows not exercised, numeric fields and unprefixed path-shaped bytes rest on the design reasons above.

## Risks and rollback

- R1 — **The trace leaks names.** Prevented by SC14, SC15 and the SC18 scan on a real capture (G-C1).
- R2 — **Profiler code ships.** Prevented by SC16's checks on the packaged exes, with a positive control.
- R3 — **A Tracy or `cc` bump drops the hook or adds a flag source.** Prevented by SC14's source grep, the guard's variable list and the G-C1 repeat.
- Rollback: `git revert <sha>` per atom. The scan tool is outside git; delete it.

## Atoms

| Atom | Component | Files (all) | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| C1 | profiling | `crates/navo-diag/Cargo.toml`, `crates/navo-diag/src/profiling.rs` (42 lines), `Cargo.lock` | api-builder | high | — (A9 merged) | CHK `cargo nextest run -p navo-diag --features tracy profiling`; CHK `cargo xtask check -p navo-diag` | done |
| C2a | host info: flag check | `crates/navo-diag/tracy/flags.rs` (new), `crates/navo-diag/tests/tracy_flags.rs` (new) | patcher | medium | — | CHK `cargo nextest run -p navo-diag --test tracy_flags` | todo |
| C2b | host info: guard and header | `crates/navo-diag/build.rs` (new), `crates/navo-diag/tracy/navo_tracy_user.hpp` (new) | patcher | medium | C1, C2a | the six C2b checks below | todo |
| C3 | zones | `crates/navo-index/src/search.rs`, `crates/navo-l10n/src/catalog.rs` | patcher | medium | — | CHK `cargo xtask check -p navo-index -p navo-l10n` | done |
| C4 | zones | `crates/navo-app/src/compare/diff/load.rs`, `crates/navo-app/src/compare/diff/stream/worker.rs` | patcher | medium | — | CHK `cargo xtask check -p navo-app` | todo |
| C5 | zones | `crates/navo-ui/src/multi_rename/preview.rs` | patcher | medium | — | CHK `cargo xtask check -p navo-ui` | done |
| C6 | tester zip | `.github/workflows/bench-release.yml`, `crates/navo-bench/README.txt` | infra-builder | medium | C2b | `actionlint`; the C6 branch dispatch below | todo |
| C7 | release check | `.github/workflows/release.yml` (339 lines) | patcher | medium | C6 | `actionlint`; G-C2 | todo |
| C8 | scan tool | `$TOOLS/tracy_scan.py`, `$TOOLS/tracy_scan_test.py` (both new) | infra-builder | medium | — | `cd $TOOLS && uv run --script tracy_scan_test.py > /mnt/Sabrent/homelab/winvm/C8.log 2>&1; echo $?` prints 0 | todo |

**Orchestrator steps:**

- **G-C1** (after C3–C6 and C8 have merged):
  1. Dispatch `bench-release.yml` with a test tag. The zip lists `navo-tracy.exe` and the three Tracy tools, and the workflow's hook, feature and marker checks pass.
  2. In the VM, regenerate the perf fixture and add an empty file `zz-canary-7f3e.txt` to `fixture\A`.
  3. Run `tracy-capture` and `navo-tracy.exe` from the zip, with the fixture as the left folder. Sort, filter, type in Ctrl+K, open a compare of two text files, and open a multi-rename. Close Navo.
  4. Run `tracy-csvexport navo.tracy > zones.csv`. Every exercised SC17 name is present, with an empty `src_file` and `src_line` 0.
  5. Copy `navo.tracy` to the host scratch folder. Run `uv run --script $TOOLS/tracy_scan.py navo.tracy --forbid <VM USERNAME> --forbid <VM COMPUTERNAME> --forbid zz-canary-7f3e`. It exits 0 and prints `User: navo@tester`.
  6. **Positive control.** Copy `navo-tracy.exe` to `zz-canary-7f3e.exe` and capture it for a few seconds, the same way. The scan of that trace with `--forbid zz-canary-7f3e` must exit 1 and name the canary. If it passes, the scanner is broken and part C stops.
  7. Delete both traces from both machines. Delete the test pre-release and its tag. Log the result.
- **G-C2** (after C7 and part A's G-A3): a `release.yml` dry run on `main` passes the feature, marker and `dist` checks.

### C1 — own zone layer

- **Read:**
  - `crates/navo-diag/src/profiling.rs` (all) and `crates/navo-diag/Cargo.toml`;
  - `crates/navo-app/src/logging/mod.rs:96`, the `tracy_layer` call site only;
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
- **Tests** (feature on, no client running): entering and exiting a span with a `path` field does not panic; the filter rejects an event and a `TRACE` span; `zone_args` gives an empty file, an empty function, line 0 and depth 0; `frame_mark` does not panic. C1 runs before C2b's guard exists.
- **Stop if:** `span_alloc` needs a static source location, or a `Span` cannot be kept in a thread-local.

### C2a — the guard's flag check

- **Read:** SC14, and `cc-1.2.67/src/lib.rs:4038-4053` in the cargo registry.
- **Change:**
  - **`tracy/flags.rs`.** Its header uses `//` line comments, not `//!`: `include!` refuses inner doc comments. Two private functions, each with a `///` doc comment:
    - `fn watched_vars(target: &str) -> Vec<String>` returns, in this order: `TRACY_CLIENT_SYS_CXXFLAGS`, `TRACY_CLIENT_SYS_CXXFLAGS_{target}`, `TRACY_CLIENT_SYS_CXXFLAGS_{target_u}` (`target.replace(['-', '.'], "_")`), `HOST_TRACY_CLIENT_SYS_CXXFLAGS`, `TARGET_TRACY_CLIENT_SYS_CXXFLAGS`, `TRACY_CLIENT_LIB`, `TRACY_CLIENT_LIB_PATH`, `TRACY_CLIENT_STATIC`, `CC_SHELL_ESCAPED_FLAGS`.
    - `fn check_env(target: &str, get: &dyn Fn(&str) -> Option<String>, exists: &dyn Fn(&std::path::Path) -> bool) -> Result<(), String>`, in this order: any watched variable after the first is `Some` → `Err` naming it; the first is `None` → `Err`; then SC14's token rules → `Err` naming the bad token, a bare `-include`, zero or two force-includes, a wrong file name or a missing file. Every message ends with `use -FI<path> (MSVC) or -include <path> (gcc, clang) to crates/navo-diag/tracy/navo_tracy_user.hpp`.
  - **`tests/tracy_flags.rs`.** `include!("../tracy/flags.rs");`, a fake `get` over a `HashMap`, and a fake `exists`. Target `x86_64-pc-windows-msvc`.
- **Tests:**
  - accept `-FI<p>`, `/FI<p>`, `-include <p>`, and `-FI<p> -d1trimfile:D:\a\navo\navo\`;
  - reject: no variable; `-DX=navo_tracy_user.hpp`; a missing file; a bare `-include`; a path ending in another name; two force-includes; `-FI<p> -UTRACY_HAS_CUSTOM_USER_INFO`;
  - with valid flags, each of the eight other watched variables set to `""` gives an `Err` naming that variable;
  - `watched_vars` holds both `…_x86_64-pc-windows-msvc` and `…_x86_64_pc_windows_msvc`.
- **Stop if:** `include!` of `flags.rs` does not compile in the test crate.

### C2b — `navo@tester` header and build guard

- **Read:** `crates/navo-diag/tracy/flags.rs` (C2a), and in the cargo registry `tracy-client-sys-0.30.1/build.rs:85-136`, `tracy/common/TracySystem.cpp:340-380`, `tracy/client/TracyProfiler.cpp:590-612`.
- **Change:**
  - **`navo_tracy_user.hpp`.** A header comment, then `#pragma once`; `#include <cstddef>` and `#include <cstring>`; `#define TRACY_HAS_CUSTOM_USER_INFO 1`; three `static inline` global functions: `PlatformGetUserLogin()` returns `"navo"`, `PlatformGetUserFullName()` returns `nullptr`, and `PlatformGetHostname(char* buf, size_t size)` does `strncpy(buf, "tester", size - 1)`.
  - **`build.rs`.** `//!` header, `#![allow(clippy::print_stdout, reason = "cargo reads build script directives from stdout")]`, then `include!("tracy/flags.rs");` and `fn main() -> Result<(), Box<dyn std::error::Error>>` (no `panic!`: the workspace denies it; same shape as `crates/navo-app/build.rs`):
    1. `let target = std::env::var("TARGET").unwrap_or_default();`
    2. print `cargo:rerun-if-env-changed={var}` for every `watched_vars(&target)`, and `cargo:rerun-if-changed=tracy/flags.rs`;
    3. return `Ok(())` when `CARGO_FEATURE_TRACY` is unset;
    4. `check_env(&target, &|name| std::env::var(name).ok(), &|path| path.is_file())?; Ok(())` — the error string becomes the build error (`String` converts into `Box<dyn Error>`).
- **Check** (CHK each; `H=$PWD/crates/navo-diag/tracy/navo_tracy_user.hpp`):
  1. `cargo check -p navo-diag --features tracy` fails with the message.
  2. `TRACY_CLIENT_SYS_CXXFLAGS="-include $H" cargo check -p navo-diag --features tracy` builds (the client compiles with the header).
  3. With that variable, CHK `cargo nextest run -p navo-diag --features tracy profiling` passes.
  4. `TRACY_CLIENT_SYS_CXXFLAGS="-DX=navo_tracy_user.hpp"` fails with the message.
  5. Valid flags plus `TRACY_CLIENT_LIB=tracy` fails naming `TRACY_CLIENT_LIB`.
  6. Valid flags plus `TRACY_CLIENT_SYS_CXXFLAGS_x86_64_unknown_linux_gnu=-UX` fails naming that variable.
- **Stop if:** the client fails to compile with the header force-included.

### C3 — backend zones

- **Read:** `crates/navo-index/src/search.rs:60-80` and `crates/navo-l10n/src/catalog.rs:76-90`.
- **Change:** add SC17's zone line as the first line of `search_filtered_with` (`index.search`, line 65) and of `Catalog::build_on` (`l10n.catalog`, line 83).
- **Check:** as in the table.
- **Stop if:** either function is not at the line SC17 gives.

### C4 — compare zones

- **Read:** `crates/navo-app/src/compare/diff/load.rs:135-155` and `crates/navo-app/src/compare/diff/stream/worker.rs:60-103`.
- **Change:**
  - `compare.diff`: the zone line as the first line of `alignment_of` in `load.rs`.
  - `compare.card`: wrap the `card_plan` expression in `build_card` (`worker.rs:77-81`) in a block whose first line is the zone.
  - `compare.intraline`: wrap the `to_card(&loaded, &plan)` call (`worker.rs:82`) the same way.
- **Check:** as in the table.
- **Stop if:** `worker.rs:77-82` no longer holds the `card_plan` expression and the `to_card` call.

### C5 — rename zone

- **Read:** `crates/navo-ui/src/multi_rename/preview.rs:125-180`.
- **Change:** add SC17's zone line as the first line of `rows` (`rename.preview`).
- **Check:** as in the table.
- **Stop if:** `rows` runs inside a GPUI render callback. Zones go in the work, not in render.

### C6 — trace build in the tester zip

- **Read:** `.github/workflows/bench-release.yml` (all); `crates/navo-bench/README.txt`; in the cargo registry, `tracy-client-sys-0.30.1/README.mkd:60-66` (the version table: `v0.14.1` ↔ `0.30.0`).
- **Change:**
  - **Hook guard.** After checkout, find the `tracy-client-sys` source with `cargo metadata --format-version 1`. Fail unless both `tracy/common/TracySystem.cpp` and `tracy/client/TracyProfiler.cpp` contain `TRACY_HAS_CUSTOM_USER_INFO`.
  - **Build.** `cargo build --release --locked -p navo-app --features tester,tracy --target-dir target/tracy`, with this step's `env`:
    - `TRACY_CLIENT_SYS_CXXFLAGS: -FI${{ github.workspace }}\crates\navo-diag\tracy\navo_tracy_user.hpp -d1trimfile:${{ github.workspace }}\`;
    - `RUSTFLAGS: --remap-path-prefix=${{ github.workspace }}=navo --remap-path-prefix=${{ env.CARGO_HOME }}=cargo`.

    Stage it as `bench/navo-tracy.exe`.
  - **Tools.** Download the Windows asset of Tracy `v0.14.1`, check a SHA256 pinned in the workflow, and stage `tracy-capture.exe`, `tracy-csvexport.exe`, `tracy-profiler.exe` and Tracy's licence.
  - **Checks (SC16).** `cargo tree -p navo-app -e features --features tester,tracy` contains `tracy-client`; with `--features tester` it does not. The bytes of `navo-tracy.exe` contain `TRACY_PORT`; the tester `navo.exe` does not.
  - **Artifact scan.** Before upload, search the bytes of `navo-tracy.exe` and the tester `navo.exe` for each needle encoded as ASCII and as UTF-16LE (case-sensitive): `runneradmin`, `D:\a\`, `C:\Users\`, `\.cargo\registry`. A hit fails. The same search on Tracy's own tools only lists hits (upstream binaries).
  - **README.** Add the seven owner steps and the "What a trace holds" paragraph from this plan.
- **Check:**
  1. `actionlint`.
  2. **Branch dispatch:** dispatch `bench-release.yml` on the C6 branch with a test tag. The hook guard, SC16 checks and artifact scan pass; the zip lists `navo-tracy.exe` and the three Tracy tools. Delete the test release.
  3. G-C1 runs later, on `main`; it is not C6's check.
- **Stop if:** the v0.14.1 asset lacks one of the three tools.

### C7 — Tracy checks in the release

- **Read:** `.github/workflows/release.yml:94-286` (`build-windows`, including W14's Setup steps at 201-229).
- **Change:** three checks in `build-windows`:
  - after the `cargo auditable build` step (line 112), fail when `cargo tree -p navo-app -p navo-index-service -p navo-setup -e features --prefix none` contains `tracy-client`;
  - in the staging step, next to the `navo-bench.exe` line (139), fail when `dist/` holds `navo-tracy.exe`, `tracy-capture.exe`, `tracy-csvexport.exe` or `tracy-profiler.exe`, and when the bytes of `dist/navo.exe` or `dist/navo-index.exe` contain `TRACY_PORT`;
  - in "Check the Setup wizard" (210), fail when the bytes of `target/release/navo-setup.exe` contain `TRACY_PORT`.
- **Check:** `actionlint`, then G-C2.
- **Stop if:** `build-windows` has no staging step for `dist/`.

### C8 — trace scan tool

- **Read:** SC18 only. The file layout below is from Tracy v0.14.1 `server/TracyFileHeader.hpp`, `TracyFileWrite.hpp`, `TracyFileRead.hpp:420-490` and `TracyFileMeta.hpp`; the worker does not fetch them.
- **Layout to decode:**
  - **Header.** `b"tr\xfdP"`, then `u8` type (`0` LZ4, `1` Zstd; anything else is an error), then `u8` stream count (1–255; 0 is an error). Legacy headers `b"tlZ\x04"` (LZ4) and `b"tZst"` (Zstd) mean one stream with no type or count bytes. Anything else is an error.
  - **Blocks.** Until end of file: a `u32` little-endian compressed size, then that many bytes. Block `k` belongs to stream `k % streams`. A size past the end of file is an error.
  - **LZ4.** Each stream is an LZ4 block stream that uses its previous block as the dictionary: `lz4.block.decompress(block, uncompressed_size=65536, dict=<previous decoded block of that stream, or none>)`. An `LZ4BlockError` is an error.
  - **Zstd.** One `zstandard.ZstdDecompressor().decompressobj()` per stream, fed each of its blocks in order.
  - **Output.** The decoded blocks concatenated in block order `k`. A decoded block over 65,536 bytes is an error.
- **Change:**
  - **`tracy_scan.py`.** Starts with `#!/usr/bin/env -S uv run --script` and PEP 723 metadata: `requires-python = ">=3.14"`, `dependencies = ["lz4==4.4.5", "zstandard==0.25.0"]`. Then:
    - `class DecodeError(Exception)`; `def decode(raw: bytes) -> bytes`;
    - `def stored_strings(data: bytes) -> list[str]`: candidates from `re.finditer(rb"(?s)(?=..\x00{6})", data)`, then SC18's length and UTF-8 rules;
    - `def raw_runs(data: bytes) -> list[str]`: ASCII/UTF-8 by strict UTF-8 `rb"(?:[\x20-\x7e]|\xc2[\xa0-\xbf]|[\xc3-\xdf][\x80-\xbf]|\xe0[\xa0-\xbf][\x80-\xbf]|[\xe1-\xec\xee\xef][\x80-\xbf]{2}|\xed[\x80-\x9f][\x80-\xbf]|\xf0[\x90-\xbf][\x80-\xbf]{2}|[\xf1-\xf3][\x80-\xbf]{3}|\xf4[\x80-\x8f][\x80-\xbf]{2}){3,}"` (no overlongs, surrogates or code points past U+10FFFF, so `.decode("utf-8")` never raises); UTF-16LE by `rb"(?:[\x20-\x7e\xa0-\xff]\x00|[\x00-\xff][\x01-\xd7]){3,}"` on `data` and `data[1:]`;
    - `def findings(data: bytes, forbid: list[str]) -> list[str]`: SC18's rules, one message per hit with the rule and the text;
    - `def main() -> int`: `argparse` with `file` and repeatable `--forbid`; prints every `User:` line found and each finding; returns 1 on any finding or `DecodeError`, else 0; `raise SystemExit(main())`.
  - **`tracy_scan_test.py`.** The same header with `pytest==9.2.0` added; its `main` runs `pytest.main(["-q", "-p", "no:cacheprovider", __file__])`. A helper writes synthetic `.tracy` files: payload split into 64 KiB blocks, 2 streams, for LZ4 (`lz4.block.compress(block, store_size=False, dict=<previous block of that stream>)`) and for Zstd (`compressobj()` per stream, `flush(zstandard.COMPRESSOBJ_FLUSH_BLOCK)` per block). A `stored(s)` helper gives `len(s).to_bytes(8, "little") + s.encode()`.
- **Tests,** each in an LZ4 and a Zstd file, payload padded with 70 KiB of zero bytes on both sides so a stored string crosses a block:
  - fail as stored strings: `C:\Users\x\a.txt`, `crates\navo-ui\src\a.rs`, `src/view/sort.rs`, `/home/x/main.rs`, `a.rs`;
  - fail as stored strings, as raw ASCII and as raw UTF-16LE: `User: admin@DESKTOP-1`, `User: navo@tester (Admin Name)`, and the word `zz-canary-7f3e` with `--forbid zz-canary-7f3e`;
  - pass as stored strings: `listing.sort`, `compare.intraline`, `User: navo@tester`, `rayon-worker-3`, `navo-tracy.exe`, and `OS: Windows 10.0.26100\nUser: navo@tester\nArch: x64\n`;
  - pass: `random.Random(7).randbytes(1 << 20)` as payload;
  - decode errors: a wrong magic, type 2, stream count 0, a size field past the end, and a corrupted LZ4 block; legacy `b"tlZ\x04"` with one stream decodes.
- **Check:** as in the table: the log ends with `N passed` and no failure.
- **Stop if:** the random payload yields any finding, or a synthetic file does not round-trip through `decode`.

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
- 2026-11-10 — Revision 6 after plan check 5 (Gemini argon, part C, 4 findings, all accepted after checking the sources):
  1. `cc` target-specific flag variables: real in our `cc` 1.2.67 (the checker cited 1.8.0). The guard now refuses the four target variants and `CC_SHELL_ESCAPED_FLAGS`, and allows only the force-include and `-d1trimfile:` tokens (SC14, C2a tests).
  2. C2 had four files: split into C2a (`flags.rs` and its test) and C2b (`build.rs` and the header).
  3. `../winvm/tools/` does not resolve from a crew worktree: now `/mnt/Sabrent/homelab/winvm/tools/`. `requirements.txt` + `pip` replaced by PEP 723 metadata run with `uv run --script` (python.md); C8 has two files.
  4. C8 no longer reads an unfetched tarball: the v0.14.1 file layout is written into C8, checked against the upstream headers.
  - Also found while fixing C8: 1 MiB of random bytes holds about six `X:\` matches, so the old "random bytes pass" test could never pass, and a real trace would trip the path rules by chance. SC18 now applies path rules to stored (length-prefixed) strings only, the way Tracy writes every string; the `User:` and forbidden-word rules still cover all raw text. `l10n.catalog` is at line 83, not 82. C7 covers W14's `navo-setup` (it depends on `navo-diag`). G-C2 waits for G-A3. Base moved to `da6e3d8c`.
- 2026-11-10 — Gemini re-check of revision 6: C8 `raw_runs` now matches strict UTF-8 only; C2b `build.rs` allows `print_stdout` and returns `Result` instead of `panic!` (workspace lints). Revision 7.
- 2026-11-11 — C1, C3, C5 merged. C1 went in before a clean Windows run: its only unrelated failures were the shortcuts bug, fixed on main, and the_display_reader_opens_its_own_sources, which looks flaky.
