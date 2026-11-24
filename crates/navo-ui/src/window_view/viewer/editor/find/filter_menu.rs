//! The "Filter lines" menu button on the editor's find bar: a popup with Keep, Remove, Copy and
//! Bookmark matching lines. Keep and Remove show dimmed while the view reads only.

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{Anchor, Role, anchored, deferred, point};
use navo_l10n::tr;

use super::filter::FilterAction;
use crate::prelude::*;
use crate::primitives::popup_menu::{ChooseHandler, Keys, PopupMenu, choose, toggle, wire_keys};
use crate::primitives::{Button, ButtonVariant};
use crate::tokens::Theme;
use crate::window_view::viewer::editor::EditorView;

const MENU_WIDTH: f32 = 220.;
const ROW_HEIGHT: f32 = 30.;

/// One menu row's look and its click target.
struct RowProps<'a> {
    theme: &'a Theme,
    position: usize,
    is_selected: bool,
    is_dimmed: bool,
    state: Entity<PopupMenu>,
    on_choose: ChooseHandler,
}

impl EditorView {
    /// The "Filter lines" button and, while open, its menu.
    pub(in super::super) fn filter_menu(&self, t: &Theme, cx: &mut Context<Self>) -> Option<Div> {
        let state = self.find.bar.as_ref()?.filter_menu.clone();
        let is_open = state.read(cx).is_open;
        let toggled = state.clone();
        let button = Button::new("find-filter-lines", tr("editor-filter-button"))
            .variant(ButtonVariant::Ghost)
            .track_focus(state.read(cx).anchor())
            .on_click(move |event, window, cx| toggle(&toggled, event, window, cx));
        let menu = is_open.then(|| self.filter_popup(t, state, cx));
        Some(div().relative().flex_none().child(button).children(menu))
    }

    /// The open menu, anchored under the button and kept inside the window.
    fn filter_popup(
        &self,
        t: &Theme,
        state: Entity<PopupMenu>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let on_choose = chooser(cx);
        let selected = state.read(cx).selected;
        let rows: Vec<AnyElement> = FilterAction::ALL
            .iter()
            .enumerate()
            .map(|(position, action)| {
                let props = RowProps {
                    theme: t,
                    position,
                    is_selected: position == selected,
                    is_dimmed: action.edits() && !self.editing,
                    state: state.clone(),
                    on_choose: on_choose.clone(),
                };
                row(action.label(), props)
            })
            .collect();
        let keys = Keys {
            state,
            count: FilterAction::ALL.len(),
            on_choose,
        };
        let c = &t.colors;
        let panel = wire_keys(div().id("filter-lines-menu"), keys, cx)
            .role(Role::Menu)
            .aria_label(tr("editor-filter-button"))
            .occlude()
            .flex()
            .flex_col()
            .w(px(MENU_WIDTH))
            .p(t.spacing.xs)
            .rounded(t.radii.md)
            .border_1()
            .border_color(c.line)
            .bg(c.surface)
            .shadow(t.shadows.menu.clone())
            .text_color(c.text)
            .text_size(t.typography.small)
            .children(rows);
        deferred(
            anchored()
                .anchor(Anchor::TopLeft)
                .offset(point(px(0.), t.spacing.path_height))
                .snap_to_window_with_margin(px(8.))
                .child(panel),
        )
        .with_priority(1)
        .into_any_element()
    }
}

/// Runs the action at the chosen row's position on this view.
fn chooser(cx: &mut Context<EditorView>) -> ChooseHandler {
    let this = cx.entity().downgrade();
    Rc::new(move |position, window, cx| {
        if let Some(&action) = FilterAction::ALL.get(position) {
            let _ = this.update(cx, |view, cx| view.filter_lines(action, window, cx));
        }
    })
}

/// One menu row: hover, keyboard selection and a click that runs its action.
fn row(label: String, props: RowProps) -> AnyElement {
    let theme = props.theme.selected_content(props.is_selected);
    let t = &theme;
    let hover = t.colors.hover;
    let (state, on_choose, position) = (props.state, props.on_choose, props.position);
    let label = SharedString::from(label);
    div()
        .id(("filter-lines-row", position))
        .role(Role::MenuItem)
        .aria_label(label.clone())
        .aria_selected(props.is_selected)
        .test_support()
        .flex()
        .items_center()
        .h(px(ROW_HEIGHT))
        .px(t.spacing.md)
        .rounded(t.radii.sm)
        .cursor_pointer()
        .when(props.is_dimmed, |d| d.text_color(t.colors.muted))
        .when(props.is_selected, |d| {
            d.bg(t.colors.selection).text_color(t.colors.on_selection)
        })
        .hover(move |style| style.bg(hover))
        .on_click(move |_, window: &mut Window, cx| {
            choose(&state, (position, &on_choose), window, cx);
        })
        .child(div().flex_1().min_w_0().truncate().child(label))
        .into_any_element()
}
