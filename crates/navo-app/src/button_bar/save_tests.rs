use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

use navo_ui::window_view::{BarEdit, DraftAction, ItemDraft, ItemPath};

use super::Saver;

const INITIAL: &str = "[[item]]\nname = \"Refresh\"\nnavo = \"pane.refresh\"\n";

fn navo_draft(name: &str, id: &str) -> ItemDraft {
    ItemDraft {
        name: name.to_owned(),
        icon: None,
        action: DraftAction::Navo(id.to_owned()),
    }
}

fn insert_edit(index: usize, name: &str, id: &str) -> BarEdit {
    BarEdit::Insert {
        at: ItemPath { index, child: None },
        draft: navo_draft(name, id),
    }
}

fn update_edit(index: usize, name: &str, id: &str) -> BarEdit {
    BarEdit::Update {
        at: ItemPath { index, child: None },
        draft: navo_draft(name, id),
    }
}

#[test]
fn save_refuses_outside_change() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("buttonbar.toml");
    fs::write(&path, INITIAL).expect("write initial");

    let saver = Saver::new(path.clone());
    saver.record_loaded(INITIAL);
    let reloads = AtomicUsize::new(0);
    let reload = || {
        reloads.fetch_add(1, Ordering::SeqCst);
    };

    saver
        .edit(&insert_edit(1, "Second", "pane.refresh"), &reload)
        .expect("first edit succeeds");
    reloads.store(0, Ordering::SeqCst);

    let outside = "[[item]]\nname = \"External\"\nnavo = \"pane.refresh\"\n";
    fs::write(&path, outside).expect("write outside change");

    let err = saver
        .edit(&insert_edit(1, "Third", "pane.refresh"), &reload)
        .expect_err("outside change must be refused");
    assert!(
        err.contains("changed outside Navo"),
        "expected outside-change error, got: {err}"
    );
    assert_eq!(reloads.load(Ordering::SeqCst), 1, "reload must be called");
    assert_eq!(
        fs::read_to_string(&path).expect("read after refusal"),
        outside,
        "disk file must not be overwritten"
    );

    // Syncing last to the outside text must still leave undo empty (cleared on refusal).
    saver.record_loaded(outside);
    assert_eq!(
        saver.undo(&reload).expect_err("undo stack must be cleared"),
        "nothing to undo"
    );
}

#[test]
fn save_refuses_unparsable_result_without_writing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("buttonbar.toml");
    fs::write(&path, INITIAL).expect("write initial");

    let saver = Saver::new(path.clone());
    saver.record_loaded(INITIAL);
    let reloads = AtomicUsize::new(0);
    let reload = || {
        reloads.fetch_add(1, Ordering::SeqCst);
    };

    let err = saver
        .edit(&insert_edit(1, "Broken", "BAD ID"), &reload)
        .expect_err("invalid navo id must be refused");
    assert!(
        err.contains("invalid navo command id"),
        "expected parse validation error, got: {err}"
    );
    assert_eq!(
        reloads.load(Ordering::SeqCst),
        0,
        "reload must not be called on refusal"
    );
    assert_eq!(
        fs::read_to_string(&path).expect("read after refusal"),
        INITIAL,
        "disk file must remain unchanged"
    );
}

#[test]
fn save_undo_restores_previous() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("buttonbar.toml");
    fs::write(&path, INITIAL).expect("write initial");

    let saver = Saver::new(path.clone());
    saver.record_loaded(INITIAL);
    let reloads = AtomicUsize::new(0);
    let reload = || {
        reloads.fetch_add(1, Ordering::SeqCst);
    };

    saver
        .edit(&insert_edit(1, "Added", "pane.refresh"), &reload)
        .expect("edit succeeds");
    assert_ne!(fs::read_to_string(&path).expect("read after edit"), INITIAL);

    saver.undo(&reload).expect("undo succeeds");
    assert_eq!(
        fs::read_to_string(&path).expect("read after undo"),
        INITIAL,
        "undo must restore exact previous text"
    );
    assert_eq!(reloads.load(Ordering::SeqCst), 2);

    // Undo stack caps at 50 entries: apply 55 edits, then undo 50 times.
    for step in 1..=55 {
        let name = format!("Step {step}");
        saver
            .edit(&update_edit(0, &name, "pane.refresh"), &reload)
            .expect("step edit succeeds");
    }
    for _ in 0..50 {
        saver.undo(&reload).expect("capped undo succeeds");
    }
    let after_50 = fs::read_to_string(&path).expect("read after 50 undos");
    assert!(
        after_50.contains("name = \"Step 5\""),
        "50 undos from Step 55 must land on Step 5, got:\n{after_50}"
    );
    assert_eq!(
        saver.undo(&reload).expect_err("51st undo must fail"),
        "nothing to undo"
    );
}

#[test]
fn save_keeps_bak() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("buttonbar.toml");
    fs::write(&path, INITIAL).expect("write initial");

    let saver = Saver::new(path.clone());
    saver.record_loaded(INITIAL);
    let reload = || {};

    saver
        .edit(&insert_edit(1, "Added", "pane.refresh"), &reload)
        .expect("edit succeeds");

    let bak = path.with_file_name("buttonbar.toml.bak");
    assert!(bak.exists(), ".bak must sit right next to buttonbar.toml");
    assert_eq!(
        fs::read_to_string(&bak).expect("read .bak"),
        INITIAL,
        ".bak must hold previous contents"
    );
}
