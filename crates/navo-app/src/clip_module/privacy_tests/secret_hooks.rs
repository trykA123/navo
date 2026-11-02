use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

use navo_ui::clip_palette::{
    ClipId, ClipKind, ClipMode, ClipboardHooks, How, SavedDraft, Scope, SecretText,
};

use super::super::tests::{Rig, XorSealer, on};
use super::{Seen, Spy, assert_clean, finish_all, stored_bytes, wait_gone};

const SECRET: &str = "SECRET-9b27f0-hook";
const TITLE: &str = "TITLE-91c0b7-clip";

fn draft(title: &str) -> SavedDraft {
    SavedDraft {
        kind: ClipKind::Secret,
        title: title.to_owned(),
        body: SecretText::new(SECRET.to_owned()),
        scope: Scope::Everywhere,
    }
}

fn wait_for_private(rig: &Rig) {
    for _ in 0..1000 {
        if !rig.sink.privates.lock().unwrap().is_empty() {
            return;
        }
        sleep(Duration::from_millis(5));
    }
    panic!("the private copy never reached the sink");
}

/// The title may sit in the store while the item lives; neither it nor the body reaches a log,
/// a report or a `Debug` text, and both leave the files with the item.
#[test]
fn privacy_secret_hooks_leave_no_trace() {
    let app = tempfile::tempdir().unwrap();
    let spy = Spy::install(app.path());
    let rig = Rig::with_sealer(Arc::new(XorSealer));
    let mut seen = Seen::default();
    rig.apply(on());
    let id = rig.module.save(draft(TITLE)).unwrap();
    assert_clean(SECRET, &stored_bytes(&rig));
    let found = rig.module.search(TITLE, ClipMode::Secrets, None);
    assert_eq!(found.rows.len(), 1);
    let revealed = rig.module.reveal(id).unwrap();
    assert_eq!(revealed.expose(), SECRET);
    seen.note("revealed", &revealed);
    drop(revealed);
    rig.module.copy(id, How::Copy).unwrap();
    wait_for_private(&rig);
    seen.service(&rig);
    rig.module.delete(id).unwrap();
    wait_gone(&rig, TITLE);
    assert_clean(SECRET, &stored_bytes(&rig));
    rig.module.stop();
    finish_all(&spy, &seen, &[SECRET, TITLE]);
}

#[test]
fn privacy_secret_refusals_name_neither_title_nor_body() {
    let app = tempfile::tempdir().unwrap();
    let spy = Spy::install(app.path());
    let rig = Rig::new();
    let mut seen = Seen::default();
    rig.apply(on());
    let refused = rig.module.save(draft(TITLE)).unwrap_err();
    seen.text("save refusal", &refused);
    let missing = rig.module.reveal(ClipId(1 << 63 | 1)).unwrap_err();
    seen.text("reveal refusal", &missing);
    seen.service(&rig);
    rig.module.stop();
    assert_clean(SECRET, &stored_bytes(&rig));
    assert_clean(TITLE, &stored_bytes(&rig));
    finish_all(&spy, &seen, &[SECRET, TITLE]);
}
