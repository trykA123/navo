# Stop Navo's Git children before a rename, delete or move of their folder

Status: ready (rev 2, after senior review) · Repo: `navo` · Branch: `git-stop` from `main` · Started: 2026-11-09

## What the user sees (human spec)

- Renaming, deleting (Recycle Bin or permanent) or moving a repository folder from Navo works while Git badges or Git reads run. The same holds for any folder above a repository or inside one. Today it can fail with "in use" when a `git status` happens to run in that repository.
- While a rename or a Recycle Bin delete runs, badges of the repositories inside keep their last state. About half a second after it ends, a repository that still exists is checked again. A permanent delete or a move to another drive removes files one by one, and `.git` may go before the folder does. A badge can then show Failed for a moment, until the folder is gone or the copy is checked.
- A background fetch, or a Pull or Push the user started, is never killed. Navo waits up to 3 s for it to end. If it is still running, the operation fails as today's "Locked", and the lock holder shows `git.exe`. Retry works once Git ends.
- Viewing or copying a file from a Git location in a repository that is being renamed or deleted fails as "Locked" too. Retry works.
- Operations Explorer performs still fail as today (see the non-goals).
- Nothing else changes: no new keys, settings or texts.

Sources: the brief (2026-11-09), the v0.4.0 fix `da1f10c9`, the senior review of rev 1, and the code at `33203a83`. Paths are under `crates/`.

## Revisions

| Rev | Date | Change | Source |
| --- | --- | --- | --- |
| 1 | 2026-11-09 | First draft, 7 atoms | architect |
| 2 | 2026-11-09 | The ticket lives in `GitTree` and drops when the tree is reaped (blocker 1). `run_process` spawns and stops through `GitTree` (major 1). Read tickets match on overlap, for submodules (major 2). The scanner split goes into `scan_retry.rs`; on Paused it still runs `plan.finish` and `forced_done` (major 3). Shell paths become a non-goal (major 4). One 3 s deadline (minor 1). Human spec reworded (minor 2). Case-folding note, the A5 control sentence and the navo-fs reader mapping (minors 3–5). Q1 and Q2 answered. Now 9 atoms | senior review, owner, coordinator |

## Why `--git-dir` alone cannot fix it (evidence)

- The v0.4.0 fix moved only the long-lived `cat-file` child to `--git-dir` plus the temp folder (`navo-git/src/cli/process.rs:102-111`, `cli/mod.rs:194-204`). Every other call still starts with `-C <repo>`:
  - plain reads through `CliGit::run` (`cli/mod.rs:154-163`): `rev-parse`, `for-each-ref`, `ls-tree`, `merge-base`, `config`, `ls-files`, `diff-*` (`changes.rs:17-45`, `sync_read.rs`, `sync_check.rs:120-135`);
  - status through `run_cancellable` (`cli/mod.rs:121-135`, `status.rs:66,85`);
  - `cat-file --filters` streams (`filters.rs:19-28`, `filtered_stream.rs:24-58`);
  - fetch (`remote.rs:57-68`) and Pull/Push (`sync_check.rs:148-159`).
- **Git moves into the work tree on its own.** Probe on 2026-11-09, Git 2.56: each command started from a temp folder with `--git-dir <repo>/.git --work-tree <repo>`, with a pager script (`git -p`, under `script`) that printed `readlink /proc/$PPID/cwd`.
  - `status`, `ls-files --others` and `check-attr` reported the repository root.
  - `rev-parse`, `for-each-ref`, `ls-tree`, `merge-base`, `config --list`, `diff --cached`, `diff-index --cached`, `ls-files -s` and `cat-file` reported the temp folder.
  - A clean filter run by `status` saw the same parent folder. Git's `setup_work_tree()` calls `chdir` for every command that needs a work tree. Git for Windows builds the same `setup.c`.
- So the status call, the one that runs most often, holds a current-folder handle on the repository root however Navo starts it. That handle is what blocks the rename. The `da1f10c9` message says so. `process_windows_tests.rs:44-89` shows that open handles inside `.git` do not block: `cat-file` holds the pack open and the rename still succeeds.
- Moving only the object and ref reads to `--git-dir` would leave the race open through status. It would also change how config includes resolve, and the D100e guard checks them with the same cwd (`cli/mod.rs:136-152`). Rejected.

## Shape

