use gpui_kit::AppContext as _;
use gpui_kit::assets::IconName;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Context, IntoElement, Render, Role, TestAppContext, Window};

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
