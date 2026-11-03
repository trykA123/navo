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
clip-head-this-folder = This folder
clip-head-other-notes = Other notes

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
clip-kind-note = Note
clip-note-global = Global note
clip-note-for = Note for { $name }
clip-note-empty = Empty
clip-note-label = Note text
clip-note-saved = Saved as you type
clip-note-saved-age = Saved as you type · { $age }
clip-note-not-saved = Not saved: { $reason }
clip-note-unreadable = This note could not be read, so it stays as it is.
clip-note-folder-missing = Folder not found
clip-note-detail = { $lines ->
    [one] { $lines } line
   *[other] { $lines } lines
} · { $chars ->
    [one] { $chars } character
   *[other] { $chars } characters
}
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
clip-empty-notes-title = No notes yet
clip-empty-notes-body = A note you write for a folder appears here, with the global note.
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
clip-act-edit-note = Edit note
clip-act-copy-path = Copy path
clip-act-paste-here = Paste into the active pane
clip-act-delete = Delete
clip-act-show = Show
clip-act-hide = Hide
clip-act-copy-user = Copy user name
clip-act-save-snippet = Save as snippet
clip-act-save-path = Save as path item
clip-act-save-from-history = Save from history
clip-act-new-saved = New saved item
clip-act-new-secret = New secret
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
clip-status-saved-item = Saved
clip-status-repointed = Note now belongs to { $name }
clip-status-note-exists = That folder already has a note
clip-status-no-folder = Open the palette from a Navo window to point a note at its folder

clip-form-new = New saved item
clip-form-edit = Edit saved item
clip-form-kind = Kind
clip-form-kind-snippet = Snippet
clip-form-kind-path = Path
clip-form-kind-secret = Secret
clip-form-title = Title
clip-form-title-hint = Name it so you can find it
clip-form-text = Text
clip-form-path = Path
clip-form-user = User name
clip-form-user-hint = Optional
clip-form-password = Password
clip-form-password-keep = Empty keeps the saved password
clip-form-scope = Available in
clip-form-scope-everywhere = Everywhere
clip-form-scope-folder = This folder
clip-form-no-title = Give it a title
clip-form-title-long = A title has at most { $max } characters
clip-form-no-password = A new secret needs a password
clip-form-no-body = Changing a secret into another kind needs its new text
clip-form-save = Save
clip-form-cancel = Cancel
clip-form-discard = Discard changes?
clip-form-discard-yes = Discard
clip-form-discard-no = Keep editing

clip-repoint-ask = Point this note at { $name }?
clip-repoint-yes = Move note
clip-repoint-no = Cancel

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
