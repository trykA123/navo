//! Single tab item, optional 14 px status slot, and close button for [`super::Tabs`].

use gpui_kit::assets::IconName;
use gpui_kit::base::{Tab as BaseTab, TestSupportExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, FontWeight, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels,
    Role, SharedString, StatefulInteractiveElement, Styled, canvas, div, px,
};

use super::{BoundsHandler, MenuHandler, SelectHandler};
use crate::drag::Landing;
use crate::file_list::row::inset_ring;
use crate::primitives::{Tooltip, icon};
use crate::tokens::Theme;

/// Width and height of the optional tab status slot in logical pixels.
pub const TAB_STATUS_SIZE: f32 = 14.0;
const STATUS_DOT_SIZE: f32 = 6.0;
const STATUS_ICON_SIZE: f32 = 12.0;
const STATUS_EXITED_SIZE: f32 = 10.0;

/// Optional 14 px status indicator rendered inside a [`Tab`] and its overflow menu row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabStatus {
    /// Accent dot for an active/busy session or unread bell.
    Dot,
    /// Failure indicator (`colors.mark`).
    Failed,
    /// Muted indicator for an exited session (`colors.muted`).
    Exited,
    /// Warning indicator (`colors.warn`), such as a missing shell.
    Warning,
}

/// One tab entry in [`super::Tabs`].
#[derive(Clone)]
pub struct Tab {
    pub(super) label: SharedString,
    pub(super) icon: Option<IconName>,
    pub(super) pinned: bool,
    pub(super) status: Option<TabStatus>,
    pub(super) italic: bool,
}

impl Tab {
    /// Creates a tab with `label` and default appearance.
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            pinned: false,
            status: None,
            italic: false,
        }
    }

    /// Marks the tab as pinned (compact width, no close button).
    pub fn pinned(mut self, pinned: bool) -> Self {
        self.pinned = pinned;
        self
    }

    /// Sets an optional leading icon on the tab.
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Sets an optional 14 px status indicator on the tab.
    pub fn status(mut self, status: TabStatus) -> Self {
        self.status = Some(status);
        self
    }

    /// Sets or clears the optional 14 px status indicator on the tab.
    pub fn maybe_status(mut self, status: Option<TabStatus>) -> Self {
        self.status = status;
        self
    }

    /// Renders the tab label in italic (used for exited terminal sessions).
    pub fn italic(mut self, italic: bool) -> Self {
        self.italic = italic;
        self
    }
}

pub(super) struct TabProps<'a> {
    pub theme: &'a Theme,
    pub index: usize,
    pub count: usize,
    pub is_selected: bool,
    pub is_highlighted: bool,
    pub min_width: Pixels,
    pub on_select: Option<SelectHandler>,
    pub on_close: Option<SelectHandler>,
    pub on_bounds: Option<BoundsHandler>,
    pub on_menu: Option<MenuHandler>,
    pub landing: Landing<usize>,
}

/// Renders the fixed 14 px status slot for `status`.
pub(super) fn status_slot(t: &Theme, index: usize, status: TabStatus) -> AnyElement {
    let (label, glyph): (String, AnyElement) = match status {
        TabStatus::Dot => (
            navo_l10n::tr("terminal-status-busy"),
            div()
                .size(px(STATUS_DOT_SIZE))
                .rounded_full()
                .bg(t.colors.accent)
                .into_any_element(),
        ),
        TabStatus::Failed => (
            navo_l10n::tr("terminal-status-failed"),
            icon(IconName::CircleX, px(STATUS_ICON_SIZE), t.colors.mark).into_any_element(),
        ),
        TabStatus::Exited => (
            navo_l10n::tr("terminal-status-exited"),
            icon(IconName::Square, px(STATUS_EXITED_SIZE), t.colors.muted).into_any_element(),
        ),
        TabStatus::Warning => (
            navo_l10n::tr("terminal-status-missing"),
            icon(IconName::TriangleAlert, px(STATUS_ICON_SIZE), t.colors.warn).into_any_element(),
        ),
    };
    div()
        .id(("tab-status", index))
        .aria_label(label)
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(TAB_STATUS_SIZE))
        .child(glyph)
        .into_any_element()
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
        .aria_label(navo_l10n::tr("tabs-close"))
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
            middle(&index, window, cx);
        })
}

pub(super) fn render_tab(tab: Tab, props: TabProps) -> BaseTab {
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
        .child(
            div()
                .min_w_0()
                .when(tab.pinned, |d| d.max_w(t.spacing.pinned_tab_label))
                .when(tab.italic, |d| d.italic())
                .truncate()
                .child(tab.label),
        )
        .when_some(tab.status, |base, status| {
            base.child(status_slot(t, index, status))
        })
        .when_some(
            props.on_close.clone().filter(|_| !tab.pinned),
            |base, handler| closable(base, (t, index, props.is_selected), handler),
        )
        .when(index < 8, |base| {
            base.tooltip(move |window, cx| {
                Tooltip::new(format!("Ctrl+{}", index + 1)).build(window, cx)
            })
        })
        .when_some(props.on_menu, |base, handler| {
            base.on_mouse_up(MouseButton::Right, move |event, window, cx| {
                cx.stop_propagation();
                handler(index, event.position, window, cx);
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
