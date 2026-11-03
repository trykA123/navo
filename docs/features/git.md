# Git status and menu

Status: in. Navo marks Git repositories in the file list and opens your Git tool from a small menu. The built-in checks pass. A pass on a real Windows desktop is still to come.

## What it is

When a folder in the list is a Git repository, a badge in the left column shows its state. A click on the badge opens a menu with the usual actions. Navo hands the actions to the Git tool you choose. With the Git command line as the tool, Navo runs pull and push itself, after a confirmation.

## The badges

Each badge has a glyph and a colour from the theme.

| Badge | Meaning |
| --- | --- |
| Warning triangle | Conflict |
| Dotted circle | Modified: changed or new files |
| Arrow up | Ahead of the upstream branch |
| Arrow down | Behind the upstream branch |
| Up and down arrows | Diverged: both ahead and behind |
| Check circle | Clean (repository root) |
| Quiet dot | Clean subfolder inside a repository |
| Question mark | Unknown. The tooltip gives the reason |
| Dashed circle | The first check is still running |

- A repository that is ahead or behind shows `N↑ M↓` after its name.
- A folder inside a repository shows the worst state of everything below it. Conflict outranks modified.
- The badge is dimmed while the state is out of date.
- Hover a badge for a short tooltip. The tooltip shows the ahead and behind counts and when the repository was last fetched.

## How the status is read

- Navo uses `git.exe` when Git is installed.
- Without Git, Navo uses a built-in reader. It refuses a repository that sets a Git filter or an fsmonitor. The badge then reads Unknown and the tooltip says why. Install Git for full status.
- Settings → Git → Git engine can force one reader. The default is Automatic.

## What never runs

Reading status never runs code from the repository. Navo turns off fsmonitor and neutralises every filter it finds in the repository's configuration. Background fetches also skip repository hook files and automatic maintenance.

## The Git menu

- Click a badge, or press Alt+G in the file list.
- The menu shows the branch, the upstream and a short state line, for example "3 changed · 1 new" or "2 commits behind · fast-forward".
- Which items appear depends on the Git tool:
  - TortoiseGit and a custom command: Commit, Pull, Push, Log and Diff.
  - Git command line: Pull, Push, Commit and Log. It has no Diff.
  - Fork, GitHub Desktop, SourceTree, GitKraken, Sublime Merge and VS Code: one item, "Open in" the tool.
  - No tool found: only "Choose a Git tool…", which opens Settings → Git.
- "Check again" reads the status once more.

Commit, Pull, Push, Log and Diff are shown only when the tool can do them. Diff stays hidden until a tool can diff.

### Pull

- Pull is fast-forward only.
- A confirmation names the branch, the remote and the number of commits. It also shows the commit the branch is at.
- The count comes from the last fetch. The confirmation says so, or says that nothing was fetched yet.
- If the repository sets options that can run programs during the operation, the confirmation names them.
- If a merge is needed, nothing changes. Navo tells you to use your Git tool.

### Push

- Push sends the current branch to its upstream branch.
- A push is never forced.
- If the branch has no upstream yet, the confirmation says Navo will create it on the remote.
- A push is refused when the branch was deleted on the remote. Navo does not recreate it.

### What is refused, and why

- A detached HEAD: switch to a branch first.
- A branch without an upstream, for pull.
- A pull that would overwrite local changes: commit or stash first.
- A pull when the histories diverged, and a push the remote rejects: pull first or use your Git tool.
- A remote name that is not safe to pass to Git, or a remote with a custom fetch setup.
- A repository whose settings changed after you confirmed, or whose branch, remote or commits changed. Nothing runs. Open the menu again.
- A repository Git does not trust, or one with a Git filter, when Git itself is not installed.
- Folders that are not on this computer.

If Git is stopped midway, the message says so. It adds a note when `.git/index.lock` may be left behind or when HEAD has moved.

## Settings → Git

| Setting | What it does | Default |
| --- | --- | --- |
| Show Git status on folders | Turns the badges on or off | On |
| Git engine | Automatic, built-in reader, or Git command line | Automatic |
| Git tool | Auto, a tool Navo found, Git command line, Other… or None | Auto |
| Command for Other | Your own command line. `{path}` is the repository folder and stands alone as an argument. `{action}` becomes commit, pull, push, log or diff | empty |
| Check for new commits in the background | Runs `git fetch` on repositories you have open | Off |
| Check every N minutes | Minutes between checks, from 5 to 120 | 15 |
| Look at network drives | Shows status for repositories on network drives | Off |

- Auto picks the first installed tool in this order: TortoiseGit, Fork, GitHub Desktop, SourceTree, GitKraken. It never picks the command line.
- The background fetch is the only time Navo writes into a repository on its own. It updates the remote-tracking branches.
- A slow network share can make the list slower, so network drives are off.

## Known limits

- A repository with more than 4096 folders with changes shows Unknown on its subfolders. The repository root still shows its state.
- The Windows parts have not been run by a person yet: tool detection, launching the tools and the Git command line on Windows.
