use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::base::{Tab as BaseTab, TestSupportExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, ElementId, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Pixels, RenderOnce, Role, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

mod more;
mod plan;

use super::fit::Fit;
use super::popup_menu::{PopupMenu, note_items, popup_state};
use super::{Tooltip, icon};
use crate::drag::Landing;
use crate::file_list::DropPayload;
use crate::file_list::row::inset_ring;
use crate::tokens::{self, Theme};
use more::{Hidden, more_tabs};
use plan::{Budget, plan};

type SelectHandler = Rc<dyn Fn(&usize, &mut Window, &mut App)>;

#[derive(Clone)]
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
    landing: Landing<usize>,
    trailing: Option<(AnyElement, Pixels)>,
    menu: Option<Entity<PopupMenu>>,
}

impl Tabs {
    pub fn new(id: impl Into<ElementId>, tabs: impl IntoIterator<Item = Tab>) -> Self {
        Self {
            id: id.into(),
            tabs: tabs.into_iter().collect(),
            selected: 0,
            is_highlighted: false,
            on_select: None,
            landing: Landing::default(),
            trailing: None,
            menu: None,
        }
    }

    pub(crate) fn menu(mut self, state: Entity<PopupMenu>) -> Self {
        self.menu = Some(state);
        self
    }

    pub fn trailing(mut self, element: impl IntoElement, width: Pixels) -> Self {
        self.trailing = Some((element.into_any_element(), width));
        self
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

    pub(crate) fn set_landing(&mut self, landing: Landing<usize>) {
        self.landing = landing;
    }

    pub fn on_drop(
        mut self,
        handler: impl Fn(&usize, &DropPayload, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.landing.on_drop(handler);
        self
    }

    pub fn on_hover(
        mut self,
        handler: impl Fn(&usize, bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.landing.on_hover(handler);
        self
    }
}

struct TabProps<'a> {
    theme: &'a Theme,
    index: usize,
    count: usize,
    is_selected: bool,
    is_highlighted: bool,
    min_width: Pixels,
    on_select: Option<SelectHandler>,
    landing: Landing<usize>,
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
        .flex_shrink(1.)
        .min_w(props.min_width)
        .overflow_hidden()
        .rounded(t.radii.pill)
        .text_color(color)
        .when_some(tab.icon, |base, name| {
            base.child(icon(name, t.spacing.icon - px(1.), color))
        })
        .child(div().min_w_0().truncate().child(tab.label))
        .when(index < 8, |base| {
            base.tooltip(move |window, cx| {
                Tooltip::new(format!("Ctrl+{}", index + 1)).build(window, cx)
            })
        })
        .when_some(props.on_select, |base, handler| {
            base.on_click(move |_, window, cx| handler(&index, window, cx))
        });
    let base = props.landing.accept(base, (index, c.accent));
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

struct Row {
    tabs: Vec<Tab>,
    selected: usize,
    is_highlighted: bool,
    on_select: Option<SelectHandler>,
    landing: Landing<usize>,
    trailing: Option<(AnyElement, Pixels)>,
    menu: Entity<PopupMenu>,
}

fn build_row(row: Row, width: Pixels, cx: &mut App) -> AnyElement {
    let count = row.tabs.len();
    let gap = f32::from(tokens::current(cx).spacing.sm);
    let trailing_width = row.trailing.as_ref().map_or(0., |(_, w)| f32::from(*w));
    let budget = Budget {
        width: f32::from(width),
        trailing: trailing_width,
        gap,
    };
    let plan = plan(count, row.selected, &budget);
    note_items(&row.menu, plan.shown.len() < count, cx);
    let t = tokens::current(cx);
    let mut shown = Vec::new();
    let mut hidden = Vec::new();
    for (index, tab) in row.tabs.into_iter().enumerate() {
        match plan.shown.contains(&index) {
            true => shown.push((index, tab)),
            false => hidden.push((index, tab)),
        }
    }
    let tabs = shown.into_iter().map(|(index, tab)| {
        let props = TabProps {
            theme: t,
            index,
            count,
            is_selected: index == row.selected,
            is_highlighted: row.is_highlighted,
            min_width: px(plan.min_tab),
            on_select: row.on_select.clone(),
            landing: row.landing.clone(),
        };
        render_tab(tab, props)
    });
    let more = (!hidden.is_empty()).then(|| {
        let hidden = Hidden {
            state: row.menu.clone(),
            tabs: hidden,
            on_select: row.on_select.clone(),
        };
        more_tabs(t, hidden, cx)
    });
    div()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .w(width)
        .h(t.spacing.tab_height)
        .children(tabs)
        .children(more)
        .children(row.trailing.map(|(element, _)| element))
        .into_any_element()
}

impl RenderOnce for Tabs {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        let height = t.spacing.tab_height;
        let menu = match self.menu {
            Some(menu) => menu,
            None => popup_state((self.id.clone(), "more"), window, cx),
        };
        let row = Row {
            tabs: self.tabs,
            selected: self.selected,
            is_highlighted: self.is_highlighted,
            on_select: self.on_select,
            landing: self.landing,
            trailing: self.trailing,
            menu,
        };
        div()
            .id(self.id)
            .role(Role::TabList)
            .aria_label(rusty_l10n::tr("tabs-label"))
            .test_support()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .child(Fit::new(height, move |width, _, cx| {
                build_row(row, width, cx)
            }))
    }
}
