use std::collections::{BTreeMap, HashMap};

use gpui_kit::base::TestSupportExt;
use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::{Animation, AnimationExt, DispatchPhase, KeystrokeEvent, Subscription, canvas};
use rusty_core::{PaneId, TabId, TabMove};

use super::FerriteWindow;
use crate::prelude::*;
use crate::primitives::{IconName, icon};
use crate::tokens::{self, Theme};

const DRAG_THRESHOLD: Pixels = px(6.);

struct Carry {
    pane: PaneId,
    index: usize,
    tab: TabId,
    title: SharedString,
    origin: Point<Pixels>,
    position: Point<Pixels>,
    started: bool,
    copy: bool,
    _escape: Subscription,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Spot {
    Row(PaneId, usize),
    Body(PaneId),
}

#[derive(Default)]
pub(super) struct TabDrag {
    bounds: HashMap<PaneId, BTreeMap<usize, Bounds<Pixels>>>,
    carry: Option<Carry>,
}

impl TabDrag {
    #[cfg(test)]
    pub(super) fn is_dragging(&self) -> bool {
        self.carry.as_ref().is_some_and(|carry| carry.started)
    }
}

fn insert_at(tabs: &BTreeMap<usize, Bounds<Pixels>>, x: Pixels, count: usize) -> usize {
    tabs.iter()
        .find(|(_, bounds)| bounds.center().x > x)
        .map_or(count, |(index, _)| *index)
}

impl FerriteWindow {
    pub(super) fn tab_bounds_reset(pane: PaneId, cx: &mut Context<Self>) -> AnyElement {
        let forget = cx.listener(move |this, (): &(), _, _| {
            this.tab_drag.bounds.remove(&pane);
        });
        canvas(
            move |_, window, cx| forget(&(), window, cx),
            |_, (), _, _| {},
        )
        .absolute()
        .into_any_element()
    }

    pub(super) fn note_tab_bounds(&mut self, pane: PaneId, index: usize, bounds: Bounds<Pixels>) {
        self.tab_drag
            .bounds
            .entry(pane)
            .or_default()
            .insert(index, bounds);
    }

    pub(super) fn press_tab(
        &mut self,
        (pane, event): (PaneId, &MouseDownEvent),
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left
            || event.modifiers.shift
            || self.drops_blocked()
            || cx.has_active_drag()
        {
            return;
        }
        let index = self.tab_drag.bounds.get(&pane).and_then(|tabs| {
            tabs.iter()
                .find(|(_, bounds)| bounds.contains(&event.position))
                .map(|(index, _)| *index)
        });
        let Some(index) = index else {
            return;
        };
        let Some(tab) = self
            .browser
            .state()
            .pane(pane)
            .and_then(|core| core.tabs().get(index))
        else {
            return;
        };
        self.tab_drag.carry = Some(Carry {
            pane,
            index,
            tab: tab.id(),
            title: self.tab_title(tab),
            origin: event.position,
            position: event.position,
            started: false,
            copy: event.modifiers.control,
            _escape: Self::tab_drag_escape(window, cx),
        });
    }

    fn tab_drag_escape(window: &Window, cx: &mut Context<Self>) -> Subscription {
        let own = window.window_handle();
        let escape = cx.listener(move |this, event: &KeystrokeEvent, window, cx| {
            if event.keystroke.key == "escape"
                && window.window_handle() == own
                && this.tab_drag.carry.take().is_some()
            {
                cx.stop_propagation();
                cx.notify();
            }
        });
        cx.intercept_keystrokes(escape)
    }

    fn tab_spot(&self, position: Point<Pixels>, window: &Window) -> Option<Spot> {
        let state = self.browser.state();
        self.shown_panes(window).into_iter().find_map(|pane| {
            if !self.pane_swap.contains(pane, position) {
                return None;
            }
            let count = state.pane(pane)?.tabs().len();
            let row = self.tab_drag.bounds.get(&pane).map(|tabs| {
                tabs.range(..count)
                    .map(|(index, bounds)| (*index, *bounds))
                    .collect::<BTreeMap<_, _>>()
            });
            let on_row = row.as_ref().filter(|tabs| {
                tabs.values()
                    .any(|bounds| (bounds.top()..bounds.bottom()).contains(&position.y))
            });
            Some(match on_row {
                Some(tabs) => Spot::Row(pane, insert_at(tabs, position.x, count)),
                None => Spot::Body(pane),
            })
        })
    }

    fn tab_move_for(&self, carry: &Carry, spot: Spot) -> Option<TabMove> {
        let state = self.browser.state();
        let source = state.pane(carry.pane)?.tabs().get(carry.index)?;
        if source.id() != carry.tab {
            return None;
        }
        let (to, at) = match spot {
            Spot::Row(pane, at) => (pane, at),
            Spot::Body(pane) if pane == carry.pane => return None,
            Spot::Body(pane) => (pane, state.pane(pane)?.tabs().len()),
        };
        let stays = to == carry.pane && (at == carry.index || at == carry.index + 1);
        (carry.copy || !stays).then_some(TabMove {
            from: carry.pane,
            index: carry.index,
            to,
            at,
            copy: carry.copy,
        })
    }

