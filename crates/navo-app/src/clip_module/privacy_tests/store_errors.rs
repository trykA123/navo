use navo_clipboard::{ClipStore, Limits};
use navo_ui::clip_palette::ClipboardHooks;

use super::super::tests::{Rig, on};
use super::{CANARY, Seen, Spy, assert_clean, finish, stored_bytes, wait_gone};

fn damaged_bytes() -> Vec<u8> {
    let mut bytes = vec![0x5a; 4096];
    bytes[100..100 + CANARY.len()].copy_from_slice(CANARY.as_bytes());
    bytes
}

#[test]
fn privacy_damaged_store_is_set_aside_without_a_trace() {
    let app = tempfile::tempdir().unwrap();
    let spy = Spy::install(app.path());
    let rig = Rig::new();
    let mut seen = Seen::default();
    std::fs::create_dir_all(&rig.data).unwrap();
    std::fs::write(rig.db(), damaged_bytes()).unwrap();
    let missing = app.path().join("no-folder").join("clipboard.db");
    let error = ClipStore::open(&missing, Limits::default()).err().unwrap();
    seen.note("open error", &error);
    seen.text("open error text", &error.to_string());
    rig.apply(on());
    seen.service(&rig);
    let live: Vec<_> = stored_bytes(&rig)
        .into_iter()
        .filter(|(name, _)| !name.contains(".corrupt-"))
        .collect();
    assert_clean(CANARY, &live);
    rig.module.delete_all().unwrap();
    wait_gone(&rig, CANARY);
    rig.module.stop();
    finish(&spy, &seen, CANARY);
}

#[test]
fn privacy_unopenable_store_logs_no_text() {
    let app = tempfile::tempdir().unwrap();
    let spy = Spy::install(app.path());
    let rig = Rig::new();
    let mut seen = Seen::default();
    std::fs::create_dir_all(rig.db()).unwrap();
    std::fs::write(rig.db().join(CANARY), damaged_bytes()).unwrap();
    rig.apply(on());
    seen.service(&rig);
    let refused = rig.module.delete_all().unwrap_err();
    assert_eq!(refused, navo_l10n::tr("clipboard-not-deleted"));
    seen.text("delete error text", &refused);
    rig.module.settle();
    seen.service(&rig);
    assert!(rig.db().is_dir(), "nothing was deleted");
    rig.module.stop();
    finish(&spy, &seen, CANARY);
}
