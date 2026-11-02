clip-palette = Clipboard and saved items
clip-search-placeholder = Search clipboard and saved
clip-search-label = Search
clip-modes-label = Modes
clip-results-label = Clipboard items
clip-preview-label = Preview
clip-mode-all = All
clip-mode-recent = Recent
clip-mode-saved = Saved
clip-mode-secrets = Secrets
clip-mode-notes = Notes
clip-head-recent = Recent clipboard
clip-head-saved = Saved

clip-manage-kinds-label = Kinds
clip-manage-history = History
clip-manage-snippets = Snippets
clip-manage-commands = Commands
clip-manage-paths = Paths
clip-manage-secrets = Secrets
clip-manage-notes = Notes
clip-manage-unavailable = { $kind }, not available yet
clip-manage-back = Back

clip-kind-text = Text
clip-kind-link = Link
clip-kind-path = Path
clip-kind-files = Files
clip-kind-image = Image
clip-kind-secret = Secret
clip-preview-from = { $kind } from { $app }
clip-meta-copied = Copied
clip-meta-used = Used
clip-meta-folder = Folder
clip-meta-kind = Kind
clip-meta-size = Size
clip-scope-global = Global
clip-size-chars = { $count ->
    [one] { $count } character
   *[other] { $count } characters
}
clip-size-files = { $count ->
    [one] { $count } file
   *[other] { $count } files
}
clip-size-hidden = Hidden
clip-secret-note = Copied without showing, cleared from the clipboard after 30 s, never recorded in history. Stored encrypted on this PC.

clip-kept = { $count } of 500 kept
clip-matches = { $count ->
    [one] { $count } match
   *[other] { $count } matches
}
clip-items = { $count ->
    [one] { $count } item
   *[other] { $count } items
}
clip-vault-line = Encrypted on this PC
clip-intro = Navo keeps what you copy on this PC. Settings › Clipboard turns it off.

clip-empty-history-title = Nothing copied yet
clip-empty-history-body = Text and file lists you copy in any app appear here, newest first. Copies that apps mark private are never recorded.
clip-empty-saved-title = Nothing saved yet
clip-empty-saved-body = Keep snippets, commands and paths here.
clip-empty-secrets-title = No secrets yet
clip-empty-secrets-body = Secrets stay masked, are stored encrypted on this PC and leave the clipboard after 30 s.
clip-empty-search-title = No matches
clip-empty-search-body = Nothing in history or saved items has this text. Secrets match by name and user only.

clip-act-copy = Copy
clip-act-go = Go to
clip-act-copy-hidden = Copy, hidden
clip-act-paste = Paste
clip-act-paste-hidden = Paste, hidden
clip-act-copy-only = Copy only
clip-act-actions = Actions
clip-act-copy-path = Copy path
clip-act-paste-here = Paste into the active pane
clip-act-delete = Delete
clip-act-show = Show
clip-act-hide = Hide
clip-act-copy-user = Copy user name
clip-act-save-snippet = Save as snippet
clip-act-save-path = Save as path item
clip-act-save-from-history = Save from history
clip-act-manage = Manage…
clip-key-clear = Clear search

clip-status-copied = Copied
clip-status-deleted = Deleted
clip-status-went = Opened in the active pane
clip-status-pasting = Pasting into the active pane
clip-status-bad-path = That is not a folder path
clip-status-copied-hidden = Copied without showing
clip-status-copied-user = Copied user name
clip-status-clears = Clears in { $secs } s
clip-status-cleared = Clipboard cleared
clip-status-saved-snippet = Saved as snippet
clip-status-saved-path = Saved as path item

clip-clear-title = Clear clipboard history?
clip-clear-count = { $count ->
    [one] { $count } history item
   *[other] { $count } history items
}
clip-clear-summary = Saved items, secrets and notes stay.
clip-clear-confirm = Clear history
clip-clear-done = Clipboard history cleared

clip-wipe-title = Delete all clipboard data?
clip-wipe-history = { $count ->
    [one] { $count } history item
   *[other] { $count } history items
}
clip-wipe-saved = { $count ->
    [one] { $count } saved item or secret
   *[other] { $count } saved items and secrets
}
clip-wipe-notes = { $count ->
    [one] { $count } note
   *[other] { $count } notes
}
clip-wipe-summary = The set-aside copies of the database, clipboard.db.corrupt-*, are deleted too. This cannot be undone.
clip-wipe-confirm = Delete all
clip-wipe-done = All clipboard data deleted

clip-pause-on = Clipboard history paused
clip-pause-off = Clipboard history recording again

clip-age-now = Just now
clip-age-minutes = { $count } min
clip-age-hours = { $count } h
clip-age-yesterday = Yesterday
clip-age-days = { $count } days
