//! One shared context-menu host (NC136): views show a `navo_ui` `ContextMenu` from action lines,
//! keep it in a `MenuSlot`, and get the chosen action dispatched on their own focus.

use std::rc::Rc;

use gpui_kit::{
    Action, App, AppContext as _, Bounds, Context, Entity, FocusHandle, KeyBinding,
    KeyBindingContextPredicate, Pixels, Point, SharedString, Subscription, Window,
};
use sidewise_core::shell_menu::ItemId;
use sidewise_kit::context_menu::{ContextMenu, ContextMenuEvent, ContextMenuInput};
use sidewise_ui::window_view::viewer::{MenuEntry, viewer_menu};

/// One line of a context menu: an action to run, or a separator.
pub enum MenuLine {
    /// A runnable line; a disabled one is dimmed and cannot be chosen.
    Run {
        label: SharedString,
        action: Box<dyn Action>,
        enabled: bool,
    },
    /// A separator between groups.
    Break,
}

/// The open menu of one host, with the subscription that watches it; `None` when closed.
pub type MenuSlot = Option<(Entity<ContextMenu>, Subscription)>;

/// Where and for whom a menu opens: the anchor, the focus that gets the chosen action, a title.
/// A closed menu gives the focus back to whatever held it when it opened, unless a chosen action
/// moved it.
pub struct MenuAt {
    pub anchor: Point<Pixels>,
    pub focus: FocusHandle,
    pub title: &'static str,
}

/// Reaches the menu slot inside a host view.
pub type SlotOf<V> = fn(&mut V) -> &mut MenuSlot;

/// Opens `lines` as a context menu at `at` in the slot of `host`; the shortcut of each line is
/// read from the live keymap in the host's focus. The window draws it with `menu_view`.
/// `host` is the view's own `&mut self` (a context cannot lend it back while it is borrowed).
pub fn show_menu<V: 'static>(
    host: &mut V,
    (lines, at, slot): (&[MenuLine], MenuAt, SlotOf<V>),
    window: &mut Window,
    cx: &mut Context<V>,
) {
    let back = window.focused(cx).unwrap_or_else(|| at.focus.clone());
    let entries = to_entries(lines);
    let mut menu = viewer_menu(&entries, &at.focus, window);
    for (item, line) in menu.items.iter_mut().zip(lines) {
        if let MenuLine::Run { enabled, .. } = line {
            item.enabled = *enabled;
        }
    }
    let input = ContextMenuInput {
        menu,
        context: at.title.into(),
        anchor: at.anchor,
        bounds: Bounds::new(Point::default(), window.viewport_size()),
        restore: Some(back.clone()),
    };
    let menu = cx.new(|cx| {
        let mut menu = ContextMenu::new(input, window, cx);
        menu.hide_classic(cx);
        menu
    });
    let focus = [at.focus, back];
    let events = cx.subscribe_in(&menu, window, move |this, _, event, window, cx| {
        on_event(this, (event, &focus, slot), window, cx);
    });
    *slot(host) = Some((menu, events));
    cx.notify();
}

/// The open menu of a slot, for the view to draw as a child; `None` when closed.
pub fn menu_view(slot: &MenuSlot) -> Option<Entity<ContextMenu>> {
    slot.as_ref().map(|(menu, _)| menu.clone())
}

/// Maps lines to viewer entries (actions are cloned; enabled flags are set afterwards).
fn to_entries(lines: &[MenuLine]) -> Vec<MenuEntry> {
    lines
        .iter()
        .map(|line| match line {
            MenuLine::Run { label, action, .. } => {
                MenuEntry::Run(label.clone(), action.boxed_clone(), false)
            }
            MenuLine::Break => MenuEntry::Break,
        })
        .collect()
}

/// Reacts to the menu: ignores a submenu opening; else closes it, then runs a chosen action on
/// `focus[0]` after `window.defer`. The focus then goes back to `focus[1]` (held before opening),
/// unless the action moved it elsewhere.
fn on_event<V: 'static>(
    host: &mut V,
    (event, [focus, back], slot): (&ContextMenuEvent, &[FocusHandle; 2], SlotOf<V>),
    window: &mut Window,
    cx: &mut Context<V>,
) {
    if matches!(event, ContextMenuEvent::OpenSubmenu(_)) {
        return;
    }
    *slot(host) = None;
    cx.notify();
    let ContextMenuEvent::Chosen(ItemId::Action(name)) = event else {
        return window.focus(back, cx);
    };
    let Ok(action) = cx.build_action(name, None) else {
        return window.focus(back, cx);
    };
    let (focus, back) = (focus.clone(), back.clone());
    window.defer(cx, move |window, cx| {
        window.focus(&focus, cx);
        focus.dispatch_action(action.as_ref(), window, cx);
        // A command that moved the focus (Open) keeps it; else the opener gets it back.
        if focus != back && focus.is_focused(window) {
            window.focus(&back, cx);
        }
    });
}

/// Keys of the open menu: (key, action name) pairs, bound in both menu contexts.
const MENU_KEYS: [(&str, &str); 12] = [
    ("up", "context_menu::MenuUp"),
    ("down", "context_menu::MenuDown"),
    ("home", "context_menu::MenuHome"),
    ("end", "context_menu::MenuEnd"),
    ("pageup", "context_menu::MenuPageUp"),
    ("pagedown", "context_menu::MenuPageDown"),
    ("enter", "context_menu::MenuEnter"),
    ("right", "context_menu::MenuRight"),
    ("left", "context_menu::MenuLeft"),
    ("backspace", "context_menu::MenuLeft"),
    ("escape", "context_menu::MenuEscape"),
    ("shift-f10", "context_menu::MenuClassic"),
];

/// Binds the menu keys by action name in `ContextMenu` and `ContextMenu > Input`; returns the
/// number bound (24). A name that does not build is skipped.
pub fn bind_menu_keys(cx: &mut App) -> usize {
    let mut all = Vec::new();
    for context in ["ContextMenu", "ContextMenu > Input"] {
        let Ok(predicate) = KeyBindingContextPredicate::parse(context) else {
            continue;
        };
        let predicate = Rc::new(predicate);
        for (key, name) in MENU_KEYS {
            let Ok(action) = cx.build_action(name, None) else {
                continue;
            };
            if let Ok(bind) =
                KeyBinding::load(key, action, Some(predicate.clone()), false, None, cx)
            {
                all.push(bind);
            }
        }
    }
    let count = all.len();
    cx.bind_keys(all);
    count
}
