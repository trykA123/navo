# Navo plans

How Navo is built: in plans, each cut into atoms, each atom one worker run. Rules come from `~/.agents/rules/planning.md`, `code-quality.md` and `rust.md`; this file adds Navo's own (D26, D28).

## Open plans

Finished plans were removed on 2026-11-04; they stay in git history. Plans still open:

**In progress or with open atoms**

- [2026-10-16-p17-context-menu](2026-10-16-p17-context-menu.md)
- [2026-10-16-p19-new-look](2026-10-16-p19-new-look.md)
- [2026-10-19-p29-crash-report](2026-10-19-p29-crash-report.md)
- [2026-10-19-p30-copy-auto](2026-10-19-p30-copy-auto.md)
- [2026-10-19-p31-dpi-placement](2026-10-19-p31-dpi-placement.md)
- [2026-10-20-p39-activity-strip](2026-10-20-p39-activity-strip.md)
- [2026-10-20-p42-compare](2026-10-20-p42-compare.md)
- [2026-10-27-beta](2026-10-27-beta.md)
- [2026-10-28-speed](2026-10-28-speed.md)
- [2026-10-29-speed-atoms](2026-10-29-speed-atoms.md)
- [2026-11-04-keys-and-mouse](2026-11-04-keys-and-mouse.md)
- [2026-11-05-index-layout](2026-11-05-index-layout.md)

**Parked**

- [2026-10-19-p28-real-pc-startup](2026-10-19-p28-real-pc-startup.md)
- [2026-10-19-p32-fluent-strings](2026-10-19-p32-fluent-strings.md)

**Older plans with open atoms to triage**

- [2026-10-14-p7-poc-browse](2026-10-14-p7-poc-browse.md)
- [2026-10-14-p8-poc-operate](2026-10-14-p8-poc-operate.md)
- [2026-10-15-audit](2026-10-15-audit.md)
- [2026-10-15-p11-select-sort](2026-10-15-p11-select-sort.md)
- [2026-10-15-p15-viewer](2026-10-15-p15-viewer.md)
- [2026-10-15-p16-automotive](2026-10-15-p16-automotive.md)
- [2026-10-16-p18-listing-cache](2026-10-16-p18-listing-cache.md)
- [2026-10-17-p22-live-settings-file](2026-10-17-p22-live-settings-file.md)
- [2026-10-18-delete-dialog-enter](2026-10-18-delete-dialog-enter.md)
- [2026-10-19-p33-view-parts](2026-10-19-p33-view-parts.md)
- [2026-10-20-p36-multi-select-drag-drop](2026-10-20-p36-multi-select-drag-drop.md)
- [2026-10-20-p40-command-center](2026-10-20-p40-command-center.md)
- [2026-10-20-p41-multi-rename](2026-10-20-p41-multi-rename.md)
- [2026-10-23-alpha-backend-continuation](2026-10-23-alpha-backend-continuation.md)
- [2026-10-24-backend-reboot-handoff](2026-10-24-backend-reboot-handoff.md)
- [2026-10-25-backend-review-index](2026-10-25-backend-review-index.md)
- [2026-10-25-opus-backend-fixes](2026-10-25-opus-backend-fixes.md)
- [2026-10-30-mimalloc-ab](2026-10-30-mimalloc-ab.md)

## Model tiers

Each atom names its role and effort. The orchestrator's role table decides the model; the old Opus-only tier table (2026-10-15) no longer applies.

## No skipping

1. **Plans run in order.** A plan starts when the previous gate is done: every atom done, `cargo xtask gate` green, CI green on Linux and Windows, `reviewer` approved, merged to `main`.
2. **An atom starts only when every atom in its `After` column is done.** Atoms without a shared file or dependency may run in parallel, each in its own worktree.
3. **Nothing is deferred or skipped.** A failed atom blocks everything after it until it passes. The gate lists every atom of its plan, so a plan cannot close with a hole in it.
4. **A big atom is split in place** before it runs: A6 becomes A6a, A6b, A6c. Atoms that depended on A6 now depend on A6c. The split is logged.
5. **Statuses:** `blocked` (waiting on `After`), `ready`, `running`, `done`, `failed`, `split`.
6. **Every gate leaves `main` shippable:** the quality gate passes, the `.exe` builds, and the plan's "Done when" items are checked for real.

## Test-driven

- **Every component with behaviour gets a Red atom, then a Green atom.**
  - **Red** (`checker`, T2): writes the public API as stubs (`todo!()`) and the tests from the contract. The check is that the tests compile and fail for the expected reason.
  - **Green** (T1–T4 by difficulty): makes those tests pass without changing them. A Green worker that thinks a test is wrong stops and reports; only the orchestrator changes a Red test.
