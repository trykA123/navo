use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::base::{Tab as BaseTab, TestSupportExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, Bounds, ElementId, Entity, FontWeight, InteractiveElement, IntoElement,
    MouseButton, ParentElement, Pixels, RenderOnce, Role, SharedString, StatefulInteractiveElement,
    Styled, Window, canvas, div, px,
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
type BoundsHandler = Rc<dyn Fn(usize, Bounds<Pixels>, &mut Window, &mut App)>;

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
    on_close: Option<SelectHandler>,
    on_bounds: Option<BoundsHandler>,
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
            on_select: None,
            on_close: None,
            on_bounds: None,
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

struct TabProps<'a> {
    theme: &'a Theme,
    index: usize,
    count: usize,
    is_selected: bool,
    is_highlighted: bool,
    min_width: Pixels,
    on_select: Option<SelectHandler>,
    on_close: Option<SelectHandler>,
    on_bounds: Option<BoundsHandler>,
    landing: Landing<usize>,
}

fn measured(base: BaseTab, handler: Option<BoundsHandler>, index: usize) -> BaseTab {
    let Some(handler) = handler else {
        return base;
    };
    base.relative().child(
        canvas(
            move |bounds, window, cx| handler(index, bounds, window, cx),
            |_, (), _, _| {},
        )
        .absolute()
        .size_full(),
    )
}

fn close_button(t: &Theme, index: usize, is_selected: bool, handler: SelectHandler) -> AnyElement {
    let group = SharedString::from(format!("tab-{index}"));
    let hover = t.colors.hover;
    div()
        .id(("tab-close", index))
        .role(Role::Button)
        .aria_label(rusty_l10n::tr("tabs-close"))
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(t.spacing.icon + t.spacing.xs)
        .rounded(t.radii.pill)
        .cursor_pointer()
        .when(!is_selected, |close| {
            close
                .opacity(0.)
                .group_hover(group, |style| style.opacity(1.))
        })
        .hover(move |style| style.bg(hover))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, window, cx| handler(&index, window, cx))
        .child(icon(IconName::X, t.spacing.icon - px(3.), t.colors.muted))
        .into_any_element()
}

fn closable(
    base: BaseTab,
    (t, index, is_selected): (&Theme, usize, bool),
    close: SelectHandler,
) -> BaseTab {
    let middle = close.clone();
    base.group(format!("tab-{index}"))
        .child(close_button(t, index, is_selected, close))
        .on_mouse_down(MouseButton::Middle, move |_, window, cx| {
            middle(&index, window, cx)
        })
}

fn render_tab(tab: Tab, props: TabProps) -> BaseTab {
    let theme = props
        .theme
        .selected_content(props.is_selected && props.is_highlighted);
    let t = &theme;
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
        .when_some(props.on_close.clone(), |base, handler| {
            closable(base, (t, index, props.is_selected), handler)
        })
        .when(index < 8, |base| {
            base.tooltip(move |window, cx| {
                Tooltip::new(format!("Ctrl+{}", index + 1)).build(window, cx)
            })
        })
        .when_some(props.on_select, |base, handler| {
            base.on_click(move |_, window, cx| handler(&index, window, cx))
        });
    let base = measured(base, props.on_bounds, index);
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
    on_close: Option<SelectHandler>,
    on_bounds: Option<BoundsHandler>,
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
    let plan = plan(count, row.selected, &budget);
    note_items(&row.menu, plan.shown.len() < count, cx);
    let t = tokens::current(cx);
    let bar = row.bar.as_ref().unwrap_or(t);
    let (shown, hidden): (Vec<_>, Vec<_>) = row
        .tabs
        .into_iter()
        .enumerate()
        .partition(|(index, _)| plan.shown.contains(index));
    let tabs = shown.into_iter().map(|(index, tab)| {
        let is_selected = index == row.selected;
        let props = TabProps {
            theme: if is_selected { t } else { bar },
            index,
            count,
            is_selected,
            is_highlighted: row.is_highlighted,
            min_width: px(plan.min_tab),
            on_select: row.on_select.clone(),
            on_close: row.on_close.clone(),
            on_bounds: row.on_bounds.clone(),
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
        let row = Row {
            tabs: self.tabs,
            selected: self.selected,
            is_highlighted: self.is_highlighted,
            on_select: self.on_select,
            on_close: self.on_close,
            on_bounds: self.on_bounds,
            landing: self.landing,
            trailing: self.trailing,
            menu,
            bar: self.bar,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::{Mode, SystemPalette, contrast_theme};
    use gpui_kit::{black, rgb, white};

    #[test]
    fn highlighted_tab_uses_the_system_selection_foreground() {
        let palette = SystemPalette {
            window: black(),
            window_text: white(),
            highlight: white(),
            highlight_text: black(),
            button_face: rgb(0x222222).into(),
            button_text: white(),
            gray_text: rgb(0x767676).into(),
            hot_light: rgb(0xffff00).into(),
        };
        let theme = contrast_theme(&Theme::new(Mode::Dark), &palette);
        for (is_selected, is_highlighted, expected) in [
            (true, true, palette.highlight_text),
            (true, false, palette.window_text),
            (false, true, palette.gray_text),
        ] {
            let mut tab = render_tab(
                Tab::new("Folder"),
                TabProps {
                    theme: &theme,
                    index: 0,
                    count: 1,
                    is_selected,
                    is_highlighted,
                    min_width: px(88.),
                    on_select: None,
                    on_close: None,
                    on_bounds: None,
                    landing: Landing::default(),
                },
            );
            assert_eq!(tab.style().text.color, Some(expected));
        }
    }
}
