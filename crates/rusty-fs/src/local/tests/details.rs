use std::time::SystemTime;

use super::*;
use crate::{DetailAttributes, FakeFs};

#[test]
fn a_temp_file_has_recent_times() {
    let (dir, _) = temp();
    write_file(dir.path(), "file", 17);
    let details = LocalFs::new()
        .details(&location_of(&dir.path().join("file")))
        .unwrap();
    let now = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap();
    assert_eq!((details.kind, details.size), (EntryKind::File, 17));
    for time in [details.modified_ms, details.accessed_ms] {
        assert!(time.unwrap().abs_diff(now) <= 2_000);
    }
    if let Some(created) = details.created_ms {
        assert!(created.abs_diff(now) <= 2_000);
    }
    #[cfg(windows)]
    assert!(details.created_ms.is_some());
}

#[cfg(target_os = "linux")]
#[test]
fn linux_details_resolve_owner_group_and_0644_permissions() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::process::Command;

    let (dir, _) = temp();
    let path = dir.path().join("file");
    fs::write(&path, b"contents").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let metadata = fs::symlink_metadata(&path).unwrap();
    let account = |database: &str, id: u32| {
        let output = Command::new("getent")
            .args([database, &id.to_string()])
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout)
            .unwrap()
            .split(':')
            .next()
            .unwrap()
            .to_owned()
    };
    let owner = account("passwd", metadata.uid());
    let group = account("group", metadata.gid());
    let details = LocalFs::new().details(&location_of(&path)).unwrap();
    assert_eq!(details.security_note, None);
    assert_eq!(details.owner.as_deref(), Some(owner.as_str()));
    assert_eq!(
        details.acl,
        vec![
            crate::AclRow {
                who: owner,
                rights: "rw-".into()
            },
            crate::AclRow {
                who: group,
                rights: "r--".into()
            },
            crate::AclRow {
                who: "Others".into(),
                rights: "r--".into()
            },
        ]
    );
    assert_eq!(details.inherited_from, None);
}

#[cfg(windows)]
#[test]
fn windows_temp_file_owner_is_the_current_user() {
    let (dir, _) = temp();
    let path = dir.path().join("file");
    fs::write(&path, b"contents").unwrap();
    let current_user = windows_user();
    icacls(&path, &["/setowner", &current_user]);
    let details = LocalFs::new().details(&location_of(&path)).unwrap();
    assert_eq!(details.security_note, None);
    assert_eq!(
        details.owner.unwrap().to_lowercase(),
        current_user.trim().to_lowercase()
    );
    assert!(!details.acl.is_empty());
}

#[cfg(windows)]
#[test]
fn windows_details_summarize_allow_permissions() {
    let (dir, _) = temp();
    let path = dir.path().join("file");
    fs::write(&path, b"contents").unwrap();
    let user = windows_user();
    icacls(&path, &["/setowner", &user]);
    for (mask, expected) in [
        ("F", "Full control"),
        ("M", "Modify"),
        ("RX", "Read and run"),
        ("R", "Read"),
        ("W", "Write"),
        ("(RD)", "Special"),
    ] {
        icacls(
            &path,
            &["/inheritance:r", "/grant:r", &format!("{user}:{mask}")],
        );
        let details = LocalFs::new().details(&location_of(&path)).unwrap();
        assert_eq!(details.security_note, None);
        assert_eq!(details.acl.len(), 1);
        assert!(details.acl[0].who.eq_ignore_ascii_case(&user));
        assert_eq!(details.acl[0].rights, expected);
        assert_eq!(details.inherited_from, None);
    }
}

#[cfg(windows)]
#[test]
fn windows_details_find_the_ancestor_that_granted_inherited_permissions() {
    let (dir, _) = temp();
    let parent = dir.path().join("parent");
    fs::create_dir(&parent).unwrap();
    let user = windows_user();
    icacls(
        &parent,
        &["/inheritance:r", "/grant:r", &format!("{user}:(OI)(CI)F")],
    );
    let child = parent.join("child");
    fs::create_dir(&child).unwrap();
    let path = child.join("file");
    fs::write(&path, b"contents").unwrap();
    let details = LocalFs::new().details(&location_of(&path)).unwrap();
    assert_eq!(details.security_note, None);
    assert!(
        details
            .acl
            .iter()
            .any(|row| { row.who.eq_ignore_ascii_case(&user) && row.rights == "Full control" })
    );
    assert_eq!(
        fs::canonicalize(details.inherited_from.unwrap()).unwrap(),
        fs::canonicalize(parent).unwrap()
    );
}

