use std::sync::{Arc, Mutex};

use navo_store::{CollectionKind, Db, DbError, NewCollection};
use navo_ui::window_view::SavedSearch;

use super::hooks;

fn shared() -> crate::files::SharedDb {
    Arc::new(Mutex::new(
        navo_store::JobJournal::new(Db::open_in_memory().unwrap()).unwrap(),
    ))
}

fn remembering() -> super::Remember {
    super::Remember::default()
}

#[test]
fn recorded_commands_come_back_with_the_most_used_first() {
    let hooks = hooks(Some(shared()), remembering());
    (hooks.record_command)("files.refresh");
    (hooks.record_command)("pane.switch");
    (hooks.record_command)("pane.switch");
    let used = (hooks.command_use)();
    let names: Vec<&str> = used.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["pane.switch", "files.refresh"]);
    assert!(used[0].1 > used[1].1);
}

#[test]
fn clearing_forgets_command_use() {
    let hooks = hooks(Some(shared()), remembering());
    (hooks.record_command)("files.refresh");
    (hooks.clear)();
    assert!((hooks.command_use)().is_empty());
}

#[test]
fn a_failed_write_is_swallowed_and_without_a_database_nothing_happens() {
    let db = shared();
    crate::files::lock(&db)
        .connection()
        .execute_batch("DROP TABLE command_use")
        .unwrap();
    let broken = hooks(Some(db), remembering());
    (broken.record_command)("files.refresh");
    assert!((broken.command_use)().is_empty());
    let none = hooks(None, remembering());
    (none.record_command)("files.refresh");
    assert!((none.command_use)().is_empty());
}

#[test]
fn visits_come_back_most_frecent_first() {
    let hooks = hooks(Some(shared()), remembering());
    (hooks.record_visit)("/home/docs", true);
    (hooks.record_visit)("/home/a.txt", false);
    (hooks.record_visit)("/home/docs", true);
    let found = (hooks.frecent)(10);
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].location, "/home/docs");
    assert!(found[0].is_folder && !found[1].is_folder);
    (hooks.clear)();
    assert!((hooks.frecent)(10).is_empty());
}

#[test]
fn with_remember_off_nothing_is_recorded_and_turning_it_on_resumes() {
    let remember = remembering();
    let hooks = hooks(Some(shared()), remember.clone());
    remember.set(false);
    (hooks.record_visit)("/home/docs", true);
    (hooks.record_command)("files.refresh");
    assert!((hooks.frecent)(10).is_empty());
    assert!((hooks.command_use)().is_empty());
    remember.set(true);
    (hooks.record_visit)("/home/docs", true);
    assert_eq!((hooks.frecent)(10).len(), 1);
}

#[test]
fn with_remember_off_stored_history_is_neither_used_nor_shown() {
    let remember = remembering();
    let hooks = hooks(Some(shared()), remember.clone());
    (hooks.record_visit)("/home/docs", true);
    (hooks.record_command)("files.refresh");
    remember.set(false);
    assert!((hooks.frecent)(10).is_empty());
    assert!((hooks.command_use)().is_empty());
    assert!((hooks.recent_commands)(10).is_empty());
    remember.set(true);
    assert_eq!((hooks.frecent)(10).len(), 1);
    assert_eq!((hooks.command_use)().len(), 1);
    assert_eq!((hooks.recent_commands)(10).len(), 1);
}

#[test]
fn save_search_and_saved_round_trip_with_optional_base() {
    let hooks = hooks(Some(shared()), remembering());
    let second = (hooks.find.save_search)("Rust files", "fn main", Some("/repo")).unwrap();
    let first = (hooks.find.save_search)("All logs", "error", None).unwrap();
    assert_eq!(
        (hooks.find.saved)(),
        vec![
            SavedSearch {
                id: first,
                name: "All logs".to_owned(),
                query: "error".to_owned(),
                base: None,
            },
            SavedSearch {
                id: second,
                name: "Rust files".to_owned(),
                query: "fn main".to_owned(),
                base: Some("/repo".to_owned()),
            },
        ]
    );
}

#[test]
fn save_snapshot_stores_items_and_saved_filters_non_search_collections() {
    let db = shared();
    let hooks = hooks(Some(db.clone()), remembering());
    let paths = vec!["/repo/b.rs".to_owned(), "/repo/a.rs".to_owned()];
    let snapshot_id = (hooks.find.save_snapshot)("Snap", "needle", Some("/repo"), &paths).unwrap();
    let search_id = (hooks.find.save_search)("Saved query", "needle", Some("/repo")).unwrap();
    crate::files::lock(&db)
        .create_collection(
            &NewCollection {
                name: "Manual picks",
                kind: CollectionKind::Manual,
                query: None,
                base: None,
            },
            100,
        )
        .unwrap();

    let saved = (hooks.find.saved)();
    assert_eq!(
        saved,
        vec![SavedSearch {
            id: search_id,
            name: "Saved query".to_owned(),
            query: "needle".to_owned(),
            base: Some("/repo".to_owned()),
        }]
    );

    let guard = crate::files::lock(&db);
    let snapshot = guard
        .list_collections()
        .unwrap()
        .into_iter()
        .find(|item| item.id == snapshot_id)
        .unwrap();
    assert_eq!(snapshot.kind, CollectionKind::Snapshot);
    assert_eq!(snapshot.query.as_deref(), Some("needle"));
    assert_eq!(snapshot.base.as_deref(), Some("/repo"));
    let stored_paths: Vec<String> = guard
        .collection_items(snapshot_id)
        .unwrap()
        .into_iter()
        .map(|item| item.location)
        .collect();
    assert_eq!(stored_paths, vec!["/repo/a.rs", "/repo/b.rs"]);
}

