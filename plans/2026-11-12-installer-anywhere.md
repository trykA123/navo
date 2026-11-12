# Installer: install anywhere, Program Files included

Status: ready · Repo: `/mnt/Sabrent/homelab/navo` · Branch: `plan-install-anywhere` · Base: `origin/main@c18314c8` (`c18314c855ceb5741325078f3048df05a1e08bca`) · Started: 2026-11-12 · Architect: Turgon

Not in scope: an install for all users of the PC, an MSI, moving an existing install, and code signing.

This plan extends the installer wizard plans (`plans/2026-11-09-installer-wizard.md`, IW0–IW24, and
`plans/2026-11-09-installer-wizard-ui.md`, S1–S16b and IW-D16). Its decisions take the next free ids,
IW25–IW31. The project record is D116 (the last one at the base is D115, `docs/decisions.md:117`).

## What the user can do (human spec)

The owner picked `C:\Program Files (x86)\Navo` in the v0.7.0 wizard and got "Setup cannot write to this
folder. Choose a folder you can write to, or fix its permissions." The owner's rule (2026-11-12) is:
"I want it to be installable wherever I want."

After this plan:

1. The user picks any local folder, Program Files and Program Files (x86) included, by its long or short
   name (`C:\PROGRA~2\Navo`), typed or through Browse.
2. When Setup cannot write there, the Folder page says so before Install:
   "Windows will ask for administrator rights once, to create this folder. Updates and uninstall then need none."
3. On Install, Windows shows one UAC prompt. An administrator approves with Yes. A standard user types an
   administrator's password. Navo is then installed for the user who ran Setup, with that user's
   shortcuts and Apps entry.
4. Navo's own updates install without a prompt. Uninstall in Settings → Apps needs no prompt either,
   apart from the existing one that removes the index service.
5. If the prompt is refused, nothing is created and the page says what to do next.
6. A few folders stay refused, each with its own text: the Windows and system folders, Common Files,
   `WindowsApps`, the index service's own `C:\Program Files\Navo`, another user's profile, a drive root,
   network and OneDrive paths, and the folders that were refused before.
7. Silent mode (`--silent --installto <folder>`) works the same way. The prompt appears there too; a
   refused prompt ends with exit 3.
8. Navo installed per user by v0.7.0 stays where it is. To move it, the user removes it in Settings →
   Apps (settings stay) and runs Setup again. The Welcome page says so.

Keys: no new keys. The UAC prompt is Windows' own (Alt+Y, Esc). While the prompt is open, the wizard
shows the Installing page, with Close and Back off (IW5).

Atom 0 ships first, before any of this: it replaces the wrong text above with an honest one,
"Program Files needs administrator rights, and this version of Setup cannot ask for them yet."

## Decided for the owner

The owner delegated every open choice (2026-11-12). These would have been the questions, each with
the answer taken and why. None is open.

1. **Who gets Navo in Program Files: the user who installs it, or every user of the PC?** Decided: the
   user who installs it (IW26). Velopack writes only the HKCU Apps entry and that user's shortcuts; an
   all-users install needs an MSI or HKLM writes of our own.
2. **Does every update ask for administrator rights?** Decided: no (IW27). Setup gives the installing
   user write access to the new folder. The cost: that user's programs can change Navo's files there,
   as they can in any per-user folder today (R1).
3. **Is an existing v0.7.0 install moved?** Decided: no (IW30). The user uninstalls and reinstalls;
   settings stay.
4. **Does silent mode prompt, or refuse?** Decided: it prompts, so scripts get the same behaviour as the
   window. A refused prompt is exit 3.
5. **May the root be `C:\Program Files\Navo`?** Decided: no. The index service owns it (IW28). The text
   suggests `C:\Program Files\Navo App`. `C:\Program Files (x86)\Navo` is free.

## Goal

Any local folder the user picks installs, including Program Files and Program Files (x86). Setup asks for
administrator rights once, only to create the folder. Updates and uninstall then need no prompt, and
the install belongs to the user who ran Setup.

## Appetite

About 19 atoms: Atom 0 (one code atom plus its VM run) ships alone first; the rest follows in order.

## Alternatives considered

