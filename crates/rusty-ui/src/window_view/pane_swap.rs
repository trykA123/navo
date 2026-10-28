use std::collections::HashMap;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{DispatchPhase, KeystrokeEvent, Subscription, canvas};
use rusty_core::{Command, PaneId};

use super::FerriteWindow;
use crate::prelude::*;
use crate::tokens;

const DRAG_THRESHOLD: Pixels = px(6.);

struct PaneDrag {
    pane: PaneId,
    origin: Point<Pixels>,
    position: Point<Pixels>,
    started: bool,
    _escape: Subscription,
}

#[derive(Default)]
pub(super) struct PaneSwap {
    bounds: HashMap<PaneId, Bounds<Pixels>>,
    drag: Option<PaneDrag>,
}

impl PaneSwap {
    #[cfg(test)]
    pub(super) fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub(super) fn contains(&self, pane: PaneId, position: Point<Pixels>) -> bool {
        self.bounds
            .get(&pane)
            .is_some_and(|bounds| bounds.contains(&position))
    }
}

impl FerriteWindow {
    pub(super) fn pane_swap_header(
        pane: PaneId,
        (header, path): (Option<Div>, Option<AnyElement>),
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .flex_none()
            .child(Self::tab_bounds_reset(pane, cx))
            .children(header)
            .children(path)
            .capture_any_mouse_down(
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    let is_editing = this
                        .path_edit
                        .as_ref()
                        .is_some_and(|edit| edit.pane == pane);
                    if !event.modifiers.shift {
                        this.press_tab((pane, event), window, cx);
                        return;
                    }
                    if event.button != MouseButton::Left
                        || is_editing
                        || this.drops_blocked()
                        || cx.has_active_drag()
                    {
                        return;
                    }
                    this.pane_swap.drag = Some(PaneDrag {
                        pane,
                        origin: event.position,
                        position: event.position,
                        started: false,
                        _escape: Self::pane_swap_escape(window, cx),
                    });
                }),
            )
    }

    fn pane_swap_escape(window: &Window, cx: &mut Context<Self>) -> Subscription {
        let own = window.window_handle();
        let escape = cx.listener(move |this, event: &KeystrokeEvent, window, cx| {
            if event.keystroke.key == "escape"
                && window.window_handle() == own
                && this.pane_swap.drag.take().is_some()
            {
                cx.stop_propagation();
                cx.notify();
            }
        });
        cx.intercept_keystrokes(escape)
    }

    fn pane_under_pointer(&self, position: Point<Pixels>, window: &Window) -> Option<PaneId> {
        self.shown_panes(window)
            .into_iter()
            .find(|pane| self.pane_swap.contains(*pane, position))
    }

    pub(super) fn pane_swap_target(
        &self,
        pane: PaneId,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = tokens::current(cx);
        let is_over = self.pane_swap.drag.as_ref().is_some_and(|drag| {
            drag.started && self.pane_under_pointer(drag.position, window) == Some(pane)
        }) || self.tab_drag_over_body(pane, window);
        let measure = cx.listener(move |this, bounds: &Bounds<Pixels>, _, _| {
            this.pane_swap.bounds.insert(pane, *bounds);
        });
        let outline = is_over.then(|| {
            div()
                .absolute()
                .inset_0()
                .rounded(t.radii.card)
                .border_2()
                .border_color(t.colors.accent)
                .id(("pane-swap-outline", pane.get()))
                .test_support()
        });
        div()
            .absolute()
            .inset_0()
            .children(outline)
            .child(
                canvas(
                    move |bounds, window, cx| measure(&bounds, window, cx),
                    |_, (), _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .into_any_element()
    }

    pub(super) fn pane_swap_layer(&self, root: Div, cx: &mut Context<Self>) -> Div {
        root.child(self.pane_swap_events(cx))
            .children(self.pane_swap_chip(cx))
    }

    fn pane_swap_events(&self, cx: &mut Context<Self>) -> AnyElement {
        let moved = cx.listener(Self::move_pane_drag);
        let released = cx.listener(Self::release_pane_drag);
        let moved = move |event: &MouseMoveEvent, phase, window: &mut Window, cx: &mut App| {
            if phase == DispatchPhase::Capture {
                moved(event, window, cx);
            }
        };
        let released = move |event: &MouseUpEvent, phase, window: &mut Window, cx: &mut App| {
            if phase == DispatchPhase::Capture {
                released(event, window, cx);
            }
        };
        canvas(
            |_, _, _| (),
            move |_, (), window, _| {
                window.on_mouse_event(moved);
                window.on_mouse_event(released);
            },
        )
        .absolute()
        .size_full()
        .into_any_element()
    }

    fn move_pane_drag(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = &mut self.pane_swap.drag else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.pane_swap.drag = None;
        } else {
            drag.position = event.position;
            let offset = drag.position - drag.origin;
            drag.started |= offset.magnitude() >= f64::from(DRAG_THRESHOLD);
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn release_pane_drag(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        let Some(drag) = self.pane_swap.drag.take() else {
            return;
        };
        if !drag.started {
            return;
        }
        if !self.drops_blocked()
            && let Some(target) = self.pane_under_pointer(event.position, window)
            && target != drag.pane
        {
            let numbered = self.tiles_layout().numbered();
            let a = numbered.iter().position(|pane| *pane == drag.pane);
            let b = numbered.iter().position(|pane| *pane == target);
            if let (Some(a), Some(b)) = (a, b) {
                self.apply(&Command::LayoutSwap { a: a + 1, b: b + 1 }, window, cx);
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn pane_swap_chip(&self, cx: &App) -> Option<AnyElement> {
        let drag = self.pane_swap.drag.as_ref().filter(|drag| drag.started)?;
        let number = self
            .tiles_layout()
            .numbered()
            .iter()
            .position(|pane| *pane == drag.pane)?
            + 1;
        let t = tokens::current(cx);
        Some(
            div()
                .id("pane-swap-chip")
                .test_support()
                .absolute()
                .left(drag.position.x + t.spacing.md)
                .top(drag.position.y + t.spacing.md)
                .flex()
                .items_center()
                .justify_center()
                .min_w(t.spacing.xxl)
                .h(t.spacing.xxl)
                .px(t.spacing.sm)
                .rounded(t.radii.md)
                .bg(t.colors.accent)
                .text_color(t.colors.background)
                .font_family(t.typography.numbers_font.clone())
                .text_size(t.typography.small)
                .line_height(t.spacing.icon)
                .font_weight(FontWeight::SEMIBOLD)
                .shadow(t.shadows.menu.clone())
                .child(number.to_string())
                .into_any_element(),
        )
    }
}
