# Updates

Status: in. An installed Navo finds, downloads and applies new versions by itself. Its index service follows with one Windows prompt, and only when the service changed. The update path and the service checks pass in a Windows test machine.

## What it is

Navo reads its releases on GitHub, pre-releases included. When a newer version exists, Navo downloads it in the background. The new version takes over at a restart you choose, or at the next start.

Only a copy installed with the Navo installer updates itself. A portable or unpacked copy never asks GitHub. Settings → Help then says "This copy does not update itself." and links to the installer.

## How a check runs

- The first check runs 10 seconds after an installed Navo starts, when automatic checks are on.
- While Navo runs, it checks again every 24 hours. Time that the computer sleeps counts.
- The 24-hour check needs both Check for updates automatically and Keep in tray.
- Check now in Settings → Help checks at any time, also with automatic checks off.
- A check gives up after 30 seconds.
- Navo takes only a version newer than its own. It never goes back to an older one.

## When an update is ready

- Settings → Help shows "Navo <version> is ready." with a Restart button.
- The status bar shows the same line once per version.
- `Ctrl+K` offers "Restart to update".
- Restart waits for running jobs, undo restores, Recently deleted work and Git actions. Navo asks first, with "Restart when they finish" and "Cancel".
- Restart stops when the viewer has unsaved changes, or when the session cannot be saved. The status bar says why.
- Navo saves the session, applies the update and starts again with the same profile.

Without a restart, the update applies at the next start that opens a window. It applies only when no other Navo from this install runs. Otherwise it waits for a later start. A start right after an update never applies again, so a failed apply cannot repeat in a loop.

## Settings

| Setting | Where | What it does | Default |
| --- | --- | --- | --- |
| Check for updates automatically | Settings → Help | Turns on the start-up and 24-hour checks | On |
| Keep in tray | Settings → Tray | Also needed for the 24-hour check | Off |

In `navo.toml` the first setting is `check` under `[updates]`. A change takes effect at once.

## The index service on an update

The index service has its own revision number. It changes only when the service itself changes. Most app updates leave the service alone and show no prompt.

A few seconds after the first window opens, an installed Navo asks the running service for its revision.

- Same revision or newer: nothing happens.
- Older: Windows shows one administrator prompt (UAC). Navo then replaces the service and checks that the new one answers.
- Declined or failed: Navo does not ask again for that revision. Settings → Search shows "Older than Navo" with "Update index service".
- A stopped or silent service, a portable copy and an unpacked copy never prompt.

The service runs as its own program, `navo-index.exe`. Task Manager shows it as "Navo Index". Services shows the description "Keeps the file index for Navo search up to date."

An older service still registered as `navo.exe` shows "Older than Navo" in any state. Update moves it to `navo-index.exe` and then deletes the old `navo.exe`.

A failed install puts the previous service back: its program, its settings and its registration. A service that ran before runs again.

## Remove and uninstall

- Remove in Settings → Search asks Windows for permission, then stops and removes the service.
- Remove deletes the service programs, their backup copies and the folders they leave empty.
- Navo never deletes a file it does not know. Such a file stays, and so does its folder.
- A file that Windows will not let go is deleted at the next restart. Settings says "The index service is removed. Windows deletes its last files at the next restart."
- Uninstalling Navo removes the service too, after one administrator prompt. The uninstall waits at most 25 seconds for it. A removal that takes longer leaves the service in place and writes the reason to Navo's log.

## One change at a time

- Only one install or remove of the service runs at a time, across all Navo processes on the computer.
- When two Navo processes start together over an older service, only one asks. The other sees the change under way and stays quiet.
- Before it asks, Navo checks the service again. A service that is already current gets no prompt.
- An install that finds the service already current changes nothing.
- An install waits up to 60 seconds for another change to finish. A remove has 20 seconds in total.

## When something fails

Settings → Help shows one line for each failure:

| Line | Meaning |
| --- | --- |
| Could not reach GitHub | No network, or GitHub did not answer |
| GitHub asks Navo to wait. Next try after <time> | Rate limit. Check now waits one hour |
| The update list could not be read | The release list is broken or empty |
| The download stopped. It starts again next time | The download was cut off |
| The download was damaged and was removed | The file did not match the release |
| Another Navo is updating | A second update request is already running |
| Another Navo window is open. Close it to restart | Restart cannot apply while another Navo runs |
| The update could not start. It installs when you next start Navo | The restart hand-off failed |

Settings → Search shows the service failures:

- "The index service change did not finish. Navo's log has the reason."
- "A Navo file is in use. Close the program using it and try again."
- A cancelled Windows prompt changes nothing and shows no error.

## Decisions

- Updates come from GitHub releases through Velopack. No account and no token.
- A downloaded update never interrupts work. It waits for Restart or the next start.
- The service updates only when its revision changes, so most updates need no prompt.
- The service revision is raised by hand when the service changes.

## Done and coming

- Done: checks, background download, Restart, apply at next start, the service step, the move to `navo-index.exe`, remove with restart cleanup, and one change at a time.
- Later: an installer wizard with a chosen install folder.

## Open questions

- A stopped or silent `navo-index.exe` from an older revision gets no Update offer yet.

Refs: IX8 IX8a–IX8v4b IX8t D22 D89
