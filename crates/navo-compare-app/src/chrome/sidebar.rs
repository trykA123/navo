//! The sessions sidebar (NC66): 280 px expanded, a 56 px icon rail collapsed, toggled by
//! Ctrl+\. The width lerps over the side token through the U12b reduced-motion gate.

use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::{AnimationExt, EventEmitter, FocusHandle, KeyBinding, ReadGlobal, Role, img};
use navo_compare_profile::recent::{StartRow, StartRows};
use sidewise_kit::prelude::*;
use sidewise_kit::primitives::icon;

use crate::icons::{MaterialIcon, cache::IconCache, icon_id};
use crate::motion;
use crate::theme::SidewiseTheme;
use crate::theme_data::palettes::{DIMS, ModeTokens};

#[path = "sidebar_actions.rs"]
mod actions;
#[path = "rail_tooltip.rs"]
mod rail_tooltip;

pub use actions::SidebarEvent;
use rail_tooltip::RailTips;

gpui_kit::actions!(
    sidebar,
    [ToggleRail, NextRow, PreviousRow, DismissTip, ShowTabMenu]
);

/// Key context of the sidebar root; Ctrl+\ and row traversal keys bind inside it.
const CONTEXT: &str = "Sidebar";
/// The mark side both modes draw: the Material pack's 1x bucket.
const MARK: u32 = 16;
/// The mark side as a length.
const MARK_PX: f32 = MARK as f32;

/// Binds `Sidebar`-scoped keys; the shell calls this from its own `bind_keys`.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-\\", ToggleRail, Some(CONTEXT)),
        KeyBinding::new("down", NextRow, Some(CONTEXT)),
        KeyBinding::new("tab", NextRow, Some(CONTEXT)),
        KeyBinding::new("up", PreviousRow, Some(CONTEXT)),
        KeyBinding::new("shift-tab", PreviousRow, Some(CONTEXT)),
        KeyBinding::new("escape", DismissTip, Some(CONTEXT)),
        KeyBinding::new("shift-f10", ShowTabMenu, Some(CONTEXT)),
    ]);
}

/// A row's mark: a bundled chrome glyph, or a Material file mark from U11c.
#[derive(Clone, Copy)]
enum Mark {
    Chrome(IconName),
    File(MaterialIcon),
}

/// One entry: its mark, its label and its optional key hint.
struct Entry {
    mark: Mark,
    label: SharedString,
    hint: Option<&'static str>,
}

/// Typed identity of each sidebar section so rendering branches without matching labels.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SectionKind {
    Primary,
    OpenTabs,
    Recent,
    Named,
}

/// One headed list of entries; `Primary` is unheaded.
struct Section {
    kind: SectionKind,
    heading: &'static str,
    entries: Vec<Entry>,
}

/// The sidebar: the rail flag, the width it animates from, and the lists it shows.
pub struct Sidebar {
    rail: bool,
    from: Pixels,
    rows: StartRows,
    tabs: Vec<SharedString>,
    active: usize,
    handle: FocusHandle,
    new_compare_focus: FocusHandle,
    tab_focuses: Vec<FocusHandle>,
    tips: RailTips,
    active_bounds: Bounds<Pixels>,
}

impl EventEmitter<SidebarEvent> for Sidebar {}

impl Sidebar {
    /// An expanded sidebar over the stored rows and the open tab titles.
    pub fn new(rows: StartRows, tabs: Vec<SharedString>, cx: &mut Context<Self>) -> Self {
        let tab_focuses = (0..tabs.len())
            .map(|_| cx.focus_handle().tab_stop(true))
            .collect();
        Self {
            rail: false,
            from: DIMS.sidebar_w,
            rows,
            tabs,
            active: 0,
            handle: cx.focus_handle(),
            new_compare_focus: cx.focus_handle().tab_stop(true),
            tab_focuses,
            tips: RailTips::new(),
            active_bounds: Bounds::default(),
        }
    }

    /// Whether the icon rail is showing.
    pub fn rail(&self) -> bool {
        self.rail
    }

    /// The width the sidebar settles at: the rail or the full sidebar.
    pub fn width(&self) -> Pixels {
        if self.rail {
            DIMS.rail_w
        } else {
            DIMS.sidebar_w
        }
    }

