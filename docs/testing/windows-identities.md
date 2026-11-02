# Windows identity verification

The alpha fix queries FileIdInfo on a non-following handle. Unsupported queries and zero 128-bit IDs use GetFileInformationByHandle on that same handle. The 64-bit index is zero-extended and tagged as a separate identity domain. The rename journal preserves that tag. A zero legacy index remains unavailable as a conservative safeguard. Unconfirmed rollback and legacy-index undo remain pending instead of discarding their journal proof.

Navo reads the filesystem name before it trusts either ID. FAT, FAT12, FAT16, FAT32, exFAT and an empty or unknown name give no proved identity. Rename on such a drive still works, as before the alpha fix. Navo then records a weaker fingerprint at rename time: the kind (file or folder), the size and the modified time. Rollback, Undo and startup recovery act only while the renamed item still matches that fingerprint. Undo leaves a changed item under its new name and reports it as changed. Startup recovery leaves a mismatched entry pending. The journal writes fingerprints as version 2 records; version 1 records stay readable. NTFS, ReFS and other filesystems keep the proved-identity path.

The fingerprint has three known limits. A folder's fingerprint is its modified time only, because its size is 0, so folders created within the same 2 s window look the same. On exFAT a folder's modified time changes when children are added, so Undo leaves that folder under its new name and reports it as changed. FAT stores local time, so a daylight-saving or time-zone change between the rename and Undo or recovery moves the modified time by exactly one hour. A fingerprint therefore also matches when the kind and size are equal and the modified time differs by exactly 3,600,000 ms in either direction. Any other difference refuses.

Microsoft documents that FAT indices can change during some renames or defragmentation. The fallback therefore requires native verification on the intended filesystem. It does not establish universal exFAT or SMB support. Sources: [legacy identity](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/ns-fileapi-by_handle_file_information), [unsupported 128-bit IDs](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-file_id_information), [handle flags](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilea).

On Windows, mount a dedicated blank exFAT test image and set NAVO_EXFAT_ROOT to its root. Do the same with a FAT32 image and NAVO_FAT32_ROOT. Each ignored native test verifies the actual filesystem and creates only its own temporary directory. It checks that the drive gives no proved identity, that a batch rename to a longer name succeeds and that the fingerprint survives the rename. The SMB test still checks distinct and repeated identities, a batch rename and an in-volume move. Do not substitute an NTFS directory.

```powershell
$env:NAVO_EXFAT_ROOT = 'X:\'
cargo test -p navo-fs native_exfat_renames_without_a_proved_identity -- --ignored --nocapture
$env:NAVO_FAT32_ROOT = 'Y:\'
cargo test -p navo-fs native_fat32_renames_without_a_proved_identity -- --ignored --nocapture
$env:NAVO_SMB_ROOT = '\\server\test-share'
cargo test -p navo-fs native_smb_identity_and_batch_rename -- --ignored --nocapture
```

To check by hand, rename a file on a FAT32 or exFAT USB stick with F2 and with multi-rename, then press Ctrl+Z: the old name returns. Rename again, change the file (save it in an editor), then press Ctrl+Z: the file keeps its new name and the activity line says it had changed.

These tests were added on Linux. Their native Windows execution remains pending. If a drive cannot supply a confirmed identity, Navo preserves data and reports that limitation.
