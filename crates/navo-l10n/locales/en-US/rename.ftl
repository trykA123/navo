rename-title = { $count ->
    [one] Rename 1 item
   *[other] Rename { $count } items
} · { $folder }
rename-presets = Presets
rename-name-mask = Name mask
rename-extension-mask = Extension mask
rename-insert-token = Insert a token
rename-find = Find
rename-replace = Replace with
rename-regex = Regex
rename-match-case = Match case
rename-first-only = First only
rename-name-only = Name only
rename-name-case = Name case
rename-extension-case = Extension case
rename-counter-start = Start
rename-counter-step = Step
rename-counter-digits = Digits
rename-case-unchanged = Unchanged
rename-case-lower = lower case
rename-case-upper = UPPER CASE
rename-case-first = First letter
rename-case-title = Title Case
rename-cleanup-trim = Trim spaces
rename-cleanup-collapse = Collapse repeats
rename-cleanup-accents = Remove accents
rename-cleanup-underscore = Spaces to _
rename-column-old = Old name
rename-column-new = New name
rename-column-status = Status
rename-problems-only = Problems only
rename-edit-names = Edit names
rename-load-names = Load names…
rename-paste-names = Paste names
rename-copy-names = Copy new names
rename-counter-per-folder = Counter per folder
rename-names-label = New names, one per line
rename-names-count = Expected { $expected } names; found { $actual }. Correct the list before renaming.
rename-names-load-failed = Could not load names: { $reason }
rename-names-clipboard-empty = The clipboard contains no text
rename-status-unchanged = Unchanged
rename-status-excluded = Excluded
rename-status-edited = Edited
rename-status-case-only = Case only
rename-status-swap = Swap
rename-status-exists = Already exists
rename-status-same-name = Same new name as row { $row }
rename-status-bad-character = Cannot contain { $character }
rename-status-empty = Name is empty
rename-status-reserved = Name is not allowed
rename-status-long-path = Path is longer than 260 characters
rename-status-loading = …
rename-summary = { $changes } to rename · { $unchanged } unchanged · { $problems ->
    [one] 1 problem
   *[other] { $problems } problems
}
rename-summary-excluded = { $count } excluded
rename-hints = Enter rename · Esc close · Ctrl+Space tokens · Alt+↓ next problem · Ctrl+S save preset
rename-cancel = Cancel
rename-run = Rename
rename-error-token = Unknown token { $token } at position { $position }
rename-error-regex = The regular expression is not valid
rename-error-counter = Digits must be from 1 to 10 and Step at least 1
rename-token-menu-filter = Type to filter tokens
rename-token-menu-empty = No token matches
rename-token-name = [N] · the whole name
rename-token-name-char = [N3] · the 3rd character of the name
rename-token-name-range = [N2-5] · characters 2 to 5 of the name
rename-token-name-from = [N2-] · from the 2nd character to the end
rename-token-name-count = [N2,5] · 5 characters from the 2nd
rename-token-extension = [E] · the whole extension
rename-token-extension-range = [E2-3] · characters 2 to 3 of the extension
rename-token-parent = [P] · the parent folder name
rename-token-grandparent = [G] · the grandparent folder name
rename-token-counter = [C] · the counter
rename-token-counter-own = [C10+5:3] · counter from 10, step 5, 3 digits
rename-token-year-full = [Y] · year, 4 digits
rename-token-year = [y] · year, 2 digits
rename-token-month = [M] · month
rename-token-day = [D] · day
rename-token-hour = [h] · hour
rename-token-minute = [i] · minute
rename-token-second = [s] · second
rename-token-date = [d] · date as YYYY-MM-DD
rename-token-time = [t] · time as hh.mm.ss
rename-token-upper = [U] · upper case from here
rename-token-lower = [L] · lower case from here
rename-token-first = [F] · first letter of each word in upper case
rename-token-original = [n] · original case from here
rename-token-open = [[ · a literal [
rename-token-close = ]] · a literal ]
rename-field-file-size = [=file.size] · size in bytes
rename-field-file-size-kb = [=file.size_kb] · size in KB, rounded up
rename-field-file-created = [=file.created] · date created
rename-field-file-accessed = [=file.accessed] · date last opened
rename-field-file-attrs = [=file.attrs] · attributes (R H S A)
rename-field-exif-date = [=exif.date] · date the photo was taken
rename-field-exif-camera = [=exif.camera] · camera maker
rename-field-exif-model = [=exif.model] · camera model
rename-field-exif-width = [=exif.width] · photo width in pixels
rename-field-exif-height = [=exif.height] · photo height in pixels
rename-field-audio-artist = [=audio.artist] · artist
rename-field-audio-title = [=audio.title] · title
rename-field-audio-album = [=audio.album] · album
rename-field-audio-track = [=audio.track] · track number
rename-field-audio-year = [=audio.year] · year
rename-field-audio-genre = [=audio.genre] · genre
rename-presets-last-used = Last used
rename-presets-empty = No saved presets
rename-presets-open = Open a preset
rename-presets-save = Save the preset
rename-presets-name = Preset name
rename-presets-replace = Replace the preset { $name }?
rename-presets-delete = Delete the preset { $name }?
rename-presets-delete-confirm = Delete
rename-open-loading = Wait until the folder has loaded
rename-busy = Wait until the running rename ends
rename-nothing = Nothing to rename.
rename-folder-changed = The folder changed. Check the preview, then press Enter again.
rename-presets-failed = The preset could not be stored: { $reason }
undo-nothing = Nothing to undo
undo-read-failed = The undo history could not be read: { $reason }
undo-retained-busy = Wait until the last undo ends
undo-retained-done = Restored { $count ->
    [one] { $count } item
   *[other] { $count } items
}
undo-retained-partial = Restored { $restored } of { $total } items. { $failed } could not be restored: { $reason }
undo-retained-none = { $total ->
    [one] The item
   *[other] The { $total } items
} could not be restored: { $reason }
undo-retained-start-failed = The undo could not start: { $reason }
undo-ask = Undo the rename of { $count ->
    [one] { $items } item
   *[other] { $items } items
} in { $folder }?
undo-ask-label = Undo
undo-ask-summary = Names go back as they were. Items that changed since are left as they are.
undo-finish-ask = An earlier undo of a rename did not finish. Finish it now?
undo-finish-label = Finish the undo
undo-finish-summary = Names that are already back stay as they are.
undo-pending-notice = A rename undo did not finish.
undo-pending-notice-key = A rename undo did not finish. Press { $key } to finish it.
undo-verb = Undo rename of
undo-verb-running = Undoing rename of
undo-verb-done = Undid rename of
