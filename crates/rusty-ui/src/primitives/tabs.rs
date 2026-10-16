use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::base::{Tab as BaseTab, TestSupportExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, RenderOnce, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use super::icon;
use crate::file_list::row::inset_ring;
use crate::tokens::{self, Theme};

type SelectHandler = Rc<dyn Fn(&usize, &mut Window, &mut App)>;

pub struct Tab {
    label: SharedString,
    icon: Option<IconName>,
}

impl Tab {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            icon: None,
        }
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }
}

#[derive(IntoElement)]
pub struct Tabs {
    id: ElementId,
    tabs: Vec<Tab>,
    selected: usize,
    is_highlighted: bool,
    on_select: Option<SelectHandler>,
}

impl Tabs {
    pub fn new(id: impl Into<ElementId>, tabs: impl IntoIterator<Item = Tab>) -> Self {
        Self {
            id: id.into(),
            tabs: tabs.into_iter().collect(),
            selected: 0,
            is_highlighted: false,
            on_select: None,
        }
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    pub fn highlighted(mut self, is_highlighted: bool) -> Self {
        self.is_highlighted = is_highlighted;
        self
    }

    pub fn on_select(mut self, handler: impl Fn(&usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(handler));
        self
    }
}

struct TabProps<'a> {
    theme: &'a Theme,
    index: usize,
    count: usize,
    is_selected: bool,
    is_highlighted: bool,
    on_select: Option<SelectHandler>,
}

fn render_tab(tab: Tab, props: TabProps) -> BaseTab {
    let t = props.theme;
    let index = props.index;
    let c = &t.colors;
    let color = if props.is_selected { c.text } else { c.muted };
    let base = BaseTab::new(index)
        .selected(props.is_selected)
        .set_position(index + 1, props.count)
        .accessibility_label(tab.label.clone())
        .gap(t.spacing.sm)
        .h(t.spacing.tab_height)
        .px(t.spacing.lg)
        .flex_none()
        .rounded(t.radii.pill)
        .text_color(color)
        .when_some(tab.icon, |base, name| {
            base.child(icon(name, t.spacing.icon - px(1.), color))
        })
        .child(tab.label)
        .when_some(props.on_select, |base, handler| {
            base.on_click(move |_, window, cx| handler(&index, window, cx))
        });
    match (props.is_selected, props.is_highlighted) {
        (false, _) => base.hover(|style| style.bg(c.hover)),
        (true, true) => base
            .font_weight(FontWeight::SEMIBOLD)
            .bg(t.surfaces.current),
        (true, false) => base
            .font_weight(FontWeight::SEMIBOLD)
            .bg(t.surfaces.field)
            .shadow(inset_ring(t.surfaces.card_edge)),
    }
}

impl RenderOnce for Tabs {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        let count = self.tabs.len();
        let selected = self.selected;
        let is_highlighted = self.is_highlighted;
        let on_select = self.on_select;
        let tabs = self.tabs.into_iter().enumerate().map(|(index, tab)| {
            let props = TabProps {
                theme: t,
                index,
                count,
                is_selected: index == selected,
                is_highlighted,
                on_select: on_select.clone(),
            };
            render_tab(tab, props)
        });
        div()
            .id(self.id)
            .role(Role::TabList)
            .test_support()
            .flex()
            .items_center()
            .gap(t.spacing.sm)
            .min_w_0()
            .overflow_hidden()
            .children(tabs)
    }
}
