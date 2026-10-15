use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, BoxShadow, Div, ElementId, FontWeight, Hsla, InteractiveElement, IntoElement,
    ParentElement, RenderOnce, Role, StatefulInteractiveElement, Styled, Window, div, point, px,
};
use rusty_core::Location;

use super::segments::{PathSegment, path_segments};
use crate::primitives::{IconName, icon};
use crate::tokens::{self, Theme};

type NavigateHandler = Rc<dyn Fn(&Location, &mut Window, &mut App)>;
type EditHandler = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct PathBar {
    id: ElementId,
    location: Location,
    is_active: bool,
    on_navigate: Option<NavigateHandler>,
    on_edit: Option<EditHandler>,
}

impl PathBar {
    pub fn new(id: impl Into<ElementId>, location: Location) -> Self {
        Self {
            id: id.into(),
            location,
            is_active: false,
            on_navigate: None,
            on_edit: None,
        }
    }

    pub fn active(mut self, is_active: bool) -> Self {
        self.is_active = is_active;
        self
    }

    pub fn on_navigate(
        mut self,
        handler: impl Fn(&Location, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_navigate = Some(Rc::new(handler));
        self
    }

    pub fn on_edit(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_edit = Some(Rc::new(handler));
        self
    }
}

fn ring(color: Hsla) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color,
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(1.),
        inset: false,
    }]
}

struct SegmentProps<'a> {
    theme: &'a Theme,
    index: usize,
    is_last: bool,
    on_navigate: Option<NavigateHandler>,
}

fn segment(item: PathSegment, props: SegmentProps) -> AnyElement {
    let t = props.theme;
    let c = &t.colors;
    let target = item.location;
    div()
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
        .child(item.label)
        .into_any_element()
}

fn drive_cell(t: &Theme, label: Option<&PathSegment>) -> Div {
    div()
        .flex()
        .items_center()
        .flex_none()
        .h_full()
        .px(t.spacing.md)
        .bg(t.colors.rail)
        .border_r_1()
        .border_color(t.colors.line)
        .rounded_l(t.radii.md)
        .font_weight(FontWeight::BOLD)
        .children(label.map(|segment| segment.label.clone()))
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
        let drive = drive_cell(t, segments.first());
        let last = segments.len().saturating_sub(1);
        let mut crumbs = Vec::with_capacity(segments.len() * 2);
        for (index, item) in segments.into_iter().enumerate() {
            if index > 0 {
                crumbs.push(icon(IconName::ChevronRight, px(11.), c.faint).into_any_element());
            }
            let props = SegmentProps {
                theme: t,
                index,
                is_last: index == last,
                on_navigate: self.on_navigate.clone(),
            };
            crumbs.push(segment(item, props));
        }
        let border = if self.is_active { c.accent } else { c.line };
        div()
            .id(self.id)
            .role(Role::Navigation)
            .aria_label("Path")
            .test_support()
            .flex()
            .items_center()
            .gap(t.spacing.sm)
            .flex_none()
            .m(t.spacing.sm)
            .h(t.spacing.path_height)
            .border_1()
            .border_color(border)
            .rounded(t.radii.md)
            .text_size(t.typography.small)
            .when(self.is_active, |d| d.shadow(ring(c.accent)))
            .bg(c.surface)
            .child(drive)
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .children(crumbs)
                    .child(edit_area(self.on_edit)),
            )
    }
}