#[cfg(windows)]
#[test]
fn windows_unresolvable_acl_keeps_metadata_and_clears_security_fields() {
    let (dir, _) = temp();
    let path = dir.path().join("file");
    fs::write(&path, b"contents").unwrap();
    icacls(&path, &["/grant", "*S-1-5-9999:R"]);
    let details = LocalFs::new().details(&location_of(&path)).unwrap();
    assert_eq!((details.kind, details.size), (EntryKind::File, 8));
    assert!(details.modified_ms.is_some());
    assert_eq!(details.owner, None);
    assert!(details.acl.is_empty());
    assert_eq!(details.inherited_from, None);
    assert!(details.security_note.is_some());
}

#[cfg(windows)]
fn windows_user() -> String {
    let output = std::process::Command::new("whoami").output().unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[cfg(windows)]
fn icacls(path: &Path, args: &[&str]) {
    let output = std::process::Command::new("icacls")
        .arg(path)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn readonly_matches_std_permissions() {
    let (dir, _) = temp();
    let path = dir.path().join("file");
    fs::write(&path, b"contents").unwrap();
    let original = fs::metadata(&path).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&path, readonly).unwrap();
    let result = LocalFs::new().details(&location_of(&path));
    fs::set_permissions(&path, original).unwrap();
    assert!(result.unwrap().attributes.readonly);
}

#[cfg(windows)]
#[test]
fn windows_archive_attribute_tracks_set_and_cleared_bits() {
    let (dir, _) = temp();
    let path = dir.path().join("file");
    fs::write(&path, b"contents").unwrap();
    let location = location_of(&path);
    super::windows::attrib("+a", &path);
    assert!(
        LocalFs::new()
            .details(&location)
            .unwrap()
            .attributes
            .archive
    );
    super::windows::attrib("-a", &path);
    assert!(
        !LocalFs::new()
            .details(&location)
            .unwrap()
            .attributes
            .archive
    );
}

#[test]
fn on_disk_covers_a_written_file_rounded_to_a_block() {
    let (dir, _) = temp();
    let path = dir.path().join("file");
    fs::write(&path, [b'x'; 4_097]).unwrap();
    let details = LocalFs::new().details(&location_of(&path)).unwrap();
    let on_disk = details.on_disk.unwrap();
    assert!(on_disk >= details.size.div_ceil(512) * 512);
    assert_eq!(on_disk % 512, 0);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(on_disk, fs::symlink_metadata(&path).unwrap().blocks() * 512);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn created_time_matches_statx_when_birth_time_is_available() {
    use rustix::fs::{AtFlags, CWD, StatxFlags, statx};

    let (dir, _) = temp();
    let path = dir.path().join("file");
    fs::write(&path, b"contents").unwrap();
    let stat = statx(CWD, &path, AtFlags::SYMLINK_NOFOLLOW, StatxFlags::BTIME).unwrap();
    let expected = (stat.stx_mask & StatxFlags::BTIME.bits() != 0)
        .then(|| stat.stx_btime.tv_sec * 1_000 + i64::from(stat.stx_btime.tv_nsec / 1_000_000));
    let details = LocalFs::new().details(&location_of(&path)).unwrap();
    assert_eq!(details.created_ms, expected);
    assert!(!details.attributes.archive);
}

#[test]
fn default_details_preserve_metadata_and_leave_unavailable_fields_empty() {
    let provider = FakeFs::new();
    provider.add_entry(
        "/",
        FsEntry {
            name: ProviderPath::from_text("file"),
            kind: EntryKind::File,
            size: 23,
            modified_ms: Some(123),
            attributes: crate::Attributes {
                readonly: true,
                hidden: true,
                system: true,
                ..crate::Attributes::default()
            },
        },
    );
    let location = provider.location("file");
    let details = (&provider as &dyn Provider).details(&location).unwrap();
    assert_eq!((details.kind, details.size), (EntryKind::File, 23));
    assert_eq!(details.modified_ms, Some(123));
    assert_eq!(details.created_ms, None);
    assert_eq!(details.accessed_ms, None);
    assert_eq!(details.on_disk, None);
    assert_eq!(
        details.attributes,
        DetailAttributes {
            readonly: true,
            hidden: true,
            system: true,
            archive: false,
        }
    );
    assert_eq!(details.owner, None);
    assert!(details.acl.is_empty());
    assert_eq!(details.inherited_from, None);
    assert_eq!(details.security_note, None);
}

#[test]
fn a_missing_file_keeps_the_error_kind_and_location() {
    let (dir, _) = temp();
    let missing = location_of(&dir.path().join("missing"));
    let error = LocalFs::new().details(&missing).unwrap_err();
    assert_eq!(error.kind, FsErrorKind::NotFound);
    assert_eq!(error.location, Some(missing));
}

#[test]
fn default_details_propagate_metadata_errors() {
    let provider = FakeFs::new();
    let missing = provider.location("missing");
    let error = provider.details(&missing).unwrap_err();
    assert_eq!(error.kind, FsErrorKind::NotFound);
    assert_eq!(error.location, Some(missing));
}
