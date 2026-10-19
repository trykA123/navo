mod entries;

use gpui_kit::{Bounds, MouseButton, MouseUpEvent, Stateful, point};
use rusty_core::shell_menu::ItemId;

pub(in crate::window_view::viewer) use entries::marking_entries;
pub use entries::viewer_menu;

use super::ShowMenu;
use crate::context_menu::{ContextMenu, ContextMenuEvent, ContextMenuInput};
use crate::prelude::*;
use crate::window_view::FerriteWindow;

const UNKNOWN_ACTION: &str = "This menu item has no command";

impl FerriteWindow {
    pub(in crate::window_view::viewer) fn menu_opening(
        root: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        root.on_mouse_up(
            MouseButton::Right,
            cx.listener(|view, event: &MouseUpEvent, window, cx| {
                view.open_viewer_menu(Some(event.position), window, cx);
            }),
        )
        .on_action(cx.listener(|view, _: &ShowMenu, window, cx| {
            view.open_viewer_menu(None, window, cx);
        }))
    }

    fn open_viewer_menu(
        &mut self,
        pointer: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(viewer) = self.viewer.as_mut() else {
            return;
        };
        let Some(body) = viewer.body.as_ref() else {
            return;
        };
        let entries = body.menu(pointer.is_some(), cx);
        if entries.is_empty() {
            return;
        }
        let focus = body.focus_handle(cx);
        let viewport = window.viewport_size();
        let anchor = pointer
            .or_else(|| body.menu_anchor(cx))
            .unwrap_or_else(|| point(viewport.width / 2., viewport.height / 2.));
        let input = ContextMenuInput {
            menu: entries::viewer_menu(&entries, &focus, window),
            context: viewer.title.clone(),
            anchor,
            bounds: Bounds::new(Point::default(), viewport),
            restore: Some(focus),
        };
        let menu = cx.new(|cx| {
            let mut menu = ContextMenu::new(input, window, cx);
            menu.hide_classic(cx);
            menu
        });
        let events = cx.subscribe_in(&menu, window, |this, _, event, window, cx| {
            this.viewer_menu_event(event, window, cx);
        });
        viewer.menu = Some((menu, events));
        viewer.is_view_menu = false;
        cx.notify();
    }

    fn viewer_menu_event(
        &mut self,
        event: &ContextMenuEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(viewer) = self.viewer.as_mut() else {
            return;
        };
        if matches!(event, ContextMenuEvent::OpenSubmenu(_)) {
            return;
        }
        viewer.menu = None;
        cx.notify();
        let Some(body) = viewer.body.as_ref() else {
            return;
        };
        let ContextMenuEvent::Chosen(ItemId::Action(name)) = event else {
            return body.menu_closed(cx);
        };
        let action = match cx.build_action(name, None) {
            Ok(action) => action,
            Err(error) => {
                debug_assert!(false, "menu item {name}: {error}");
                body.set_note(format!("{UNKNOWN_ACTION} ({name})").into(), cx);
                return body.menu_closed(cx);
            }
        };
        let focus = body.focus_handle(cx);
        let this = cx.weak_entity();
        window.defer(cx, move |window, cx| {
            focus.dispatch_action(action.as_ref(), window, cx);
            let _ = this.update(cx, |view, cx| view.viewer_menu_done(cx));
        });
    }

    fn viewer_menu_done(&self, cx: &mut App) {
        if let Some(body) = self.viewer.as_ref().and_then(|viewer| viewer.body.as_ref()) {
            body.menu_closed(cx);
        }
    }

    pub(in crate::window_view) fn viewer_menu_view(&self) -> Option<Entity<ContextMenu>> {
        let (menu, _) = self.viewer.as_ref()?.menu.as_ref()?;
        Some(menu.clone())
    }
}
