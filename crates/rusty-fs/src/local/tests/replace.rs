use super::*;
use crate::ProviderOps;

fn child(folder: &Location, name: &str) -> Location {
    folder.join(&ProviderPath::from_text(name)).unwrap()
}

fn replace(root: &Location, from: &str, to: &str) -> Result<(), FsError> {
    LocalFs::new().replace(&child(root, from), &child(root, to))
}

#[test]
fn replace_puts_the_new_file_over_the_old_one() {
    let (dir, root) = temp();
    write_file(dir.path(), "new", 5);
    write_file(dir.path(), "old", 3);
    replace(&root, "new", "old").unwrap();
    assert!(!dir.path().join("new").exists());
    assert_eq!(fs::read(dir.path().join("old")).unwrap(), b"xxxxx");
}

#[test]
fn replace_onto_a_free_name_moves_the_file() {
    let (dir, root) = temp();
    write_file(dir.path(), "new", 5);
    replace(&root, "new", "free").unwrap();
    assert_eq!(fs::metadata(dir.path().join("free")).unwrap().len(), 5);
}

#[test]
fn replace_never_overwrites_a_folder() {
    let (dir, root) = temp();
    write_file(dir.path(), "new", 5);
    fs::create_dir(dir.path().join("sub")).unwrap();
    let replaced = replace(&root, "new", "sub");
    assert_eq!(replaced.map_err(|e| e.kind), Err(FsErrorKind::NameInvalid));
    assert!(dir.path().join("sub").is_dir() && dir.path().join("new").is_file());
}

#[cfg(windows)]
mod on_windows {
    use std::os::windows::fs::MetadataExt;

    use super::super::windows::attrib;
    use super::*;

    const READONLY: u32 = 0x1;
    const HIDDEN: u32 = 0x2;

    fn attributes(path: &Path) -> u32 {
        fs::metadata(path).unwrap().file_attributes()
    }

    #[test]
    fn replace_clears_a_read_only_target_and_replaces_it() {
        let (dir, root) = temp();
        write_file(dir.path(), "new", 5);
        write_file(dir.path(), "old", 3);
        attrib("+r", &dir.path().join("old"));
        replace(&root, "new", "old").unwrap();
        assert_eq!(fs::metadata(dir.path().join("old")).unwrap().len(), 5);
        assert_eq!(attributes(&dir.path().join("old")) & READONLY, 0);
    }

    #[test]
    fn the_replaced_file_keeps_the_new_streams_and_attributes() {
        let (dir, root) = temp();
        write_file(dir.path(), "new", 5);
        write_file(dir.path(), "old", 3);
        fs::write(dir.path().join("new:tag"), b"from source").unwrap();
        fs::write(dir.path().join("old:stale"), b"from old").unwrap();
        attrib("+r", &dir.path().join("old"));
        attrib("+h", &dir.path().join("old"));
        replace(&root, "new", "old").unwrap();
        let old = dir.path().join("old");
        assert_eq!(
            fs::read(dir.path().join("old:tag")).unwrap(),
            b"from source"
        );
        assert!(fs::read(dir.path().join("old:stale")).is_err());
        assert_eq!(attributes(&old) & (READONLY | HIDDEN), 0);
    }
}