- **Appetite:** 1.5 days of agent runs, 9 atoms. The plan has more than the usual 3–6 atoms: the review fixes touch more components, and the three-file rule splits them. Work not done by then stops and needs a new decision.
- **Non-goals:**
  - The in-process gix reader, whose Rust handles allow delete and which has no cwd.
  - The user's own Git tool and terminal, and Git started by other programs.
  - Repositories reached through a junction, or through a different path spelling than the scanner's.
  - `cat-file` batch children: temp cwd, proven by `process_windows_tests.rs`.
  - A new error text.
  - **Shell paths (known gap, orchestrator decision).** Explorer performs these operations, so Navo cannot pause first: the shell context menu's Delete, Rename and Cut; a move by dragging out of Navo; a Cut from Navo pasted in Explorer. They can still meet "in use" while a status runs.
    - *Later:* a `stop_under` pause held for the duration of a move drag, taken off the UI thread. The UI thread never waits on it.
- **Alternatives that lost:**
  - *`--git-dir`/`--work-tree` for all calls.* Status still enters the root (see above).
  - *Poll less, or close children sooner.* That narrows the window but does not close it (brief).
  - *A pause inside each job runner.* Recovery, undo, restore, rename batch and retention all reach the disk through `LocalFs` (`job/run.rs:218-262`), so a hook there covers every caller once. `SlowFs` delegates to it (`slow.rs:121,191,221`).
  - *Kill fetch, Pull and Push too.* A killed fetch can leave `refs/...lock` files. A killed Pull can leave `index.lock` and a half-updated tree. D100g and R-G2 (`plans/2026-10-27-beta.md:1825,1865`) forbid changing a repository the user did not ask to change.

## Decisions

Final. Workers never reopen them; a worker that finds one wrong stops and reports.

- GF1 — **A registry of running Git children** lives in `navo-git/src/cli/running.rs`: one process-wide `static RUNNING: Running`.
  - Every child started with `-C <repo>` registers a `Ticket` before it spawns.
  - The ticket is attached to its `GitTree` (`cli/process_tree.rs`). It drops as soon as the tree is reaped, by `try_wait` seeing the exit or by `stop`, or when the tree drops. It never waits for the consumer of a stream.
  - Children started with `repo: None` (version probe, `--exec-path`, the batch child) do not register.
- GF2 — **Work kinds.**
  - `Work::Read` is every single command.
  - `Work::Write` is a session ticket. `fetch_with`, `pull_with` and `push_with` hold one for their whole run, besides the Read tickets of their inner commands.
  - A *session repository* is one with a live Write ticket. Its tickets are never stopped. They are waited for only when the path covers that repository.
