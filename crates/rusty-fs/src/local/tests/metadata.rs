use super::*;

fn child(folder: &Location, name: &str) -> Location {
    folder.join(&ProviderPath::from_text(name)).unwrap()
}

#[test]
fn a_file_has_kind_file_and_its_size() {
    let (dir, folder) = temp();
    write_file(dir.path(), "a.txt", 5);
    let entry = LocalFs::new().metadata(&child(&folder, "a.txt")).unwrap();
    assert_eq!((entry.kind, entry.size), (EntryKind::File, 5));
}

#[test]
fn a_file_is_named_by_its_last_segment() {
    let (dir, folder) = temp();
    write_file(dir.path(), "a.txt", 5);
    let entry = LocalFs::new().metadata(&child(&folder, "a.txt")).unwrap();
    assert_eq!(entry.name, ProviderPath::from_text("a.txt"));
}

#[test]
fn metadata_matches_the_listed_entry() {
    let (dir, folder) = temp();
    write_file(dir.path(), "a.txt", 5);
    let entry = LocalFs::new().metadata(&child(&folder, "a.txt")).unwrap();
    assert_eq!(entry, entry_named(&folder, "a.txt"));
}

#[test]
fn a_folder_has_kind_folder_and_size_zero() {
    let (dir, folder) = temp();
    fs::create_dir(dir.path().join("sub")).unwrap();
    write_file(&dir.path().join("sub"), "inner", 100);
    let entry = LocalFs::new().metadata(&child(&folder, "sub")).unwrap();
    assert_eq!((entry.kind, entry.size), (EntryKind::Folder, 0));
}

#[cfg(unix)]
#[test]
fn a_symlink_is_a_link_and_not_followed() {
    let (dir, folder) = temp();
    fs::create_dir(dir.path().join("target")).unwrap();
    std::os::unix::fs::symlink(dir.path().join("target"), dir.path().join("link")).unwrap();
    let entry = LocalFs::new().metadata(&child(&folder, "link")).unwrap();
    assert_eq!(entry.kind, EntryKind::Link);
    assert!(entry.attributes.link_to_folder);
}

#[cfg(unix)]
#[test]
fn the_root_is_a_folder_named_by_its_whole_path() {
    let root = Location::local(ProviderPath::from_text("/")).unwrap();
    let entry = LocalFs::new().metadata(&root).unwrap();
    assert_eq!(
        (entry.name, entry.kind),
        (ProviderPath::from_text("/"), EntryKind::Folder)
    );
}

#[test]
fn a_missing_path_is_not_found() {
    let (_dir, folder) = temp();
    let result = LocalFs::new().metadata(&child(&folder, "missing"));
    assert_eq!(kind_of(result), Some(FsErrorKind::NotFound));
}

#[test]
fn a_missing_path_error_carries_the_location() {
    let (_dir, folder) = temp();
    let missing = child(&folder, "missing");
    let error = LocalFs::new().metadata(&missing).unwrap_err();
    assert_eq!(error.location, Some(missing));
}
