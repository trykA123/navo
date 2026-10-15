use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::base::{Tab as BaseTab, TestSupportExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, RenderOnce, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use super::icon;
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
    on_select: Option<SelectHandler>,
}

impl Tabs {
    pub fn new(id: impl Into<ElementId>, tabs: impl IntoIterator<Item = Tab>) -> Self {
        Self {
            id: id.into(),
            tabs: tabs.into_iter().collect(),
            selected: 0,
            on_select: None,
        }
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
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
    on_select: Option<SelectHandler>,
}

fn render_tab(tab: Tab, props: TabProps) -> BaseTab {
    let t = props.theme;
    let index = props.index;
    let c = &t.colors;
    let base = BaseTab::new(index)
        .selected(props.is_selected)
        .set_position(index + 1, props.count)
        .accessibility_label(tab.label.clone())
        .gap(t.spacing.sm)
        .h(t.spacing.tab_height)
        .px(t.spacing.lg)
        .flex_none()
        .text_size(t.typography.small)
        .text_color(c.muted)
        .when_some(tab.icon, |base, name| {
            base.child(icon(name, t.typography.body, c.folder))
        })
        .child(tab.label)
        .when_some(props.on_select, |base, handler| {
            base.on_click(move |_, window, cx| handler(&index, window, cx))
        });
    if !props.is_selected {
        return base;
    }
    base.text_color(c.text)
        .font_weight(FontWeight::SEMIBOLD)
        .mb(px(-1.))
        .rounded_t(t.radii.md)
        .border_1()
        .border_color(c.line)
        .bg(c.surface)
}

impl RenderOnce for Tabs {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        let count = self.tabs.len();
        let selected = self.selected;
        let on_select = self.on_select;
        let tabs = self.tabs.into_iter().enumerate().map(|(index, tab)| {
            let props = TabProps {
                theme: t,
                index,
                count,
                is_selected: index == selected,
                on_select: on_select.clone(),
            };
            render_tab(tab, props)
        });
        div()
            .id(self.id)
            .role(Role::TabList)
            .test_support()
            .flex()
            .items_end()
            .flex_none()
            .min_w_0()
            .overflow_hidden()
            .px(t.spacing.sm)
            .pt(t.spacing.sm)
            .bg(t.colors.rail)
            .border_b_1()
            .border_color(t.colors.line)
            .children(tabs)
    }
}
