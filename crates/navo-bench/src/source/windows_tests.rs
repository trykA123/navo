use super::*;
use std::fs::OpenOptions;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_SHARE_READ,
};

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "share-mode proof opens only temporary fixture entries read-only"
)]
fn read_only_share_open_succeeds_during_walk_and_content_read() {
    let fx = fixture::set().unwrap();
    let source = crate::source::test_open(fx.root()).unwrap();
    let (entries, _) = records(&source);
    let marker = entries
        .iter()
        .find(|record| *record.id.path == in_source(&source, &fx, &fx.marker_file))
        .unwrap();
    let _reader = source.open_read(&marker.id).unwrap();
    let mut visited = 0;
    let summary = source.walk(WalkConfig::default(), |record| {
        let ancestors = record
            .id
            .path
            .ancestors()
            .take_while(|path| path.starts_with(source.folder()));
        let paths = [fx.root(), fx.marker_file.as_path()]
            .into_iter()
            .chain(ancestors);
        for path in paths {
            let _second = OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(path)
                .expect("read-only sharing must work while walk and reader handles are held");
        }
        visited += 1;
    });
    assert!(visited > 0);
    assert_eq!(summary.outcome, WalkOutcome::Complete);
}

#[test]
fn junction_is_detected_by_attributes_and_never_entered() {
    let fx = fixture::set().unwrap();
    let link = fx
        .link_out
        .as_ref()
        .expect("junction creation is required for the Windows proof");
    let metadata = fs::symlink_metadata(link).unwrap();
    assert_ne!(metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT, 0);
    let source = crate::source::test_open(fx.root()).unwrap();
    let link = in_source(&source, &fx, link);
    let (entries, summary) = records(&source);
    let junction = entries
        .iter()
        .find(|record| *record.id.path == link)
        .unwrap();
    assert_eq!(junction.kind, EntryKind::Junction);
    assert_eq!(summary.errors, 0);
    assert!(!entries.iter().any(|record| {
        record
            .id
            .path
            .file_name()
            .is_some_and(|name| name == "outside.txt")
    }));
}

/// NTFS updates a name's listing copy only when that name is opened, so the walk must read
/// each item's own record to see a write made through another hard link.
#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "appends one byte through the fixture's own hard link"
)]
fn a_write_through_another_hard_link_shows_under_every_name() {
    use std::io::Write;
    let fx = fixture::set().unwrap();
    let source = crate::source::test_open(fx.root()).unwrap();
    OpenOptions::new()
        .append(true)
        .open(&fx.hard_link)
        .unwrap()
        .write_all(b"+")
        .unwrap();
    let (entries, _) = records(&source);
    let size = |path: &Path| {
        let path = in_source(&source, &fx, path);
        entries
            .iter()
            .find(|record| *record.id.path == path)
            .map(|record| record.size)
    };
    let grown = u64::try_from(fixture::CONTENT.len() + 1).ok();
    assert_eq!(size(&fx.marker_file), grown);
    assert_eq!(size(&fx.hard_link), grown);
}