    /// Ctrl+\: swap rail and sidebar, animating from the width being left, and emit `RailToggled`.
    pub fn toggle_rail(&mut self, cx: &mut Context<Self>) {
        self.from = self.width();
        self.rail = !self.rail;
        self.tips.clear();
        cx.emit(SidebarEvent::RailToggled(self.rail));
        cx.notify();
    }

    /// Replaces the open tab labels and clamps `active`, keeping handles for unchanged indices.
    pub fn set_tabs(&mut self, tabs: Vec<SharedString>, active: usize, cx: &mut Context<Self>) {
        if self.tabs != tabs {
            self.tabs = tabs;
            self.tips.sync_tabs(&self.tabs);
        }
        self.active = if self.tabs.is_empty() {
            0
        } else {
            active.min(self.tabs.len() - 1)
        };
        self.tab_focuses.truncate(self.tabs.len());
        while self.tab_focuses.len() < self.tabs.len() {
            self.tab_focuses.push(cx.focus_handle().tab_stop(true));
        }
        cx.notify();
    }

    /// Moves focus onto the mounted `New comparison` button.
    pub fn focus_primary(&self, window: &mut Window, cx: &mut App) {
        self.new_compare_focus.focus(window, cx);
    }

    /// Whether keyboard focus currently sits on the sidebar or any mounted child inside it.
    pub fn contains_focus(&self, window: &Window, cx: &App) -> bool {
        self.handle.contains_focused(window, cx)
    }

    /// The primary actions, then the three lists, in prototype order.
    fn sections(&self) -> Vec<Section> {
        let recent = self
            .rows
            .recent
            .iter()
            .map(|r| entry_of(r, false))
            .collect();
        let named = self.rows.named.iter().map(|r| entry_of(r, true)).collect();
        let tabs = self
            .tabs
            .iter()
            .map(|title| Entry {
                mark: Mark::File(MaterialIcon::Document),
                label: title.clone(),
                hint: None,
            })
            .collect();
        vec![
            Section {
                kind: SectionKind::Primary,
                heading: "",
                entries: primary(),
            },
            Section {
                kind: SectionKind::OpenTabs,
                heading: "Open tabs",
                entries: tabs,
            },
            Section {
                kind: SectionKind::Recent,
                heading: "Recent",
                entries: recent,
            },
            Section {
                kind: SectionKind::Named,
                heading: "Named",
                entries: named,
            },
        ]
    }

    /// The label and hint of one entry, or nothing in the rail: the rail is icon-only.
    fn label_view(&self, entry: &Entry, m: &ModeTokens) -> Option<Div> {
        if self.rail {
            return None;
        }
        let mut text = div()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .text_color(m.text)
            .child(entry.label.clone());
        if let Some(hint) = entry.hint {
            text = text.child(div().text_color(m.faint).child(hint));
        }
        Some(text)
    }

    /// One row: its mark, then its label only when the sidebar is expanded.
    fn row_view(&self, entry: &Entry, m: &ModeTokens, cx: &mut Context<Self>) -> Div {
        let mut row = div()
            .flex()
            .items_center()
            .gap_2()
            .min_h(DIMS.row_h)
            .child(mark_view(entry.mark, m.muted, cx));
        if let Some(label) = self.label_view(entry, m) {
            row = row.child(label);
        }
        row
    }

    /// One non-focusable, non-clickable row retaining its full accessible label in rail mode.
    fn static_row(id: ElementId, label: SharedString, row: Div) -> AnyElement {
        row.id(id)
            .aria_label(label)
            .test_support()
            .into_any_element()
    }

    /// One headed list; open tabs expose `Role::TabList` and drop headings along with labels in rail mode.
    fn group_view(&self, section: &Section, m: &ModeTokens, cx: &mut Context<Self>) -> AnyElement {
        let mut group = div().flex().flex_col().gap_1();
        if !section.heading.is_empty() && !self.rail {
            group = group.child(div().text_color(m.muted).child(section.heading));
        }
        let group = group.children(section.entries.iter().enumerate().map(|(i, entry)| {
            match (section.kind, i) {
                (SectionKind::Primary, 0) => self.primary_row(entry, m, cx),
                (SectionKind::OpenTabs, i) => self.tab_row(actions::TabRow::new(i, entry), m, cx),
                (SectionKind::Recent, i) => self.recent_row((i, entry), m, cx),
                _ => Self::static_row(
                    (section.heading, i).into(),
                    entry.label.clone(),
                    self.row_view(entry, m, cx),
                ),
            }
        }));
        if section.kind == SectionKind::OpenTabs {
            group
                .id("tabs")
                .role(Role::TabList)
                .aria_label(section.heading)
                .test_support()
                .into_any_element()
        } else {
            group.into_any_element()
        }
    }

