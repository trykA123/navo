use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

use super::{FerriteWindow, StartOptions, fake, open_options, press, settle, two_panes};
use crate::app_menu::{Identity, Install};
use crate::browser::Operations;
use crate::tokens::{self, Mode, ThemeChoice};
use crate::window_view::AppMenuHooks;

struct Rig {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
    saved: Rc<RefCell<Vec<ThemeChoice>>>,
    sessions: Rc<Cell<usize>>,
}

fn rig(cx: &mut TestAppContext, theme: ThemeChoice) -> Rig {
    let fs = fake();
    let state = two_panes(&fs);
    let saved: Rc<RefCell<Vec<ThemeChoice>>> = Rc::default();
    let sessions: Rc<Cell<usize>> = Rc::default();
    let (log, count) = (saved.clone(), sessions.clone());
    let options = StartOptions {
        state,
        provider: fs,
        on_save: Rc::new(move |_| count.set(count.get() + 1)),
        mode: Mode::Light,
        operations: Operations::unlogged(),
        launcher: Arc::new(|_| {}),
        editor: super::super::EditorHooks::none(),
        files: super::super::FileHooks::none(),
        kinds: super::super::Kinds::builtin(),
        drives: super::super::DriveHooks::none(),
        app_menu: AppMenuHooks {
            identity: Identity {
                version: "0.0.1".into(),
                install: Install::Installed,
                folder: r"C:\Ferrite".into(),
            },
            theme,
            save_theme: Rc::new(move |choice| log.borrow_mut().push(choice)),
        },
        view: None,
        listing_cache: rusty_cache::ListingCache::default(),
        cache_keys: rusty_cache::CacheKeys::default(),
    };
    let (window, view) = open_options(cx, options);
    Rig {
        window,
        view,
        saved,
        sessions,
    }
}

impl Rig {
    fn is_open(&self, cx: &mut TestAppContext) -> bool {
        self.view.read_with(cx, |view, _| view.app_menu.is_some())
    }

    fn press(&self, cx: &mut TestAppContext, key: &str) {
        press(cx, self.window, &self.view, key);
    }

    fn click(&self, cx: &mut TestAppContext, id: impl Into<gpui_kit::ElementId>) {
        let id = id.into();
        cx.update_window(self.window, |_, window, cx| window.click(id, cx))
            .expect("window");
        settle(cx, &self.view);
    }

    fn pane_has_focus(&self, cx: &mut TestAppContext) -> bool {
        let view = self.view.clone();
        cx.update_window(self.window, |_, window, cx| {
            view.update(cx, |view, cx| {
                let pane = view.browser.state().focused_pane();
                view.handle(pane, cx).is_focused(window)
            })
        })
        .expect("window")
    }

    fn mode(&self, cx: &mut TestAppContext) -> Mode {
        cx.read(|cx| tokens::current(cx).mode)
    }

    fn button_expanded(&self, cx: &mut TestAppContext) -> Option<bool> {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.find("title-bar-title").expanded()
        })
        .expect("window")
    }
}

#[gpui_kit::test]
fn the_app_button_opens_the_menu_and_takes_focus(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    assert_eq!(rig.button_expanded(cx), Some(false));
    rig.click(cx, "title-bar-title");
    assert!(rig.is_open(cx));
    assert_eq!(rig.button_expanded(cx), Some(true));
    assert!(!rig.pane_has_focus(cx));
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let identity = window.find("app-menu-identity");
        assert_eq!(
            identity.label(),
            Some(r"Ferrite 0.0.1, Installed · C:\Ferrite")
        );
    })
    .expect("window");
}

#[gpui_kit::test]
fn alt_space_opens_and_closes_the_menu(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "alt-space");
    assert!(rig.is_open(cx));
    rig.press(cx, "alt-space");
    assert!(!rig.is_open(cx));
    assert!(rig.pane_has_focus(cx));
}

#[gpui_kit::test]
fn escape_closes_the_menu_and_gives_focus_back_to_the_pane(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    assert!(rig.pane_has_focus(cx));
    rig.press(cx, "alt-space");
    assert!(rig.is_open(cx));
    rig.press(cx, "down");
    rig.press(cx, "escape");
    assert!(!rig.is_open(cx));
    assert!(rig.pane_has_focus(cx));
}

#[gpui_kit::test]
fn a_click_outside_closes_the_menu_and_restores_focus(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.click(cx, "title-bar-title");
    rig.click(cx, "app-menu-backdrop");
    assert!(!rig.is_open(cx));
    assert!(rig.pane_has_focus(cx));
}

#[gpui_kit::test]
fn the_theme_tile_applies_and_saves_each_choice(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    assert_eq!(rig.mode(cx), Mode::Light);
    rig.press(cx, "alt-space");
    rig.press(cx, "enter");
    assert_eq!(rig.mode(cx), Mode::Light);
    rig.press(cx, "enter");
    assert_eq!(rig.mode(cx), Mode::Dark);
    rig.click(cx, ("app-menu-tile", 0usize));
    assert_eq!(rig.mode(cx), Mode::Light);
    assert_eq!(
        *rig.saved.borrow(),
        [ThemeChoice::Light, ThemeChoice::Dark, ThemeChoice::System]
    );
    assert!(rig.is_open(cx), "a tile keeps the menu open");
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find(("app-menu-tile", 0usize)).value(),
            Some("System")
        );
    })
    .expect("window");
}

#[gpui_kit::test]
fn the_saved_theme_applies_at_start(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::Dark);
    assert_eq!(rig.mode(cx), Mode::Dark);
    assert!(rig.saved.borrow().is_empty());
}

#[gpui_kit::test]
fn unavailable_rows_neither_act_nor_close_the_menu(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "alt-space");
    for key in ["c", "n", "w", "h", "a"] {
        rig.press(cx, key);
        rig.press(cx, "enter");
        assert!(rig.is_open(cx), "{key} closed the menu");
    }
    rig.click(cx, ("app-menu-row", 1usize));
    assert!(rig.is_open(cx));
    assert!(rig.saved.borrow().is_empty());
    assert_eq!(rig.sessions.get(), 0);
}

#[gpui_kit::test]
fn quit_saves_the_session(cx: &mut TestAppContext) {
    let rig = rig(cx, ThemeChoice::System);
    rig.press(cx, "alt-space");
    rig.press(cx, "q");
    rig.press(cx, "enter");
    assert_eq!(rig.sessions.get(), 1);
}