- GF3 — **Matching.** A path `P` *covers* repository `R` when `R` equals `P` or lies inside it. `P` and `R` *overlap* when either covers the other.
  - A Read ticket matches on overlap. A Git child in a parent repository can sit inside its submodule folder, and status recurses into submodules.
  - A Write ticket matches only when `P` covers `R`.
  - Keys strip the `\\?\` and `\\?\UNC\` prefixes and compare components. On Windows the comparison ignores case with `to_lowercase` of each component; on Unix it is exact.
  - `to_lowercase` differs from NTFS's upcase table for a few letters (for example `ß`, the Turkish dotted I). The difference is harmless. A false match only stops a read early; a missed match falls back to today's behaviour for that path.
- GF4 — **`stop_under(path, wait) -> Result<GitPause, GitBusy>`**, under one mutex and a `Condvar`. One deadline, `start + wait`, bounds the whole call (steps 1 and 3 together).
  1. While a Write ticket covered by `P` exists, wait. At the deadline, return `GitBusy { repo }`; no pause is left behind.
  2. Still under the lock, install the pause. Set the stop flag of every Read ticket that overlaps `P` and is not in a session repository.
  3. Wait until none of those Read tickets remains. At the deadline, remove the pause and return `GitBusy`.
  4. Return the `GitPause`. Dropping it removes the pause and wakes waiters.

  While a pause on `P` lives, `register` fails with `GitError::Paused` before any spawn:
  - for a Read ticket whose repository overlaps `P` and is not a session repository;
  - for a Write ticket whose repository overlaps `P`. A refusal before the start changes nothing.

  The inner reads of a running Pull are never refused, so a Pull never stops halfway.
- GF5 — **Stopping a child.** Each `Ticket` owns a `Cancel` (`process_watch.rs:17`) as its stop flag. The spawner keeps a clone.
  - `run_process` checks the flag in its 5 ms wait loop.
  - The `Watchdog` (`process_watch.rs:75-218`) checks it at least every 5 ms through a new `stop` argument. `Control` and the timeout's `since` logic stay unchanged.
  - A stop kills the whole tree with `GitTree::stop`: the job object on Windows, the process group on Unix. That includes the `cmd\git.exe` shim's real git (`process.rs:39-55`) and the shells of `!` aliases.
  - A stopped call returns `Err(GitError::Paused)`, never `Cancelled`.
  - `GitTree::stop` becomes idempotent. After the tree is reaped, a second call signals nothing, so a reused PID is never killed.
- GF6 — **The hook.** `LocalFs` (`navo-fs/src/local/ops.rs`) calls `git_paused(location)` first in five methods and keeps the guard until the method returns:
  - `rename_atomic` (`:64`), `rename` (`:200`) and `rename_verified` (`:206`), on the source;
  - `remove_empty_folder` (`:249`) and `recycle` (`:253`).

  `GIT_STOP_WAIT` is 3 s (owner, Q1). `GitBusy` maps to `FsErrorKind::Locked` with `holders: lock_holders(location)`, so the existing Locked message and Retry apply. File-only methods are not hooked: a file is never a repository and holds none. The hook runs on every platform.
- GF7 — **The scanner rescans.**
  - `GitError::Paused` becomes `ScanError::Paused`.
  - On Paused, `finish` still calls `shared.forced_done` (when the listing is current) and `plan.finish`. It skips `project`, `expire_at`, publishing and caching, and sets `Timer::Retry` 500 ms later. When due, the timer calls `rescan_repo`.
  - A timer for a listing that has gone is dropped as today (`scan_plan.rs:106-112`).
  - The new code lives in `scan_retry.rs`. `scan_worker.rs` (386 counted lines) and `scan_plan.rs` (382) stay under 400.
  - Background fetch maps `Paused` to `Outcome::Cancelled`: no back-off. Pull and Push map it to `SyncFailure::Failed` through the existing wildcard (`pullpush.rs:135`); a refused session has changed nothing.
- GF8 — **`GitError::Paused`** is a new unit variant with the plain text "a file operation paused Git in this folder". The matches outside `navo-git` already have wildcards (`navo-fs/src/git/errors.rs:41`, `navo-app/src/git/source.rs:145`, `fetch_ports.rs:109`, `pullpush.rs:135`).
- GF9 — **Git-location readers.** Today a `Paused` would reach the user as Unknown. Two edits fix that:
  - `navo-fs/src/git/errors.rs:26-42` maps `GitError::Paused` to `FsErrorKind::Locked`. That covers a refused start, through `GitFs::fail`, `git/mod.rs:105-109`.
  - `copy_pump.rs:31-40` maps a read error that carries `GitError::Paused` to `Locked`. A stream stopped mid-read reaches the copy as an `io::Error` holding `Paused` (`filtered_stream.rs:194-201`).
  - `FinishRead::record_error` (`git/reading.rs:183-191`) already records the `GitProblem` for diagnostics.
  - The viewer or copy shows Locked, with Retry.

## Risks and rollback

- R1 — **A stopped Pull or fetch damages a repository.** Prevented by GF2 and GF4: session repositories are never stopped, and their inner reads are never refused. Tested in A1 and A4.
- R2 — **A file operation blocks for 3 s.** This happens only while a covered Write session runs. Hooked calls run on job workers. A5 stops if it finds a hooked call on the UI thread.
- R3 — **Deadlock.** `stop_under` never holds a lock across a spawn. `register` fails fast and never waits. A1 tests a ticket dropped from another thread while `stop_under` waits.
- R4 — **A stop that never ends.** One 3 s deadline. A Read ticket outliving it gives `GitBusy`, and the pause is removed (A1).
- R5 — **Churn.** Overlap matching stops a repository's status on every folder rename or removal inside it. Each stop costs one status rerun 500 ms later. Accepted; the manual VM run in Done when watches it.
- R6 — **A stray kill.** `GitTree::stop` is idempotent (GF5), so a reused PID is never signalled.
- Rollback: revert the squash commit. Nothing is stored and no formats change.

## Components

| Component | Layer | Contract (in → out) | New / change | Test |
| --- | --- | --- | --- | --- |
| `git-running` | lib (`navo-git`) | `register(&Path, Work) -> Result<Ticket, GitError>`; `stop_under(&Path, Duration) -> Result<GitPause, GitBusy>` | new | `cli/running_tests.rs` |
| `git-spawn` | adapter (`navo-git`) | `GitTree` carries the ticket; `run_process`, `run_cancellable`, `FilteredStream::start` register and honour the stop flag | change | `cli/process_tests.rs`, `tests/cli_process.rs`, `tests/cli_filters/paused.rs` |
| `git-sessions` | adapter (`navo-git`) | `fetch_with`, `pull_with`, `push_with` hold a Write ticket | change | `tests/cli_sync/paused.rs` |
| `fs-git-pause` | adapter (`navo-fs`) | `LocalFs` mutations → `stop_under` first | change | `local/tests/git_pause_windows.rs` |
| `fs-git-read` | adapter (`navo-fs`) | `GitError::Paused` → `FsErrorKind::Locked` | change | cases in `git/tests.rs`, `copy_pump` tests |
| `git-scan` | service (`navo-app`) | Paused → forced done, proof dropped, badge kept, retry in 500 ms | change | `git/scan_tests/paused.rs` |

## Atoms

Rules for every atom:
- Each new test fails first, and the report shows it.
- Check: `cargo xtask check -p <crate> > <wt>/../<atom>.log 2>&1; echo $?`, then `tail -n 40` of the log. Exit 0 passes. One build at a time.
- New test files are registered with `#[cfg(test)] #[path = "..."] mod ...;` in the file or test root named in the atom.
- Each new file starts with a header comment; each function has a doc comment.
- Lock poisoning uses `unwrap_or_else(PoisonError::into_inner)`, as in `process_watch.rs`.
- Every brief below names GF entries. Read those entries in this file first: they are part of the brief.

