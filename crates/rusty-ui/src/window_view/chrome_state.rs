use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::{FocusHandle, Size, Subscription};
use rusty_core::PaneId;

use super::FerriteWindow;
use crate::prelude::*;
use crate::primitives::popup_menu::{self, PopupMenu};

#[derive(Clone)]
pub(super) struct PaneChrome {
    pub(super) tabs: Entity<PopupMenu>,
    pub(super) path: Entity<PopupMenu>,
    pub(super) list_width: Rc<Cell<Option<Pixels>>>,
    _menu_watches: Rc<[Subscription; 2]>,
}

#[derive(Default)]
pub(super) struct Chromes {
    panes: HashMap<PaneId, PaneChrome>,
    viewport: Size<Pixels>,
}

impl Chromes {
    pub(super) fn get(&self, pane: PaneId) -> Option<&PaneChrome> {
        self.panes.get(&pane)
    }

    fn menus(&self) -> impl Iterator<Item = (PaneId, &Entity<PopupMenu>)> {
        self.panes
            .iter()
            .flat_map(|(pane, chrome)| [(*pane, &chrome.tabs), (*pane, &chrome.path)])
    }
}

#[derive(Clone, Copy)]
pub(super) enum Hidden {
    Tabs,
    Path,
}

impl FerriteWindow {
    pub(super) fn ensure_chrome(
        &mut self,
        pane: PaneId,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> PaneChrome {
        let is_alone = self.shown_panes(window).len() == 1;
        let known = self
            .chrome
            .panes
            .values()
            .find_map(|chrome| chrome.list_width.get());
        let chrome = self.chrome.panes.entry(pane).or_insert_with(|| {
            let tabs = cx.new(|cx| PopupMenu::new(cx));
            let path = cx.new(|cx| PopupMenu::new(cx));
            let watch = |menu: &Entity<PopupMenu>, cx: &mut Context<Self>| {
                cx.observe(menu, |_, _, cx| cx.notify())
            };
            PaneChrome {
                _menu_watches: Rc::new([watch(&tabs, cx), watch(&path, cx)]),
                tabs,
                path,
                list_width: Rc::default(),
            }
        });
        if is_alone && chrome.list_width.get().is_none() {
            chrome.list_width.set(known);
        }
        chrome.clone()
    }

    pub(super) fn show_hidden(
        &mut self,
        which: Hidden,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pane = self.browser.state().focused_pane();
        let Some(chrome) = self.chrome.get(pane) else {
            return;
        };
        let state = match which {
            Hidden::Tabs => chrome.tabs.clone(),
            Hidden::Path => chrome.path.clone(),
        };
        if state.read(cx).has_items() {
            popup_menu::open(&state, window, cx);
        }
    }

    pub(super) fn viewport_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let viewport = window.viewport_size();
        if viewport != self.chrome.viewport {
            self.chrome.viewport = viewport;
            for chrome in self.chrome.panes.values() {
                chrome.list_width.set(None);
            }
            self.close_popups(window, cx);
        }
    }

    pub(super) fn close_popups(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focused = window.focused(cx);
        let menus: Vec<Entity<PopupMenu>> =
            self.chrome.menus().map(|(_, menu)| menu.clone()).collect();
        for menu in menus {
            let held = focused
                .as_ref()
                .is_some_and(|focused| menu.read(cx).holds(focused));
            if let Some(restore) = popup_menu::dismiss(&menu, cx).filter(|_| held) {
                self.refocus(restore, window, cx);
            }
        }
    }

    pub(super) fn popup_focus_lost(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(focused) = window.focused(cx) else {
            return;
        };
        let menu = self
            .chrome
            .menus()
            .find(|(_, menu)| menu.read(cx).holds(&focused))
            .map(|(_, menu)| menu.clone());
        let Some(menu) = menu else {
            return;
        };
        let restore = popup_menu::dismiss(&menu, cx).flatten();
        self.refocus(restore, window, cx);
    }

    fn refocus(
        &mut self,
        restore: Option<FocusHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match restore.filter(|handle| self.is_drawn(handle, window, cx)) {
            Some(handle) => window.focus(&handle, cx),
            None => self.focus_core_pane(window, cx),
        }
    }

    fn is_drawn(&self, handle: &FocusHandle, window: &Window, cx: &App) -> bool {
        let shown = self.shown_panes(window);
        if let Some(pane) = self
            .focus
            .iter()
            .find(|(_, h)| *h == handle)
            .map(|(p, _)| *p)
        {
            return shown.contains(&pane);
        }
        match self
            .chrome
            .menus()
            .find(|(_, menu)| menu.read(cx).anchor() == handle)
        {
            Some((pane, menu)) => shown.contains(&pane) && menu.read(cx).has_items(),
            None => true,
        }
    }
}
