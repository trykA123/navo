job-journal-unavailable = The job journal is unavailable. Restart Navo after resolving the database recovery notice.
job-profile-unavailable = A writable profile is required before changing files. Restart Navo with a valid profile.
job-wait-cancelled = The operation was cancelled while waiting for another profile.
job-user-unavailable = Cannot coordinate file changes for this user. Restart Navo with a valid home folder.
job-profile-busy = Another file operation is using this profile. Wait for it to finish and retry.
job-settings-read = Cannot read retention settings: { $error }
job-settings-invalid = Fix navo.toml before changing files: { $error }
job-settings-unknown = Fix unknown retention keys in navo.toml before changing files.
job-profile-protected = Navo profile files cannot be changed by file operations.
job-retained-protected = Retained files are protected. Restore them through retention before changing them.
job-retained-parent-protected = This operation would move or change retained files. Restore or purge them first.
job-retention-unsupported = This provider cannot safely preserve overwritten files. Use a supported local volume.
job-retention-failed = Previous versions were kept for recovery because retention could not be completed: { $error }
job-delete-too-large-title = Too large to keep. Delete permanently?
job-delete-too-large-help = More than { $limit } items; deletion cannot be undone.

job-delete-over-capacity-help = This selection is beyond the retention capacity of { $limit }; deletion cannot be undone.
job-delete-retained-title = Delete { $items }?
job-delete-retained-label = Delete
job-delete-retained-help = Kept in Recently deleted for { $days ->
    [one] 1 day
   *[other] { $days } days
}
job-retention-changed = This delete can no longer run as you confirmed it. Nothing was deleted. Check the retention settings, then delete again.
