use super::*;

fn missing_folder() -> (TempDir, Location) {
    let (dir, folder) = temp();
    let missing = folder.join(&ProviderPath::from_text("missing")).unwrap();
    (dir, missing)
}

#[test]
fn a_missing_folder_is_not_found() {
    let (_dir, missing) = missing_folder();
    assert_eq!(kind_of(list_all(&missing).0), Some(FsErrorKind::NotFound));
}

#[test]
fn a_missing_folder_error_carries_the_location() {
    let (_dir, missing) = missing_folder();
    let error = list_all(&missing).0.unwrap_err();
    assert_eq!(error.location, Some(missing));
}

#[test]
fn a_missing_folder_error_carries_the_os_code() {
    let (_dir, missing) = missing_folder();
    let error = list_all(&missing).0.unwrap_err();
    assert!(error.os_code.is_some());
}

#[test]
fn a_missing_folder_sends_no_batch() {
    let (_dir, missing) = missing_folder();
    assert!(list_all(&missing).1.is_empty());
}

#[test]
fn listing_a_file_is_name_invalid() {
    let (dir, folder) = temp();
    write_file(dir.path(), "file", 1);
    let file = folder.join(&ProviderPath::from_text("file")).unwrap();
    assert_eq!(kind_of(list_all(&file).0), Some(FsErrorKind::NameInvalid));
}

#[cfg(unix)]
#[test]
fn a_folder_without_read_permission_is_access_denied() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, folder) = temp();
    if running_as_root(dir.path()) {
        return;
    }
    let locked = dir.path().join("locked");
    fs::create_dir(&locked).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let location = folder.join(&ProviderPath::from_text("locked")).unwrap();
    let result = std::panic::catch_unwind(|| list_all(&location).0);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
    let result = result.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    assert_eq!(kind_of(result), Some(FsErrorKind::AccessDenied));
}

#[test]
fn a_fake_location_has_no_os_path() {
    let location = Location::new("fake://a/b").unwrap();
    assert_eq!(kind_of(os_path(&location)), Some(FsErrorKind::NameInvalid));
}

#[test]
fn listing_a_fake_location_is_name_invalid() {
    let location = Location::new("fake://a/b").unwrap();
    assert_eq!(
        kind_of(list_all(&location).0),
        Some(FsErrorKind::NameInvalid)
    );
}

#[test]
fn a_fake_location_error_carries_the_location() {
    let location = Location::new("fake://a/b").unwrap();
    assert_eq!(os_path(&location).unwrap_err().location, Some(location));
}

#[cfg(unix)]
#[test]
fn a_wide_path_on_unix_is_name_invalid() {
    let location = Location::local(ProviderPath::from_wide(&[0x2F, 0xD800])).unwrap();
    assert_eq!(kind_of(os_path(&location)), Some(FsErrorKind::NameInvalid));
}

#[cfg(unix)]
#[test]
fn metadata_of_a_wide_path_on_unix_is_name_invalid() {
    let location = Location::local(ProviderPath::from_wide(&[0x2F, 0xD800])).unwrap();
    assert_eq!(
        kind_of(LocalFs::new().metadata(&location)),
        Some(FsErrorKind::NameInvalid)
    );
}
