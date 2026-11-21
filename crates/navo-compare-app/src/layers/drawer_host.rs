//! The right-hand drawer slot: Ctrl+B opens the screen's list, Ctrl+I the details.
//! The shell routes its actions through `wire`; the screens fill the bodies later.

use std::time::{Duration, Instant};

use gpui_kit::base::TestSupportExt as _;
use gpui_kit::{AnimationExt, AnyView, KeyBinding};
use sidewise_kit::prelude::*;

use super::keys_sheet::tokens_of;
use crate::motion;

gpui_kit::actions!(layers, [ToggleList, ToggleDetails]);

/// Drawer width in pixels when open.
const DRAWER_W: f32 = 300.;

/// Binds Ctrl+B and Ctrl+I in a compare view.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-b", ToggleList, Some("CompareView")),
        KeyBinding::new("ctrl-i", ToggleDetails, Some("CompareView")),
    ]);
}

/// Hooks the two drawer actions onto `root`, so the shell opens and closes the drawers.
pub fn wire(root: Div, drawers: &Entity<DrawerHost>) -> Div {
    let (list, details) = (drawers.clone(), drawers.clone());
    root.on_action(move |_: &ToggleList, _, cx| list.update(cx, |host, cx| host.toggle_list(cx)))
        .on_action(move |_: &ToggleDetails, _, cx| {
            details.update(cx, |host, cx| host.toggle(Drawer::Details, cx));
        })
}

/// Which drawer is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drawer {
    Changes,
    Outline,
    Details,
}

/// The right-hand drawer slot: `0 <-> 300 px` wide, holding the screen's list or the details.
pub struct DrawerHost {
    list: Drawer,
    open: Option<Drawer>,
    bodies: Vec<(Drawer, AnyView)>,
    /// Animation endpoints in pixels, when it started and how long it runs; `serial` changes
    /// on each toggle so the animation restarts.
    from: f32,
    to: f32,
    started: Option<Instant>,
    dur: Duration,
    serial: u64,
}

impl DrawerHost {
    /// A closed host whose Ctrl+B drawer is `list` (`Changes` or `Outline`).
    pub fn new(list: Drawer) -> Self {
        Self {
            list,
            open: None,
            bodies: Vec::new(),
            from: 0.,
            to: 0.,
            started: None,
            dur: Duration::ZERO,
            serial: 0,
        }
    }

    /// The drawer showing, if any.
    pub fn open(&self) -> Option<Drawer> {
        self.open
    }

    /// The width in pixels the drawer is heading to (0 when closed).
    pub fn target_width(&self) -> f32 {
        self.to
    }

    /// Sets what `drawer` shows; until then it shows an empty-state line.
    pub fn set_body(&mut self, drawer: Drawer, body: AnyView, cx: &mut Context<Self>) {
        self.bodies.retain(|(kind, _)| *kind != drawer);
        self.bodies.push((drawer, body));
        cx.notify();
    }

    /// Ctrl+B: toggles this screen's list drawer.
    pub fn toggle_list(&mut self, cx: &mut Context<Self>) {
        self.toggle(self.list, cx);
    }

    /// Opens `drawer`, swaps to it when another is open, or closes it when it is the one open.
    pub fn toggle(&mut self, drawer: Drawer, cx: &mut Context<Self>) {
        self.toggle_at(drawer, Instant::now(), cx);
    }

    /// The width in pixels on screen at `now`: `from` eased towards `to` over `dur`.
    fn width_at(&self, now: Instant) -> f32 {
        let Some(start) = self.started else {
            return self.to;
        };
        let run = self.dur.as_secs_f32().max(f32::EPSILON);
        let t = (now.saturating_duration_since(start).as_secs_f32() / run).min(1.);
        self.from + (self.to - self.from) * motion::ease_out_quint(t)
    }

    /// `toggle` with the clock passed in: the new animation starts from the width on screen.
    fn toggle_at(&mut self, drawer: Drawer, now: Instant, cx: &mut Context<Self>) {
        self.from = self.width_at(now);
        self.dur = motion::side(cx);
        self.started = Some(now);
        self.open = (self.open != Some(drawer)).then_some(drawer);
        self.to = if self.open.is_some() { DRAWER_W } else { 0. };
        self.serial += 1;
        cx.notify();
    }
}

impl Render for DrawerHost {
    /// The frame clips to its animated width; the fixed-width inner column never reflows.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (side, m) = tokens_of(cx);
        let title = match self.open {
            Some(Drawer::Changes) => "Changes",
            Some(Drawer::Outline) => "Outline",
            Some(Drawer::Details) => "Details",
            None => "",
        };
        let body = self
            .bodies
            .iter()
            .find(|(kind, _)| Some(*kind) == self.open);
        let content = match body {
            Some((_, view)) => view.clone().into_any_element(),
            None => (div().text_color(m.faint).child("Nothing to show yet.")).into_any_element(),
        };
        let inner = (div()
            .flex()
            .flex_col()
            .gap_2()
            .w(px(DRAWER_W))
            .h_full()
            .p_3())
        .child(div().text_color(m.muted).child(title))
        .child(content);
        let frame = (div()
            .id("drawer-frame")
            .test_support()
            .h_full()
            .flex_none()
            .overflow_hidden()
            .border_l_1())
        .border_color(m.sep)
        .bg(m.card)
        .child(inner);
        let (from, to) = (self.from, self.to);
        let id = SharedString::from(format!("drawer-{}", self.serial));
        match motion::anim(id.clone(), side.motion.side_ms, cx).filter(|_| self.serial > 0) {
            Some(anim) => frame
                .with_animation(id, anim, move |el, t| el.w(px(from + (to - from) * t)))
                .into_any_element(),
            None => frame.w(px(to)).into_any_element(),
        }
    }
}

#[cfg(test)]
#[path = "drawer_host_tests.rs"]
mod tests;
