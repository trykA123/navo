# Navo installer wizard, part 2: wizard UI, release and VM run

Status: drafting (revised after the third Terra plan check and W0, 2026-11-09) · Repo: `homelab/navo` · Branch: `installer-wizard` · Started: 2026-11-09

Architect: Turgon. This part builds on [part 1, the install engine](2026-11-09-installer-wizard.md). Part 1 holds the human spec, the decisions IW0–IW24 (with IW8b), the risks and the plan-check answers. This file adds IW16 and IW19–IW23, the atoms that build the window, the release change, the VM run and the docs. Nothing here starts before part 1 is done.

## Goal

The engine from part 1 gets its window, matching the human spec in part 1. The release then builds `Navo-Setup.exe`. A rehearsal run proves the build without publishing it. The wizard is published only after the VM run and the senior review pass. The owner then switches the repo variable.

## Appetite

Three agent days, part of the seven for both parts. Cut order as in part 1.

## Non-goals

- A web or HTML installer UI.
- Animation beyond the 80 ms fade.
- Theme or language pickers inside the wizard; it follows Windows.
- Publishing before S1–S15 pass and the review is done.

## Decisions (continued from part 1)

- **IW16 — Release assets.**
  - `Navo-Setup.exe` is the wizard, carrying `NavoApp-win-Setup.exe` and the full nupkg of the same version inside (IW15). Fresh installs use the first, upgrades the second (IW8b).
  - The raw `NavoApp-win-Setup.exe` keeps being published for scripts and as a fallback. The portable zip and the feed files stay as they are.
  - Publication of `Navo-Setup.exe` waits on the repo variable `NAVO_PUBLISH_SETUP_WIZARD == 'true'`. The owner sets it after S1–S15 pass and W16 has no must-fix left. Until then the release is the same as today.
  - Two lists, kept apart.
    - **Release assets:** every file `out/` holds today (`release.yml:144-204`): `NavoApp-win-Portable.zip`, `NavoApp-win-Setup.exe`, the full nupkg the feed names, `releases.win.json`, `RELEASES` and `navo-windows-x64-pdb.zip`. `Navo-Setup.exe` joins them only when the variable is on.
    - **Manifests:** `assets.txt` (the release-asset names) and `SHA256SUMS.txt` (hashes of the release assets only, never of a manifest).
  - The release uploads the release assets and `SHA256SUMS.txt`, never `./*`. `assets.txt` stays in the build artifact.
  - The embedded payload's version equals the pack version, and `Navo-Setup.exe --version` prints it (IW15).
- **IW19 — Licence page (P2 decided 2026-11-09: MIT OR Apache-2.0; keep the "I accept" box).** The page shows the MIT text from `crates/navo-setup/assets/LICENSE.txt`, embedded, in a read-only scroll view. A link opens the Apache-2.0 text on GitHub.
- **IW20a — Release builds are locked.** Every `cargo build` in `release.yml` uses `--locked`.
- **IW21 — Look.**
  - Navo's tokens and `navo-ui` primitives only. The colour scheme follows Windows (`AppsUseLightTheme`).
  - The window is 640 × 480 logical px, fixed size, centred, with a title bar, no resize and no menu.
  - Only 80 ms fades, no other motion. Nothing flashes.
- **IW22 — Keys and access.** As in the human spec. Every control has a UIA name and role through gpui's AccessKit. A check box reports its state, and the progress text is a live region.
- **IW23 — Strings.** All text lives in `navo-l10n` as `setup.ftl`, in `en-US`, the only locale today.

## Components

| Component | Layer | Contract (in → out) | New / change | Test |
| --- | --- | --- | --- | --- |
| `setup-view-model` | UI contract, `navo-ui/src/setup/mod.rs` | `SetupView { page, can_back, can_next, primary, folder_text, problem, progress }` from `navo_setup::flow::Flow` mirrored as plain data | new | `navo-ui/src/setup/tests.rs` |
| `setup-pages` | UI, `navo-ui/src/setup/pages_a.rs`, `pages_b.rs`, `pages_c.rs` | `render_page(&SetupView, &Strings, cx) -> AnyElement` in `mod.rs` dispatches to each file's `render` | new | gallery page plus UIA dump |
| `setup-gallery` | dev UI | gallery page with every page and state | new | `cargo xtask check -p navo-ui-gallery` |
| `setup-strings` | l10n | `locales/en-US/setup.ftl` | new | `cargo nextest run -p navo-l10n`; `cargo xtask l10n` |
| `setup-window` | app shell, `navo-setup/src/window.rs` | opens the window, maps flow effects to the runner | new | `window_tests.rs`, S12 |
| `licence-text` | asset | `assets/LICENSE.txt` | new | `cargo xtask check -p navo-setup` |
| `release-wizard` | CI | `release.yml` builds, checks and (when switched on) publishes `Navo-Setup.exe` | change | rehearsal run green |
| `setup-e2e` | VM test | `scripts/setup-e2e.ps1` (dispatcher) and `scripts/setup-e2e/*.ps1` (common helpers and case groups) run S1–S15 | new | the script's own PASS line |

The view model is plain data, so `navo-ui` does not depend on `navo-setup`. `navo-setup` maps its `Flow` onto `SetupView`.

## Atoms

