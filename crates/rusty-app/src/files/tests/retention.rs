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

#[test]
fn editor_save_without_a_profile_offers_safe_mode_and_preserves_the_file() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("file.txt");
    std::fs::write(&path, b"original").unwrap();
    let hooks = super::super::profile_hooks(None, None, true);
    let Err(SaveError::Failed(message)) = (hooks.write)(&at(&path), &save()) else {
        panic!("saving without a profile must remain blocked");
    };
    assert!(message.ends_with(&rusty_l10n::tr("retention-safe-mode-help")));
    assert_eq!(std::fs::read(path).unwrap(), b"original");
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
    let root = tempfile::tempdir().unwrap();
    let dirs = rusty_store::StoreDirs::in_profile(root.path());
    dirs.ensure().unwrap();
    let path = root.path().join("a.txt");
    std::fs::write(&path, b"original").unwrap();
    let location = Location::new(path.to_string_lossy().into_owned()).unwrap();
    let exclusive = crate::jobs::test_mutation_file(&dirs.data);
    exclusive.try_lock().unwrap();
    std::fs::rename(&dirs.config, root.path().join("parked-config")).unwrap();
    let hooks = super::super::profile_hooks(None, Some(&dirs), false);
    (hooks.write)(&location, &save()).unwrap();
    assert_eq!(std::fs::read(path).unwrap(), b"replacement");
}

#[test]
fn editor_save_waits_for_a_held_exclusive_lease_then_saves() {
    let root = tempfile::tempdir().unwrap();
    let dirs = rusty_store::StoreDirs::in_profile(root.path());
    dirs.ensure().unwrap();
    let path = root.path().join("a.txt");
    std::fs::write(&path, b"original").unwrap();
    let location = Location::new(path.to_string_lossy().into_owned()).unwrap();
    let _journal = crate::jobs::open(&dirs.journal_file()).unwrap();
    let exclusive = crate::jobs::test_mutation_file(&dirs.data);
    exclusive.try_lock().unwrap();
    let release = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        drop(exclusive);
    });
    let started = std::time::Instant::now();
    let hooks = super::super::profile_hooks(None, Some(&dirs), true);
    (hooks.write)(&location, &save()).unwrap();
    assert!(started.elapsed() >= std::time::Duration::from_millis(200));
    release.join().unwrap();
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

#[test]
fn safe_mode_editor_save_refuses_containers_and_the_profile_but_saves_ordinary_files() {
    let root = tempfile::tempdir().unwrap();
    let dirs = rusty_store::StoreDirs::in_profile(&root.path().join("profile"));
    dirs.ensure().unwrap();
    std::fs::write(dirs.settings_file(), b"original settings").unwrap();
    let held = root.path().join(".navo-retained-ab/payload");
    std::fs::create_dir_all(&held).unwrap();
    std::fs::write(held.join("a"), b"original retained bytes").unwrap();
    let hooks = super::super::profile_hooks(None, Some(&dirs), false);
    for path in [held.join("a"), held.join("new"), dirs.settings_file()] {
        let at = crate::startup::path_location(&path).unwrap();
        assert!((hooks.write)(&at, &save()).is_err(), "{path:?}");
    }
    assert_eq!(
        std::fs::read(held.join("a")).unwrap(),
        b"original retained bytes"
    );
    assert!(!held.join("new").exists());
    assert_eq!(
        std::fs::read(dirs.settings_file()).unwrap(),
        b"original settings"
    );
    let ordinary = root.path().join("notes.txt");
    std::fs::write(&ordinary, b"old").unwrap();
    let at = crate::startup::path_location(&ordinary).unwrap();
    (hooks.write)(&at, &save()).unwrap();
    assert_eq!(std::fs::read(ordinary).unwrap(), b"replacement");
}