    fn shown_landing(&self, window: &Window) -> Option<(Spot, TabMove)> {
        let carry = self.tab_drag.carry.as_ref().filter(|carry| carry.started)?;
        let spot = self.tab_spot(carry.position, window)?;
        Some((spot, self.tab_move_for(carry, spot)?))
    }

    pub(super) fn tab_drag_over_body(&self, pane: PaneId, window: &Window) -> bool {
        matches!(self.shown_landing(window), Some((Spot::Body(over), _)) if over == pane)
    }

    pub(super) fn tab_drag_layer(&self, root: Div, window: &Window, cx: &mut Context<Self>) -> Div {
        let t = tokens::current(cx).clone();
        let calm = cx.reduce_motion();
        root.child(self.tab_drag_events(cx))
            .children(self.insert_line(&t, calm, window))
            .children(self.tab_drag_chip(&t, calm))
    }

    fn tab_drag_events(&self, cx: &mut Context<Self>) -> AnyElement {
        let moved = cx.listener(Self::move_tab_drag);
        let released = cx.listener(Self::release_tab_drag);
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

    fn move_tab_drag(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(carry) = &mut self.tab_drag.carry else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.tab_drag.carry = None;
            cx.notify();
            return;
        }
        carry.position = event.position;
        carry.copy = event.modifiers.control;
        let offset = carry.position - carry.origin;
        carry.started |= offset.magnitude() >= f64::from(DRAG_THRESHOLD);
        if carry.started {
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn release_tab_drag(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        let Some(mut carry) = self.tab_drag.carry.take() else {
            return;
        };
        if !carry.started {
            return;
        }
        carry.copy = event.modifiers.control;
        let landing = self
            .tab_spot(event.position, window)
            .filter(|_| !self.drops_blocked())
            .and_then(|spot| self.tab_move_for(&carry, spot));
        if let Some(change) = landing {
            let result = self.browser.move_tab(change).map_err(Into::into);
            self.drag_list_changed(cx);
            self.show_refusal(result);
            self.focus_core_pane(window, cx);
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn insert_line(&self, t: &Theme, calm: bool, window: &Window) -> Option<AnyElement> {
        let (Spot::Row(pane, at), _) = self.shown_landing(window)? else {
            return None;
        };
        let tabs = self.tab_drag.bounds.get(&pane)?;
        let before = tabs.range(..at).next_back().map(|(_, bounds)| *bounds);
        let after = tabs.range(at..).next().map(|(_, bounds)| *bounds);
        let gap = t.spacing.sm / 2.;
        let (x, row) = match (before, after) {
            (Some(left), Some(right)) => ((left.right() + right.left()) / 2., right),
            (None, Some(right)) => (right.left() - gap, right),
            (Some(left), None) => (left.right() + gap, left),
            (None, None) => return None,
        };
        let width = px(2.);
        let line = div()
            .id("tab-insert-line")
            .test_support()
            .absolute()
            .left(x - width / 2.)
            .top(row.top())
            .w(width)
            .h(row.size.height)
            .rounded(width)
            .bg(t.colors.accent);
        Some(fade_in(line, "tab-insert-line-fade", calm, t))
    }

    fn tab_drag_chip(&self, t: &Theme, calm: bool) -> Option<AnyElement> {
        let carry = self.tab_drag.carry.as_ref().filter(|carry| carry.started)?;
        let glyph = if carry.copy {
            IconName::Plus
        } else {
            IconName::Folder
        };
        let chip = div()
            .id("tab-drag-chip")
            .test_support()
            .absolute()
            .left(carry.position.x + t.spacing.md)
            .top(carry.position.y + t.spacing.md)
            .flex()
            .items_center()
            .gap(t.spacing.sm)
            .max_w(px(240.))
            .h(t.spacing.tab_height)
            .px(t.spacing.md)
            .rounded(t.radii.pill)
            .bg(t.surfaces.card)
            .border_1()
            .border_color(t.surfaces.card_ring)
            .shadow(t.shadows.menu.clone())
            .text_size(t.typography.small)
            .text_color(t.colors.text)
            .child(icon(glyph, t.spacing.icon - px(1.), t.colors.muted))
            .child(div().min_w_0().truncate().child(carry.title.clone()));
        Some(fade_in(chip, "tab-drag-chip-fade", calm, t))
    }
}

fn fade_in<E: IntoElement + Styled + 'static>(
    element: E,
    id: &'static str,
    calm: bool,
    t: &Theme,
) -> AnyElement {
    if calm {
        return element.into_any_element();
    }
    let enter = Animation::new(t.motion.fade).with_easing(ease_out_cubic);
    element
        .with_animation(id, enter, |element, delta| element.opacity(delta))
        .into_any_element()
}