| Atom | Component | Files (`crates/`) | Role | Effort | After | Check |
| --- | --- | --- | --- | --- | --- | --- |
| A1 | `git-running` | `navo-git/src/cli/running.rs` (new), `cli/running_tests.rs` (new), `navo-git/src/error.rs`; `cli/mod.rs` (2 lines) | senior | high | — | `-p navo-git` |
| A2 | `git-spawn` | `navo-git/src/cli/process_tree.rs`, `cli/process.rs`, `cli/process_tests.rs` | senior | high | A1 | `-p navo-git`; `windows-quick.yml` |
| A3a | `git-spawn` | `cli/process_watch.rs`, `cli/process_cap.rs`, `tests/cli_process.rs`; `cli/filtered_stream.rs` (1 line) | senior | high | A2 | `-p navo-git`; `windows-quick.yml` |
| A3b | `git-spawn` | `cli/filtered_stream.rs`, `tests/cli_filters/paused.rs` (new; 3 lines in `tests/cli_filters.rs`) | api-builder | high | A3a | `-p navo-git` |
| A4 | `git-sessions` | `navo-git/src/cli/remote.rs`, `cli/sync.rs`, `tests/cli_sync/paused.rs` (new; 2 lines in `tests/cli_sync.rs`) | api-builder | medium | A1 | `-p navo-git` |
| A5 | `fs-git-pause` | `navo-fs/src/local/ops.rs`, `local/tests/git_pause_windows.rs` (new), `local/tests/mod.rs` (2 lines) | senior | medium | A3a | `-p navo-fs`; `windows-quick.yml` |
| A5p | `fs-git-read` | `navo-fs/src/git/errors.rs`, `local/copy_pump.rs`, `git/tests.rs` | patcher | medium | A1 | `-p navo-fs` |
| A6a | `git-scan` | `navo-app/src/git/scan.rs`, `source.rs`, `scan_listing.rs`, `fetch_ports.rs` (one line each) | patcher | medium | A1 | `-p navo-app` |
| A6b | `git-scan` | `navo-app/src/git/scan_retry.rs` (new), `scan_plan.rs`, `scan_worker.rs`, `scan_tests/paused.rs` (new); `scan.rs` and `scan_tests.rs` (2 lines each) | api-builder | high | A6a | `-p navo-app` |

Order: A1 first. Then A2 → A3a → A3b, in parallel with A4, A5p and A6a → A6b. A5 runs after A3a. Atoms in parallel use separate worktrees; builds still run one at a time. A6a touches four files with one line each, so it stays a patcher atom.

### A1 — the registry

