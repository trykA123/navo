use navo_ui::clip_palette::{ClipboardHooks, How};

use super::super::tests::{Rig, off, on, wait_until_len};
use super::{CANARY, Payload, Seen, Spy, finish, only_row};

#[test]
fn privacy_text_flow_leaves_no_trace() {
    flow(Payload::Text);
}

#[test]
fn privacy_path_flow_leaves_no_trace() {
    flow(Payload::Paths);
}

fn flow(payload: Payload) {
    let app = tempfile::tempdir().unwrap();
    let spy = Spy::install(app.path());
    let rig = Rig::new();
    let mut seen = Seen::default();
    rig.apply(on());
    payload.capture(&rig);
    seen.service(&rig);
    copy_and_delete(&rig, &mut seen, payload);
    payload.capture(&rig);
    rig.module.clear_history().unwrap();
    wait_until_len(&rig, 0);
    payload.capture(&rig);
    toggle(&rig);
    seen.service(&rig);
    rig.module.delete_all().unwrap();
    wait_until_len(&rig, 0);
    payload.capture(&rig);
    rig.apply(off());
    rig.module.delete_all().unwrap();
    rig.module.settle();
    seen.service(&rig);
    rig.module.stop();
    finish(&spy, &seen, CANARY);
}

/// The copy answers once written; the fake sink refuses file lists, so a path copy fails.
fn copy_and_delete(rig: &Rig, seen: &mut Seen, payload: Payload) {
    let id = only_row(rig);
    let copied = rig.module.copy(id, How::Copy);
    assert_eq!(copied.is_ok(), matches!(payload, Payload::Text));
    assert!(copied.err().is_none_or(|error| !error.contains(CANARY)));
    seen.service(rig);
    rig.module.delete(id).unwrap();
    wait_until_len(rig, 0);
}

fn toggle(rig: &Rig) {
    rig.apply(off());
    rig.apply(on());
    wait_until_len(rig, 1);
}
