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
compare-card-viewed = Viewed
compare-card-gap = { $lines ->
    [one] 1 unchanged line
   *[other] { $lines } unchanged lines
}
compare-card-more = { $hunks ->
    [one] 1 more change · Enter opens the full diff
   *[other] { $hunks } more changes · Enter opens the full diff
}
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
compare-stream-title = Differences
compare-stream-files = { $count ->
    [one] 1 file
   *[other] { $count } files
}
compare-stream-viewed = Viewed { $viewed } of { $total }
compare-stream-rows = Changed files
compare-stream-empty = No differences under this folder.
compare-stream-keys = J / K next change
compare-stream-details-changes = { $count ->
    [one] 1 change
   *[other] { $count } changes
} · +{ $added } −{ $removed }
compare-outline-title = Outline
compare-outline-top = Top level
compare-view-title = View
compare-view-simple = Simple
compare-view-rich = Rich
compare-view-custom = Custom
compare-view-words = Changed words
compare-view-map = Change map
compare-view-unified = Unified
compare-view-side-by-side = Side by side
compare-drawer-changes = Changes
compare-drawer-change = Line { $line }  +{ $added } −{ $removed }
compare-drawer-details = Details
compare-drawer-encoding = Encoding
compare-drawer-endings = Line endings
compare-drawer-sides = { $left } ↔ { $right }
compare-drawer-raw-differs = The raw bytes differ; the active options hide the difference.
compare-drawer-same = The two sides are the same.
compare-drawer-fallbacks = Fallbacks