- **Read:** `cli/process_watch.rs:1-35` (`Cancel`), `cli/mod.rs:1-50`, `error.rs:9-110,167-195`; GF1–GF4 and GF8.
- **Change:** new `running.rs`:

  ```rust
  pub(crate) enum Work { Read, Write }
  pub(crate) struct Ticket { id: u64, stop: Cancel, registry: &'static Running }  // Drop: remove entry, notify_all
  impl Ticket { pub(crate) fn stop_flag(&self) -> Cancel /* clone */ }
  #[must_use] pub struct GitPause { id: u64, registry: &'static Running }           // Drop: remove pause, notify_all
  #[derive(Debug, Clone, PartialEq, Eq)] pub struct GitBusy { pub repo: PathBuf }
  pub(crate) struct Running { state: Mutex<State>, changed: Condvar }            // const fn new()
  struct State { next: u64, entries: Vec<Entry>, pauses: Vec<(u64, Key)> }
  struct Entry { id: u64, repo: PathBuf, key: Key, work: Work, stop: Cancel }
  struct Key(Vec<String>);
  fn key(path: &Path) -> Key;
  fn covers(outer: &Key, inner: &Key) -> bool   // inner.0.starts_with(&outer.0)
  fn overlaps(a: &Key, b: &Key) -> bool         // covers(a, b) || covers(b, a)
  static RUNNING: Running = Running::new();
  pub(crate) fn register(repo: &Path, work: Work) -> Result<Ticket, GitError>
  pub fn stop_under(path: &Path, wait: Duration) -> Result<GitPause, GitBusy>
  ```

  - The free functions call the same-named methods on `RUNNING`. Tests use `Box::leak(Box::new(Running::new()))`.
  - `stop_under` follows GF4 exactly, with one `deadline = Instant::now() + wait` for both waits (`Condvar::wait_timeout` with the time left).
  - `register` follows the refusal rules under GF4.
  - "Session repository" means some entry with `work == Write` and an equal `key`.
  - In `error.rs`, add `Paused` and its `plain_text` arm.
  - In `cli/mod.rs`, add `mod running;` and `pub use running::{GitBusy, GitPause, stop_under};`.
