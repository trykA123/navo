use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, Role, StatefulInteractiveElement, Styled, Window, div, px,
};
use rusty_core::Location;

use super::segments::{PathSegment, path_segments};
use crate::drag::Landing;
use crate::file_list::DropPayload;
use crate::primitives::{IconName, icon};
use crate::tokens::{self, Theme};

const CRUMB_HEIGHT: f32 = 30.;

type NavigateHandler = Rc<dyn Fn(&Location, &mut Window, &mut App)>;
type EditHandler = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct PathBar {
    id: ElementId,
    location: Location,
    on_navigate: Option<NavigateHandler>,
    on_edit: Option<EditHandler>,
    landing: Landing<Location>,
}

impl PathBar {
    pub fn new(id: impl Into<ElementId>, location: Location) -> Self {
        Self {
            id: id.into(),
            location,
            on_navigate: None,
            on_edit: None,
            landing: Landing::default(),
        }
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
        .flex_none()
        .px(t.spacing.xs)
        .rounded(t.radii.xs)
        .cursor_pointer()
        .text_color(if props.is_last { c.text } else { c.muted })
        .when(props.is_last, |d| d.font_weight(FontWeight::SEMIBOLD))
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

impl RenderOnce for PathBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        let c = &t.colors;
        let segments = path_segments(&self.location);
        let last = segments.len().saturating_sub(1);
        let mut crumbs = Vec::with_capacity(segments.len() * 2);
        for (index, item) in segments.into_iter().enumerate() {
            if index > 0 {
                crumbs.push(icon(IconName::ChevronRight, px(12.), c.faint).into_any_element());
            }
            let props = SegmentProps {
                theme: t,
                index,
                is_last: index == last,
                on_navigate: self.on_navigate.clone(),
                landing: self.landing.clone(),
            };
            crumbs.push(segment(item, props));
        }
        div()
            .id(self.id)
            .role(Role::Navigation)
            .aria_label("Path")
            .test_support()
            .flex()
            .items_center()
            .flex_none()
            .mx(t.spacing.md)
            .mb(t.spacing.sm)
            .h(px(CRUMB_HEIGHT))
            .px(t.spacing.xs)
            .rounded(t.radii.md)
            .text_color(c.muted)
            .bg(t.surfaces.field)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.spacing.xxs)
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .children(crumbs)
                    .child(edit_area(self.on_edit)),
            )
    }
}
