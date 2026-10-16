use gpui_kit::{Action, ClipboardItem, Stateful};

use super::body::BodyAct;
use super::{
    ClearMarks, CopyFilePath, Mark1, Mark2, Mark3, Mark4, Mark5, Mark6, Mark7, Mark8, NextBookmark,
    PreviousBookmark, ShowLineEnds, ToggleBookmark, ToggleFollow,
};
use crate::prelude::*;
use crate::window_view::FerriteWindow;

fn act_on<A: Action>(
    root: Stateful<Div>,
    act: BodyAct,
    cx: &mut Context<FerriteWindow>,
) -> Stateful<Div> {
    root.on_action(cx.listener(move |view, _: &A, window, cx| view.body_act(act, window, cx)))
}

impl FerriteWindow {
    fn body_act(&mut self, act: BodyAct, window: &mut Window, cx: &mut Context<Self>) {
        if act == BodyAct::Follow && self.follow_from_editor(window, cx) {
            return;
        }
        if let Some(body) = self.viewer.as_ref().and_then(|viewer| viewer.body.as_ref()) {
            body.act(act, window, cx);
        }
    }

    fn copy_file_path(&mut self, cx: &mut Context<Self>) {
        let Some(viewer) = self.viewer.as_mut() else {
            return;
        };
        let path = viewer.location.as_str().to_owned();
        cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
        viewer.say(format!("Copied {path}"), cx);
    }

    pub(super) fn marking_actions(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        let root = act_on::<Mark1>(root, BodyAct::Mark(0), cx);
        let root = act_on::<Mark2>(root, BodyAct::Mark(1), cx);
        let root = act_on::<Mark3>(root, BodyAct::Mark(2), cx);
        let root = act_on::<Mark4>(root, BodyAct::Mark(3), cx);
        let root = act_on::<Mark5>(root, BodyAct::Mark(4), cx);
        let root = act_on::<Mark6>(root, BodyAct::Mark(5), cx);
        let root = act_on::<Mark7>(root, BodyAct::Mark(6), cx);
        let root = act_on::<Mark8>(root, BodyAct::Mark(7), cx);
        let root = act_on::<ClearMarks>(root, BodyAct::ClearMarks, cx);
        let root = act_on::<ToggleBookmark>(root, BodyAct::ToggleBookmark, cx);
        let next = BodyAct::StepBookmark { backward: false };
        let root = act_on::<NextBookmark>(root, next, cx);
        let previous = BodyAct::StepBookmark { backward: true };
        let root = act_on::<PreviousBookmark>(root, previous, cx);
        let root = act_on::<ShowLineEnds>(root, BodyAct::LineEnds, cx);
        let root = act_on::<ToggleFollow>(root, BodyAct::Follow, cx);
        root.on_action(cx.listener(|view, _: &CopyFilePath, _, cx| view.copy_file_path(cx)))
    }
}
