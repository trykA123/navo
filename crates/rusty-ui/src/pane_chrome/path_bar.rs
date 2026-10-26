use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, ElementId, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Pixels, RenderOnce, Role, StatefulInteractiveElement, Styled, Window, div, px,
};
use rusty_core::Location;

use super::crumb_menu::{CHEVRON, Hidden, Plan, ellipsis_crumb, hidden_menu, plan};
use super::segments::{PathSegment, path_segments};
use crate::drag::Landing;
use crate::file_list::DropPayload;
use crate::primitives::fit::Fit;
use crate::primitives::popup_menu::{PopupMenu, note_items, popup_state};
use crate::primitives::{IconName, icon};
use crate::tokens::{self, Theme};

const EDIT_MIN: f32 = 24.;

pub(super) type NavigateHandler = Rc<dyn Fn(&Location, &mut Window, &mut App)>;
type EditHandler = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct PathBar {
    id: ElementId,
    location: Location,
    on_navigate: Option<NavigateHandler>,
    on_edit: Option<EditHandler>,
    landing: Landing<Location>,
    menu: Option<Entity<PopupMenu>>,
}

impl PathBar {
    pub fn new(id: impl Into<ElementId>, location: Location) -> Self {
        Self {
            id: id.into(),
            location,
            on_navigate: None,
            on_edit: None,
            landing: Landing::default(),
            menu: None,
        }
    }

    pub(crate) fn menu(mut self, state: Entity<PopupMenu>) -> Self {
        self.menu = Some(state);
        self
    }

    pub fn on_navigate(
        mut self,
        handler: impl Fn(&Location, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_navigate = Some(Rc::new(handler));
        self
    }

    pub fn on_drop(
        mut self,
        handler: impl Fn(&Location, &DropPayload, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.landing.on_drop(handler);
        self
    }

    pub fn on_hover(
        mut self,
        handler: impl Fn(&Location, bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.landing.on_hover(handler);
        self
    }

    pub fn on_edit(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_edit = Some(Rc::new(handler));
        self
    }
}

struct SegmentProps<'a> {
    theme: &'a Theme,
    index: usize,
    is_last: bool,
    on_navigate: Option<NavigateHandler>,
    landing: Landing<Location>,
}

fn segment(item: PathSegment, props: SegmentProps) -> AnyElement {
    let t = props.theme;
    let c = &t.colors;
    let target = item.location.clone();
    let crumb = div()
        .id(("segment", props.index))
        .role(Role::Button)
        .aria_label(item.label.clone())
        .test_support()
        .when(!props.is_last, |d| d.flex_none())
        .when(props.is_last, |d| {
            d.min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .font_weight(FontWeight::SEMIBOLD)
        })
        .px(t.spacing.xs)
        .rounded(t.radii.xs)
        .cursor_pointer()
        .text_color(if props.is_last { c.text } else { c.muted })
        .hover(|style| style.bg(c.hover))
        .when_some(props.on_navigate, |d, handler| {
            d.on_click(move |_, window, cx| handler(&target, window, cx))
        })
        .child(item.label);
    props
        .landing
        .accept(crumb, (item.location, c.accent))
        .into_any_element()
}

fn edit_area(on_edit: Option<EditHandler>) -> impl IntoElement {
    div()
        .id("path-edit")
        .role(Role::Button)
        .aria_label("Type a path")
        .test_support()
        .flex_1()
        .h_full()
        .min_w(px(24.))
        .cursor_text()
        .when_some(on_edit, |d, handler| {
            d.on_click(move |_, window, cx| handler(window, cx))
        })
}

impl PathBar {
    fn crumbs(&self, plan: Plan, state: &Entity<PopupMenu>, cx: &App) -> Vec<AnyElement> {
        let t = tokens::current(cx);
        let open = state.read(cx).is_open;
        let c = &t.colors;
        let Plan { segments, hidden } = plan;
        let last = segments.len().saturating_sub(1);
        let mut items = Vec::with_capacity(segments.len() * 2 + 2);
        let mut hidden_items = Vec::new();
        for (index, item) in segments.into_iter().enumerate() {
            if hidden.contains(&index) {
                hidden_items.push((index, item));
                continue;
            }
            if index > 0 {
                items.push(icon(IconName::ChevronRight, px(CHEVRON), c.faint).into_any_element());
            }
            if index == hidden.end && !hidden.is_empty() {
                let hidden = Hidden {
                    items: std::mem::take(&mut hidden_items),
                    on_navigate: self.on_navigate.clone(),
                    state: state.clone(),
                };
                let menu = open.then(|| hidden_menu(hidden, t, cx));
                items.push(ellipsis_crumb(t, state, menu, cx));
                items.push(icon(IconName::ChevronRight, px(CHEVRON), c.faint).into_any_element());
            }
            let props = SegmentProps {
                theme: t,
                index,
                is_last: index == last,
                on_navigate: self.on_navigate.clone(),
                landing: self.landing.clone(),
            };
            items.push(segment(item, props));
        }
        items
    }

    fn crumb_row(
        self,
        (width, state): (Pixels, Entity<PopupMenu>),
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let t = tokens::current(cx).clone();
        let available = f32::from(width) - EDIT_MIN;
        let plan = plan(path_segments(&self.location), window, &t, available);
        note_items(&state, !plan.hidden.is_empty(), cx);
        let items = self.crumbs(plan, &state, cx);
        div()
            .flex()
            .items_center()
            .gap(t.spacing.xxs)
            .size_full()
            .min_w_0()
            .overflow_hidden()
            .children(items)
            .child(edit_area(self.on_edit))
            .into_any_element()
    }
}

impl RenderOnce for PathBar {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let c = &t.colors;
        let id = self.id.clone();
        let state = match self.menu.take() {
            Some(state) => state,
            None => popup_state((id.clone(), "crumbs"), window, cx),
        };
        let height = bar_height(&t);
        div()
            .id(id)
            .role(Role::Navigation)
            .aria_label("Path")
            .test_support()
            .flex()
            .items_center()
            .flex_none()
            .mx(t.spacing.md)
            .mb(t.spacing.sm)
            .h(height)
            .px(t.spacing.xs)
            .rounded(t.radii.md)
            .text_color(c.muted)
            .bg(t.surfaces.field)
            .child(Fit::new(height, move |width, window, cx| {
                self.crumb_row((width, state), window, cx)
            }))
    }
}

pub(super) fn bar_height(t: &Theme) -> Pixels {
    t.spacing.path_height + t.spacing.xxs
}