- **Tests (`running_tests.rs`, each under 300 ms):**
  - `covers` and `overlaps`: equal; ancestor; a descendant overlaps but does not cover; `repo` does not cover `repo2`; a trailing separator. On Windows also case and the `\\?\` prefix.
  - `register` under a pause gives `Paused`, outside it gives `Ok`, and after the pause drops gives `Ok`.
  - A covered Read ticket gets its stop flag. When another thread drops it, `stop_under` returns `Ok`.
  - Submodule: a Read ticket on `/a/repo` is stopped by `stop_under("/a/repo/sub")`.
  - Submodule with a session: a Write ticket and a Read ticket on `/a/repo`, then `stop_under("/a/repo/sub")`. It returns `Ok` at once and stops nothing. During that pause, a Read `register` on `/a/repo` succeeds and a Write `register` on `/a/repo` gives `Paused`.
  - A Read ticket that is never dropped gives `GitBusy` at the deadline, and `register` works again afterwards.
  - A covered Write ticket is not stopped. If it drops before the deadline, the result is `Ok`, and only then are the Read tickets under it stopped. If it outlives the deadline, the result is `GitBusy`, no Read ticket is stopped and no pause remains.
  - One deadline: a Write that drops at 80 % of `wait`, followed by a Read that never drops, gives `GitBusy` by `wait` plus 50 ms, not later.
  - Tickets outside the path are untouched.
- **Stop if:** `Cancel` cannot be cloned and shared as described.

### A2 — `GitTree` carries the ticket; plain commands kill the whole tree

- **Read:** `cli/process_tree.rs` (all, 103 lines), `cli/process.rs:20-60,128-290`, `cli/process_tests.rs`, A1's `running.rs`; `tests/cli_process.rs:84-136` (pid files, `running`, `assert_reaped`) and `:176-240` (Windows: `native_sleeper`, `/proc/$!/winpid`); GF1 and GF5.
- **Change:**
  - `process_tree.rs`:
    - `GitTree` gains `ticket: Option<Ticket>`, `pub(super) fn attach(&mut self, ticket: Option<Ticket>)` and `reaped: bool`.
    - When `try_wait` returns `Some`, set `reaped` and drop the ticket.
    - `stop` returns `Ok(())` at once when `reaped`. Otherwise it stops as today, then sets `reaped` and drops the ticket.
  - `process.rs`:
    - `run_process` registers first, with `let ticket = spec.repo.map(|repo| running::register(repo, Work::Read)).transpose()?;`, and keeps `let stop = ticket.as_ref().map(Ticket::stop_flag);`.
    - `spawn` uses `GitTree::spawn(&mut command)` and `take_pipes` instead of `command.spawn()`, then `tree.attach(ticket)`.
    - `wait` takes the `GitTree` and `stop: Option<&Cancel>`. When the flag is set, it returns `Err(GitError::Paused)`.
    - The error arm (`:138-145`) and `stop` (`:271-274`) call `GitTree::stop`.
    - `resolve_git` and `version_of` (`repo: None`) behave as before, without a ticket.
- **Tests (`process_tests.rs`):**
  - Run `run_process` with `repo: Some(root)` and args `-c alias.navo-wait=!<body> navo-wait`. The body records the pids of the shell and of a background `sleep 20`, as at `tests/cli_process.rs:116` (Unix) and `:223` (Windows, winpid). Then `stop_under(root, 2 s)` returns `Ok` within 300 ms, the call returns `Paused`, and neither pid is running.
  - Under a live pause, `run_process` returns `Paused` and spawns nothing (no pid file appears).
  - A run with `repo: None` registers nothing: `stop_under` returns at once.
- **Check:** `-p navo-git`, then `windows-quick.yml`.
- **Stop if:** a caller of `run_process` needs `Cancelled` rather than `Paused`; or `GitTree::spawn` cannot take the `Command` that `base_command` builds.

### A3a — cancellable runs register; the watchdog honours the stop flag

- **Read:** `cli/process_watch.rs:38-218`, `cli/process_cap.rs:47-190`, `cli/filtered_stream.rs:50-57`, `tests/cli_process.rs:15-140`, A2's `GitTree::attach`; GF5.
- **Change:**
  - `Watchdog::start` gains a last argument, `stop: Option<Cancel>`, stored in `Activity`.
  - In `watch`, a set `stop` gives reason `Cancelled` and runs `stop_shared`. While `stop` is `Some`, every wait lasts at most 5 ms, also when `since` is `None`. `since` and `Control` behave as today.
  - `filtered_stream.rs:52-57` passes `None` (one line; A3b wires it).
  - `run_cancellable` registers `Work::Read` when `spec.repo` is `Some`, before spawning. It keeps the flag clone in `Capture` as `stop: Option<Cancel>`, attaches the ticket to the `GitTree` and passes the flag to `Watchdog::start`.
  - In `finish`, a set flag returns `Err(GitError::Paused)` after the readers are joined.
- **Tests (`tests/cli_process.rs`, with the helpers at `:19-36`):** run `run_cancellable` on the alias body of `:116` (`:223` on Windows). `stop_under(repo, 2 s)` returns `Ok` within 300 ms, the call returns `Err(GitError::Paused)`, the parent is reaped (`assert_reaped`) and the child is not running.
- **Check:** `-p navo-git`, then `windows-quick.yml`.
- **Stop if:** a caller outside `navo-git` matches on `ProcessOutcome` in a way `Paused` breaks.

### A3b — filter streams register

- **Read:** `cli/filtered_stream.rs` (all), `cli/filters.rs:13-90`, `tests/cli_filters.rs:1-40`, `tests/cli_filters/streaming.rs` (fixture idiom); GF1 and GF5.
- **Change:**
  - `FilteredStream::start` registers `Work::Read` when `spec.repo` is `Some`, attaches the ticket to the `GitTree` and passes the flag to `Watchdog::start`. It keeps the flag clone as `stop: Option<Cancel>`.
  - When the flag is set, `read` and `complete` give `GitError::Paused` (wrapped by `stream_error` for readers).
  - The ticket never lives in `FilteredStream` itself: it drops with the reaped tree.
- **Tests (`tests/cli_filters/paused.rs`, `#[cfg(unix)]` like `streaming.rs`):**
  - A smudge filter prints `hello` and then sleeps 20 s. `read_blob_worktree_form` returns once the prefix is read (`filters.rs:69-88`).
  - Hold the returned blob without reading it. `stop_under(repo, 2 s)` returns `Ok` within 300 ms.
  - Reading the blob then fails with an `io::Error` whose inner error is `GitError::Paused`.
- **Stop if:** the watchdog's timeout would start counting before the first read.

### A4 — Write sessions