| Atom | Component | Role | Effort | After | Owner gate | Check | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| W9 | prototype | designer | high | part 1 done | — | owner picks one variant | done (A, Rail) |
| W10a | `setup-view-model` | api-builder | medium | W9 | — | `cargo nextest run -p navo-ui setup` | done |
| W10b | `setup-pages` A | ui-builder | high | W10a | — | `cargo xtask check -p navo-ui` | done |
| W11a | `setup-pages` B | ui-builder | high | W10b | — | `cargo xtask check -p navo-ui` | done |
| W11b | `setup-pages` C | ui-builder | medium | W11a | — | `cargo xtask check -p navo-ui` | done |
| W11g | `setup-gallery` | ui-builder | medium | W10b, W11a, W11b | — | `cargo xtask check -p navo-ui-gallery`; screenshots of every page in both themes | done (shots pending: VM lock) |
| W12a | `setup-strings` | patcher | medium | W10b, W11a, W11b | — | `cargo nextest run -p navo-l10n`; `cargo xtask l10n` | done |
| W12b | `setup-window` | senior | high | W11g, W12a, W13b | — | `cargo nextest run -p navo-setup window`; VM: the window opens and installs (S1b) | part 1 done (VM: UNTESTED) |
| W12c | `setup-window` part 2 | senior | high | W12b | — | rail and footer shell, Alt+letter access keys, editable Folder field, access keys on Buttons; `cargo xtask check -p navo-setup -p navo-ui` | done (VM: UNTESTED) |
| W13a | `licence-text` (repo) | patcher | medium | — | — | `cargo metadata --no-deps --format-version 1` shows `MIT OR Apache-2.0` for every package | done |
| W13b | `licence-text` (wizard) | patcher | medium | W13a, part 1's W3 | — | `cargo nextest run -p navo-setup licence` | done |
| W14 | `release-wizard` | infra-builder | high | W12b | — | rehearsal run (`publish: false`) green, with `Navo-Setup.exe` in the artifact; `sha256sum -c` passes; `assets.txt` matches v0.3.0's asset names apart from versions | done |
| W15h | snapshot control | infra-builder | medium | — | — | `winvm.sh snapshot` save, load and delete round trip (see W15h) | done |
| W15a | `setup-e2e` controller, common and install cases | tester | high | W14, G1, W15h | — | `setup-e2e.ps1 -Cases S1a,S1b,S2,S2b,S3,S8,S9` prints PASS | done |
| W15b | `setup-e2e` refusal cases | tester | high | W15a | — | `setup-e2e.ps1 -Cases S3b,S5,S6,S7,S10,S14,S15` prints PASS | done |
| W15c | `setup-e2e` uninstall and DLL cases | tester | high | W15a | — | `setup-e2e.ps1 -Cases S4a,S4b,S13` prints PASS | ready |
| W15d | `setup-e2e` UI and network cases | tester | high | W15a | — | `setup-e2e.ps1 -Cases S11,S12` prints PASS; screenshots attached | ready |
| W16 | review | reviewer-senior | high | W15b, W15c, W15d | — | no must-fix left | blocked |
| W17 | docs | patcher | medium | W16 | — | every changed doc link resolves (`rg -o` of the links, then `test -e`); the owner reads the user-guide text | blocked |

18 atoms. W10b, W11a and W11b run in that order, since each registers itself in `setup/mod.rs`. W15b–W15d run one at a time in the VM. **G1** is part 1's gate: the orchestrator re-reads `scripts/update-e2e.ps1` on main after the ix8v merge and updates the helper line numbers in W15a. The numbers below are from ix8v `78a73adc`. G1 done 2026-11-09: unchanged on main `91e7a4e7`.

### W9 — Prototype

- Four directions, per the design rules: two in Navo's current look, two free. Neutral labels behind one picker, both themes, at 640 × 480.
- Every page from part 1's human spec, including Blocked, Same, Newer, Ambiguous, Elevated, NoPayload and Failed.
- The owner picks one. The pick is recorded in this Log before W10a.

### W10a — View-model contract

- **Change:** `navo-ui/src/lib.rs` (one line, `pub mod setup;`), `navo-ui/src/setup/mod.rs` and `navo-ui/src/setup/tests.rs`.
  - `SetupView` and `SetupPage` as plain data, plus `SetupAction` (Next, Back, Primary, Browse, Toggle(Box), Close, TryAgain, OpenFolder, CopyDetails).
  - `render_page` in `mod.rs` shows each page's name as a placeholder. W10a creates no page file, so it touches three files.
  - W10b, W11a and W11b then run in order. Each creates its own page file and registers it in `mod.rs` (one `mod` line and its match arms), and `cargo xtask check -p navo-ui` passes after each.
  - Tests: `can_next` is false when `problem` is set; Back is false on Installing.

### W10b — Pages A: Welcome, Licence, Folder

- **Change:** `navo-ui/src/setup/pages_a.rs`, and its registration in `setup/mod.rs`.
  - Folder page: the field, Browse…, the space line, the problem line, and the uninstall warning from the human spec. In Upgrade the field is locked and Browse is off (IW8).
  - Licence page: per IW19.
  - Every control has an access key and a UIA name.
- **Stop if:** a page needs a primitive `navo-ui` does not have. Report it; do not add one.

### W11a — Pages B: Shortcuts, Installing, Finish, Failed

- **Change:** `navo-ui/src/setup/pages_b.rs`, and its registration in `setup/mod.rs`.
  - Installing shows the elapsed time, and after 180 s the "still working" text with the log path. It shows no button that is active.
  - Failed: the problem, the log path, Copy details and Back; for an unmarked install (IW7), Retry. A failed fresh install that left files names the root with "Delete this folder before you run Setup again.", and Retry is off (IW7).
