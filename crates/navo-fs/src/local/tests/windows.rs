use std::path::PathBuf;
use std::process::Command;

use super::*;

fn os_path_of(text: &str) -> PathBuf {
    os_path(&Location::new(text).unwrap()).unwrap()
}

pub(super) fn attrib(flag: &str, path: &Path) {
    let status = Command::new("attrib").arg(flag).arg(path).status().unwrap();
    assert!(status.success(), "attrib {flag} failed");
}

#[test]
fn a_drive_path_gets_the_verbatim_prefix() {
    assert_eq!(os_path_of(r"C:\a"), PathBuf::from(r"\\?\C:\a"));
}

#[test]
fn a_unc_path_gets_the_verbatim_unc_prefix() {
    assert_eq!(
        os_path_of(r"\\srv\share\x"),
        PathBuf::from(r"\\?\UNC\srv\share\x")
    );
}

#[test]
fn a_verbatim_path_is_unchanged() {
    assert_eq!(os_path_of(r"\\?\C:\a"), PathBuf::from(r"\\?\C:\a"));
}

#[test]
fn a_verbatim_unc_path_is_unchanged() {
    assert_eq!(
        os_path_of(r"\\?\UNC\srv\share\x"),
        PathBuf::from(r"\\?\UNC\srv\share\x")
    );
}

#[test]
fn a_bytes_path_is_name_invalid() {
    let location = Location::local(ProviderPath::from_bytes(&[b'C', b':', 0xFF])).unwrap();
    assert_eq!(kind_of(os_path(&location)), Some(FsErrorKind::NameInvalid));
}

#[test]
fn a_hidden_file_is_hidden() {
    let (dir, folder) = temp();
    write_file(dir.path(), "h", 1);
    attrib("+h", &dir.path().join("h"));
    assert!(entry_named(&folder, "h").attributes.hidden);
}

#[test]
fn a_system_file_is_system() {
    let (dir, folder) = temp();
    write_file(dir.path(), "s", 1);
    attrib("+s", &dir.path().join("s"));
    assert!(entry_named(&folder, "s").attributes.system);
}

#[test]
fn a_dot_file_is_not_hidden_on_windows() {
    let (dir, folder) = temp();
    write_file(dir.path(), ".rc", 1);
    assert!(!entry_named(&folder, ".rc").attributes.hidden);
}

#[test]
fn the_drive_root_is_a_folder_named_by_its_whole_path() {
    let root = Location::new(r"C:\").unwrap();
    let entry = LocalFs::new().metadata(&root).unwrap();
    assert_eq!(
        (entry.name, entry.kind),
        (ProviderPath::from_text(r"C:\"), EntryKind::Folder)
    );
}

#[test]
fn recycle_moves_a_file_out_of_its_folder() {
    use crate::ProviderOps;
    let (dir, root) = temp();
    write_file(dir.path(), "recycle-me.txt", 3);
    let location = root
        .join(&ProviderPath::from_text("recycle-me.txt"))
        .unwrap();
    LocalFs::new().recycle(&location).unwrap();
    assert!(!dir.path().join("recycle-me.txt").exists());
}

mod sizes {
    use super::*;
    use crate::SizeEntry;

    fn list(folder: &Location) -> Vec<SizeEntry> {
        let mut entries = Vec::new();
        LocalFs::new()
            .list_sizes(
                folder,
                &mut |entry| {
                    entries.push(entry);
                    ControlFlow::Continue(())
                },
                &CancelToken::new(),
            )
            .unwrap();
        entries.sort_by_key(|entry| entry.name.to_text());
        entries
    }

    #[test]
    fn hard_links_share_a_key_and_plain_files_have_distinct_keys() {
        let (dir, folder) = temp();
        write_file(dir.path(), "first", 7);
        fs::hard_link(dir.path().join("first"), dir.path().join("second")).unwrap();
        write_file(dir.path(), "plain", 11);
        let entries = list(&folder);
        assert_eq!(entries.len(), 3);
        assert!(entries.iter().all(|entry| entry.link.is_some()));
        assert_eq!(entries[0].link, entries[2].link);
        assert_ne!(entries[0].link, entries[1].link);
        assert_eq!(entries[1].size, 11);
        assert!(entries[1].on_disk.is_some());
        assert_eq!(
            entries[0].link.unwrap().1,
            super::super::super::win_volume::volume_serial(dir.path()).unwrap()
        );
    }

    #[test]
    fn sparse_file_uses_less_disk_space_than_its_apparent_size() {
        let (dir, folder) = temp();
        let path = dir.path().join("sparse");
        let file = fs::File::create(&path).unwrap();
        let output = Command::new("fsutil")
            .args(["sparse", "setflag"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        file.set_len(64 * 1024 * 1024).unwrap();
        drop(file);
        let entries = list(&folder);
        assert_eq!(entries[0].size, 64 * 1024 * 1024);
        assert!(entries[0].on_disk.unwrap() < entries[0].size);
    }

    #[test]
    fn folders_have_no_link_key_or_disk_size() {
        let (dir, folder) = temp();
        fs::create_dir(dir.path().join("folder")).unwrap();
        let entries = list(&folder);
        assert_eq!(entries.len(), 1);
        assert_eq!((entries[0].kind, entries[0].size), (EntryKind::Folder, 0));
        assert_eq!(entries[0].link, None);
        assert_eq!(entries[0].on_disk, None);
    }

    #[test]
    fn cancellation_before_listing_sends_no_entries() {
        let (_dir, folder) = with_files(3);
        let cancel = CancelToken::new();
        cancel.cancel();
        let mut seen = 0;
        let result = LocalFs::new().list_sizes(
            &folder,
            &mut |_| {
                seen += 1;
                ControlFlow::Continue(())
            },
            &cancel,
        );
        assert_eq!(kind_of(result), Some(FsErrorKind::Cancelled));
        assert_eq!(seen, 0);
    }

    #[test]
    fn cancellation_in_sink_stops_before_the_next_buffer() {
        let (_dir, folder) = with_files(1400);
        let cancel = CancelToken::new();
        let mut seen = 0;
        let result = LocalFs::new().list_sizes(
            &folder,
            &mut |_| {
                seen += 1;
                cancel.cancel();
                ControlFlow::Continue(())
            },
            &cancel,
        );
        assert_eq!(kind_of(result), Some(FsErrorKind::Cancelled));
        assert!((1..1400).contains(&seen));
    }

    #[test]
    fn breaking_from_sink_stops_with_ok() {
        let (_dir, folder) = with_files(3);
        let mut seen = 0;
        let result = LocalFs::new().list_sizes(
            &folder,
            &mut |_| {
                seen += 1;
                ControlFlow::Break(())
            },
            &CancelToken::new(),
        );
        assert_eq!(result, Ok(()));
        assert_eq!(seen, 1);
    }
}
