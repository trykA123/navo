use super::*;
use crate::atomic_write::{Stage, tests::on_stage};

#[test]
fn a_retry_refuses_a_hand_edit_to_the_target_key() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "[appearance]\nmode = \"light\"\n");
    let (mut store, _) = SettingsStore::load(&path);
    let mine = "[appearance]\nmode = \"system\"\n";
    let mut edit = Some(mine);
    on_stage(move |path, stage| {
        if stage == Stage::TempWritten
            && let Some(mine) = edit.take()
        {
            write(path, mine);
        }
        Ok(())
    });
    let error = store
        .save_preserving_edits(MODE, Some(text("dark")))
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("changed in navo.toml while saving")
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), mine);
}

#[test]
fn a_retry_can_patch_an_unrelated_hand_edit() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "[appearance]\nmode = \"light\"\n");
    let (mut store, _) = SettingsStore::load(&path);
    let mut edit = Some("[appearance]\nmode = \"light\"\nfont-size = 20\n");
    on_stage(move |path, stage| {
        if stage == Stage::TempWritten
            && let Some(mine) = edit.take()
        {
            write(path, mine);
        }
        Ok(())
    });
    store
        .save_preserving_edits(MODE, Some(text("dark")))
        .unwrap();
    assert_eq!(store.current().appearance.font_size, 20);
    assert_eq!(store.current().appearance.mode, ThemeMode::Dark);
}

#[test]
fn a_retry_stops_after_a_second_conflict() {
    use std::cell::Cell;
    use std::rc::Rc;
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "[appearance]\nmode = \"light\"\n");
    let (mut store, _) = SettingsStore::load(&path);
    let attempts = Rc::new(Cell::new(0));
    let reached = attempts.clone();
    on_stage(move |path, stage| {
        if stage == Stage::TempSynced {
            let count = reached.get() + 1;
            reached.set(count);
            write(
                path,
                &format!(
                    "[appearance]\nmode = \"light\"\nfont-size = {}\n",
                    16 + count
                ),
            );
        }
        Ok(())
    });
    assert_eq!(
        store.save_preserving_edits(MODE, Some(text("dark"))),
        Err(SettingsError::ChangedOnDisk)
    );
    assert_eq!(attempts.get(), 2);
    assert_eq!(
        SettingsStore::load(&path).0.current().appearance.font_size,
        18
    );
}

#[test]
fn a_hand_edit_after_backup_is_kept() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "[appearance]\nmode = \"light\"\n");
    let (mut store, _) = SettingsStore::load(&path);
    let mine = "[appearance]\nmode = \"system\"\n";
    on_stage(move |path, stage| {
        if stage == Stage::BackupMade {
            write(path, mine);
        }
        Ok(())
    });
    assert_eq!(
        store.save(MODE, Some(text("dark"))),
        Err(SettingsError::ChangedOnDisk)
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), mine);
}

#[test]
fn a_committed_completion_keeps_state_after_folder_sync_failure() {
    let dir = tmp();
    let path = settings_file(&dir);
    let old = "[appearance]\nmode = \"light\"\n";
    write(&path, old);
    let db = Db::open_in_memory().unwrap();
    let (mut store, _) = SettingsStore::load(&path);
    let mut defaults = dark_mode();
    defaults.appearance.font_size = 18;
    let generation = store.generation();
    on_stage(|path, stage| {
        if path.file_name().unwrap() == "navo.toml" && stage == Stage::Replaced {
            Err(io::Error::other("injected folder fsync failure"))
        } else {
            Ok(())
        }
    });
    let done = store.complete(&db, &defaults).unwrap();
    assert!(done.written);
    assert!(matches!(
        done.durability_error,
        Some(SettingsError::Durability(_))
    ));
    assert!(store.generation() > generation);
    assert_eq!(store.current().appearance.font_size, 18);
    assert_eq!(store.current().files, defaults.files);
    assert_eq!(
        db.settings_defaults().unwrap().get("files.sort").unwrap(),
        "\"name\""
    );
    on_stage(|_, _| Ok(()));
    store.save(MODE, Some(text("dark"))).unwrap();
    assert_eq!(fs::read_to_string(backup_path(&path)).unwrap(), old);
}

#[test]
fn undo_after_file_deletion_restores_the_previous_effective_value() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "[appearance]\nmode = \"light\"\n");
    let (mut store, _) = SettingsStore::load(&path);
    let old = store.current().value_at(MODE);
    fs::remove_file(&path).unwrap();
    let saved = store.save(MODE, Some(text("dark"))).unwrap();
    store
        .save(MODE, if saved.was_present { old } else { None })
        .unwrap();
    assert_eq!(store.current().appearance.mode, ThemeMode::Light);
}

#[test]
fn shared_session_serializes_completion_and_sheet_save() {
    use std::sync::{Arc, Barrier};
    let dir = tmp();
    let path = settings_file(&dir);
    let old = "[appearance]\nmode = \"light\"\n";
    write(&path, old);
    let backup = SessionBackup::default();
    let (mut completing, _) = SettingsStore::load_sharing(&path, backup.clone());
    let (mut saving, _) = SettingsStore::load_sharing(&path, backup.clone());
    let checked = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let (arrived, resume) = (checked.clone(), release.clone());
    let completion = std::thread::spawn(move || {
        on_stage(move |path, stage| {
            if path.file_name().unwrap() == "navo.toml" && stage == Stage::TempSynced {
                arrived.wait();
                resume.wait();
            }
            Ok(())
        });
        completing.complete(&Db::open_in_memory().unwrap(), &Settings::default())
    });
    checked.wait();
    let serialized = backup.0.try_lock().is_err();
    let attempt = Arc::new(Barrier::new(2));
    let entering = attempt.clone();
    let save = std::thread::spawn(move || {
        entering.wait();
        saving.save(MODE, Some(text("dark")))
    });
    attempt.wait();
    release.wait();
    let completed = completion.join().unwrap();
    let saved = save.join().unwrap();
    assert!(
        serialized,
        "the session lock must cover validation through replacement"
    );
    assert!(completed.is_ok());
    assert!(saved.is_ok());
    assert_eq!(
        SettingsStore::load(&path).0.current().appearance.mode,
        ThemeMode::Dark
    );
    assert_eq!(fs::read_to_string(backup_path(&path)).unwrap(), old);
}

#[test]
fn settings_completion_releases_journal_during_durable_writes() {
    use std::sync::{Arc, Mutex};
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "[appearance]\nmode = \"light\"\n");
    let journal = Arc::new(Mutex::new(
        crate::JobJournal::new(Db::open_in_memory().unwrap()).unwrap(),
    ));
    let shared = journal.clone();
    let stages = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = stages.clone();
    on_stage(move |_, _| {
        assert!(
            shared.try_lock().is_ok(),
            "filesystem writes must release the shared journal"
        );
        observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    });
    let (mut store, _) = SettingsStore::load(&path);
    store
        .complete(journal.as_ref(), &Settings::default())
        .unwrap();
    on_stage(|_, _| Ok(()));
    assert!(stages.load(std::sync::atomic::Ordering::SeqCst) > 0);
}