#[test]
fn delete_saved_removes_a_saved_search() {
    let hooks = hooks(Some(shared()), remembering());
    let id = (hooks.find.save_search)("Temp", "needle", None).unwrap();
    assert_eq!((hooks.find.saved)().len(), 1);
    (hooks.find.delete_saved)(id).unwrap();
    assert!((hooks.find.saved)().is_empty());
}

#[test]
fn find_hooks_propagate_database_error_display_text() {
    let db = shared();
    let hooks = hooks(Some(db.clone()), remembering());
    let blank = (hooks.find.save_search)("   ", "needle", None).unwrap_err();
    assert_eq!(blank, DbError::InvalidCollectionName.to_string());

    (hooks.find.save_search)("Unique", "first", None).unwrap();
    let duplicate = (hooks.find.save_search)("Unique", "second", None).unwrap_err();
    assert!(
        duplicate.contains("database error:"),
        "expected DbError Display text, got: {duplicate}"
    );

    crate::files::lock(&db)
        .connection()
        .execute_batch("DROP TABLE collection_items")
        .unwrap();
    let paths = vec!["/repo/a.rs".to_owned()];
    let snapshot_err =
        (hooks.find.save_snapshot)("Broken snap", "needle", None, &paths).unwrap_err();
    assert!(
        snapshot_err.contains("database error:"),
        "expected DbError Display text, got: {snapshot_err}"
    );
    assert!(
        crate::files::lock(&db)
            .list_collections()
            .unwrap()
            .iter()
            .all(|item| item.name != "Broken snap"),
        "failed snapshot must not leave an orphan collection row"
    );

    crate::files::lock(&db)
        .connection()
        .execute_batch("DROP TABLE collections")
        .unwrap();
    let delete_err = (hooks.find.delete_saved)(1).unwrap_err();
    assert!(
        delete_err.contains("database error:"),
        "expected DbError Display text, got: {delete_err}"
    );
    assert!((hooks.find.saved)().is_empty());
}

/// Creates a hidden top-level window of the system `STATIC` class for owner tests.
#[cfg(windows)]
fn hidden_test_window(name: &str) -> windows_sys::Win32::Foundation::HWND {
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::UI::WindowsAndMessaging::CreateWindowExW;

    let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
    let name: Vec<u16> = format!("{name}\0").encode_utf16().collect();
    // SAFETY: STATIC is a registered system class; a null parent and no WS_VISIBLE create a
    // hidden top-level window owned by this thread, so no message pump or show is needed.
    unsafe {
        CreateWindowExW(
            0,
            class.as_ptr(),
            name.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            null_mut(),
            null(),
        )
    }
}

/// The no-DB `find.ensure_owner` hook must still repair `GW_OWNER` between two hidden windows.
#[cfg(windows)]
#[test]
fn without_a_database_the_owner_hook_sets_gw_owner() {
    use std::num::NonZeroIsize;

    use raw_window_handle::{RawWindowHandle, Win32WindowHandle};
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DestroyWindow, GW_OWNER, GWL_STYLE, GetWindow, GetWindowLongPtrW, WS_VISIBLE,
    };

    struct TestHwnd(HWND);
    impl Drop for TestHwnd {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: `self.0` is a live test window owned by this guard on the current thread.
                unsafe {
                    DestroyWindow(self.0);
                }
            }
        }
    }

    let navo = TestHwnd(hidden_test_window("navo"));
    let find = TestHwnd(hidden_test_window("find"));
    assert!(
        !navo.0.is_null() && !find.0.is_null(),
        "create hidden test windows"
    );
    // SAFETY: both handles are live windows created on this thread.
    unsafe {
        assert_eq!(
            GetWindowLongPtrW(navo.0, GWL_STYLE) & WS_VISIBLE as isize,
            0
        );
        assert_eq!(
            GetWindowLongPtrW(find.0, GWL_STYLE) & WS_VISIBLE as isize,
            0
        );
        assert!(GetWindow(find.0, GW_OWNER).is_null(), "find starts unowned");
    }

    let raw = |hwnd: HWND| {
        RawWindowHandle::Win32(Win32WindowHandle::new(
            NonZeroIsize::new(hwnd as isize).expect("non-null window handle"),
        ))
    };
    let hooks = hooks(None, remembering());
    assert!((hooks.find.ensure_owner)(raw(find.0), raw(navo.0)));
    // SAFETY: both handles are live windows created on this thread.
    unsafe {
        assert_eq!(GetWindow(find.0, GW_OWNER), navo.0, "hook sets the owner");
    }
}
