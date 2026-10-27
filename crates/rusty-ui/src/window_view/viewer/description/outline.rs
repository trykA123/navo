use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{Action, Role, Stateful};
use rusty_automotive::{Outline, OutlineRow};

use super::{Bind, DescriptionView, LeftDrawer};
use crate::prelude::*;
use crate::primitives::{IconName, icon};
use crate::tokens::Theme;
use crate::window_view::viewer::{
    FindOnEnter, GoEnd, GoStart, RowDown, RowUp, ScrollLeft, ScrollRight,
};

const INDENT: f32 = 18.;

pub(super) struct OutlineState {
    source: Arc<Outline>,
    pub(super) visible: Vec<usize>,
    selected: Option<usize>,
    folded: HashSet<usize>,
}

impl OutlineState {
    pub(super) fn new(source: Arc<Outline>) -> Self {
        Self {
            visible: (0..source.rows.len()).collect(),
            selected: source
                .rows
                .iter()
                .position(|row| row.group || row.item.is_some()),
            source,
            folded: HashSet::new(),
        }
    }

    pub(super) fn select(&mut self, at: usize) -> bool {
        if self
            .source
            .rows
            .get(at)
            .is_some_and(|row| row.group || row.item.is_some())
            && self.visible.binary_search(&at).is_ok()
        {
            self.selected = Some(at);
            return true;
        }
        false
    }

    pub(super) fn position(&self) -> Option<usize> {
        self.visible.binary_search(&self.selected?).ok()
    }

    pub(super) fn move_by(&mut self, by: isize) {
        let Some(position) = self.position() else {
            return;
        };
        let target = position
            .saturating_add_signed(by)
            .min(self.visible.len().saturating_sub(1));
        if let Some(at) = self.visible[target..].iter().copied().find(|at| {
            let row = &self.source.rows[*at];
            row.group || row.item.is_some()
        }) {
            self.select(at);
        }
    }

    pub(super) fn left(&mut self) {
        let Some(at) = self.selected else {
            return;
        };
        if self.source.rows[at].group && !self.folded.contains(&at) {
            self.toggle();
        } else if let Some(parent) = self.source.parent(at) {
            self.select(parent);
        }
    }

    pub(super) fn right(&mut self) {
        if self.selected.is_some_and(|at| self.folded.contains(&at)) {
            self.toggle();
        }
    }

    pub(super) fn toggle(&mut self) {
        let Some(at) = self.selected.filter(|at| self.source.rows[*at].group) else {
            return;
        };
        if !self.folded.remove(&at) {
            self.folded.insert(at);
        }
        self.rebuild();
    }

    fn rebuild(&mut self) {
        self.visible.clear();
        let mut hidden_depth = None;
        for (at, row) in self.source.rows.iter().enumerate() {
            if hidden_depth.is_some_and(|depth| row.depth > depth) {
                continue;
            }
            self.visible.push(at);
            hidden_depth = self.folded.contains(&at).then_some(row.depth);
        }
    }
}

fn action<A: Action>(
    cx: &Context<DescriptionView>,
    run: fn(&mut DescriptionView, &mut Context<DescriptionView>),
) -> Bind {
    let listener = Rc::new(cx.listener(move |view, _: &A, _, cx| run(view, cx)));
    Box::new(move |root: Stateful<Div>| {
        let listener = listener.clone();
        root.on_action(move |event: &A, window, cx| listener(event, window, cx))
    })
}

pub(super) fn bindings(cx: &Context<DescriptionView>) -> Vec<Bind> {
    vec![
        action::<RowUp>(cx, |view, cx| view.move_by(-1, cx)),
        action::<RowDown>(cx, |view, cx| view.move_by(1, cx)),
        action::<GoStart>(cx, |view, cx| view.move_by(isize::MIN, cx)),
        action::<GoEnd>(cx, |view, cx| view.move_by(isize::MAX, cx)),
        action::<ScrollLeft>(cx, |view, cx| {
            view.outline.left();
            view.refresh(cx);
        }),
        action::<ScrollRight>(cx, |view, cx| {
            view.outline.right();
            view.refresh(cx);
        }),
        action::<FindOnEnter>(cx, |view, cx| {
            view.outline.toggle();
            view.refresh(cx);
        }),
        action::<LeftDrawer>(cx, DescriptionView::toggle_outline),
    ]
}

impl DescriptionView {
    pub(super) fn click_row(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        if !self.outline.select(at) {
            return;
        }
        self.outline.toggle();
        self.refresh(cx);
    }

    fn move_by(&mut self, by: isize, cx: &mut Context<Self>) {
        self.outline.move_by(by);
        self.refresh(cx);
    }

    pub(super) fn row(&self, at: usize, t: &Theme) -> AnyElement {
        let row = &self.file.outline.rows[at];
        let selected = self.outline.selected == Some(at);
        let on_click = self.row_clicks[at].clone();
        let color = match (selected, row.group || row.item.is_some()) {
            (true, _) => t.colors.on_selection,
            (false, true) => t.colors.text,
            (false, false) => t.colors.muted,
        };
        div()
            .id(("description-row", at))
            .role(Role::TreeItem)
            .aria_label(format!("{} {}", row.label, row.aux).trim().to_owned())
            .aria_selected(selected)
            .when(row.group, |item| {
                item.aria_expanded(!self.outline.folded.contains(&at))
            })
            .test_support()
            .flex()
            .items_center()
            .size_full()
            .min_w_0()
            .gap(t.spacing.sm)
            .pr(t.spacing.xl)
            .pl(t.spacing.xl + px(INDENT * f32::from(row.depth)))
            .font_family(t.typography.mono_font.clone())
            .text_color(color)
            .when(row.group, |item| item.font_weight(FontWeight::SEMIBOLD))
            .border_l(px(3.))
            .border_color(gpui_kit::transparent_black())
            .when(selected, |item| {
                item.bg(t.colors.selection).border_color(t.colors.accent)
            })
            .when(!selected, |item| {
                item.hover(|style| style.bg(t.colors.hover))
            })
            .child(self.chevron(at, t))
            .child(row_text(row, t, selected))
            .on_click(move |event, window, cx| on_click(event, window, cx))
            .into_any_element()
    }

    fn chevron(&self, at: usize, t: &Theme) -> Div {
        let row = &self.file.outline.rows[at];
        div().w(t.spacing.icon).flex_none().when(row.group, |slot| {
            let name = if self.outline.folded.contains(&at) {
                IconName::ChevronRight
            } else {
                IconName::ChevronDown
            };
            slot.child(icon(name, t.spacing.icon, t.colors.muted))
        })
    }
}

fn row_text(row: &OutlineRow, t: &Theme, selected: bool) -> Div {
    let color = if selected {
        t.colors.on_selection
    } else {
        t.colors.muted
    };
    div()
        .flex()
        .flex_1()
        .min_w_0()
        .gap(t.spacing.sm)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(SharedString::from(row.label.clone())),
        )
        .child(
            div()
                .flex_none()
                .text_size(t.typography.small)
                .text_color(color)
                .child(SharedString::from(row.aux.clone())),
        )
}