- Kinds of test, by layer:

| Kind | Tool | Where |
| --- | --- | --- |
| Unit | `cargo nextest` | `tests.rs` beside the module |
| Property | `proptest` | Pure logic: layout tree, selection, redaction, plans |
| Snapshot | `insta` | Serialised formats: commands, settings, session, reports |
| Integration | fake file system with fault injection | `crates/<crate>/tests/` |
| Migration | fixtures of every old format | `crates/navo-store/tests/fixtures/` |
| UI | `gpui-kit` headless tests | `crates/navo-ui/tests/` |
| Windows | the same tests on `windows-latest` | CI job `test-windows` |
| Soak, start-up, size | `dhat`, timing spans, file size | Nightly and per-push CI jobs |

- **Coverage floors**, enforced by `cargo xtask coverage` from `cargo llvm-cov`: `navo-core` 90 %, `navo-store`, `navo-diag`, `navo-cache`, `navo-index` 85 %, `navo-fs` 80 %, `navo-ui` 60 %. A floor never goes down without a decision.
- Between a Red and its Green atom the plan branch is red on purpose; only gates must be green.

## Size limits, with room where it is earned

From `code-quality.md`, counted as non-blank, non-comment lines and enforced by `cargo xtask size` and clippy:

| Unit | Aim | Fails the gate over | Enforced by |
| --- | --- | --- | --- |
| File | 300 | 400 | `xtask size` |
| Function | 30 | 50 | `clippy::too_many_lines` |
| Parameters | 3 | 4 | `clippy::too_many_arguments` |
| Nesting | 2 | 3 | `clippy::excessive_nesting` |

- **Exceptions** are allowed for files whose job really is one large unit: a state machine, a parser, a lookup table, a Windows API binding. Each is listed in `size-exceptions.toml` with its own limit and a reason, and `reviewer` must accept it.
- **No exception goes above 800 lines.** A file over 800 lines is split, without discussion. That keeps the room for real needs and rules out 5,000-line monoliths.
- Tests (`tests.rs`, `tests/`), fixtures and generated code are exempt.

## Latest versions

- Every tool and dependency starts at its **latest stable release** when it is added: Rust 1.99.0 (current stable), `gpui-kit` 0.7.3, and the latest release of every crate in the architecture's library table. Exact versions are pinned in `Cargo.lock` and `rust-toolchain.toml` and recorded in `docs/dev-tools.md`.
- Every dependency comes from its official source and passes `cargo deny` (licences, advisories, sources) before it lands.
- **Libraries enter with their feature (D88, 2026-10-29).** A library from the map in [the speed plan](2026-10-28-speed.md) comes in only when its feature is built, pinned, and only after a short test or measurement shows it helps. GPL, AGPL and LGPL crates never enter.
- A monthly update atom (T1) runs `cargo update`, bumps the toolchain if a new stable is out, runs the gate and logs what changed. Major version bumps get their own atom.

## Repository layout

```text
navo/
├── Cargo.toml              workspace, shared lints, release profile
├── rust-toolchain.toml     pinned toolchain
├── clippy.toml             size thresholds
├── deny.toml               licences, advisories, sources
├── size-exceptions.toml    files allowed above 400 lines, with reasons
├── .config/nextest.toml
├── .github/workflows/      release.yml (manual: Windows gate, build, pre-release), checks.yml and bench.yml (manual)
├── crates/
│   ├── navo-core/         pure model: command/, layout/, state/, selection/, location/, error/, replay/
│   ├── navo-store/        atomic_write, settings/, session/, db/
│   ├── navo-fs/           provider, fake/, local/ (windows.rs, unix.rs), jobs/, io_queue/
│   ├── navo-cache/        bounded caches
│   ├── navo-index/        matching, folder search, global index
│   ├── navo-shell/        Windows shell integration
│   ├── navo-term/         terminal providers
│   ├── navo-plugins/      module host
│   ├── navo-diag/         redaction, logs, statistics, reports
│   ├── navo-ui/           tokens/, primitives/, components/, views/
│   ├── navo-ui-gallery/   binary: every component in every state
│   └── navo-app/          the binary
├── xtask/                  cargo xtask gate | size | l10n | coverage | binsize
├── spikes/                 throwaway experiments, outside the default build
├── docs/  design/  plans/
```

- **Folders by domain, not by kind:** `layout/`, `session/`, never `structs.rs` or `utils.rs`.
- **A module folder** has a small `mod.rs` (the public API and nothing else), one file per job, and `tests.rs`.
- Platform code sits in `windows.rs` and `unix.rs` behind one trait; callers never use `cfg` themselves.
- `pub` only what another crate uses; everything else is `pub(crate)` or private.
