# Install

Status: built, not released. The Navo installer wizard passes its tests in a Windows test machine. It is not published yet. v0.7.0 waits for it. Until then, the installer download on GitHub is the plain Velopack setup without the wizard, and the portable zip stays available.

## What it is

`Navo-Setup.exe` is a guided installer for one user. It has five pages: Welcome, Licence, Install folder, Shortcuts and Finish. It never asks for administrator rights. It works offline, because everything it needs is inside the file.

Velopack still does the install, the update and the uninstall. The wizard adds the choice of folder and the safety checks around it. Updates keep working from any folder Setup accepts. See [updates.md](updates.md).

## The pages

- Welcome says "Install Navo <version>". If Navo is already installed, it names the folder and says "Setup updates it there. Your settings and shortcuts stay."
- Licence shows the licence text. You tick "I accept the licence" before Next.
- Install folder shows the folder Setup will create, the space needed and the free space. Browse picks a parent folder. Setup always installs into a new `Navo` folder inside it.
- Shortcuts has two boxes, both ticked: Start menu and Desktop. An update skips this page.
- Installing shows the folder and the elapsed time. After 3 minutes it says "Setup is still working…" and names the log. Back and Close stay off until the install step has ended.
- Finish says "Navo is installed." The folder is on its own line below. An Open folder link and a ticked "Start Navo" follow. Navo is not started until you click Finish.
- Failed says what went wrong in plain words. It names the log and offers Copy details and Back.

## Keys

- Enter presses the main button.
- Esc closes the wizard before the install starts. Nothing has changed by then.
- Alt+B goes back. Alt+N goes next. Alt shows the access keys.
- Tab moves through the page. Space ticks a box.
- Focus starts on the main button and is always visible.

## The install folder

The default is `Programs\Navo` inside your own Local AppData folder. Setup shows this line under the field:

"Setup creates this folder. Uninstalling Navo deletes it with everything inside, so keep your own files elsewhere."

Setup takes a new folder only. It refuses a folder when:

- the path has a `%`, `^`, `&` or `!` in it, because the uninstall step cannot handle these characters;
- the folder already exists;
- the path crosses a link (reparse point), a `subst` drive or a network drive;
- the folder is a system, profile or OneDrive folder;
- the drive has less free space than four times the size of the install;
- Setup cannot write to it.

A refused folder gets one line that says what is wrong and what to do.

### Folders others could change

Setup also refuses a folder when another user on the PC could change a folder above it. This covers:

- a folder made by a user directly under `C:\`, such as `C:\Tools`;
- a drive root that lets other users delete or rename its children, for example a root that gives Everyone full control;
- FAT32 and exFAT drives, which have no access lists.

The message is: "Other people on this PC can change a folder above this one. Choose a folder directly on a drive, such as C:\Navo."

A folder made directly under `C:\`, such as `C:\Navo`, works for a standard user.

Every folder Setup creates gets a protected access list. Only you, SYSTEM and Administrators can change it. Setup holds these folders while it works.

## Install, update and repair

- Only one Navo install exists at a time. While Navo is installed, Setup offers no other folder.
- A newer Setup upgrades the existing install in place. The folder shows locked, and the Shortcuts page is skipped.
- Same version installed: the page offers Open Navo and Close. Setup starts nothing.
- Newer version installed: the page says so and offers Close. To go back to an older version, uninstall first. Setup never downgrades.
- A damaged or unclear install shows "Repair by uninstalling first: remove Navo in Windows Settings → Apps, then run Setup again." Setup does not touch that folder.
- An install made by the older installer upgrades in place, in its old folder.
- If Navo runs from the folder, the page says "Close Navo to continue" with Try again. Setup never closes Navo itself.
- If you start Setup as administrator, it asks you to start it normally and offers Close only.

## Shortcuts

- An unticked shortcut is deleted after a fresh install.
- An update never creates a shortcut again, and never removes one you made.

## After the install step

A fresh install counts as done only when six checks pass. One of them is a marker file that Setup writes into the folder.

If the install step reports success but a check fails, the wizard does not claim success. It says to remove Navo in Windows Settings → Apps. It never offers to delete the folder. A failed install can leave files, because Velopack does not roll back a fresh install. Setup removes only the empty folders it made.

## Silent install

`Navo-Setup.exe --silent` installs without a window and follows the same rules. Options: `--installto <new folder>`, `--no-desktop` and `--no-start-menu`. While Navo is installed, `--installto` may name only that folder. Messages go to the error stream. Silent mode never starts Navo.

| Exit code | Meaning |
| --- | --- |
| 0 | Installed or upgraded |
| 2 | Bad arguments |
| 3 | Folder refused, or a second folder while Navo is installed |
| 4 | Navo is running |
| 5 | The install step failed, or a fresh install left files |
| 6 | The existing install is unclear |
| 7 | Setup runs as administrator, or this is unknown |
| 8 | The same or a newer version is installed |
| 9 | The file has no payload |

## Uninstall

Remove Navo in Windows Settings → Apps.

- Uninstall deletes the whole install folder, with everything inside.
- It also deletes every shortcut that points into that folder. This includes shortcuts you made yourself.
- Your settings and your data folders stay.
- Uninstall removes the index service after one administrator prompt. It waits at most 25 seconds. If you refuse the prompt, or the removal takes too long, the service stays. Remove it later with `sc.exe delete "Navo Index"` in an administrator window.

## Safety and trust

- The threat is a careless folder choice. The wizard does not defend against someone who already runs as you, or has administrator rights.
- Setup loads Windows libraries from the system folder only. This stops a planted library next to the file.
- The install files inside Setup carry a hash. It finds damage. It does not prove who made the file. The build is not signed yet.
- A Navo that starts after Setup's last check can be stopped by the update step. Setup does not scan programs of other users or other sessions.

## Not switched on yet

- The wizard is not part of any release. The repository setting that publishes it is off.
- Until it is, the release holds the plain Velopack setup and the portable zip.
- Code signing is not set up. Windows may warn about the file.

## Known issues

- Windows UI Automation shows an empty access key on every element. The Alt keys still work.
- A refused administrator prompt at uninstall still waits the full 25 seconds before the service step ends.

## Decisions

- Velopack stays the engine. The wizard wraps it.
- The install is for one user, never elevated, and offline.
- Setup takes a new folder only, and treats the folder as its own.
- A folder with an open parent is refused. The owner may revisit this rule.
- The wizard is published only when the owner switches it on. v0.7.0 waits for it.

## Done and coming

- Done: all five pages, the folder rules, protected folders, the upgrade in place, the checks after install, silent install, uninstall behaviour, and a review with fixes.
- Coming: release with v0.7.0, code signing, a long soak test and an update check on the owner's own PC.

## Open questions

- Should Setup allow a folder under an open parent after a clear warning? Today it refuses.

Refs: IW0–IW24 IW-D16 W16a W16b D88 D89 D115
