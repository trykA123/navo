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
activity-dragging = Dragging { $count ->
    [one] { $items } item
   *[other] { $items } items
}
activity-drag-keys = Ctrl copy · Shift move · Alt shortcut · Esc cancel
activity-percent = { $percent } %
activity-chip-copy = Copy { $percent }
activity-chip-move = Move { $percent }
activity-chip-delete = Delete { $percent }
activity-chip-copy-failed = Copy failed
activity-chip-move-failed = Move failed
activity-chip-delete-failed = Delete failed
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
activity-strip = Activity
activity-pause = Pause
activity-resume = Resume
activity-cancel = Cancel
activity-show = Show
activity-dismiss = Dismiss
activity-all = All { $count }
activity-drawer-title = { $count ->
    [one] { $items } activity
   *[other] { $items } activities
}
activity-drawer-close = Close
