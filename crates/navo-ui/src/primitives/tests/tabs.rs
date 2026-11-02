use gpui_kit::AppContext as _;
use gpui_kit::assets::IconName;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    Context, IntoElement, ParentElement, Render, Role, Styled, TestAppContext, Window, div, px,
};

use super::open;
use crate::primitives::{Tab, Tabs};

struct Picker {
    selected: usize,
}

impl Render for Picker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        Tabs::new(
            "tabs",
            [
                Tab::new("commander"),
                Tab::new("core").icon(IconName::Folder),
                Tab::new("Downloads"),
            ],
        )
        .selected(self.selected)
        .on_select(cx.listener(|this, index: &usize, _, cx| {
            this.selected = *index;
            cx.notify();
        }))
    }
}

#[gpui_kit::test]
fn tabs_mark_the_selected_tab(cx: &mut TestAppContext) {
    let (window, _) = open(cx, |_, _| Picker { selected: 1 });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("tabs").role(), Some(Role::TabList));
        let tabs = window.within("tabs");
        assert_eq!(tabs.find(0usize).role(), Some(Role::Tab));
        assert_eq!(tabs.find(0usize).selected(), Some(false));
        assert_eq!(tabs.find(1usize).selected(), Some(true));
        assert_eq!(tabs.find(1usize).label(), Some("core"));
    })
    .expect("window");
}

#[gpui_kit::test]
fn clicking_a_tab_makes_it_the_active_tab(cx: &mut TestAppContext) {
    let (window, view) = open(cx, |_, _| Picker { selected: 0 });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within("tabs").click(2usize, cx);
        let tabs = window.within("tabs");
        assert_eq!(tabs.find(0usize).selected(), Some(false));
        assert_eq!(tabs.find(2usize).selected(), Some(true));
    })
    .expect("window");
    assert_eq!(view.read_with(cx, |view, _| view.selected), 2);
}

const TITLES: [&str; 10] = [
    "commander",
    "core",
    "Downloads",
    "measurements",
    "calibration",
    "thermal-soak",
    "cold-start",
    "endurance",
    "exports",
    "archive",
];

struct Narrow {
    width: f32,
    count: usize,
    selected: usize,
}

impl Render for Narrow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = TITLES[..self.count].iter().map(|title| Tab::new(*title));
        div()
            .w(px(self.width))
            .child(
                Tabs::new("tabs", tabs)
                    .selected(self.selected)
                    .on_select(cx.listener(|this, index: &usize, _, cx| {
                        this.selected = *index;
                        cx.notify();
                    })),
            )
    }
}

fn narrow(width: f32, count: usize, selected: usize) -> Narrow {
    Narrow {
        width,
        count,
        selected,
    }
}

fn shown_tabs(window: &mut Window, count: usize) -> Vec<usize> {
    let tabs = window.within("tabs");
    (0..count)
        .filter(|index| tabs.try_find(*index).is_some())
        .collect()
}

fn tab_width(window: &mut Window, index: usize) -> f32 {
    f32::from(window.within("tabs").find(index).bounds().size.width)
}

#[gpui_kit::test]
fn tabs_shrink_before_any_collapses(cx: &mut TestAppContext) {
    let (wide, _) = open(cx, |_, _| narrow(800., 4, 0));
    let natural = cx
        .update_window(wide, |_, window, cx| {
            window.render_frame(cx);
            tab_width(window, 3)
        })
        .expect("window");
    let (tight, _) = open(cx, |_, _| narrow(400., 4, 0));
    cx.update_window(tight, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(shown_tabs(window, 4), [0, 1, 2, 3]);
        assert!(window.within("tabs").try_find("tab-more").is_none());
        assert!(tab_width(window, 3) < natural);
        assert!(tab_width(window, 3) >= 88.);
    })
    .expect("window");
}

#[gpui_kit::test]
fn the_extra_tabs_collapse_into_a_count(cx: &mut TestAppContext) {
    let (window, _) = open(cx, |_, _| narrow(400., 10, 0));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let shown = shown_tabs(window, 10);
        assert!(!shown.is_empty() && shown.len() < 10);
        let more = window.within("tabs").find("tab-more");
        let hidden = 10 - shown.len();
        assert_eq!(more.label(), Some(format!("{hidden} more tabs").as_str()));
        let row = window.within("tabs").find("tab-more").bounds();
        let tabs = window.find("tabs").bounds();
        assert!(row.right() <= tabs.right());
    })
    .expect("window");
}

#[gpui_kit::test]
fn the_active_tab_stays_visible_beyond_the_fold(cx: &mut TestAppContext) {
    let (window, _) = open(cx, |_, _| narrow(400., 10, 9));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let shown = shown_tabs(window, 10);
        assert_eq!(shown.last(), Some(&9));
        assert_eq!(window.within("tabs").find(9usize).selected(), Some(true));
        assert!(window.within("tabs").try_find("tab-more").is_some());
    })
    .expect("window");
}

#[gpui_kit::test]
fn choosing_a_hidden_tab_activates_and_shows_it(cx: &mut TestAppContext) {
    let (window, view) = open(cx, |_, _| narrow(400., 10, 0));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .within("tabs")
                .try_find(("tab-more-row", 8usize))
                .is_none()
        );
        window.within("tabs").click("tab-more", cx);
        window.render_frame(cx);
        assert_eq!(
            window.within("tabs").find("tab-more").expanded(),
            Some(true)
        );
        window.within("tabs").click(("tab-more-row", 8usize), cx);
        window.render_frame(cx);
        assert_eq!(window.within("tabs").find(8usize).selected(), Some(true));
        assert!(
            window
                .within("tabs")
                .try_find(("tab-more-row", 8usize))
                .is_none()
        );
    })
    .expect("window");
    assert_eq!(view.read_with(cx, |view, _| view.selected), 8);
}

struct PinnedTabs {
    closed: bool,
}

impl Render for PinnedTabs {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        Tabs::new("pinned-tabs", [Tab::new("Renamed folder").pinned(true)]).on_close(cx.listener(
            |this, _: &usize, _, cx| {
                this.closed = true;
                cx.notify();
            },
        ))
    }
}

#[gpui_kit::test]
fn pinned_tab_shows_label_without_close_and_ignores_middle_click(cx: &mut TestAppContext) {
    use gpui_kit::{InputEvent as _, Modifiers, MouseButton, MouseDownEvent};
    let (window, view) = open(cx, |_, _| PinnedTabs { closed: false });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let tabs = window.within("pinned-tabs");
        assert_eq!(tabs.find(0usize).label(), Some("Renamed folder"));
        assert!(tabs.try_find(("tab-close", 0usize)).is_none());
        let position = tabs.find(0usize).bounds().center();
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Middle,
                position,
                modifiers: Modifiers::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    assert!(!view.read_with(cx, |view, _| view.closed));
}

#[gpui_kit::test]
fn crowded_mixed_labels_keep_tab_minimum_widths(cx: &mut TestAppContext) {
    let (window, _) = open(cx, |_, _| narrow(400., 4, 0));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        for index in shown_tabs(window, 4) {
            assert!(tab_width(window, index) >= 88.);
        }
    })
    .unwrap();
}