- **Stop if:** as in W10b.

### W11b — Pages C: Blocked, Same, Newer, Ambiguous, Elevated, NoPayload

- **Change:** `navo-ui/src/setup/pages_c.rs`, and its registration in `setup/mod.rs`.
  - Same: Open Navo and Close. Newer: "A newer Navo `<version>` is installed in `<path>`." and Close only (IW8).
  - Ambiguous: the "Repair by uninstalling first" text, then Close. The `UnsafeName` reason shows its own text and never suggests uninstalling (IW8). Elevated and NoPayload: Close only.
- **Stop if:** as in W10b.

### W11g — Gallery

- **Change:** `navo-ui-gallery/src/setup_page.rs`, a `mod setup_page;` line in `main.rs`, and the wiring in `bars_page.rs` next to `MultiRenamePage` (line 134).
  - One entry per page and state, built from fixed `SetupView` values.
- **Check:** screenshots of each page in both themes, at the window size, looked at.

### W12a — Strings

- **Change:**
  - `navo-l10n/locales/en-US/setup.ftl`, next to the other 13 files;
  - `navo-l10n/src/catalog.rs:5`: `FILES` grows from 13 to 14 entries, with `setup.ftl` added.
- **Check:** `cargo nextest run -p navo-l10n` and `cargo xtask l10n` pass.

### W12b — Window

- **Change:** `navo-setup/src/window.rs`, `navo-setup/src/window_tests.rs` and `navo-setup/src/main.rs` (without `--silent`, open the window).
  - `Flow` maps to `SetupView`. Effects run on a background task, and results come back as events tagged with the candidate.
  - Browse uses the native folder dialog and returns `browse_target(picked)`.
  - The folder is checked while typing, with a 300 ms debounce, never on the UI thread.
  - Tests: the mapping for each page; a stale check result is dropped; Close is ignored while Installing.

### W13a — Licence files (repo)

- **Change:** root `LICENSE-MIT` and `LICENSE-APACHE` (standard texts, copyright "the Navo contributors"), and `license = "MIT OR Apache-2.0"` in the root `Cargo.toml`'s `[workspace.package]`. W13a also adds `license.workspace = true` to every member crate's `[package]` (orchestrator, 2026-11-09: the whole public repo is MIT OR Apache-2.0). This is one mechanical line per crate, an accepted exception to the three-file limit.

### W13b — Licence text in the wizard

- **Change:** `crates/navo-setup/assets/LICENSE.txt` (a copy of `LICENSE-MIT`) and `crates/navo-setup/src/licence.rs` (`pub const TEXT: &str = include_str!("../assets/LICENSE.txt")`, and a test that it equals the root `LICENSE-MIT`). Registrations: `mod licence;` in `main.rs`, and `license.workspace = true` in `navo-setup/Cargo.toml` (W3 creates the crate after W13a).

### W14 — Release workflow

- **Read:** `.github/workflows/release.yml` (inputs at 4–20, `check-tag` 33–43, `build-windows` 82–210, `publish` 211–end).
- **Change**, in `release.yml` only:
  1. A new input `publish` (boolean, default `true`). When it is `false`:
     - `check-tag` skips the "release exists" test;
     - the `publish` job is skipped;
     - the run ends with the build artifact only.
  2. Every `cargo build` gets `--locked`. Line 97 becomes `cargo build --locked --release -p navo-app -p navo-index-service`.
  3. After `vpk pack`, build the wizard:
     - `cargo build --locked --release -p navo-setup` with `NAVO_SETUP_PAYLOAD=out\NavoApp-win-Setup.exe`, `NAVO_SETUP_PACKAGE=out\<the feed's full nupkg>` (the file the "Check the release feed" step resolves) and `NAVO_SETUP_REQUIRE_PAYLOAD=1`;
     - copy the result to `Navo-Setup.exe`.
  4. Check the wizard:
     - `Navo-Setup.exe --version` prints `navo-setup $env:PACK_VERSION`, the embedded payload's version;
     - `Navo-Setup.exe --payload-sha256` equals `Get-FileHash` of the packed Setup.exe, and `--package-sha256` that of the full nupkg;
     - `scripts/setup-imports.ps1 Navo-Setup.exe` passes.
  5. In `build-windows`, before the upload:
     - Write `out/assets.txt` with IW16's release-asset names, one per line. It holds `Navo-Setup.exe` only when `vars.NAVO_PUBLISH_SETUP_WIZARD == 'true'`.
     - Fail when `out/` misses a listed file, or holds a file that is not listed and is none of `Navo-Setup.exe`, `assets.txt` and `SHA256SUMS.txt`.
     - Write `out/SHA256SUMS.txt` over the listed release assets only, in `sha256sum` format (lowercase hash, two spaces, name, LF). No manifest is hashed.
  6. In `publish`: run `sha256sum -c SHA256SUMS.txt`, then upload exactly the files in `assets.txt` plus `SHA256SUMS.txt`. The old `sha256sum -- *` step (line 231) goes.
- **Check:** a `publish: false` run on the branch is green, and the artifact holds `Navo-Setup.exe`. The orchestrator runs `sha256sum -c SHA256SUMS.txt` on the downloaded artifact and compares `assets.txt` with the asset names of v0.3.0; only versions may differ. No release or tag is created. The worker never runs a publishing workflow; the orchestrator does.
- **Stop if:** the feed check needs an asset the list misses.

