git-state-conflict = Conflict
git-state-modified = Modified
git-state-ahead = Ahead
git-state-behind = Behind
git-state-diverged = Diverged
git-state-clean = Clean
git-state-unknown = Unknown
git-state-checking = Checking

git-spoken = Git: { $state }
git-spoken-ahead = { $count } ahead
git-spoken-behind = { $count } behind
git-column = Git

git-inside = Inside a repository
git-branch-upstream = { $branch } → { $upstream }
git-detail-conflict = { $count ->
    [one] 1 conflict
   *[other] { $count } conflicts
}
git-detail-modified = { $count } changed
git-detail-new = { $count } new
git-detail-clean = Up to date
git-detail-clean-below = Nothing changed below
git-detail-changed-below = { $count } changed below
git-detail-checking = First check running
git-detail-timed-out = Timed out
git-detail-no-git = Install Git to see changes
git-detail-not-trusted = Git does not trust this folder, or a Git filter is set. Install Git for full status
git-detail-failed = Git could not read this folder
git-detail-state = { $state }: { $detail }
git-tip-counts = { $ahead } ahead · { $behind } behind · fetched { $time }
git-tip-never-fetched = { $ahead } ahead · { $behind } behind · not fetched yet

git-menu-aria = Git menu for { $name }
git-menu-fetched = fetched { $time }
git-menu-via = via
git-menu-no-tool = No Git tool set
git-menu-rebindable = rebindable
git-menu-recheck = Check again
git-menu-choose-note = Settings → Git
git-menu-unknown = Status unknown
git-menu-finish-merge = Finish the merge first
git-menu-resolve = Resolve { $count ->
    [one] 1 conflict
   *[other] { $count } conflicts
} first
git-menu-changed = { $count } changed
git-menu-changed-new = { $count } changed · { $new } new
git-menu-nothing-to-commit = Nothing to commit
git-menu-behind = { $count ->
    [one] 1 commit behind
   *[other] { $count } commits behind
}
git-menu-behind-ff = { $count ->
    [one] 1 commit behind · fast-forward
   *[other] { $count } commits behind · fast-forward
}
git-menu-fetch-merge = Fetch and merge
git-menu-diverged = Diverged: merge needed
git-menu-needs-merge = Needs a merge: use a Git tool
git-menu-ahead = { $count ->
    [one] 1 commit ahead
   *[other] { $count } commits ahead
}
git-menu-nothing-to-push = Nothing to push
git-menu-pull-first = Pull first: histories diverged
git-menu-this-folder = this folder
git-menu-working-tree = working tree
git-menu-started = { $action } sent to { $tool }

git-item-commit = Commit…
git-item-pull = Pull…
git-item-push = Push…
git-item-log = Log…
git-item-diff = Diff…
git-item-open = Open in { $tool }
git-item-choose = Choose a Git tool…

git-confirm-pull = Pull { $count ->
    [one] 1 commit
   *[other] { $count } commits
} into { $branch } from { $remote }/{ $remote_branch }. Fast-forward only; nothing changes if a merge is needed.
git-confirm-push = Push { $count ->
    [one] 1 commit
   *[other] { $count } commits
} from { $branch } to { $remote }/{ $remote_branch }. Never forced.
git-confirm-push-creates = Push { $branch } to { $remote } and create { $remote_branch } there. Never forced.
git-confirm-head = { $branch } is at { $head }.
git-confirm-counted = Counted from the last fetch.
git-confirm-never-fetched = Not fetched yet, so the count may be out of date.
git-confirm-guarded = This repository sets { $keys }, which can run programs during the operation.

git-done-up-to-date = { $branch } is already up to date
git-done-pulled-count = Pulled { $count ->
    [one] 1 commit
   *[other] { $count } commits
} into { $branch } from { $remote }
git-done-push-nothing = Nothing to push from { $branch }
git-done-pushed = Pushed { $branch } to { $remote }

git-tree-may-have-changed = The working tree may have changed.
git-fail-no-tool = No Git tool is set. Choose one in Settings → Git.
git-fail-busy = A Pull or Push is already running in this repository.
git-fail-reopen = The confirmation is no longer valid. Open the Git menu again.
git-fail-not-local = Git actions need a folder on this computer.
git-fail-bad-path = This folder's path cannot be passed to a Git tool.
git-fail-not-ff = Pull needs a merge; use your Git tool
git-fail-local-changes = Pull would overwrite local changes; commit or stash them first
git-fail-rejected = The remote rejected the push; pull first
git-fail-auth = Git could not sign in to the remote
git-fail-no-upstream = This branch has no upstream to pull from or push to
git-fail-unsafe-remote = The remote's name is not safe to pass to Git
git-fail-refspec = The remote has a custom fetch setup; use your Git tool
git-fail-remote-gone = The branch is gone on the remote; Navo will not recreate it
git-fail-detached = HEAD is detached; switch to a branch first
git-fail-config-changed = The repository settings changed since you confirmed. Nothing ran; open the Git menu again.
git-fail-target-changed = The branch, remote or commits changed since you confirmed. Nothing ran; open the Git menu again.
git-fail-interrupted = Git was stopped before it finished.
git-fail-interrupted-lock = It may have left .git/index.lock.
git-fail-interrupted-moved = HEAD has moved.
git-fail-unfinished = The pull did not reach the upstream or left work behind.
git-fail-cancelled = The push was cancelled. The remote may have been updated.
git-fail-timed-out = Git took too long. The remote may have been updated.
git-fail-no-git = Git is not installed or is too old.
git-fail-not-trusted = Git does not trust this folder, or a Git filter is set.
git-fail-failed = Git reported an error

git-tool-auto = Auto
git-tool-auto-resolved = Auto — { $tool }
git-tool-git-cli = Git command line
git-tool-other = Other…
git-tool-none = None

git-tool-tortoise-git = TortoiseGit
git-tool-fork = Fork
git-tool-github-desktop = GitHub Desktop
git-tool-source-tree = SourceTree
git-tool-git-kraken = GitKraken
git-tool-sublime-merge = Sublime Merge
git-tool-vs-code = VS Code
git-tool-git-command-line = Git command line
git-tool-custom = Custom command
