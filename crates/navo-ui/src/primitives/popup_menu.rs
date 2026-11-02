use std::rc::Rc;

use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{
    App, ClickEvent, Div, ElementId, Entity, FocusHandle, Hsla, InteractiveElement, MouseButton,
    Pixels, Point, Stateful, Styled, Window,
};

use crate::file_list::row::inset_ring;

const CONTEXT: &str = "PopupMenu";

gpui_kit::actions!(popup_menu, [MenuUp, MenuDown, MenuChoose, MenuClose]);

pub(crate) fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "popup_menu");
}

pub(crate) type ChooseHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;

pub(crate) struct PopupMenu {
    pub(crate) is_open: bool,
    pub(crate) selected: usize,
    has_items: bool,
    focus: FocusHandle,
    anchor: FocusHandle,
    restore: Option<FocusHandle>,
    dismissed_at: Option<Point<Pixels>>,
}

impl PopupMenu {
    pub(crate) fn new(cx: &mut App) -> Self {
        Self {
            is_open: false,
            selected: 0,
            has_items: false,
            focus: cx.focus_handle(),
            anchor: cx.focus_handle().tab_stop(true),
            restore: None,
            dismissed_at: None,
        }
    }

    pub(crate) fn holds(&self, focused: &FocusHandle) -> bool {
        *focused == self.focus || *focused == self.anchor
    }

    pub(crate) fn has_items(&self) -> bool {
        self.has_items
    }

    pub(crate) fn anchor(&self) -> &FocusHandle {
        &self.anchor
    }
}

pub(crate) fn popup_state(
    key: impl Into<ElementId>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<PopupMenu> {
    window.use_keyed_state(key, cx, |_, cx| PopupMenu::new(cx))
}

pub(crate) fn note_items(state: &Entity<PopupMenu>, has_items: bool, cx: &mut App) {
    if state.read(cx).has_items != has_items {
        state.update(cx, |menu, _| menu.has_items = has_items);
    }
}

pub(crate) fn wire_anchor<E: InteractiveElement>(
    anchor: E,
    (state, ring): (&Entity<PopupMenu>, Hsla),
    cx: &App,
) -> E {
    anchor
        .track_focus(&state.read(cx).anchor)
        .focus_visible(move |style| style.shadow(inset_ring(ring)))
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
}

fn pressed_at(event: &ClickEvent) -> Option<Point<Pixels>> {
    match event {
        ClickEvent::Mouse(click) => Some(click.down.position),
        _ => None,
    }
}

pub(crate) fn toggle(
    state: &Entity<PopupMenu>,
    event: &ClickEvent,
    window: &mut Window,
    cx: &mut App,
) {
    let pressed = pressed_at(event);
    let (dismissed_now, is_open) = state.update(cx, |menu, _| {
        let dismissed = menu.dismissed_at.take();
        (pressed.is_some() && dismissed == pressed, menu.is_open)
    });
    match (dismissed_now, is_open) {
        (true, _) => {}
        (false, true) => close(state, window, cx),
        (false, false) => open(state, window, cx),
    }
}

pub(crate) fn open(state: &Entity<PopupMenu>, window: &mut Window, cx: &mut App) {
    let restore = window.focused(cx);
    let focus = state.update(cx, |menu, cx| {
        menu.is_open = true;
        menu.selected = 0;
        menu.restore = restore;
        cx.notify();
        menu.focus.clone()
    });
    window.focus(&focus, cx);
}

pub(crate) fn close(state: &Entity<PopupMenu>, window: &mut Window, cx: &mut App) {
    if let Some(Some(restore)) = dismiss(state, cx) {
        window.focus(&restore, cx);
    }
}

pub(crate) fn dismiss(state: &Entity<PopupMenu>, cx: &mut App) -> Option<Option<FocusHandle>> {
    if !state.read(cx).is_open {
        return None;
    }
    Some(state.update(cx, |menu, cx| {
        menu.is_open = false;
        cx.notify();
        menu.restore.take()
    }))
}

pub(crate) fn choose(
    state: &Entity<PopupMenu>,
    (index, on_choose): (usize, &ChooseHandler),
    window: &mut Window,
    cx: &mut App,
) {
    close(state, window, cx);
    on_choose(index, window, cx);
}

fn step(state: &Entity<PopupMenu>, forward: bool, count: usize, cx: &mut App) {
    state.update(cx, |menu, cx| {
        menu.selected = match forward {
            true => (menu.selected + 1).min(count.saturating_sub(1)),
            false => menu.selected.saturating_sub(1),
        };
        cx.notify();
    });
}

pub(crate) struct Keys {
    pub(crate) state: Entity<PopupMenu>,
    pub(crate) count: usize,
    pub(crate) on_choose: ChooseHandler,
}

pub(crate) fn wire_keys(
    panel: Stateful<Div>,
    keys: Keys,
    cx: &App,
) -> ObservedElement<Stateful<Div>> {
    let Keys {
        state,
        count,
        on_choose,
    } = keys;
    let focus = state.read(cx).focus.clone();
    let (up, down, enter, escape, out) = (
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state,
    );
    panel
        .key_context(CONTEXT)
        .test_support()
        .track_focus(&focus)
        .on_action(move |_: &MenuUp, _, cx| step(&up, false, count, cx))
        .on_action(move |_: &MenuDown, _, cx| step(&down, true, count, cx))
        .on_action(move |_: &MenuChoose, window, cx| {
            let index = enter.read(cx).selected;
            choose(&enter, (index, &on_choose), window, cx);
        })
        .on_action(move |_: &MenuClose, window, cx| close(&escape, window, cx))
        .on_mouse_down_out(move |event, window, cx| {
            out.update(cx, |menu, _| menu.dismissed_at = Some(event.position));
            close(&out, window, cx);
        })
}
