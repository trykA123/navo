//! Draws the compare screen where the viewer draws: the Direction 4 header (back, file icon,
//! name, sides, encoding), the rows of the pair and one status line with the change counter.

use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::Icon;
use navo_fs::EntryKind;
use navo_l10n::tr;

use super::drawers::{changes_view, details_view};
use super::{FileScreen, FileState};
use crate::commands::compare::{Exit, NextDifference, PreviousDifference};
use crate::file_list::kind;
use crate::prelude::*;
use crate::primitives::{IconButton, IconName};
use crate::tokens::{self, Theme};
use crate::window_view::NavoWindow;

/// The key context of the file diff (a focus-path root, D27).
const CONTEXT: &str = "CompareView";

/// Splits "a/b/c.txt" into ("a/b", "c.txt"); a bare name has an empty folder.
fn split_path(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

/// The folder and the two compare roots, as "a/b · left ↔ right".
fn meta(folder: &str, sides: &SharedString) -> SharedString {
    match (folder.is_empty(), sides.is_empty()) {
        (true, _) => sides.clone(),
        (false, true) => folder.to_owned().into(),
        (false, false) => format!("{folder} · {sides}").into(),
    }
}

/// The encodings once the pair opened as text: one name, or "left ↔ right" when they differ.
fn encoding(state: &FileState) -> Option<SharedString> {
    let FileState::Opened {
        text: Some(text), ..
    } = state
    else {
        return None;
    };
    let (left, right) = (text.facts.left_encoding, text.facts.right_encoding);
    Some(match left == right {
        true => tr(left).into(),
        false => format!("{} ↔ {}", tr(left), tr(right)).into(),
    })
}

/// The body's message: "Comparing…" while loading, the failure text, or none once opened.
fn message(state: &FileState) -> Option<SharedString> {
    match state {
        FileState::Loading => Some(tr("compare-screen-comparing").into()),
        FileState::Failed(text) => Some(text.clone()),
        FileState::Opened { .. } => None,
    }
}

/// Small muted text that may shrink and truncate.
fn small(t: &Theme) -> Div {
    div()
        .min_w_0()
        .truncate()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
}

/// The header row; `back` pops the screen.
fn header(
    t: &Theme,
    file: &FileScreen,
    back: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let (folder, name) = split_path(&file.request.path);
    let (_, extension) = kind::split_name(name, EntryKind::File);
    let (glyph, tint) = kind::icon(EntryKind::File, extension, &t.colors);
    let back_label = tr("compare-screen-back");
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.md)
        .px(t.spacing.md)
        .h(t.spacing.path_height)
        .border_b_1()
        .border_color(t.colors.line)
        .bg(t.colors.rail)
        .child(
            IconButton::new("compare-screen-back", IconName::ArrowLeft, back_label).on_click(back),
        )
        .child(
            Icon::new(glyph)
                .size(t.spacing.icon)
                .flex_none()
                .text_color(tint),
        )
        .child(
            div()
                .id("compare-screen-title")
                .test_support()
                .flex_none()
                .max_w(t.spacing.rail_width * 2.)
                .truncate()
                .font_weight(FontWeight::SEMIBOLD)
                .child(SharedString::from(name.to_owned())),
        )
        .child(
            small(t)
                .id("compare-screen-sides")
                .test_support()
                .child(meta(folder, &file.sides)),
        )
        .when_some(encoding(&file.state), |row, encoding| {
            row.child(
                small(t)
                    .flex_none()
                    .id("compare-screen-encoding")
                    .test_support()
                    .child(encoding),
            )
        })
}

/// The body: the rows of an opened text pair between the drawers that are open, with the
/// change map beside them; else "Comparing…" or the failure text.
fn body(t: &Theme, file: &mut FileScreen, window: &Window, cx: &mut Context<NavoWindow>) -> Div {
    let area = div().flex().flex_col().flex_1().min_h_0();
    let (look, drawers) = (file.look, &file.drawers);
    if let Some(view) = file.view.as_mut() {
        let layout = look.layout_in(window);
        let list = div().flex_1().min_w_0().child(view.list(layout, cx));
        let map = look.map.then(|| view.map_element(layout, cx));
        view.watch_rows(layout, window, cx);
        let changes = drawers
            .changes
            .then(|| changes_view(t, view.text(), drawers));
        let details = drawers.details.then(|| details_view(t, &view.text().facts));
        let rows = div().flex().flex_1().min_h_0();
        return area.child(
            rows.children(changes)
                .child(list)
                .children(details)
                .children(map),
        );
    }
    let failed = matches!(file.state, FileState::Failed(_));
    area.when_some(message(&file.state), |area, text| {
        area.child(
            div()
                .id("compare-screen-status")
                .role(Role::Status)
                .aria_label(text.clone())
                .test_support()
                .p(t.spacing.xxl)
                .text_color(if failed {
                    t.colors.warn
                } else {
                    t.colors.muted
                })
                .child(text),
        )
    })
}

/// The one status line at the bottom: the change counter, then the way back.
fn status_line(t: &Theme, changes: Option<SharedString>) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xl)
        .px(t.spacing.xl)
        .h(t.spacing.path_height)
        .border_t_1()
        .border_color(t.colors.line)
        .bg(t.colors.rail)
        .when_some(changes, |line, changes| {
            line.child(
                small(t)
                    .id("compare-screen-changes")
                    .role(Role::Status)
                    .aria_label(changes.clone())
                    .test_support()
                    .child(changes),
            )
        })
        .child(div().flex_1())
        .child(small(t).child(tr("compare-screen-esc")))
}

impl NavoWindow {
    /// The compare screen in place of the panes: the file diff, else the Stream, else `None`.
    pub(in crate::window_view) fn compare_screen_view(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self
            .compare
            .screen
            .as_ref()
            .is_some_and(|s| s.file.is_none())
        {
            return self.stream_view(window, cx);
        }
        let t = tokens::current(cx).clone();
        let exit = cx.listener(|view, _: &Exit, window, cx| view.pop_file_diff(window, cx));
        let back = cx.listener(|view, _: &ClickEvent, window, cx| view.pop_file_diff(window, cx));
        let next = cx.listener(|view, _: &NextDifference, window, cx| {
            view.step_change(true, window, cx);
        });
        let previous = cx.listener(|view, _: &PreviousDifference, window, cx| {
            view.step_change(false, window, cx);
        });
        let root = Self::walk_keys(div().id("compare-screen"), cx);
        let root = Self::drawer_actions(Self::look_actions(root, cx), cx);
        let file = self.compare.screen.as_mut()?.file.as_mut()?;
        let changes = file.view.as_ref().map(super::file::FileView::status);
        let root = root
            .key_context(CONTEXT)
            .track_focus(&file.focus)
            .on_action(exit)
            .on_action(next)
            .on_action(previous)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .bg(t.colors.surface)
            .child(header(&t, file, back))
            .child(body(&t, file, window, cx))
            .child(status_line(&t, changes));
        Some(root.into_any_element())
    }
}

#[cfg(test)]
mod tests {
    use super::{meta, split_path};

    #[test]
    fn compare_screen_splits_the_path_and_joins_the_meta() {
        assert_eq!(split_path("a/b/c.txt"), ("a/b", "c.txt"));
        assert_eq!(split_path("c.txt"), ("", "c.txt"));
        assert_eq!(meta("a/b", &"l ↔ r".into()).as_ref(), "a/b · l ↔ r");
        assert_eq!(meta("", &"l ↔ r".into()).as_ref(), "l ↔ r");
        assert_eq!(meta("a", &"".into()).as_ref(), "a");
    }
}
