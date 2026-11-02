use navo_ui::clip_palette::ClipboardHooks;

use super::{Rig, off, on};

#[test]
fn the_first_open_with_the_module_on_is_due_once() {
    let rig = Rig::new();
    rig.apply(on());
    assert!(rig.module.take_intro());
    assert!(!rig.module.take_intro());
    assert!(rig.data.join("clipboard-intro-shown").exists());
}

#[test]
fn a_module_that_is_off_keeps_the_line_for_later() {
    let rig = Rig::new();
    rig.apply(off());
    assert!(!rig.module.take_intro());
    rig.apply(on());
    assert!(rig.module.take_intro());
}
