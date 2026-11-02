# Navo plans

How Navo is built: in plans, each cut into atoms, each atom one worker run. Rules come from `~/.agents/rules/planning.md`, `code-quality.md` and `rust.md`; this file adds Navo's own (D26, D28).

## The plans, in order

Each plan ends in a gate. The next plan starts only after that gate passes.

| Plan | File | Delivers | Gate |
| --- | --- | --- | --- |
| P1 | [Workspace, gate and CI](2026-10-14-p1-workspace-gate-ci.md) | Eleven crates, `cargo xtask gate`, CI on Linux and Windows, a portable `.exe` per push | G1 |
| P2 | [GPUI spike](2026-10-14-p2-gpui-spike.md) | Design B on `gpui-kit` with 100k rows; measured frame time, memory, start-up, fuzzy matching, accessibility | G2: go or no-go for D2 |
| P3 | [Core model](2026-10-14-p3-core-model.md) | Commands, layout tree, state, `apply()`, TC selection, record and replay | G3 |
| P4 | [Storage](2026-10-14-p4-storage.md) | Atomic writes, settings, session snapshot, `navo.db`, `cache.db`, migrations | G4 |
| P5 | [Diagnostics](2026-10-14-p5-diagnostics.md) | Redacted logs, event ring, memory statistics, report, canary test | G5 |
| P6 | [Locations and file systems](2026-10-14-p6-locations-fs.md) | Locations and providers, fake file system with faults, hostile names, local listing, I/O queues, error model | G6 |
| P7 | [PoC 1: Browse](2026-10-14-p7-poc-browse.md) | Components and gallery; a window with two panes that browse real folders; session restore | G7: **PoC 1** |
| P8 | [PoC 2: Operate](2026-10-14-p8-poc-operate.md) | Plans, job journal, copy, move, Recycle Bin, Shift+Del, progress and cancel | G8: **PoC 2** |

P1 is done (G1, 2026-10-14). P2 is running. P2–P8 list their components, atoms and decided design; each plan's briefs get their exact `Read` line ranges when the previous gate passes, because the code they point at does not exist yet.

The rest of Alpha (workspaces, drive bar, context menu, Explorer clipboard and so on) is planned after PoC 2, from the roadmap.

## Tiers: which model runs an atom

| Tier | Model | Roles | Used for |
| --- | --- | --- | --- |
| **T1** | Opus 5.5, `medium` | `patcher`, `infra-builder` | Moves, exact edits, small config, scaffolds |
| **T2** | Opus 5.5, `medium` | `ui-builder`, `checker`, `infra-builder` (large) | UI work, medium implementation, writing the failing tests (Red atoms), CI |
| **T3** | Opus 5.5, `medium`; `high` for complex work | Claude `general-purpose` agent | Backend Rust outside the core model: store, fs, index, diag, xtask, wiring |
| **T4** | Opus 5.5, `high`: the orchestrator itself, or `ui-builder-high` for hard UI | orchestrator, `ui-builder-high` | The core and the hardest atoms: every `navo-core` Green atom, everything that can lose user data (`atomic_write`, settings, session, job journal, job engine, standard copy), the spike, `FileList`, the pane view, the I/O queue, hard refactors, cross-cutting changes |

- **Tiers as of 2026-10-15 (owner):** every editing agent runs on Opus 5.5 `medium`; complex work runs on Opus 5.5 `high`. Read-only `scout` and `researcher` run on Sonnet 5.5 `low` (`medium` for wider searches); `reviewer` runs on Opus `high`. No Haiku, no Sonnet for editing work, no gpt-6.1-sol, Codex or Gemini (D35). Every spawn passes `model` and `effort` explicitly.
- The tier follows the atom's difficulty, but every `navo-core` Green atom and every data-safety atom is T4. A one-line move elsewhere is T1.
- Red atoms for T4 components run on Opus `medium` (D34), and the orchestrator reviews their tests against the contract before the Green atom starts.
- Escalation follows `planning.md`: one retry with the failure output and one effort step more, then one tier up (T1 → T2 → T3 for backend or T4). Every escalation is logged in the plan.
- Red and Green atoms run in different sessions at different efforts (Red: a `checker` agent on Opus `medium`; Green: the orchestrator on Opus `high`), and the orchestrator reviews every Red test against the contract, so the tests are not shaped by the code that passes them.

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
