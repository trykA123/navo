files-marks-restore-none = No saved marks match this folder
files-marks-hidden = { $count ->
    [one] { $count } marked not shown
   *[other] { $count } marked not shown
}
activity-copying = Copying { $count ->
    [one] { $files } file
   *[other] { $files } files
} to { $target }
activity-moving = Moving { $count ->
    [one] { $files } file
   *[other] { $files } files
} to { $target }
activity-recycling = Moving { $count ->
    [one] { $files } file
   *[other] { $files } files
} to the Recycle Bin
activity-deleting = Deleting { $count ->
    [one] { $files } file
   *[other] { $files } files
}
activity-renaming = Renaming { $count ->
    [one] { $files } file
   *[other] { $files } files
}
activity-linking = Creating { $count ->
    [one] { $files } shortcut
   *[other] { $files } shortcuts
} in { $target }
activity-progress = { $done } of { $total }
activity-left = { $time } left
activity-pausing = Pausing…
activity-paused = Paused
activity-copy-failed = { $count ->
    [one] { $files } file could not be copied
   *[other] { $files } files could not be copied
}
activity-move-failed = { $count ->
    [one] { $files } file could not be moved
   *[other] { $files } files could not be moved
}
activity-recycle-failed = { $count ->
    [one] { $files } file could not be moved to the Recycle Bin
   *[other] { $files } files could not be moved to the Recycle Bin
}
activity-delete-failed = { $count ->
    [one] { $files } file could not be deleted
   *[other] { $files } files could not be deleted
}
activity-rename-failed = { $count ->
    [one] { $files } file could not be renamed
   *[other] { $files } files could not be renamed
}
activity-undoing = Undoing the rename of { $count ->
    [one] { $items } item
   *[other] { $items } items
}
activity-undone = Undid the rename of { $count ->
    [one] { $items } item
   *[other] { $items } items
}
activity-undo-failed = The undo of the rename of { $count ->
    [one] { $items } item
   *[other] { $items } items
} did not finish
activity-chip-undo = Undo { $percent }
activity-chip-undo-failed = Undo failed
activity-chip-undone = Undid { $count ->
    [one] { $items } item
   *[other] { $items } items
}
activity-undo-hint = { $key } undo
activity-undo-skipped = { $count ->
    [one] { $items } item had changed and was left as it is
   *[other] { $items } items had changed and were left as they are
}
activity-undo-left = { $count ->
    [one] Still under a temporary name: { $names }
   *[other] Still under temporary names: { $names }
}
activity-rename-left = Still renamed: { $names }
activity-link-failed = { $count ->
    [one] { $files } shortcut could not be created
   *[other] { $files } shortcuts could not be created
}
activity-copied = Copied { $count ->
    [one] { $items } item
   *[other] { $items } items
} to { $target }
activity-moved = Moved { $count ->
    [one] { $items } item
   *[other] { $items } items
} to { $target }
activity-recycled = Moved { $count ->
    [one] { $items } item
   *[other] { $items } items
} to the Recycle Bin
activity-deleted = Deleted { $count ->
    [one] { $items } item
   *[other] { $items } items
}
activity-renamed = Renamed { $count ->
    [one] { $items } item
   *[other] { $items } items
}
activity-linked = Created { $count ->
    [one] { $items } shortcut
   *[other] { $items } shortcuts
} in { $target }
activity-dragging = Dragging { $count ->
    [one] { $items } item
   *[other] { $items } items
}
activity-drag-keys = Ctrl copy · Shift move · Alt shortcut · Esc cancel
files-link-confirm = Create shortcuts for { $items } in { $target }?
files-link-label = Create shortcuts
link-verb = Create shortcuts for
link-verb-running = Creating shortcuts for
link-verb-done = Created shortcuts for
files-rename-confirm = Rename { $items }?
activity-percent = { $percent }%
activity-chip-copy = Copy { $percent }
activity-chip-move = Move { $percent }
activity-chip-delete = Delete { $percent }
activity-chip-recycle = Recycle { $percent }
activity-chip-rename = Rename { $percent }
activity-chip-link = Shortcuts { $percent }
activity-chip-paused = { $chip } · Paused
activity-chip-copy-failed = Copy failed
activity-chip-move-failed = Move failed
activity-chip-delete-failed = Delete failed
activity-chip-recycle-failed = Recycle failed
activity-chip-rename-failed = Rename failed
activity-chip-link-failed = Shortcuts failed
activity-chip-copied = Copied { $count ->
    [one] { $items } item
   *[other] { $items } items
}
activity-chip-moved = Moved { $count ->
    [one] { $items } item
   *[other] { $items } items
}
activity-chip-deleted = Deleted { $count ->
    [one] { $items } item
   *[other] { $items } items
}
activity-chip-recycled = Recycled { $count ->
    [one] { $items } item
   *[other] { $items } items
}
activity-chip-renamed = Renamed { $count ->
    [one] { $items } item
   *[other] { $items } items
}
activity-chip-linked = Created { $count ->
    [one] { $items } shortcut
   *[other] { $items } shortcuts
}
activity-strip = Activity
activity-pause = Pause
activity-resume = Resume
activity-cancel = Cancel
activity-show = Show
activity-dismiss = Dismiss
activity-stop = Stop
activity-open-folder = Open folder
activity-retry = Retry
perf-recording = Recording performance · { $time }
perf-saved = Performance recording saved
perf-not-saved = Performance recording not saved
perf-no-service = this build cannot save recordings
activity-all = All { $count }
activity-drawer-title = { $count ->
    [one] { $items } activity
   *[other] { $items } activities
}
activity-drawer-close = Close
drag-move = Move { $count ->
    [one] { $count } item
   *[other] { $count } items
} to { $target }
drag-copy = Copy { $count ->
    [one] { $count } item
   *[other] { $count } items
} to { $target }
drag-link = Create { $count ->
    [one] { $count } shortcut
   *[other] { $count } shortcuts
} in { $target }
drag-carry = { $count ->
    [one] { $name }
   *[other] { $count } items
}
drag-keys = Ctrl copy · Shift move · Alt shortcut · Esc cancel
drag-refused-same-folder = Already in this folder
drag-refused-into-itself = Cannot drop a folder into itself
drag-refused-list-changed = The list changed during the drag. Nothing was moved.
files-path-copied = { $count ->
    [one] Copied path
   *[other] Copied { $count } paths
}
activity-rename-log-failed = Renamed, but could not write rename.log: { $error }. Check the profile folder.

drag-refused-platform = These files cannot be dragged here.
drag-refused-effect = The source does not allow this operation.
drag-refused-modal = Close the open dialog before dropping files.
drag-drop-title = Drop files
drag-drop-copy = Copy here
drag-drop-move = Move here
drag-drop-link = Create shortcuts here
drag-drop-cancel = Cancel

path-hidden-menu = Hidden folders
path-hidden-show = Show hidden folders

drag-refused-path = Cannot drop { $path }. Only absolute local paths are supported.

context-menu-type-to-filter = Type to filter
context-menu-filter-keys = ↑↓ move · Enter run · Esc clear
context-menu-page-keys = ← back · type to filter this page
context-menu-classic-only = { $count ->
    [one] { $count } item only in the classic menu
   *[other] { $count } items only in the classic menu
}
context-menu-loading = Loading…
context-menu-could-not-load = Could not load
context-menu-retry = Retry
context-menu-no-match = No matching commands
shell-menu-loading = Loading Windows items…
shell-menu-slow = Windows menu is slow
tabs-more = { $count ->
    [one] { $count } more tab
   *[other] { $count } more tabs
}
tabs-hidden-menu = Hidden tabs