### W15h — Snapshot control (host)

- **Change:** the owner's VM tool `winvm.sh`, outside this repo (`NAVO_WINVM` names it; no machine path is committed). Add `snapshot save|load|delete <name>` through QMP `human-monitor-command` (`savevm`, `loadvm`, `delvm`): internal qcow2 snapshots with RAM, so `u`'s logged-in session comes back.
- **Check:** save `e2e-probe`; create `C:\e2e-probe.txt` over SSH; load `e2e-probe`; the file is gone, and SSH answers within 120 s; delete the snapshot.
- **Stop if:** `savevm` refuses a device, for example a raw UEFI vars file. The fallback, decided by the orchestrator, is a disk-only `qemu-img snapshot` with the VM stopped, then a cold boot with auto-logon.

### W15a–W15d — VM end-to-end

- **Host controller** (W15a): `scripts/setup-e2e-host.sh <case>…` runs on the Linux host. For each case it:
  1. loads the snapshot `navo-e2e` with `$NAVO_WINVM snapshot load navo-e2e` and waits for SSH;
  2. starts `setup-e2e.ps1 -Case <case>` in `u`'s interactive session through a scheduled task, because SSH runs in session 0;
  3. waits for `C:\e2e\out\<case>\done`, at most 20 min;
  4. copies `C:\e2e\out\<case>\` to `target/setup-e2e/<case>/` and prints its `PASS` or `FAIL` line.

  It ends with a summary and exits nonzero on any FAIL or timeout.
- **Snapshot `navo-e2e`** (W15a, once): from `navo-clean`, build the packs 0.0.1–0.0.3 with `Build-Pack`, the `tester` wizard and the S10 fakes into `C:\e2e\`. Nothing is installed, and `u` is logged in. Then `snapshot save navo-e2e`.
- **In-VM layout**, so each file stays under 400 lines and each case function under 50:
  - `scripts/setup-e2e.ps1` (W15a) runs one case. It dot-sources `setup-e2e/common.ps1` and every `setup-e2e/cases-*.ps1`, runs `Test-<case>`, then writes the evidence and `done`.
  - `setup-e2e/common.ps1` (W15a): `reg export`/`reg import`, the PASS and FAIL lines, and the helpers below.
  - `setup-e2e/cases-install.ps1` (W15a), `cases-refuse.ps1` (W15b), `cases-uninstall.ps1` and `cases-dll.ps1` (W15c), `cases-ui.ps1` (W15d). One function per case. A later atom adds only its own file.
- **Helpers.** `update-e2e.ps1` is never dot-sourced, because its top level runs its own harness. `common.ps1` holds adapted copies, each with a header line naming its source (ix8v lines, re-checked by G1):
  - as they are: `Assert`, `Quote-Arguments`, `Run-Process`, `Run-Checked`, `Wait-Until`, `Observe-Consent`, `Get-IndexService`, and `Build-Pack` (578–605, used only to make the snapshot);
  - `Start-Interactive([string]$exe, [string[]]$arguments, [bool]$elevated)`, from `Start-App` (221–256), without its Navo arguments (`--profile`, `--left`, the feed);
  - `Install-Raw([string]$setup, [string]$version)`, from `Install-Base` (281–291): runs a raw Setup.exe with `--silent`, waits for `%LocalAppData%\NavoApp\current\navo.exe`, and asserts `$version`;
  - `Install-ServiceFrom([string]$navo)`, from `Install-Service` (317–326): runs `$navo --index-service install` elevated through `Observe-Consent`, waits until the service runs, and checks that its image is under `$navo`'s root;
  - `Uninstall-Root([string]$root)`, from `Uninstall-Copy` (419–430): runs `<root>\Update.exe uninstall --silent`.
- The wizard under test is a `tester` build run as `Navo-Setup.exe --payload <Build-Pack Setup.exe> --payload-package <its full nupkg> --payload-version <its pack version>`, for example `--payload-version 0.0.2` (IW15). S12 and the release check use the rehearsal artifact instead.
- Each case:
  - starts from the controller's fresh load of `navo-e2e`;
  - backs up every registry key it changes with `reg export`, and runs `reg import` in `finally`;
  - writes `PASS <case>` or `FAIL <case>: <reason>` into its evidence folder.
- The cases per atom are in the atom table.

### W16 — Senior review

- Review both parts' diffs against IW0–IW24, with a focus on data safety (IW6–IW8b, including the apply steps against the library's), elevation (IW13), DLL loading (IW24) and the release gate (IW16).
- One verdict, then must-fix, should-fix and nice, each with file:line.

### W17 — Docs

- **Change:**
  - `docs/decisions.md`: D113 records IW0–IW24, and D88 gets a note that it is extended by D113.
  - `docs/features/install.md` (new).
  - `docs/user-guide.md`, install section: the folder rule (files inside are deleted at uninstall); uninstall also deletes every shortcut that points into the folder, including ones the user made (IW20); the refused characters `%`, `^`, `&` and `!` (IW6); one install at a time, and a downgrade only by uninstalling first (IW8); the elevation refusal; the portable fallback; the IW9 race; and the IW20 service limits with `sc.exe delete "Navo Index"`.
  - `docs/roadmap.md`: v1.0 item 2.
  - The README's download line.
- **Check:** as in the table; there is no docs xtask.

## VM cases

These run on the Windows VM snapshot `navo-e2e`, made from `navo-clean` by W15a. It has the standard user `u` and an admin account `a`, both with split tokens and UAC on. Unless a case says otherwise, the wizard runs as `u` through `Start-Interactive`.

| # | Case | Commands and pass condition |
| --- | --- | --- |
| S1a | Fresh, default folder, silent | `Navo-Setup.exe --silent` exits 0. Pass when the S1b conditions hold, except that `navo.exe` is not running (silent never starts it). |
| S1b | Fresh, default folder, window | From its own fresh snapshot: Next ×3, Install, Finish with Start Navo ticked. Pass when all of these hold: `%LocalAppData%\Programs\Navo\Update.exe` exists; the run's `-velopack.log` sits beside the wizard's log; `current\sq.version` has 0.0.2; `navo-install.json` has packId `NavoApp`; the HKCU `NavoApp` key has that `InstallLocation`; both `.lnk` files target the root; `navo.exe` runs with `TokenElevation` false; no `%LocalAppData%\NavoApp`. |
| S2 | Custom folder, then update | Browse to `C:\Users\u\Prog Ă`, so the root is `…\Prog Ă\Navo`. Install 0.0.2, set `NAVO_UPDATE_FEED_DIR` to the 0.0.3 feed, and update through Navo's Restart. Pass: `sq.version` has 0.0.3 in the same root; the marker is still present; no `%LocalAppData%\NavoApp\packages`. |
| S2b | Upgrade through apply, with a replacement attempt | S2's root with 0.0.2 and the Desktop link deleted, then wizard 0.0.3. While `Update.exe apply` runs (polled by process name and command line), `Rename-Item` on `…\Prog Ă\Navo` and on `…\Prog Ă` both fail. Pass: exit 0; `sq.version` has 0.0.3; `<root>\Update.exe` is the one from the 0.0.3 nupkg (hash); `packages\` holds the 0.0.3 nupkg; the root's file id and the marker's `installId` are unchanged; the Desktop link is still absent; no Setup.exe process ran (process log); Navo did not start. After the wizard exits, renaming `…\Prog Ă` succeeds and is undone (positive control). |
| S3 | Upgrade of a legacy install | `Install-Raw <0.0.1 Setup.exe> 0.0.1` (raw Setup.exe to `%LocalAppData%\NavoApp`). Wizard 0.0.2. Pass: the Welcome text names the legacy path; the Folder page is locked; the root is the same (file id); no `NavoApp.<random>` sibling appeared (Setup.exe's rename, W0 Q2); the marker is written; settings in `%AppData%\Navo` are unchanged (hash before and after). |
| S3b | One install at a time | 0.0.2 installed by the wizard in the default root, then `Navo-Setup.exe --silent --installto C:\Users\u\Other\Navo` for 0.0.3. Again after `Install-Raw <0.0.1 Setup.exe> 0.0.1` on a fresh snapshot. Pass: each exits 3; `C:\Users\u\Other` holds nothing new; the HKCU `InstallLocation` is unchanged. |
| S4a | Uninstall, service, consent given | As `a` (unelevated) with `ConsentPromptBehaviorAdmin=0`. Install. Run `Install-ServiceFrom <root>\current\navo.exe` (it runs `navo.exe --index-service install` elevated; `navo-index.exe` takes only `run` and `--version`). Put `inside.txt` in the root and `beside.txt` next to it. Put a user-made `Mine.lnk` on the Desktop that targets `<root>\current\navo.exe`. Run the `UninstallString`. Pass: the root and `inside.txt` are gone; `Mine.lnk` is gone too (IW20, W0 Q6); `beside.txt`, `%AppData%\Navo` and `%LocalAppData%\Navo` stay; `sc.exe query "Navo Index"` fails; the shortcuts are gone; the HKCU key is gone. |
| S4b | Uninstall, consent declined | As `u` with `ConsentPromptBehaviorUser=0` (requests are denied). The service is installed by `a` beforehand. Pass: the root is gone, the service stays, and the Navo log names the skip. |
| S4c | Uninstall, prompt left open | Run by the owner by hand: the prompt is left open for 30 s. Pass: uninstall ends after about 25 s, the service stays, and the log names the timeout. |
| S5 | Navo running | Start 0.0.2 from the root, then wizard 0.0.3. Pass: silent exits 4; the window shows "Close Navo to continue"; Navo keeps running; nothing was written in the root (no new file in `packages\`, `Update.exe` hash unchanged); after Navo closes, Try again upgrades. |
| S6 | Refused folders | Silent `--installto` for each: `C:\`; `C:\Windows\Navo`; `C:\PROGRA~1\Navo`; `C:\Program Files\Navo`; a junction `C:\J` → `C:\Windows` with `C:\J\Navo`; `subst X: C:\Windows` with `X:\Navo`; `subst Y: C:\Users\u\Prog` with `Y:\Navo` (a harmless target, still refused); `%OneDrive%\Navo`; Documents redirected to `D:\Docs` (registry, snapshot) with `D:\Docs`; an existing empty folder; `\\localhost\c$\N`; `C:\a\..\Navo`; `C:\Users\u\a%TEMP%b\Navo`; `C:\Users\u\R&D\Navo`; `C:\Users\u\a^b\Navo`; `C:\Users\u\Hi!\Navo`. On the clean snapshot, after asserting that none exists: `%LocalAppData%\Navo`; `%LOCALAPPDATA%\navo\x\Navo`; `%AppData%\Navo`; `%LocalAppData%\NavoApp`. Pass: each exits 3, and no folder is created (listing before and after). `subst` is removed in `finally`. |
| S7 | Started elevated | As `a`, elevated: `Navo-Setup.exe --silent`. Pass: exit 7, nothing created. The window shows the Elevated page (screenshot). |
| S8 | Same and newer (downgrade guard) | 0.0.2 installed, then wizard 0.0.2, then wizard 0.0.1. Pass: both exit 8; the root is unchanged (hash listing); no Setup.exe or `Update.exe` process started (process log). |
| S9 | Shortcut choices | Fresh 0.0.2 with `--no-desktop`, then upgraded to 0.0.3 by the wizard (apply). On a fresh snapshot, fresh 0.0.2 with `--no-start-menu`, then upgraded to 0.0.3 by Navo's own update through `NAVO_UPDATE_FEED_DIR`. Pass: the unticked link is absent after the install and after each upgrade, and the ticked one still targets the root (IW11, W0 Q6). |
| S10 | Setup.exe or apply fails | Fault injection after placement passes: the payload is a fake Setup.exe, built in the VM with `Add-Type -OutputType ConsoleApplication`. `fail-empty.exe` sleeps 5 s and exits 1. `fail-partial.exe` writes `<--installto>\partial.txt`, sleeps 5 s and exits 1. Run with `--payload <fake> --payload-package <0.0.2 nupkg> --payload-version 0.0.2`. Pass: exit 5, the log path is printed, the log file exists, and no second Setup.exe started (process list). In the window, Back stays off until the fake exits; Close then removes the empty root (`fail-empty`) or names the non-empty one with Retry off (`fail-partial`). `fail-apply`: 0.0.2 installed, then the wizard with a copy of the 0.0.3 nupkg whose `Squirrel.exe` entry is a fake that exits 1. Pass: exit 5; `current\sq.version` still has 0.0.2; the root and the marker are kept; the Failed page names both logs. A real full disk is recorded once in W1 P7. |
| S11 | Offline | Save `Get-NetFirewallProfile \| Select Name,DefaultOutboundAction`. Run `Set-NetFirewallProfile -All -DefaultOutboundAction Block`. Install as in S1a. Restore each profile's saved value in `finally`. Pass: exit 0. The SSH session to the VM survives, because replies are stateful. |
| S12 | UI, keys, access, themes | For `AppsUseLightTheme` = 0, then 1 (`HKCU\…\Themes\Personalize`, restored in `finally`), using the rehearsal artifact. A UIA script runs in the interactive session through a scheduled task, as `Start-App` does. It loads `UIAutomationClient` and dumps Name, ControlType, AccessKey, IsEnabled and the toggle state of every element on each page into `setup-uia-<theme>-<page>.json`. It walks the pages with keys only: Tab, Space, Alt+N, Alt+B, Enter, Esc on Welcome. It saves a PNG of the window's `BoundingRectangle` through `System.Drawing.Graphics.CopyFromScreen`. Pass: every control has a name and role; the keys reach Finish; Esc on Welcome exits without changes. The orchestrator looks at all PNG files. |
| S13 | Planted DLL | Build `plant.dll`: its DllMain writes `C:\Users\u\plant-out\<own module name>.txt`. `plant-out` is created by the script and owned by `u`. Copy it as each of `version.dll`, `dwmapi.dll`, `uxtheme.dll`, `cryptbase.dll` and each name on W8h's delay-load list, beside `Navo-Setup.exe` in `C:\Users\u\Downloads`. Positive control first: as `u`, `rundll32.exe C:\Users\u\Downloads\version.dll,Ping` must create `plant-out\version.dll.txt`; then empty `plant-out`. Run the wizard to Finish. Pass: the control wrote its file, and `plant-out` stays empty. |
| S14 | Ambiguous installs | Snapshot set-ups, one per IW8 reason: the registry points to a missing folder; `sq.version` has id `Other`; a damaged `sq.version`; two valid folders; a non-legacy folder without a marker; a junction in the registry folder's chain; a raw Setup.exe install with `--installto C:\Users\u\R&D\Navo` (`UnsafeName`). Pass: each exits 6, and the folders are unchanged (hash listing). The window shows the Ambiguous page; the `UnsafeName` text does not mention uninstalling. |
| S15 | Upgrade root not writable | 0.0.2 installed, then `icacls <root> /deny u:(W,AD)` (restored in `finally`), then wizard 0.0.3. Pass: exit 3 with "Setup cannot write"; no `consent.exe` process appeared; no `Update.exe` process started; the root is unchanged (hash listing); no `%LocalAppData%\NavoApp` appeared. |

## Plan-check answers (Terra, second and third checks, 2026-11-09)

Rows 21–29, 37–46, 50 and 51 are in part 1.

| # | Answer |
| --- | --- |
| 30 | IW15 (part 1): the payload carries its version; tester runs pass `--payload-version`. IW4 and IW8 compare with it. |
| 31 | S1 split into S1a (silent) and S1b (window), each from its own fresh snapshot. |
| 32 | S4a installs the service with `<root>\current\navo.exe --index-service install` (now through `Install-ServiceFrom`, row 48). |
| 33 | S10 uses fake Setup.exe payloads that fail after placement passes; the real full disk stays in W1 P7. |
| 34 | S13 writes into a folder `u` owns and runs a `rundll32` positive control first. |
| 35 | IW16 and W14 name every current asset, including `RELEASES` and the PDB zip; `assets.txt` and `SHA256SUMS.txt` are written in the build job and compared in rehearsal. |
| 36 | W11 split into W11a and W11b; W15 split into W15a–W15d, one case file each, behind a dispatcher. (Stubs replaced by ordered registration, row 50.) |
| 47 | Third check. IW16 keeps release assets and manifests apart; `SHA256SUMS.txt` hashes only release assets; `out/` may hold both manifests. W14 step 5 follows. |
| 48 | `update-e2e.ps1` is never dot-sourced. `common.ps1` holds adapted copies with stated inputs: `Start-Interactive`, `Install-Raw`, `Install-ServiceFrom`, `Uninstall-Root`. S3 and S4a use them. |
| 49 | A host-side controller (`setup-e2e-host.sh`) loads the snapshot `navo-e2e` per case, runs the case in the interactive session and collects evidence. New atom W15h adds `snapshot save|load|delete` to the VM tool, with a cold-boot fallback. |
| 50 | W10a touches three files and creates no page stubs; W10b → W11a → W11b register themselves in order. W13 split into W13a (repo licence files) and W13b (the wizard's text), three files each. |
| 64 | W0 decisions (part 1, rows 52–63). IW16 and W14: the wizard embeds the full nupkg beside Setup.exe; W14 passes `NAVO_SETUP_PACKAGE` and checks `--package-sha256`. Tester runs add `--payload-package`. |
| 65 | S2b and S3 now run the apply path: S2b checks the new `Update.exe`, `packages\`, a deleted link that stays deleted and that no Setup.exe ran; S3 checks that no renamed sibling appeared. S5 checks that a block writes nothing. |
| 66 | New S3b (one install at a time) and S15 (unwritable root: exit 3, no UAC). S8 checks that no Velopack process starts (downgrade guard). W15b runs them. Done when now names S1–S15. |
| 67 | IW6 characters: S6 gains four rows; S14 gains an `UnsafeName` row. S4a checks that a user-made link into the root is deleted. S10 gains `fail-apply`, and `fail-partial` has Retry off. |
| 68 | W10b locks the folder in Upgrade; W11a names a root with partial files; W11b shows the `UnsafeName` text; W17's user guide adds shortcut deletion, the refused characters, one install and downgrades. |

## Done when

- Part 1's Done when holds.
- W14's rehearsal run is green, and every case from S1a to S15 prints PASS. S4c is done by hand.
- W16 has no must-fix left, and the PNG files were looked at in both themes.
- D113 and the docs are merged.
- The owner has set `NAVO_PUBLISH_SETUP_WIZARD`. Then the next release ships `Navo-Setup.exe` next to the raw Setup.exe.

## Log

- 2026-11-09 — Revised after the second Terra xhigh plan check (architect Turgon), rows 30–36. W11 and W15 split; S1, S4a, S10 and S13 fixed; release asset list named in full; W13 effort raised to medium (planning rule: never `low` for an edit); owner gates cleared. Still drafting.
- 2026-11-09 — Revised after the third Terra xhigh plan check (architect Turgon), rows 47–50. Manifests kept apart from release assets; adapted e2e helpers with stated inputs; a host controller with snapshot loads per case (new W15h); W10a and W13 recut, page atoms ordered. Still drafting.
- 2026-11-09 — Revised from W0 (architect Turgon), rows 64–68, following part 1's rows 52–63. The wizard embeds the full nupkg for upgrades through `Update.exe apply`; S2b, S3, S5, S8–S10 and S14 updated; new S3b and S15; docs name shortcut deletion and the refused characters. Still drafting.
- 2026-11-10 — W9: the owner picked direction **A (Rail)** from `design/prototypes/installer-wizard/index.html`, and asked for one playful touch: while Installing, the brand's n and _ spar in the rail footer (transform-only loop, ~2.4 s, off under reduced motion). This is an owner-approved exception to the fade-only motion rule, for this one spot. W10a may start.
- 2026-11-10 — Owner asked for an n/_ scene on every page and a finish greeting: rail footer scenes (Welcome wave, Licence sign, Folder floor, Shortcuts copy, Installing spar, Finish/Same high five, Failed fall, refusals "no") and on Finish a one-shot "n → _ → navo_" with "navo.. → explore it". Prototype v3 holds the exact keyframes; W10b/W11 port them as GPUI transform/opacity animations, off under reduced motion.
- 2026-11-10 — Owner left polish to the orchestrator. Added (prototype v4): Installing shows "While you wait" with four key tips cross-fading every 4 s (Ctrl+K, F5/F6, Ctrl+L, Ctrl+F7; hidden on the 3-minute page, first tip only under reduced motion); Finish lists three first keys (Ctrl+K, F1, Shift+F1) under the greeting. Tip and key texts go to setup.ftl (W12a) and must match the real keymap.
- 2026-11-10 — W10b stopped: navo-ui had no check box, no access-key primitive, and `render_page` had no action sink. Orchestrator added **W10c** (before W10b): public `primitives/check_box.rs` (Role::CheckBox, state exposed), `primitives/access_label.rs` (`&` marker, underline while Alt is held, pure `access_key` parser), `setup/strings.rs` (`Strings::english()` placeholders until W12a), `show_keys` in `SetupView`, pure `setup::access_keys` (no letter clash per page), and `render_page(view, strings, on_action)`. Alt+letter handling stays in W12b. W10a's `Box` enum is named `SetupCheck`.
- 2026-11-10 — W10c and W10b merged. Follow-ups for W12b: the Folder field is read-only (SetupView has no text-edit channel; Browse only); `Button` takes plain text, so Browse shows no underlined access key; the Apache link has no access key (no open-URL action); the check box state change has no 80 ms fade; `xtask/l10n-allow.txt` was raised 664 → 674 for placeholder strings, W12a lowers it again.
- 2026-11-10 — W15h done, with a cost finding. `savevm` refused the raw UEFI vars (`pflash1 is writable but does not support snapshots`); the orchestrator moved the vars to qcow2 (`OVMF_VARS.qcow2`, raw backup kept), and the round trip passes. But on the ~155 GB disk, `loadvm` took 306 s and `delvm` 295 s (save 15 s), slower than a cold boot. **W15a decides per run:** prefer a throwaway qcow2 overlay on the base disk plus a cold boot with auto-logon (discard = delete the overlay); use `snapshot load` only if the overlay path is slower in practice.
- 2026-11-10 — W11a and W11b merged. `SetupView.leftover_root` replaces the overloaded `notice` flag on Failed. The rail-footer n/_ spar moves to W12b (navo-ui has no rail; the window shell draws it). Blocked page wording taken from the prototype.
- 2026-11-10 — W12a done: setup.ftl holds every installer text; `Strings::load()` reads it (`english()` stays as an alias until W11g lands). Tip 4 now names Ctrl+Shift+D (compare two folders); Ctrl+F7 is Continue compare on a network drive. Setup shortcuts now compare long names (8.3 short paths failed on the GitHub runner).
- 2026-11-10 — W12b part 1 merged (cf8699ba): window, worker thread, 300 ms debounced check, stale results dropped, Close ignored while installing. Gemini argon review: REJECT (Close after exit-0 could remove the installed root; Back kept the root held; Navo started with `current\` as working folder; Extract mapped to NoPayload; WorkerLost stuck; UnsafeName text named the uninstaller); fix round a5e816b8, re-review ACCEPT. ~1,300 lines, twice the brief: the footer, access keys and editable Folder field moved to **W12c**. Window mode only without arguments (tester-payload args do not reach it: matters for S1b). Window is fixed 640×480 (no DPI scaling yet). UNTESTED on Windows: open, console hide, folder check, Browse dialog, start Navo from USERPROFILE.
- 2026-11-10 — W12c merged (b4a3625f): rail and footer shell (`navo-ui/setup/frame.rs`), pure key map (`setup/keys.rs`), Alt+letter keys (AltGr ignored), Enter = primary, Esc = Close except Installing, focus starts on the primary button, editable Folder field, 80 ms check-box fade. Gemini argon: ACCEPT WITH FIXES (Alt+R ignored the leftover-root Retry gate; focus on the invisible root; the caret blinked forever) → fixed in one round. App-wide side effect: carets rest after 5 s idle (`tokens/kit.rs`). ~830 lines, over the guide. Also on main: 20e47e34 drops navo-setup's own manifest; gpui-pre's default `windows-manifest` already carries asInvoker + PerMonitorV2 (IW13), and two manifests failed the Windows link (CVT1100). UNTESTED in the VM (S12): Alt via WM_SYSKEYDOWN, bare Alt, IME in the Folder field, focus ring, Tab order, both themes, Browse → field.
- 2026-11-11 — W14 closed on rehearsal run 42913818848: artifact holds Navo-Setup.exe, sha256sum -c passes, assets.txt matches v0.3.0 apart from versions. W15a started; the wizard ships once every case passes.
- 2026-11-11 — Owner decision: v0.7.0 waits for this wizard and ships `Navo-Setup.exe`. It publishes after V39 lands, S1–S15 pass, the owner has run S4c, W16 has no must-fix left, W17 is done and the owner has set `NAVO_PUBLISH_SETUP_WIZARD`.
- 2026-11-11 — W15a done: S1a, S1b, S2, S2b, S3, S8 and S9 PASS on the VM with the default-folder fix (`setup-default-root`, reviewed by Sonnet: APPROVE; Qwen review failed on a provider content filter). The harness found the bug: on a fresh profile `%LocalAppData%\Programs` does not exist, and the wizard exited 3. Fix: resolve the default root with `KF_FLAG_DONT_VERIFY`; create the folder through `KF_FLAG_CREATE` only when an install of the default root starts. S8/S9 no longer pre-create the folder. Owner decision: the orchestrator sets `NAVO_PUBLISH_SETUP_WIZARD` once every release condition holds. `--make-base` now needs `base.ready` moved aside to rebuild the wizards; packs and accounts are reused.
- 2026-11-11 — W15b merged (85e3706b..467718f4 cherry-picked, plus a helper split for `silent()` at 51 lines): access keys reach every page through `intercept_keystrokes`; silent failures 3–8 name the wizard and Velopack logs; the Failed page lists the wizard log first; one IW8 UnsafeName text for silent mode and the Ambiguous page. VM batch PASS S3b, S5, S6, S7, S10, S14, S15 (builder); S1a and S2 reruns pass. Gate on main: navo-setup 160 tests, xtask check all 8 steps. Sonnet review: only the function size; gap noted: the success test does not assert empty stderr and log.