| Option | Why not |
| --- | --- |
| Relaunch the whole wizard elevated, then run Setup.exe from it | W1 P9: with an administrator's password, the Apps entry and shortcuts go to that administrator's profile, not the user's. W1 P8: every update raises a prompt, and `%LocalAppData%\NavoApp` is left after uninstall (also velopack #989). GPUI and Navo's start would run elevated. |
| A true per-machine install (folder writable by admins only) | The same update and uninstall prompts as above (P8), plus a stale package in the root. |
| Velopack MSI with `--instLocation PerMachine` | A second release artefact and an HKLM entry; per-machine lifecycle hooks run unelevated (velopack #1040), and the fallback folder stays (#989). |
| An update broker in the index service (LocalSystem) | A new privileged surface that installs code for any caller. Too large for this need. |
| Prompt on every update, keep the root admin-only | One prompt at every Navo start after a staged update (`navo-app/src/updater/apply.rs:68` `at_start`), and the P8 leftovers. |

## Decisions

Final. Workers never reopen them; a worker that finds one wrong stops and reports.

- **IW25 — Elevation model: a narrow helper creates the folder; everything else stays unelevated.**
  Reason: only the folder creation needs rights. Setup.exe, the HKCU entry, shortcuts and Navo must run
  as the user (W1 P9).
  - The wizard keeps `asInvoker` and still refuses to run elevated (IW13: the "Started as
    administrator" page, silent exit 7).
  - When the nearest existing folder of a fresh root is not writable (`Facts.writable == false`), the
    wizard starts its own exe through `ShellExecuteExW` with the verb `runas`. The arguments are
    `--elevated-create --root "<root>" --for <user SID> --parent <volume>:<id>`. The working folder is
    System32. This is the pattern of `navo-index-service/src/elevation/windows.rs:34` (`launch`) and
    `:70` (`wait_for_child`).
  - The helper creates only the missing folders, each with the descriptor
    `O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1301bf;;;<user SID>)`. The user gets Modify
    (`0x1301bf`), without `WRITE_DAC` and `WRITE_OWNER`. Then it exits. It writes no file, log or
    registry value.
  - The wizard then adopts the new folders (checks owner, DACL, identity and emptiness), holds them like
    `Pin::fresh` does, and runs Setup.exe unelevated as before.
  - IW0 is extended: for the helper only, an adversary running as the invoking user is in scope. The
    helper re-checks everything itself and creates folders relative to a held handle, so it cannot be
    redirected by a junction or a rename.
- **IW26 — Scope: one user, even in Program Files.** Reason: Velopack's Setup.exe writes HKCU
  `Uninstall\NavoApp` and the user's own links (W1 P8a, P9). Other users of the PC can see the folder
  and run Navo from it, but get no Apps entry or shortcut. Supersedes IW18 in part: "per user" now means
  the install's owner, not its folder.
- **IW27 — Updates: no prompt.** Reason: the root is writable by its user, so Velopack's locator keeps
  `packages\` in the root and never switches to the `%LocalAppData%\NavoApp` fallback
  (`velopack-1.2.161/src/locator.rs:133-184`). IW8b's writable check for upgrades and S15 stay: a root
  the user cannot write is still refused with exit 3 and no prompt.
- **IW28 — Folder rules.** Reason: "wherever I want", minus the folders where an install breaks Windows,
  the index service or another user.
  - Allowed: Program Files and Program Files (x86) by identity (so short names match), any new folder
    directly under a drive root, and any other local folder outside the refused sets, through the
    helper when it is not writable.
  - Still refused, as today: a drive root itself, Windows, System, ProgramData, ProgramFilesCommon,
    Navo's data and config folders, the legacy folders, OneDrive, Startup, the never-equal user folders,
    the fresh-only `%LocalAppData%\NavoApp`, existing folders, reparse chains, `subst`, network and
    device paths, `% ^ & !`, reserved names and paths over 150 units (IW6, IW7, IW8a).
  - Newly refused: ProgramFilesCommonX86 and `%ProgramFiles%\WindowsApps` (never inside).
  - `%ProgramFiles%\Navo` and `%ProgramFiles%\Ferrite` (the index service folders,
    `navo-index-service/src/install_plan.rs:14-26`, `change_lock/file.rs:40`) give `ServiceFolder`.
  - A folder Setup cannot write inside `FOLDERID_UserProfiles` gives `OtherUserFolder`. The helper never
    creates there.
  - The refused-set checks run before the writable check (`placement.rs:144-168` today has the
    writable check first; that is why the owner saw `NotWritable`).
  - Texts (message, then fix):
    - `NeedsAdministrator` (Atom 0 only; removed by E10): "Program Files needs administrator rights, and
      this version of Setup cannot ask for them yet." / "Choose a folder such as C:\Navo, or keep the
      default folder."
    - `ServiceFolder`: "Navo's index service uses this folder." / "Choose another name, such as
      C:\Program Files\Navo App."
    - `OtherUserFolder`: "This folder belongs to another user of this PC." / "Choose a folder in your
      own user folder, or one outside C:\Users, such as C:\Navo."
    - `AdminDeclined`: "Windows did not give Setup administrator rights, so the folder was not
      created." / "Choose Install again and approve the Windows prompt, or choose a folder you can write
      to, such as C:\Navo."
    - `AdminFailed`: "Setup could not create this folder with administrator rights." / "Choose another
      folder, such as C:\Navo."
    - `FolderWarning::Administrator`: "Windows will ask for administrator rights once, to create this
      folder. Updates and uninstall then need none."
    - `setup-welcome-blurb`: "Setup puts Navo in a folder you choose. For a folder such as Program
      Files, Windows asks for administrator rights once."
  - All folder problems exit 3 in silent mode (IW17 unchanged).
- **IW29 — Uninstall: unelevated, by the installing user.** Reason: the user has Modify on the root,
  which includes `DELETE`, so Velopack's `rmdir /s /q` removes it. Settings → Apps runs it without a
  prompt; the index-service removal prompt stays (IW20). Parent folders the helper created stay, as
  parent folders do today. An administrator can delete a leftover folder.
- **IW30 — Upgrade from v0.7.0: no move.** Reason: P3 (no move) stands, and a move would need a second
  elevated writer. The Welcome page for an installed Navo adds: "To install Navo in another folder,
  remove it in Windows Settings → Apps first. Your settings stay." Declining the index-service prompt
  during that uninstall is fine; the new install reuses the service.
- **IW31 — Delivery order keeps main shippable.** Reason: every atom lands on main alone. The helper,
  launcher, adoption and host branch land while placement still refuses unwritable folders, so their
  code is unreachable. E10 turns the feature on. No release is published between E10 and the E12–E13
  VM pass.

Supersedes: IW13 in part (the wizard still never runs elevated; its helper does), IW18, the owner answer
P1 (`plans/2026-11-09-installer-wizard.md:45`), and the ProgramFiles and ProgramFilesX86 entries of IW8a
(`:188`). E14 marks them in that plan and records D116.

## Risks and rollback

- **R1 — Navo's files in Program Files are writable by their user.** Another administrator who starts
  `navo.exe` from there runs code the installing user could change. Prevented in part: the folder grants
  only that user, SYSTEM and Administrators; the user guide says to run Navo only from your own install.
  Accepted (IW27).
- **R2 — A DLL planted beside `Navo-Setup.exe` loads into the elevated helper.** Covered by IW24 (static
  imports from KnownDLLs only, `SetDefaultDllDirectories` first in `main`, `main.rs`) and proven by the
  new VM case S13b.
- **R3 — The helper is abused through a consent the admin gives.** A same-user process could swap
  `Navo-Setup.exe` before the prompt, or call the helper with its own arguments. The prompt names the
  program either way; a same-user attacker can raise any prompt already. The helper re-runs IW6, the
  refused sets and the profile rule, accepts only a user SID, and creates only below the parent id it
  was given. S17h proves the refusals.
- **R4 — A race between the wizard's check and the helper's create.** The helper holds the parent with
  `FILE_FLAG_OPEN_REPARSE_POINT`, checks its id, and creates each name with `NtCreateFile` relative to
  that handle. A change after the create is caught by its re-resolve and by the wizard's adoption.
- **R5 — The prompt shows "Unknown publisher".** The exe is unsigned (IW24, D88). Accepted until code
  signing is bought.
- **R6 — Adoption fails after the helper created folders.** The wizard leaves them (it cannot prove they
  are its own) and shows `AdminFailed`; a retry then meets `RootExists`. This only happens on tampering
  or a helper bug. The user guide says to delete the empty folder as an administrator.
- **R7 — `C:\Program Files\Navo` collides with the index service.** Refused by `ServiceFolder`.
- Rollback: revert the E10 commit. Placement refuses Program Files again with Atom 0's text; the helper
  code stays unreachable. Installs made in the meantime keep working, since they are ordinary per-user
  installs in a user-writable folder.

## Components

| Component | Layer | Contract (in → out) | New / change | Test |
| --- | --- | --- | --- | --- |
| `placement` | core, pure | `check(chain, refused, facts, space, names) → Result<(), FolderProblem>`; new `refused_problem(chain, refused, names) → Option<FolderProblem>`; `warning(facts) → Option<FolderWarning>` | change | `placement_admin_tests.rs` (new) |
| `syntax` | core, pure | `FolderProblem` gains `NeedsAdministrator` (A0, removed by E10), `ServiceFolder`, `OtherUserFolder`, `AdminDeclined`, `AdminFailed` | change | same |
| `refused` | probe, Windows | `refused() → io::Result<Refused>`, moved from `chain.rs:56-104` | new file | windows-quick |
| `elevate` | core, pure | `HelperArgs`, `helper_command_line`, `parse_helper_args`, `valid_user_sid`, `admin_root_sddl`, `admin_descriptor_ok`, `HelperExit` | new | `elevate_tests.rs` |
| `helper_checks` | probe, Windows | `helper_chain(&HelperArgs) → Result<Chain, HelperExit>` | new | `helper_checks_tests.rs` |
| `elevated_create` | probe, Windows | `create_verified(&HelperArgs, &Chain, sddl) → Result<(), HelperExit>` | new | `elevated_create_tests.rs` |
| `elevate_entry` | probe, Windows | `run(&[OsString]) → ExitCode` | new | VM S17h |
| `elevate_launch` | probe, Windows | `launch_helper(&HelperArgs) → Result<u32, LaunchError>` | new | VM S17a–d |
| `pin_adopt` | probe, Windows | `Pin::adopt(before, after, user_sid) → Result<Pin, PinError>` | new | `pin_adopt_tests.rs` |
| `host_admin` | host, Windows | `pin_with_helper(root, chain) → io::Result<Pin>` | new | VM S17a–d |
| setup strings | UI | Welcome blurb and other-folder line | change | `cargo nextest run -p navo-ui setup` |
| `setup-e2e` | VM | `keys-<name>.req` host requests; `cases-admin.ps1` S17a–S17i; S13b | new | `setup-e2e-host.sh` PASS |

## Atoms

| Atom | Component | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| A0 | `placement`, `syntax`, `chain` | api-builder | high | — | `cargo nextest run -p navo-setup placement`; windows-quick | ready |
| A0v | `setup-e2e` S6 | tester | high | A0 | `scripts/setup-e2e-host.sh S6` PASS | blocked |
| T0 | `refused` | patcher | medium | A0 | windows-quick, no diff in behaviour | blocked |
| E1 | `elevate` | api-builder | high | — | `cargo nextest run -p navo-setup elevate` | ready |
| E2 | `placement`, `syntax` | api-builder | high | T0 | `cargo nextest run -p navo-setup placement` | blocked |
| E3 | `refused`, discovery, host checks | senior | medium | E2 | windows-quick | blocked |
| E4 | `helper_checks` | senior | high | E1, E3 | windows-quick | blocked |
| E5 | `elevated_create` | senior | high | E4 | windows-quick | blocked |
| E6 | `elevate_entry` | senior | high | E5 | windows-quick | blocked |
| E7 | `elevate_launch` | senior | medium | E1 | windows-quick | blocked |
| E8 | `pin_adopt` | senior | high | E1 | windows-quick | blocked |
| E9 | `host_admin` | senior | high | E6, E7, E8 | windows-quick | blocked |
| E10 | go live: `placement`, `refused`, `syntax` | api-builder | high | E9 | `cargo nextest run -p navo-setup placement`; windows-quick | blocked |
| E10v | `setup-e2e` S6 rows | tester | high | E10 | `scripts/setup-e2e-host.sh S6` PASS | blocked |
| E11 | setup strings | ui-builder | medium | E10 | `cargo nextest run -p navo-ui setup` | blocked |
| E12 | `setup-e2e` keys + S17a–S17d | tester | high | E10v | `scripts/setup-e2e-host.sh S17a S17b S17c S17d` PASS | blocked |
| E13 | `setup-e2e` S17e–S17i, S13b | tester | high | E12, E11 | `scripts/setup-e2e-host.sh S17e S17f S17g S17h S17i S13b` PASS | blocked |
| E14 | docs | documenter | medium | E13 | typos and link check in the gate | blocked |
| E15 | review | reviewer-senior | high | E13 | verdict with no must-fix | blocked |

The gate (all steps, `navo-setup` included) runs before every merge. `windows-quick` is
`.github/workflows/windows-quick.yml`.

### A0 — Atom 0: an honest refusal for Program Files

- **Read:** `crates/navo-setup/src/placement.rs` (268 lines) all; `syntax.rs` (225) `:28-125`;
  `probe/chain.rs` (373) `:56-104`; `placement_tests.rs` (419) `:1-60, :338-350`.
- **Change:**
  - `placement.rs`: `Refused` gains `pub admin_only: Vec<Protected>` (doc: "Atom 0: Program Files
    folders that need administrator rights; removed by E10"). Add
    `pub fn refused_problem(chain, refused, names) -> Option<FolderProblem>`: an `admin_only` anchor
    the candidate is `inside` gives `NeedsAdministrator`; then `never_inside`, `never_equal`,
    `fresh_only` give `ProtectedFolder` exactly as `:154-168` today. `check` order becomes: ProbeFailed,
    RootExists, ReparsePoint, SubstDrive, DriveNotLocal, `refused_problem`, NotWritable,
    NotEnoughSpace. Add `#[cfg(test)] #[path = "placement_admin_tests.rs"] mod admin_tests;`.
  - `syntax.rs`: variant `NeedsAdministrator` with the IW28 message and fix.
  - `chain.rs` `refused()`: `admin_only` gets `FOLDERID_ProgramFiles` and `FOLDERID_ProgramFilesX86`.
    Both stay in `never_inside`, so discovery and host checks keep refusing existing roots there.
  - `placement_tests.rs:339`: the literal ends with `..Refused::default()` (one line), so later fields
    never break it.
  - New `placement_admin_tests.rs` with its own small helpers (do not touch the 419-line file further).
- **Check:** red first: a test "an unwritable folder below Program Files (x86) gives NeedsAdministrator"
  fails with `NotWritable`. Green: that test; the same with `writable: true`; a `never_inside` folder
  with `writable: false` gives `ProtectedFolder`; an unwritable plain folder still gives `NotWritable`;
  the exact texts. `cargo nextest run -p navo-setup placement` passes; windows-quick green.
- **Stop if:** another caller builds `Refused` with a struct literal, or a test outside these files
  expects `NotWritable` for a protected folder.

### A0v — Atom 0 in the VM

- **Read:** `setup-e2e/cases-refuse.ps1` (274) `:209-245`.
- **Change:** `Get-S6Rows` adds `C:\Program Files (x86)\Navo`, `C:\PROGRA~2\Navo`,
  `c:\program files (x86)\navo` and `C:\Program Files (x86)\Tools\Navo`. These four rows and the two
  existing Program Files rows also require stderr to contain "Program Files needs administrator
  rights". `Get-S6Listing` adds `C:\Program Files (x86)`. S6 asserts first that `C:\PROGRA~2` resolves.
- **Check:** `scripts/setup-e2e-host.sh S6` prints PASS with the six rows in its evidence.
- **Stop if:** the VM has no 8.3 name for `C:\Program Files (x86)`.

### T0 — Move `refused()` out of `chain.rs`

- **Read:** `probe/chain.rs` (373) `:56-104, :228-270`.
- **Change:** move `refused()` (`:56-104`) into new `probe/refused.rs`, declared in `chain.rs` as
  `#[path = "refused.rs"] mod refused; pub use refused::refused;`. It calls `super::add_known` and
  `super::add_known_unverified`. Callers (`host_windows.rs:70,107,134`, `window_worker.rs:242`,
  `discovery.rs:45`, `tester.rs:92,251`) stay unchanged.
- **Check:** windows-quick green; `git diff --stat` shows only the two files.
- **Stop if:** a caller must change.

### E1 — The helper's pure contract

- **Read:** `syntax.rs:139` (`parse_candidate`), `placement.rs:12-17` (`FileId`),
  `payload_security.rs:102` (`root_sddl`).
- **Change:** new `crates/navo-setup/src/elevate.rs` and `elevate_tests.rs` (in-file
  `#[cfg(test)] #[path]`), and `pub mod elevate;` in `main.rs` (218). All pure, built on every OS:
  - `pub const HELPER_FLAG: &str = "--elevated-create";`
  - `pub struct HelperArgs { pub root: PathBuf, pub user_sid: String, pub parent: FileId }`.
  - `pub fn helper_command_line(args: &HelperArgs) -> String`: `--root "<root>" --for <sid> --parent
    <volume as 16 lowercase hex>:<id as 32 lowercase hex>`, after `HELPER_FLAG`. Trailing `\` are
    stripped from the root first, so the closing quote cannot be escaped. IW6 already refuses `"`.
  - `pub fn parse_helper_args(args: &[OsString]) -> Result<HelperArgs, HelperExit>`: exactly the six
    items after the flag, in that order; the root must pass `parse_candidate`; the SID must pass
    `valid_user_sid`; the id must have exactly those hex widths. Anything else is `BadArguments`.
  - `pub fn valid_user_sid(sid: &str) -> bool`: `S-1-5-21-a-b-c-r` with four decimal `u32` parts, no
    leading zeros, `r >= 1000`; or `S-1-12-1-a-b-c-d` (Entra ID). Everything else is false.
  - `pub fn admin_root_sddl(sid: &str) -> String`: the IW25 descriptor.
  - `pub struct AceView { pub kind: u8, pub flags: u8, pub mask: u32, pub sid: String }` and
    `pub fn admin_descriptor_ok(owner: &str, protected: bool, aces: &[AceView], user: &str) -> bool`:
    owner is `S-1-5-32-544` or `user`; `protected`; exactly three ACEs, each kind 0 (allow) and flags
    exactly `0x3`; the set is `{(S-1-5-18, 0x1F01FF), (S-1-5-32-544, 0x1F01FF), (user, 0x1301BF)}`.
  - `#[repr(u8)] pub enum HelperExit { Created = 0, NotElevated = 20, BadArguments = 21, Refused = 22,
    Changed = 23, Exists = 24, CreateFailed = 25 }` with `code()` and `from_code(u32) -> Option<Self>`.
- **Check:** red first, then `cargo nextest run -p navo-setup elevate` green, with a round trip of
  `helper_command_line` through Windows argv rules for a root with spaces, `(x86)` and a trailing `\`.
- **Stop if:** the round trip needs a quoting rule beyond the trailing-separator strip.

### E2 — Final folder groups and texts (rules still off)

- **Read:** `placement.rs` after A0; `syntax.rs`; `placement_admin_tests.rs`; `probe/refused.rs`.
- **Change:**
  - `Refused` gains `pub service: Vec<Protected>` and `pub profiles: Vec<Protected>`.
    `refused_problem` checks `service` (inside) first, giving `ServiceFolder`, then `admin_only`, then
    the three old groups. `profiles` is not read yet.
  - `FolderWarning::Administrator` with its IW28 text. `warning()` is unchanged.
  - `syntax.rs`: `ServiceFolder`, `OtherUserFolder`, `AdminDeclined`, `AdminFailed` with their texts.
  - `refused.rs` literal: `service: Vec::new(), profiles: Vec::new()` (two lines).
- **Check:** red first, then `cargo nextest run -p navo-setup placement` green: a service-folder chain
  gives `ServiceFolder` before `NeedsAdministrator`; texts exact.
- **Stop if:** a `match` on `FolderProblem` outside `syntax.rs` stops compiling.

### E3 — Fill the groups on Windows

- **Read:** `probe/refused.rs`; `probe/discovery.rs` (363) `:100-140` (`chain_is_safe`);
  `host_checks.rs` (61) `:25-61`.
- **Change:** `refused.rs`: `service` gets `FOLDERID_ProgramFiles` + `brand::NAME` and
  + `brand::LEGACY_NAME`; `profiles` gets `FOLDERID_UserProfiles`; `never_inside` adds
  `FOLDERID_ProgramFilesCommonX86` and `FOLDERID_ProgramFiles` + `WindowsApps`. `chain_is_safe` and
  `refuse_existing_folders` treat `service` like `never_inside`.
- **Check:** windows-quick green. `navo-setup --tester-probe` output for `C:\Program Files\Navo` names
  the service group (proved in E10v).
- **Stop if:** `FOLDERID_UserProfiles` resolves to an error other than those `add_known` skips.

### E4 — The helper's own checks

- **Read:** `probe/chain.rs:120` (`resolve_path`); `probe/facts.rs` (263) `drive_kind`, `subst_drive`;
  `placement.rs` `refused_problem`, `inside`.
- **Change:** new `probe/helper_checks.rs` and `helper_checks_tests.rs`; one line in `probe/mod.rs`;
  `drive_kind` and `subst_drive` become `pub(crate)`. `pub fn helper_chain(args: &HelperArgs) ->
  Result<Chain, HelperExit>`, in this order: `resolve_path(&args.root)` (error: `Refused`); no missing
  name: `Exists`; last existing id differs from `args.parent`: `Changed`; any reparse link, a drive
  that is not Fixed or Removable, or `subst`: `Refused`; `refused_problem` returns any problem:
  `Refused`; the chain is inside any `profiles` entry: `Refused`.
- **Check:** windows-quick runs the new tests: a temp chain gives `Ok`; a parent id mismatch gives
  `Changed`; a junction in the chain gives `Refused`; an existing root gives `Exists`.
- **Stop if:** the tests need elevation.

### E5 — Create relative to a held handle

- **Read:** `probe/pin.rs` (387) `:218-330` (`create_and_hold`, `open_hold`, `read_id`,
  `delete_created`); `payload_security.rs` (155) `:113-135`.
- **Change:** new `probe/elevated_create.rs` and `elevated_create_tests.rs`; one line in
  `probe/mod.rs`; `payload_security.rs` `descriptor` becomes `pub(crate)`; `Cargo.toml` windows-sys
  features add `Wdk_Foundation`, `Wdk_Storage_FileSystem`, `Win32_System_IO`.
  `pub fn create_verified(args: &HelperArgs, chain: &Chain, sddl: &str) -> Result<(), HelperExit>`:
  1. Open the last existing link with `FILE_TRAVERSE | FILE_ADD_SUBDIRECTORY | FILE_READ_ATTRIBUTES |
     SYNCHRONIZE`, flags `BACKUP_SEMANTICS | OPEN_REPARSE_POINT`. Its id must equal `args.parent` and
     it must not be a reparse point; else `Changed`.
  2. For each missing name (no `\`, `/`, `:`, not `.` or `..`; else `Refused`): `NtCreateFile` with
     `RootDirectory` = the previous handle, `OBJ_CASE_INSENSITIVE`, the descriptor from `sddl`, access
     `FILE_TRAVERSE | FILE_ADD_SUBDIRECTORY | FILE_READ_ATTRIBUTES | DELETE | SYNCHRONIZE`, share read
     and write, `FILE_CREATE`, options `FILE_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT |
     FILE_SYNCHRONOUS_IO_NONALERT`. A name collision is `Exists`; any other failure is `CreateFailed`.
  3. Re-resolve `args.root`. The existing ids must equal `chain`'s, and each new link's id must equal
     the id read through its handle; else `Changed`.
  4. On any error after a create: delete the created folders newest first through their own handles
     (`SetFileInformationByHandle`, `FileDispositionInfo`), then return the error.
- **Check:** windows-quick runs the tests (unelevated is fine): two folders created under a temp
  parent carry the given descriptor; a wrong parent id creates nothing; a pre-existing name gives
  `Exists` and removes nothing it did not create.
- **Stop if:** `NtCreateFile` needs a feature beyond the three named.

### E6 — The helper entry

- **Read:** `main.rs` (218) `:55-70`; `probe/facts.rs:62` (`elevated`).
- **Change:** new `probe/elevate_entry.rs`; one line in `probe/mod.rs`; `main.rs`: right after
  `restrict_dll_search` and before the empty-args test, a `#[cfg(windows)]` branch returns
  `elevate_entry::run(&args[1..])` when the first argument equals `HELPER_FLAG`. `run`: `elevated()`
  must be `Ok(true)`, else `NotElevated`; then `parse_helper_args`, `helper_chain`,
  `create_verified(…, &admin_root_sddl(&args.user_sid))`; exit with the `HelperExit` code. No console
  attach, no log, no GPUI.
- **Check:** windows-quick green. VM proof: S17h.
- **Stop if:** anything before the branch in `main` writes a file.

### E7 — The launcher

- **Read:** `navo-index-service/src/elevation/windows.rs:34-100`.
- **Change:** new `probe/elevate_launch.rs`; one line in `probe/mod.rs`.
  `pub enum LaunchError { Declined, Io(io::Error) }`;
  `pub fn launch_helper(args: &HelperArgs) -> Result<u32, LaunchError>`: `ShellExecuteExW` with
  `runas`, the file `std::env::current_exe()`, the parameters `HELPER_FLAG` + `helper_command_line`,
  `lpDirectory` = System32, `SW_HIDE`, mask `SEE_MASK_NOASYNC | SEE_MASK_NOCLOSEPROCESS`;
  `ERROR_CANCELLED` is `Declined`. Wait with no deadline, then return `GetExitCodeProcess`.
- **Check:** windows-quick green. VM proof: S17a–S17d.
- **Stop if:** the call needs COM set up on the worker thread; report it.

### E8 — Adopt what the helper created

- **Read:** `probe/pin.rs` (387) `:35-110, :186-330`; `probe/parent_acl.rs` (235) `:44-110`.
- **Change:** new `probe/pin_adopt.rs` and `pin_adopt_tests.rs`; in `pin.rs` the line
  `#[path = "pin_adopt.rs"] mod adopt;` and `PinError::Descriptor` with its `Display` arm (about five
  lines; `pin.rs` stays under 395). `impl Pin { pub fn adopt(before: &Chain, after: &Chain, user_sid:
  &str) -> Result<Pin, PinError> }`:
  1. `before` has existing links and missing names, else `InvalidChain`.
  2. `after.missing` is empty; `after.existing` is `before.existing` (same ids) plus one link per
     missing name, names equal under `OrdinalNames`; else `Identity`. A reparse link is `Reparse`.
  3. Each new folder: open with `READ_CONTROL | FILE_READ_ATTRIBUTES`, `BACKUP_SEMANTICS |
     OPEN_REPARSE_POINT`; `read_id` must match; `GetSecurityInfo` for owner and DACL feeds
     `admin_descriptor_ok`; else `Descriptor`. Then hold it with `open_hold`.
  4. The root (last new folder) has no entries, else `RootExists`.
  5. Return `Pin { existing_root: None, created }`, outermost first, as `Pin::fresh` does. Adopt
     never deletes; on failure it drops its handles only.
- **Check:** windows-quick runs the tests. Folders made with `admin_root_sddl` adopt, and
  `remove_created` then removes them by handle. A `D:P` without the user ACE is `Descriptor`. A
  renamed folder is `Identity`.
- **Stop if:** `pin.rs` must grow past 395 lines.

### E9 — The host branch

- **Read:** `host_windows.rs` (384) `:60-102, :300-330` (`resolved` at `:317`); `host.rs` (98) `:25-45`;
  `runner_run.rs` (380) `:325-345`.
- **Change:** new `host_admin.rs`, declared in `host_windows.rs` as
  `#[path = "host_admin.rs"] mod admin;`. In `pin_fresh`, `pin::Pin::fresh(&chain)` runs only when
  `facts.writable`; otherwise `admin::pin_with_helper(root, &chain)?`. The rest of `pin_fresh` is
  shared. `host_windows.rs` grows by at most eight lines; if it would pass 395, move the `PinError`
  mapping closure into `host_admin.rs`.
  `pub(super) fn pin_with_helper(root: &Path, chain: &Chain) -> io::Result<Pin>`:
  `user_sid()` (must pass `valid_user_sid`, else `AdminFailed`); `launch_helper`; `Declined` is
  `folder_error(AdminDeclined)`; `Io` or a non-zero exit is `folder_error(AdminFailed)`; then
  `super::resolved(root)` and `Pin::adopt(chain, &after, &sid)`; an adopt error is
  `folder_error(AdminFailed)`. The runner maps these to `Failure::Folder` (exit 3) as it does today.
- **Check:** windows-quick green. The path is unreachable until E10; VM proof in E12.
- **Stop if:** the runner or `Host` trait must change.

### E10 — Go live

- **Read:** `placement.rs`, `placement_admin_tests.rs`, `probe/refused.rs`, `syntax.rs`.
- **Change:** `refused.rs` drops `admin_only` and removes `FOLDERID_ProgramFiles` and
  `FOLDERID_ProgramFilesX86` from `never_inside`. `placement.rs` drops `Refused::admin_only`; after
  `refused_problem`, `!facts.writable` inside `profiles` is `OtherUserFolder`, and any other
  `!facts.writable` passes. `warning()` returns `Administrator` when `!writable`, else `ParentOpen`
  when `parent_open`. `syntax.rs` removes `NeedsAdministrator`. The tests change to match.
- **Check:** red first: "an unwritable folder below Program Files (x86) passes with the Administrator
  warning". Then `cargo nextest run -p navo-setup placement` and windows-quick green.
- **Stop if:** `NotWritable` loses its last user (`runner_run.rs:218` keeps it for upgrades).

### E10v — S6 after go live

- **Read:** `setup-e2e/cases-refuse.ps1` `:209-245`.
- **Change:** the four Program Files (x86) rows leave S6 (they now install; S17 covers them). The two
  `C:\Program Files\Navo` rows expect "Navo's index service uses this folder". Add
  `C:\Program Files\Common Files\Navo` and `C:\Program Files\WindowsApps\Navo`.
- **Check:** `scripts/setup-e2e-host.sh S6` PASS. Merge with E10, no release in between.

### E11 — Welcome texts

- **Read:** `crates/navo-l10n/locales/en-US/setup.ftl` (71) `:5-7`; `crates/navo-ui/src/setup/strings.rs`
  (192) `:11-13, :88-90`; `pages_a.rs` (220).
- **Change:** `setup-welcome-blurb` gets the IW28 text. New key `setup-welcome-other-folder` with the
  IW30 text, field `welcome_other_folder`, shown under the installed line on the Welcome page.
- **Check:** `cargo nextest run -p navo-ui setup` green; a screenshot of both Welcome states at the
  wizard's window size, both themes.

### E12 — VM: keys and the first four admin cases

- **Read:** `scripts/setup-e2e-host.sh` (306) `:220-260`; `setup-e2e/cases-uninstall.ps1` (273)
  `:1-60, :170-180`; `setup-e2e/common.ps1` (375) `:267`; `testing/installer-w1-report.md:331`.
- **Change:** new `scripts/setup-e2e-keys.sh`, sourced by the host script, whose part loop calls
  `serve_keys` next to `serve_shots` (about three lines). A case writes `keys-<name>.req`; each line is
  one of `alt-y`, `esc`, `tab`, `ret`, `wait-<ms>`, `type-secret`. The host sends them with QMP
  `send-key` and writes `keys-<name>.ok`. `type-secret` reads `a.secret` over SSH into a variable and
  types it (US layout, shift for capitals, `!` and `+`); it is never printed or logged. New
  `setup-e2e/cases-admin.ps1` with S17a–S17d. Its initializer calls `Initialize-AdminAccount`, then
  sets `ConsentPromptBehaviorAdmin=5`; finalize restores the policy.
- **Check:** `scripts/setup-e2e-host.sh S17a S17b S17c S17d` PASS; `scripts/setup-imports.ps1` on the
  release `navo-setup.exe` PASS.

### E13 — VM: update, uninstall, refusals, helper abuse, window, DLLs

- **Read:** `setup-e2e/cases-admin.ps1`; `cases-uninstall.ps1:253` (S13); `scripts/update-e2e.ps1`.
- **Change:** S17e–S17i in a second file `setup-e2e/cases-admin-more.ps1`; S13b next to S13 or in it.
- **Check:** `scripts/setup-e2e-host.sh S17e S17f S17g S17h S17i S13b` PASS; the full S1–S17 run PASS.

### E14 — Docs

- `docs/decisions.md`: D116 (IW25–IW31, supersedes IW13 in part, IW18, P1). `docs/features/install.md`
  and `docs/user-guide.md`: Program Files, the one prompt, R1, R6. The old plan marks IW13, IW18 and
  IW8a's two entries as superseded by this plan, by name.

### E15 — Review

- reviewer-senior on E4–E9 and E10 against IW25, R2–R4 and IW0's extension. Must-fix items go back to
  their atom's builder.

## VM cases (S17)

UAC on (`ConsentPromptBehaviorAdmin=5`, secure desktop). `u` is the standard user; `a` the local admin,
with a new random password per case (`Initialize-AdminAccount`). Prompts are answered by host key
presses only; each case counts `consent.exe` starts.

| Case | Set-up | Pass |
| --- | --- | --- |
| S17a | As `a`, silent `--installto "C:\Program Files (x86)\Navo"`; Alt+Y | Exit 0; one prompt; root owner BA, DACL exactly the IW25 three ACEs, protected; HKCU key and links in `a`'s hive and profile; `packages\` in the root; no `%LocalAppData%\NavoApp`; Navo, if started, unelevated |
| S17b | As `a`, same; Esc | Exit 3, stderr "Windows did not give Setup administrator rights"; no new entry in `C:\Program Files (x86)`; no key |
| S17c | As `u`, `ConsentPromptBehaviorUser=3`; `type-secret` of `a` | Exit 0; key and links in `u`'s hive and profile, none in `a`'s; the user ACE names `u`'s SID |
| S17d | As `u`, `ConsentPromptBehaviorUser=0` | Exit 3 with the declined text; nothing created |
| S17e | S17c's install; Navo's own update through `NAVO_UPDATE_FEED_DIR`, then the wizard 0.0.3 upgrade | Zero prompts in both; `sq.version` 0.0.3; packages in the root; no `%LocalAppData%\NavoApp` |
| S17f | S17e's install; uninstall as `u` through `UninstallString` | Zero prompts (no service installed); root, key and links gone; `C:\Program Files (x86)` otherwise unchanged |
| S17g | As `u`: `C:\Program Files\Navo`, `C:\Program Files\Common Files\Navo`, `C:\Program Files\WindowsApps\Navo`, `C:\Users\a\Navo`, `C:\` | Exit 3 each with its IW28 text; zero prompts |
| S17h | As the SSH admin (elevated), the helper called directly: `C:\Windows\Navo`, `--for S-1-1-0`, a junction in the chain, a wrong `--parent`, an existing root; and once as `u` unelevated | Exits 22, 21, 22, 23, 24 and 20; nothing created |
| S17i | As `a`, window mode: Browse to `C:\Program Files (x86)`, Next, Install, Alt+Y | Screenshots of the Folder page warning and the Done page; same pass as S17a |
| S13b | S13's planted DLLs beside `Navo-Setup.exe`; as `a`, S17a's install with Alt+Y | No `plant-out` file from any process, the elevated helper included |

## Done when

- The owner's case works: `C:\Program Files (x86)\Navo` installs with one prompt, by S17a, S17c, S17i.
- Updates and uninstall raise no prompt, by S17e and S17f.
- The refused folders say why, by S6 and S17g; the helper refuses abuse, by S17h; no DLL plant, by S13b.
- The full S1–S17 VM run and the gate pass; E15 has no must-fix; D116 is recorded.

## Log

- 2026-11-12 — Plan written by Turgon on `plan-install-anywhere` from `c18314c8`. The owner delegated
  every open choice; they are recorded under "Decided for the owner".
