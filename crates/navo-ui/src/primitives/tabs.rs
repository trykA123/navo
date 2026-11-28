use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    AnyElement, App, Bounds, ElementId, Entity, InteractiveElement, IntoElement, ParentElement,
    Pixels, Point, RenderOnce, Role, SharedString, StatefulInteractiveElement, Styled, Window, div,
    px,
};

mod more;
mod plan;
mod tab;

use super::fit::Fit;
use super::popup_menu::{PopupMenu, note_items, popup_state};
use crate::drag::Landing;
use crate::file_list::DropPayload;
use crate::tokens::{self, Theme};
use more::{Hidden, more_tabs};
use plan::{Budget, MIN_PINNED_TAB, plan};
pub use tab::{TAB_STATUS_SIZE, Tab, TabStatus};
use tab::{TabProps, render_tab, status_slot};

type SelectHandler = Rc<dyn Fn(&usize, &mut Window, &mut App)>;
type MenuHandler = Rc<dyn Fn(usize, Point<Pixels>, &mut Window, &mut App)>;
type BoundsHandler = Rc<dyn Fn(usize, Bounds<Pixels>, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Tabs {
    id: ElementId,
    tabs: Vec<Tab>,
    selected: usize,
    is_highlighted: bool,
    aria_label: Option<SharedString>,
    on_select: Option<SelectHandler>,
    on_close: Option<SelectHandler>,
    on_bounds: Option<BoundsHandler>,
    on_menu: Option<MenuHandler>,
    landing: Landing<usize>,
    trailing: Option<(AnyElement, Pixels)>,
    menu: Option<Entity<PopupMenu>>,
    bar: Option<Theme>,
}

impl Tabs {
    pub fn new(id: impl Into<ElementId>, tabs: impl IntoIterator<Item = Tab>) -> Self {
        Self {
            id: id.into(),
            tabs: tabs.into_iter().collect(),
            selected: 0,
            is_highlighted: false,
            aria_label: None,
            on_select: None,
            on_close: None,
            on_bounds: None,
            on_menu: None,
            landing: Landing::default(),
            trailing: None,
            menu: None,
            bar: None,
        }
    }

    pub(crate) fn menu(mut self, state: Entity<PopupMenu>) -> Self {
        self.menu = Some(state);
        self
    }

    pub fn bar_theme(mut self, theme: Theme) -> Self {
        self.bar = Some(theme);
        self
    }

    pub fn aria_label(mut self, label: impl Into<SharedString>) -> Self {
        self.aria_label = Some(label.into());
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

    pub fn on_close(mut self, handler: impl Fn(&usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(handler));
        self
    }

    pub(crate) fn on_menu(mut self, handler: Option<MenuHandler>) -> Self {
        self.on_menu = handler;
        self
    }

    pub(crate) fn on_bounds(mut self, handler: Option<BoundsHandler>) -> Self {
        self.on_bounds = handler;
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

struct Row {
    tabs: Vec<Tab>,
    selected: usize,
    is_highlighted: bool,
    on_select: Option<SelectHandler>,
    on_close: Option<SelectHandler>,
    on_bounds: Option<BoundsHandler>,
    on_menu: Option<MenuHandler>,
    landing: Landing<usize>,
    trailing: Option<(AnyElement, Pixels)>,
    menu: Entity<PopupMenu>,
    bar: Option<Theme>,
}

fn build_row(row: Row, width: Pixels, cx: &mut App) -> AnyElement {
    let count = row.tabs.len();
    let budget = Budget {
        width: f32::from(width),
        trailing: row.trailing.as_ref().map_or(0., |(_, w)| f32::from(*w)),
        gap: f32::from(tokens::current(cx).spacing.sm),
    };
    let pinned: Vec<_> = row.tabs.iter().map(|tab| tab.pinned).collect();
    let plan = plan(&pinned, row.selected, &budget);
    note_items(&row.menu, plan.shown.len() < count, cx);
    let t = tokens::current(cx);
    let bar = row.bar.as_ref().unwrap_or(t);
    let (shown, hidden): (Vec<_>, Vec<_>) = row
        .tabs
        .into_iter()
        .enumerate()
        .partition(|(index, _)| plan.shown.contains(index));
    let pinned_end = shown
        .iter()
        .filter(|(_, tab)| tab.pinned)
        .map(|(index, _)| *index)
        .max();
    let has_unpinned = shown.iter().any(|(_, tab)| !tab.pinned);
    let tabs = shown.into_iter().flat_map(|(index, tab)| {
        let is_selected = index == row.selected;
        let props = TabProps {
            theme: if is_selected { t } else { bar },
            index,
            count,
            is_selected,
            is_highlighted: row.is_highlighted,
            min_width: px(if tab.pinned {
                plan.min_tab.min(MIN_PINNED_TAB)
            } else {
                plan.min_tab
            }),
            on_select: row.on_select.clone(),
            on_close: row.on_close.clone(),
            on_bounds: row.on_bounds.clone(),
            on_menu: row.on_menu.clone(),
            landing: row.landing.clone(),
        };
        let divider = (pinned_end == Some(index) && has_unpinned).then(|| {
            div()
                .flex_none()
                .w(px(1.))
                .h(t.spacing.icon)
                .bg(bar.surfaces.card_edge)
                .into_any_element()
        });
        [Some(render_tab(tab, props).into_any_element()), divider]
            .into_iter()
            .flatten()
    });
    let more = (!hidden.is_empty()).then(|| {
        let hidden = Hidden {
            state: row.menu.clone(),
            tabs: hidden,
            on_select: row.on_select.clone(),
        };
        more_tabs(t, bar, hidden, cx)
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
        let label = self
            .aria_label
            .unwrap_or_else(|| SharedString::from(navo_l10n::tr("tabs-label")));
        let row = Row {
            tabs: self.tabs,
            selected: self.selected,
            is_highlighted: self.is_highlighted,
            on_select: self.on_select,
            on_close: self.on_close,
            on_bounds: self.on_bounds,
            on_menu: self.on_menu,
            landing: self.landing,
            trailing: self.trailing,
            menu,
            bar: self.bar,
        };
        div()
            .id(self.id)
            .role(Role::TabList)
            .aria_label(label)
            .test_support()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .child(Fit::new(height, move |width, _, cx| {
                build_row(row, width, cx)
            }))
    }
}

#[cfg(test)]
mod tests;
