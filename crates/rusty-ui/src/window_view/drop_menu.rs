use gpui_kit::{Bounds, Point, Subscription};
use rusty_core::shell_menu::{Group, ItemId, MenuItem, ShellMenu};
use rusty_core::{DropOp, Location};
use rusty_l10n::tr;

use super::FerriteWindow;
use super::drop::{DragSession, op_for};
use super::drop_external::NativeDrop;
use crate::context_menu::{ContextMenu, ContextMenuEvent, ContextMenuInput};
use crate::prelude::*;
use crate::status_bar::StatusNotice;

pub(super) struct DropMenu {
    view: Entity<ContextMenu>,
    sources: Vec<Location>,
    target: Location,
    native: NativeDrop,
    _events: Subscription,
}

impl FerriteWindow {
    pub(super) fn drop_notice(&mut self, text: impl Into<SharedString>) {
        self.show_notice(StatusNotice {
            text: text.into(),
            is_error: false,
        });
    }

    pub(super) fn finish_drop(
        &mut self,
        (session, target): (DragSession, Location),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_drop_effect(None, window);
        if self.drops_blocked() {
            self.drop_notice(tr("drag-refused-modal"));
            return;
        }
        if let Some(error) = session.error {
            self.drop_notice(error);
            return;
        }
        let keys = session.native.as_ref().map_or_else(
            || super::drop::keys_of(&window.modifiers()),
            |native| native.keys,
        );
        let op = op_for(&session.carried.sources, &target, keys);
        if let Some(refused) = crate::browser::drop_refusal(&target, &session.carried.sources)
            .and_then(super::drop::refusal_of)
        {
            self.drop_notice(refused.text());
            return;
        }
        if let Some(native) = session.native {
            if native.right {
                self.open_drop_menu((session.carried.sources, target, native), window, cx);
                return;
            }
            if !native.allows(op) {
                self.drop_notice(tr("drag-refused-effect"));
                return;
            }
        }
        self.set_drop_effect(Some(op), window);
        self.run_drop(session.carried.sources, target, op);
    }

    fn open_drop_menu(
        &mut self,
        (sources, target, native): (Vec<Location>, Location, NativeDrop),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let items = drop_items(|op| native.allows(op));
        let pane = self.browser.state().focused_pane();
        let input = ContextMenuInput {
            menu: ShellMenu { items },
            context: target.as_str().to_owned().into(),
            anchor: window.mouse_position(),
            bounds: Bounds::new(Point::default(), window.viewport_size()),
            restore: Some(self.handle(pane, cx)),
        };
        let view = cx.new(|cx| {
            let mut menu = ContextMenu::new(input, window, cx);
            menu.hide_classic(cx);
            menu.name_ferrite_group(&tr("drag-drop-title"), cx);
            menu
        });
        let events = cx.subscribe_in(&view, window, |view, _, event, _, cx| {
            view.drop_menu_event(event);
            cx.notify();
        });
        self.drag.menu = Some(DropMenu {
            view,
            sources,
            target,
            native,
            _events: events,
        });
        cx.notify();
    }

    fn drop_menu_event(&mut self, event: &ContextMenuEvent) {
        let op = match event {
            ContextMenuEvent::Chosen(ItemId::Action("drop.copy")) => Some(DropOp::Copy),
            ContextMenuEvent::Chosen(ItemId::Action("drop.move")) => Some(DropOp::Move),
            ContextMenuEvent::Chosen(ItemId::Action("drop.link")) => Some(DropOp::Link),
            ContextMenuEvent::Dismissed
            | ContextMenuEvent::Chosen(_)
            | ContextMenuEvent::ShowClassic => None,
            ContextMenuEvent::OpenSubmenu(_) => return,
        };
        let Some(menu) = self.drag.menu.take() else {
            return;
        };
        if let Some(op) = op.filter(|op| menu.native.allows(*op))
            && !self.drops_blocked()
        {
            self.run_drop(menu.sources, menu.target, op);
        }
    }

    pub(super) fn drop_menu_view(&self) -> Option<Entity<ContextMenu>> {
        self.drag.menu.as_ref().map(|menu| menu.view.clone())
    }
}

#[must_use]
pub fn drop_items(allows: impl Fn(DropOp) -> bool) -> Vec<MenuItem> {
    [
        ("drop.copy", "drag-drop-copy", Some(DropOp::Copy)),
        ("drop.move", "drag-drop-move", Some(DropOp::Move)),
        ("drop.link", "drag-drop-link", Some(DropOp::Link)),
        ("drop.cancel", "drag-drop-cancel", None),
    ]
    .into_iter()
    .map(|(id, label, op)| {
        let mut item = MenuItem::new(ItemId::Action(id), &tr(label), Group::Ferrite);
        item.enabled = op.is_none_or(&allows);
        item
    })
    .collect()
}
