use navo_ui::clip_palette::ClipboardHooks;

use super::super::tests::{Rig, off, on, wait_until_len};
use super::{CANARY, Payload, leaks, only_row, stored_bytes, wait_gone};

#[derive(Clone, Copy)]
enum Op {
    Delete,
    Clear,
    DeleteAllOn,
    DeleteAllOff,
}

#[test]
fn privacy_delete_leaves_no_text_in_the_database_files() {
    gone_after(Payload::Text, Op::Delete);
}

#[test]
fn privacy_delete_leaves_no_path_in_the_database_files() {
    gone_after(Payload::Paths, Op::Delete);
}

#[test]
fn privacy_clear_history_leaves_no_text_in_the_database_files() {
    gone_after(Payload::Text, Op::Clear);
}

#[test]
fn privacy_delete_all_leaves_no_text_in_any_file_while_on() {
    gone_after(Payload::Text, Op::DeleteAllOn);
}

#[test]
fn privacy_delete_all_leaves_no_text_in_any_file_while_off() {
    gone_after(Payload::Text, Op::DeleteAllOff);
}

fn gone_after(payload: Payload, op: Op) {
    let rig = Rig::new();
    rig.apply(on());
    payload.capture(&rig);
    assert!(
        !leaks(CANARY, &stored_bytes(&rig)).is_empty(),
        "history text is stored, so the checker has something to find"
    );
    run(op, &rig);
    wait_gone(&rig, CANARY);
}

fn run(op: Op, rig: &Rig) {
    match op {
        Op::Delete => {
            let id = only_row(rig);
            rig.module.delete(id).unwrap();
            wait_until_len(rig, 0);
        }
        Op::Clear => {
            rig.module.clear_history().unwrap();
            wait_until_len(rig, 0);
        }
        Op::DeleteAllOn => {
            plant_aside(rig);
            rig.module.delete_all().unwrap();
        }
        Op::DeleteAllOff => {
            plant_aside(rig);
            rig.apply(off());
            rig.module.delete_all().unwrap();
            rig.module.settle();
        }
    }
}

fn plant_aside(rig: &Rig) {
    let aside = rig.data.join("clipboard.db.corrupt-1700000000000");
    std::fs::write(aside, format!("older damaged copy with {CANARY}")).unwrap();
}