    /// The footer actions, pushed to the bottom and icon-only in the rail.
    #[expect(dead_code, reason = "the Appearance row is hidden in alpha")]
    fn footer_view(&self, m: &ModeTokens, cx: &mut Context<Self>) -> Div {
        let appearance = Entry {
            mark: Mark::Chrome(IconName::Palette),
            label: "Appearance".into(),
            hint: None,
        };
        div()
            .mt_auto()
            .flex()
            .flex_col()
            .gap_1()
            .child(Self::static_row(
                "footer-appearance".into(),
                appearance.label.clone(),
                self.row_view(&appearance, m, cx),
            ))
    }
}

/// The primary action; Git compare is not offered in this alpha.
#[rustfmt::skip]
fn primary() -> Vec<Entry> {
    vec![
        Entry { mark: Mark::Chrome(IconName::Plus), label: "New comparison".into(), hint: Some("Ctrl+T") },
    ]
}

/// One stored session as an entry: a named session keeps its name, a recent one its two sides.
fn entry_of(row: &StartRow, named: bool) -> Entry {
    let mark = if named {
        Mark::Chrome(IconName::Star)
    } else {
        Mark::File(icon_id(&row.left, true, false))
    };
    let label = match &row.name {
        Some(name) if named => name.clone().into(),
        _ => format!("{} ↔ {}", row.left, row.right).into(),
    };
    Entry {
        mark,
        label,
        hint: None,
    }
}

/// Draws one mark at the 1x bucket; a missing raster leaves the row without a glyph.
fn mark_view(mark: Mark, color: Hsla, cx: &mut App) -> AnyElement {
    match mark {
        Mark::Chrome(name) => icon(name, px(MARK_PX), color).into_any_element(),
        Mark::File(material) => match IconCache::image(material, MARK, cx) {
            Some(image) => img(image).size(px(MARK_PX)).flex_none().into_any_element(),
            None => div().size(px(MARK_PX)).flex_none().into_any_element(),
        },
    }
}

impl Render for Sidebar {
    /// The sidebar on `--bg2` with a `--sep` hairline, its text kept 16 px off the sides and
    /// 12 px off the top. The width lerps from the previous one, or snaps under reduced motion.
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_rail_focus(window);
        let side = *SidewiseTheme::global(cx);
        let m = if side.mode.is_dark() {
            side.spec.dark
        } else {
            side.spec.light
        };
        let (from, to) = (self.from, self.width());
        let body = div()
            .key_context(CONTEXT)
            .track_focus(&self.handle)
            .tab_group()
            .on_action(cx.listener(|this, _: &ToggleRail, _, cx| {
                cx.stop_propagation();
                this.toggle_rail(cx);
            }))
            .on_action(cx.listener(|this, _: &NextRow, window, cx| {
                this.focus_row(actions::RowDirection::Next, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &PreviousRow, window, cx| {
                this.focus_row(actions::RowDirection::Previous, window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(Self::on_dismiss_tip))
            .on_action(cx.listener(Self::on_show_tab_menu))
            .flex()
            .flex_col()
            .flex_none()
            .h_full()
            .w(to)
            .overflow_hidden()
            .bg(m.surface2)
            .border_r_1()
            .border_color(m.sep)
            .px(DIMS.inset_x)
            .pt(DIMS.inset_y)
            .gap_3()
            .children(self.sections().iter().map(|s| self.group_view(s, &m, cx)))
            .children(self.rail_tip_overlay(&m, cx));
        match motion::anim("sidebar-width", side.motion.side_ms, cx) {
            Some(enter) => body
                .with_animation("sidebar-width", enter, move |el, delta| {
                    el.w(from + (to - from) * delta)
                })
                .into_any_element(),
            None => body.into_any_element(),
        }
    }
}

#[cfg(test)]
#[path = "sidebar_tests.rs"]
mod sidebar_tests;

#[cfg(test)]
#[path = "sidebar_tip_tests.rs"]
mod sidebar_tip_tests;