- **Read:** `cli/remote.rs:27-75`, `cli/sync.rs:127-265`, `tests/cli_sync.rs`, `tests/cli_sync/pull.rs` (fixtures); GF2 and GF4.
- **Change:** the first line of `fetch_with`, `pull_with` and `push_with` is `let _session = running::register(&self.repo, Work::Write)?;`.
- **Tests (`tests/cli_sync/paused.rs`):**
  - Under `stop_under(repo, 100 ms)`, `fetch_with`, `pull_with` and `push_with` each return `Err(GitError::Paused)`. HEAD, the index's bytes, `FETCH_HEAD` and the remote ref stay unchanged.
  - Under `stop_under(repo/sub, 100 ms)` the same three calls also return `Paused`, by the overlap rule for Write `register`.
- **Stop if:** a Pull or Push reaches Git through a path that does not start in these three functions.

### A5 — the file-operation hook and the Windows proof

- **Read:** `navo-fs/src/local/ops.rs:1-70,185-262`, `local/mod.rs:257-262,355-385`, `local/tests/mod.rs`, `local/tests/ops.rs:1-60` (setup idiom), `navo-git/src/cli/process_windows_tests.rs`; GF6.
- **Change:**
  - In `ops.rs`, add `const GIT_STOP_WAIT: Duration = Duration::from_secs(3);` and `fn git_paused(location: &Location) -> Result<GitPause, FsError>`.
  - `git_paused` calls `navo_git::cli::stop_under(&os_path(location)?, GIT_STOP_WAIT)`. It maps `GitBusy` to `FsError { kind: FsErrorKind::Locked, location: Some(location.clone()), os_code: None, holders: lock_holders(location) }`.
  - The first line of the five methods in GF6 is `let _git = git_paused(source_or_location)?;`.
- **Test (`git_pause_windows.rs`, `#[cfg(windows)]`, in the style of `process_windows_tests.rs`):**
  1. Initialise `<tmp>/repo` with `git.exe`. Set `alias.navo-wait` to `!echo started > ../started.txt; sleep 10`.
  2. Run `CliGit::run_cancellable(&["navo-wait"], ...)` on a thread. Wait for `<tmp>/started.txt`.
  3. Control: assert that `std::fs::rename(repo, moved)` fails. The child runs with `-C <repo>`, so its current-folder handle is the same handle `status` holds after its own `chdir` into the root. The control therefore reproduces the status race.
  4. Assert that `LocalFs::rename` of the same `Location`s succeeds, and that the thread returns `Err(GitError::Paused)` within 1 s.
  5. Repeat steps 1–4 with `recycle` in place of `rename`, against a second repository.
  6. Rename back and clean up.

  A Unix twin without step 3 runs on the local gate.
- **Check:** `-p navo-fs`, then `windows-quick.yml` on the branch. Both test names show as passed in the junit artifact.
- **Stop if:** any of the five methods is reached from the GPUI thread (search the callers of `rename` and `recycle` in `navo-ui`); or step 3 succeeds on the VM, which would mean the test does not reproduce the bug.

### A5p — Git-location reads report Locked

- **Read:** `navo-fs/src/git/errors.rs:20-45`, `local/copy_pump.rs:16-45`, `git/tests.rs` (find the tests of `errors::git`), `git/mod.rs:100-110`; GF9.
- **Change:**
  - `errors.rs`: add the arm `GitError::Paused => FsErrorKind::Locked` before the wildcard.
  - `copy_pump.rs`: add `Some(navo_git::GitError::Paused) => error(FsErrorKind::Locked, source),` after the `Timeout` arm (`:35`).
- **Tests:**
  - In `git/tests.rs`: `errors::git(GitError::Paused, &loc).kind == Locked`.
  - In the `copy_pump` tests at `:88`, or a `#[cfg(test)]` block there if none exists: `PumpError::Read(io::Error::other(GitError::Paused)).located(..)` gives `Locked`.
- **Stop if:** either match differs from the lines above.

### A6a — `Paused` reaches the scanner and the fetch

- **Read:** `navo-app/src/git/scan.rs:55-72`, `source.rs:139-147`, `scan_listing.rs:230-241`, `fetch_ports.rs:95-110`.
- **Change:** four edits.
  - `scan.rs`: add `#[error("repository status paused for a file operation")] Paused`.
  - `source.rs`: map `GitError::Paused => ScanError::Paused`.
  - `scan_listing.rs:239`: `ScanError::Failed | ScanError::Cancelled | ScanError::Paused => BadgeReason::Failed`. A6b keeps Paused from reaching it.
  - `fetch_ports.rs:98`: `Err(GitError::Cancelled | GitError::Paused) => Outcome::Cancelled`.
- **Check:** `-p navo-app`. One new case in the `fetch_ports` tests: `Paused` gives `Cancelled`.
- **Stop if:** any line differs from the ranges above.

