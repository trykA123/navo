//! A copy in flight (VR2): a second copy key says "copy running" instead of returning silently,
//! a refusal reloads nothing, and the end of one copy frees the keys again.

use gpui_kit::TestAppContext;
use navo_l10n::tr;

use super::super::compare_screen::rig;
use crate::compare_diff::{CopyDirection, CopyResult};

#[gpui_kit::test]
fn compare_copy_a_second_key_while_one_runs_says_so(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_pair(cx);
    rig.press(cx, "alt-down");
    rig.press(cx, "ctrl-alt-right");
    rig.press(cx, "ctrl-alt-left");
    assert_eq!(rig.diff.copies().len(), 1, "one copy at a time");
    let said = tr("compare-copy-running");
    assert_eq!(rig.note(cx), Some((said, false, false)));
    rig.finish(cx, CopyDirection::ToRight, CopyResult::Copied);
    rig.press(cx, "ctrl-alt-left");
    assert_eq!(rig.diff.copies().len(), 2, "the end frees the keys");
}

#[gpui_kit::test]
fn compare_copy_a_refusal_frees_the_keys_without_reloading(cx: &mut TestAppContext) {
    let _shown = crate::compare_visible::show_compare();
    let rig = rig(cx);
    rig.open_pair(cx);
    rig.press(cx, "alt-down");
    rig.press(cx, "ctrl-alt-right");
    rig.finish(
        cx,
        CopyDirection::ToRight,
        CopyResult::Refused("no".to_owned()),
    );
    assert_eq!(rig.diff.refreshed(), 0);
    assert!(rig.diff.reloaded().is_empty());
    rig.press(cx, "ctrl-alt-right");
    assert_eq!(rig.diff.copies().len(), 2);
}
