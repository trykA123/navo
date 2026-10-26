use super::*;

fn save() -> Save<'static> {
    Save {
        text: "replacement",
        crlf: false,
        expected: None,
        encoding: Default::default(),
        ends: None,
    }
}

#[cfg(unix)]
#[test]
fn editor_save_refuses_alias_ancestors_and_leaf_links_to_retained_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let held = dir.path().join(".navo-retained-ab/payload");
    std::fs::create_dir_all(&held).unwrap();
    std::fs::write(held.join("a"), b"original retained bytes").unwrap();
    std::os::unix::fs::symlink(&held, dir.path().join("alias")).unwrap();
    std::os::unix::fs::symlink(held.join("a"), dir.path().join("leaf")).unwrap();
    let dirs = rusty_store::StoreDirs::in_profile(dir.path());
    dirs.ensure().unwrap();
    let hooks = super::super::profile_hooks(None, Some(&dirs), true);
    for path in [dir.path().join("alias/a"), dir.path().join("leaf")] {
        let location = Location::new(path.to_string_lossy().into_owned()).unwrap();
        assert!((hooks.write)(&location, &save()).is_err());
    }
    assert_eq!(
        std::fs::read(held.join("a")).unwrap(),
        b"original retained bytes"
    );
}

#[test]
fn editor_save_refuses_files_inside_retained_folders() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = rusty_store::StoreDirs::in_profile(dir.path());
    dirs.ensure().unwrap();
    let hooks = super::super::profile_hooks(None, Some(&dirs), true);
    for folder in [".navo-retained-ab", ".ferrite-held-1-2"] {
        let held = dir.path().join(folder).join("payload");
        std::fs::create_dir_all(&held).unwrap();
        std::fs::write(held.join("a"), b"original retained bytes").unwrap();
        let location = Location::new(held.join("a").to_string_lossy().into_owned()).unwrap();
        assert!((hooks.write)(&location, &save()).is_err(), "{folder}");
        assert_eq!(
            std::fs::read(held.join("a")).unwrap(),
            b"original retained bytes"
        );
    }
}

#[test]
fn review_editor_saves_can_run_beside_another_ordinary_mutation() {
    let _isolation = crate::jobs::test_mutation_isolation();
    let dir = tempfile::tempdir().unwrap();
    let dirs = rusty_store::StoreDirs::in_profile(dir.path());
    dirs.ensure().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, b"original").unwrap();
    let location = Location::new(path.to_string_lossy().into_owned()).unwrap();
    let _journal = crate::jobs::open(&dirs.journal_file()).unwrap();
    let lease = crate::jobs::claim_write(&dirs, &location, true).unwrap();
    assert!(lease.is_some());
    let hooks = super::super::profile_hooks(None, Some(&dirs), true);
    (hooks.write)(&location, &save()).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"replacement");
    drop(lease);
    (hooks.write)(&location, &save()).unwrap();
    assert_eq!(std::fs::read(path).unwrap(), b"replacement");
}

#[test]
fn default_editor_save_ignores_an_exclusive_experimental_lease() {
    let _isolation = crate::jobs::test_mutation_isolation();
    let root = tempfile::tempdir().unwrap();
    let dirs = rusty_store::StoreDirs::in_profile(root.path());
    dirs.ensure().unwrap();
    let path = root.path().join("a.txt");
    std::fs::write(&path, b"original").unwrap();
    let location = Location::new(path.to_string_lossy().into_owned()).unwrap();
    let exclusive = crate::jobs::test_mutation_file();
    exclusive.try_lock().unwrap();
    std::fs::rename(&dirs.config, root.path().join("parked-config")).unwrap();
    let hooks = super::super::profile_hooks(None, Some(&dirs), false);
    (hooks.write)(&location, &save()).unwrap();
    assert_eq!(std::fs::read(path).unwrap(), b"replacement");
}

#[test]
fn main_compat_editor_saves_without_a_profile() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("file.txt");
    std::fs::write(&path, b"old").unwrap();
    let at = crate::startup::path_location(&path).unwrap();
    let hooks = super::super::profile_hooks(None, None, false);
    (hooks.write)(&at, &save()).unwrap();
    assert_eq!(std::fs::read(path).unwrap(), b"replacement");
}

#[cfg(unix)]
#[test]
fn main_compat_default_save_keeps_the_link_and_writes_its_target() {
    let root = tempfile::tempdir().unwrap();
    let dirs = rusty_store::StoreDirs::in_profile(root.path());
    dirs.ensure().unwrap();
    let target = root.path().join("target");
    let link = root.path().join("link");
    std::fs::write(&target, b"old target").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let hooks = super::super::profile_hooks(None, Some(&dirs), false);
    (hooks.write)(&crate::startup::path_location(&link).unwrap(), &save()).unwrap();
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read(target).unwrap(), b"replacement");
}