### A6b — on Paused: settle, keep the badge, retry in 500 ms

- **Read:** `scan.rs:1-30` (module list), `scan_plan.rs:35-115,225-280,360-375`, `scan_worker.rs:146-226,305-380`, `scan_queue.rs:243-340` (`rescan_repo`, `forcing`, `forced_done`, `is_current`), `scan_tests.rs:1-80`, `scan_tests/results.rs` and `scan_tests/skip_flow.rs` (harness and proofs); GF7.
- **Change:**
  - New `scan_retry.rs`, registered in `scan.rs` as `#[path = "scan_retry.rs"] mod retry;`. It holds:
    - `pub(super) const PAUSE_RETRY: Duration = Duration::from_millis(500);`
    - `pub(super) fn settle_paused(shared: &Mutex<Shared>, plan: &mut Plan, done: Done, now: Instant)`. It locks `shared` and, if `is_current(&listing)`, calls `forced_done(&listing, &root_key(&root))`. After the lock, it calls `plan.finish(&root, check, &result)` (which drops the proof on `Err`) and `plan.retry_at(&listing, &root, now + PAUSE_RETRY)`. It never publishes, projects or caches.
    - `pub(super) fn retry_now(shared: &Mutex<Shared>, listing: &Listing, root: PathBuf)`, which calls `rescan_repo(&listing.folder, root)`.
  - `scan_plan.rs`: add `Timer::Retry { listing: Arc<Listing>, root: PathBuf }` and its `listing()` arm. Add `pub(super) fn retry_at(&mut self, listing: &Arc<Listing>, root: &Path, at: Instant)`, shaped like `expire_at` (`:365`).
  - `scan_worker.rs`:
    - The first statement of `finish` is `if matches!(done.result, Err(ScanError::Paused)) { retry::settle_paused(&self.shared.0, &mut self.plan, done, self.ports.probe.now()); return; }`.
    - `due` gets the arm `Timer::Retry { listing, root } => { retry::retry_now(&self.shared.0, &listing, root); None }`.
    - Make `Done` `pub(super)` if `scan_retry.rs` needs it.
  - Afterwards `scan_worker.rs` and `scan_plan.rs` each stay at or below 400 non-blank, non-comment lines; measure with `grep -cvE '^\s*$|^\s*//'`.
- **Tests (`scan_tests/paused.rs`):** the fake source returns `Paused` once, then a clean status.
  - The published badges keep their previous state, with no `Failed` reason.
  - A Recheck-forced root is no longer listed by `forcing` after the Paused result.
  - A root with a kept proof (set up as in `skip_flow.rs`) runs a real status on the retry instead of skipping.
  - No second call before 500 ms on the fake clock; one after it, and the clean badge appears.
  - A forgotten listing gets no retry.
- **Stop if:** `rescan_repo` cannot be called from the worker thread without a lock it already holds; or either file passes 400 counted lines.

## Done when

- The A5 Windows test passes on the VM. A rename and a recycle of a repository with a running short-lived Git child succeed through `LocalFs`, and the plain `fs::rename` control fails.
- The A2 and A3a tests show that no descendant survives a stop, on Linux and on the VM.
- `cargo xtask check` passes on `navo-git`, `navo-fs` and `navo-app`, then the full gate once on the branch.
- `reviewer-senior` reviews the branch (data safety: session repositories are never stopped), and Sol reviews the diff.
- A manual VM run: open a folder of 10 repositories, one with a submodule. Rename one repository 20 times in a row while badges refresh, then rename the submodule folder. No "in use" error appears, and every badge settles.

## Open questions (owner: Eru, before A5 runs)

- Q1 — Answered 2026-11-09 (owner): a file operation waits 3 s for a running fetch, Pull or Push, then fails as Locked.
- Q2 — Answered 2026-11-09 (owner): a background fetch is waited for, never killed (a killed fetch can leave `refs/remotes/...lock` files).

## Log

- 2026-11-09 — Plan drafted (architect). Probe: Git 2.56 chdirs into the work tree for status, `ls-files --others` and `check-attr` even with `--git-dir`/`--work-tree` from a temp folder.
- 2026-11-09 — Rev 2 (architect) applies the senior review ("fix first"): blocker 1, majors 1–4 and minors 1–5. The owner answered Q1 (3 s) and Q2 (never kill a fetch). Atoms grew from 7 to 9 (A3 split, A5p added). Status: ready, pending a plan check.
