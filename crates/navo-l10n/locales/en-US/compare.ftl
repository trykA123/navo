cmd-compare-with-other-pane = Compare with the other pane
cmd-compare-refs = Compare branches…
cmd-compare-next-difference = Next difference
cmd-compare-previous-difference = Previous difference
cmd-compare-copy-right = Copy to the right
cmd-compare-copy-left = Copy to the left
cmd-compare-enter = Open in both panes
cmd-compare-up = Up in both panes
cmd-compare-exit = Leave compare
cmd-compare-save = Save edited side
cmd-compare-stop-checking = Stop checking contents
cmd-compare-continue = Continue the compare on a network drive
cmd-compare-view-bytes = Show bytes
cmd-compare-view-text = Show text
cmd-compare-show-bytes-here = Show bytes here
compare-needs-two-panes = Compare needs two panes side by side.
compare-could-not-start = The compare could not start.
compare-filter-all = All
compare-filter-differences = Differences
compare-filter-orphans = Orphans
compare-filter-same = Same
compare-filter-label = Show
compare-total-different = { $count } different
compare-total-only-left = { $count } only left
compare-total-only-right = { $count } only right
compare-total-same = { $count } same
compare-total-checking = { $count } checking
compare-running = Comparing…
compare-stopped = Stopped at the limit on a network drive: { $count } entries so far. Continue?
compare-continue = Continue
compare-continue-tip = Go on listing the folders from where the compare stopped
compare-leave = Leave compare
compare-leave-tip = Leave compare and return both panes to where they were
compare-empty-waiting = Comparing…
compare-empty-folders = Both folders are empty.
compare-empty-folder = This folder is empty on both sides.
compare-empty-identical = The folders are identical.
compare-empty-filter = No rows match this filter.
compare-gap = Nothing on this side
compare-list-left = Left side of the compare
compare-list-right = Right side of the compare
compare-progress = { $done } of { $total }
compare-status-same = same
compare-status-same-after-rules = same after rules
compare-status-same-size-time = same size and time
compare-status-different = different
compare-status-contains-differences = contains differences
compare-status-only-left = only left
compare-status-only-right = only right
compare-status-type-conflict = type conflict
compare-status-case-conflict = case conflict
compare-status-checking = same size and time · checking
compare-status-not-checked = Not checked ({ $reason })
compare-status-unavailable = unavailable: { $reason }
compare-row-not-in-left = not in left
compare-row-not-in-right = not in right
compare-reason-access-denied = access denied
compare-reason-locked = locked
compare-reason-not-found = not found
compare-reason-read-error = read error
compare-reason-stopped = stopped
compare-reason-link = link
compare-reason-submodule = submodule
compare-reason-unsupported = not supported
compare-reason-changed-on-disk = changed while reading
compare-reason-worker-panicked = internal error
compare-reason-disk-full = not enough disk space
compare-reason-missing-blob = missing git blob
cmd-compare-stream = Stream all differences
cmd-compare-next-file = Next changed file
cmd-compare-previous-file = Previous changed file
cmd-compare-view-menu = View menu of the file diff
cmd-compare-changes = List of changes
cmd-compare-details = Details of the pair
cmd-compare-toggle-viewed = Mark file as viewed
cmd-compare-fold = Fold or unfold the card
cmd-compare-open-file = Open the file diff
cmd-compare-search = Search the differences
cmd-compare-copy-all-left = Copy all matches to the left
cmd-compare-copy-all-right = Copy all matches to the right
cmd-compare-toggle-wrap = Wrap lines
cmd-compare-pick-down = Pick the next line
cmd-compare-pick-up = Pick the previous line
cmd-compare-pick-toggle = Pick or unpick the line
cmd-compare-remove-picked = Remove the picked lines
cmd-compare-show-menu = Open the menu at the cursor line
compare-screen-back = Back
compare-screen-comparing = Comparing…
compare-screen-sides = { $left } ↔ { $right }
compare-screen-esc = Esc goes back.
compare-encoding-none = not present
compare-encoding-utf-8 = UTF-8
compare-encoding-utf-16le = UTF-16 LE
compare-encoding-utf-16be = UTF-16 BE
compare-encoding-windows-1250 = Windows-1250
compare-encoding-windows-1251 = Windows-1251
compare-encoding-windows-1252 = Windows-1252
compare-encoding-windows-1253 = Windows-1253
compare-encoding-windows-1254 = Windows-1254
compare-encoding-windows-1255 = Windows-1255
compare-encoding-windows-1256 = Windows-1256
compare-encoding-windows-1257 = Windows-1257
compare-encoding-windows-1258 = Windows-1258
compare-encoding-iso-8859-2 = ISO-8859-2
compare-encoding-iso-8859-15 = ISO-8859-15
compare-encoding-koi8-r = KOI8-R
compare-encoding-shift-jis = Shift_JIS
compare-encoding-euc-jp = EUC-JP
compare-encoding-gbk = GBK
compare-encoding-big5 = Big5
compare-encoding-euc-kr = EUC-KR
compare-encoding-ibm866 = IBM866
compare-encoding-other = other encoding
compare-endings-none = no line ending
compare-endings-lf = LF
compare-endings-crlf = CRLF
compare-endings-mixed = mixed line endings
compare-fallback-token-cap = over the line limit
compare-screen-rows = Rows of the file diff
compare-screen-change = Change { $current } of { $total }
compare-screen-changes = { $total ->
    [one] 1 change
   *[other] { $total } changes
}
compare-screen-no-changes = No changes
compare-screen-looking-further = Looking further… { $percent } %
compare-screen-comparing-percent = Comparing… { $percent } %
compare-screen-notice-coarse-budget = { $sections ->
    [one] 1 section shown as one block to stay in the memory budget.
   *[other] { $sections } sections shown as one block to stay in the memory budget.
}
compare-screen-notice-coarse-cap = { $sections ->
    [one] 1 section shown as one block: too large to compare line by line.
   *[other] { $sections } sections shown as one block: too large to compare line by line.
}
compare-screen-notice-paged = Changes far from the view are kept as counts to stay in the memory budget.
compare-screen-budget-link = Raise the memory budget
compare-card-viewed = Viewed
compare-card-gap = { $lines ->
    [one] 1 unchanged line
   *[other] { $lines } unchanged lines
}
compare-card-more = { $hunks ->
    [one] 1 more change · Enter opens the full diff
   *[other] { $hunks } more changes · Enter opens the full diff
}
compare-card-others = { $hunks ->
    [one] 1 other change
   *[other] { $hunks } other changes
}
compare-card-large-found = { $hunks ->
    [one] 1 change so far
   *[other] { $hunks } changes so far
}
compare-card-large-full = { $hunks ->
    [one] 1+ change · Enter opens the full diff
   *[other] { $hunks }+ changes · Enter opens the full diff
}
compare-card-large-hunk = Lines { $left } → { $right }
compare-card-hex-first = First change at byte { $offset }
compare-card-rules = { $count ->
    [one] 1 file is the same after the rules
   *[other] { $count } files are the same after the rules
}
compare-card-open-f3 = Open with F3
compare-card-summary-large = Large text file · { $left } and { $right }
compare-card-summary-binary = Binary file
compare-card-summary-image = Image
compare-card-summary-left = Only on the left
compare-card-summary-right = Only on the right
compare-card-summary-text = No changes to show
compare-copy-changed = Changed since shown. Nothing was copied.
compare-copy-read-only = This side is read-only. Nothing was copied.
compare-copy-not-copyable = This change cannot be copied.
compare-copy-failed = The change was not copied: { $reason }
compare-copy-bar-left = Copy to left
compare-copy-bar-right = Copy to right
compare-copy-pick = Pick a change first. Nothing was copied.
compare-copy-no-digest = This change is too large or too coarse to copy. Nothing was copied.
compare-copy-running = A copy is still running. Wait for it to end.
compare-copy-pick-side = The picked lines are on the other side. Nothing was copied.
compare-bytes-here-none = Put the cursor on a line over 1 MiB to show its bytes
compare-remove-whole = Pick every line of a change that exists on one side only. Nothing was removed.
compare-removed-left = Removed from the left
compare-removed-right = Removed from the right
compare-tick-label = Pick line { $line }
compare-copied-left = Copied to the left
compare-copied-right = Copied to the right
compare-copy-undo = Undo
compare-sheet-title-left = Copy all matches to the left
compare-sheet-title-right = Copy all matches to the right
compare-sheet-confirm = Copy all
compare-sheet-summary = { $changes ->
    [one] 1 change
   *[other] { $changes } changes
} in { $files ->
    [one] 1 file
   *[other] { $files } files
} · { $size } · Ctrl+Z undoes them all
compare-sheet-none = Nothing in the matches can be copied.
compare-sheet-need-search = Search first: copy all copies the changes that match.
compare-sheet-wait = The search is still running. Wait for it to finish.
compare-sheet-line = { $path } · { $changes ->
    [one] 1 change
   *[other] { $changes } changes
} · { $size }
compare-sheet-skipped = Skipped, { $why } · { $path }
compare-sheet-why-changed = changed since shown
compare-sheet-why-read-only = read-only side
compare-sheet-why-not-copyable = cannot be copied
compare-sheet-why-failed = could not be read
compare-sheet-why-not-loaded = not loaded yet
compare-sheet-why-no-digest = too large to copy
compare-sheet-copied-left = Copied { $changes ->
    [one] 1 change
   *[other] { $changes } changes
} in { $files ->
    [one] 1 file
   *[other] { $files } files
} to the left · { $size }
compare-sheet-copied-right = Copied { $changes ->
    [one] 1 change
   *[other] { $changes } changes
} in { $files ->
    [one] 1 file
   *[other] { $files } files
} to the right · { $size }
compare-sheet-button = Copy all { $count }
compare-sheet-button-left = Copy all matches to the left
compare-sheet-button-right = Copy all matches to the right
compare-stream-title = Differences
compare-stream-files = { $count ->
    [one] 1 file
   *[other] { $count } files
}
compare-stream-viewed = Viewed { $viewed } of { $total }
compare-stream-rows = Changed files
compare-stream-empty = No differences under this folder.
compare-search-label = Search all differences
compare-search-found = { $changes ->
    [one] 1 change
   *[other] { $changes } changes
} in { $files ->
    [one] 1 file
   *[other] { $files } files
} { $changes ->
    [one] matches
   *[other] match
}
compare-search-progress = Searching… { $searched } of { $total }
compare-search-none = No difference contains this text.
compare-search-clear = Clear search
compare-stream-keys = J / K next change
compare-stream-details-changes = { $count ->
    [one] 1 change
   *[other] { $count } changes
} · +{ $added } −{ $removed }
compare-outline-title = Outline
compare-outline-top = Top level
compare-view-title = View
compare-menu-title = Line actions
compare-menu-copy-left = Copy this change to the left
compare-menu-copy-right = Copy this change to the right
compare-menu-copy-picked-left = Copy picked lines to the left
compare-menu-copy-picked-right = Copy picked lines to the right
compare-menu-pick = Pick this line
compare-menu-unpick = Unpick this line
compare-menu-view = View
compare-view-simple = Simple
compare-view-rich = Rich
compare-view-custom = Custom
compare-view-words = Changed words
compare-view-map = Change map
compare-view-wrap = Wrap lines
compare-view-unified = Unified
compare-view-side-by-side = Side by side
compare-hex-align-offset = Align by offset
compare-hex-align-content = Align by content
compare-hex-region = Region { $current } of { $total }
compare-hex-regions = { $total } regions
compare-hex-rows = Hex rows of the byte diff
compare-hex-no-side = A side of this pair is missing.
compare-drawer-changes = Changes
compare-drawer-change = Line { $line }  +{ $added } −{ $removed }
compare-drawer-details = Details
compare-drawer-encoding = Encoding
compare-drawer-endings = Line endings
compare-drawer-sides = { $left } ↔ { $right }
compare-drawer-raw-differs = The raw bytes differ; the active options hide the difference.
compare-drawer-same = The two sides are the same.
compare-drawer-fallbacks = Fallbacks
compare-drawer-kind = Kind
compare-drawer-kind-large = Large text
compare-drawer-kind-binary = Binary
compare-drawer-kind-image = Image
compare-drawer-sizes = Sizes
compare-drawer-step = Engine step
compare-drawer-step-whole = Whole file
compare-drawer-step-sections = By section
compare-drawer-step-coarse = Coarse blocks
compare-drawer-step-paged = Paged changes
compare-drawer-align = Alignment
compare-drawer-view = View
compare-drawer-progress = Progress
compare-drawer-progress-finished = Finished
compare-image-side-by-side = Side by side
compare-image-overlay = Overlay
compare-image-mask = Difference mask
compare-image-wipe-label = Overlay wipe
compare-image-wipe-keys = ArrowLeft ArrowRight Home End
compare-image-wipe-hint = Left and Right arrows move the wipe. Home and End go to the edges.
compare-image-left = Left
compare-image-right = Right
compare-image-loading = Reading the picture…
compare-image-similar = { $percent } % similar
compare-image-note-unreadable = A side cannot be read as a picture.
compare-image-note-hash = Compared by hash only: the two files are the same or different.
compare-image-note-sizes = Sizes differ, so there is nothing to overlay.
compare-image-note-format = This picture format cannot be compared.
compare-image-note-pixels = Too many pixels to compare: the mask is missing.
compare-image-note-encoded = The file is too large to decode: the mask is missing.
compare-image-note-decode = A side cannot be decoded: the mask is missing.

compare-gutter-changed = changed
compare-gutter-only-left = ◀ only
compare-gutter-only-right = only ▶
compare-gutter-type = type ≠
compare-gutter-inside = { $count } inside
compare-gutter-rules = same*
compare-gutter-checking = { $percent }%
compare-gutter-pending = …
compare-gutter-not-checked = unchecked
compare-gutter-unavailable = can't read
