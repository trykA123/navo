use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, Bounds, Context, Entity, FocusHandle, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, Styled, TestAppContext, Window, div, point, px,
};

use super::{SheetFrame, SheetState, Side, SideSheet, on_resize_keys};
use crate::test_window;

mod frame;
mod motion;
mod resize;
mod slots;

pub(super) const SHEET: &str = "sheet";

pub(super) fn frame(window: f32, active: Side) -> SheetFrame {
    let body = px(window - 16.);
    SheetFrame::new(body, (body - px(8.)) / 2., active)
}

struct Host {
    state: Entity<SheetState>,
    frame: SheetFrame,
    focus: FocusHandle,
    closes: Rc<Cell<usize>>,
    is_ringed: bool,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let closes = self.closes.clone();
        let sheet = SideSheet::new(SHEET, &self.state, self.frame)
            .label("Preview")
            .header(div().child("crates"))
            .body(div().child("sample body"))
            .hint(&["[", "]"], "")
            .hint(&["ctrl-shift-q"], "Browse")
            .close("ctrl-q", move |_, _| closes.set(closes.get() + 1))
            .accent_ring(self.is_ringed);
        let root = div()
            .track_focus(&self.focus)
            .relative()
            .w(self.frame.body)
            .h_full()
            .child(sheet);
        on_resize_keys(root, &self.state, self.frame)
    }
}

pub(super) struct Rig {
    window: AnyWindowHandle,
    host: Entity<Host>,
    pub closes: Rc<Cell<usize>>,
}

pub(super) fn open_closed(cx: &mut TestAppContext, frame: SheetFrame) -> Rig {
    let closes: Rc<Cell<usize>> = Rc::default();
    let count = closes.clone();
    let width = frame.body + px(16.);
    let (window, host) = test_window::open_sized(cx, width, move |window, cx| {
        super::bind_keys(cx);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Host {
            state: cx.new(|_| SheetState::new()),
            frame,
            focus,
            closes: count,
            is_ringed: false,
        }
    });
    Rig {
        window,
        host,
        closes,
    }
}

pub(super) fn open(cx: &mut TestAppContext, frame: SheetFrame) -> Rig {
    let rig = open_closed(cx, frame);
    rig.show(cx);
    rig
}

pub(super) fn reduce_motion(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
}

impl Rig {
    pub fn state(&self, cx: &mut TestAppContext) -> Entity<SheetState> {
        self.host.read_with(cx, |host, _| host.state.clone())
    }

    pub fn show(&self, cx: &mut TestAppContext) {
        let state = self.state(cx);
        state.update(cx, |state, cx| state.open(cx));
        self.frame(cx);
    }

    pub fn hide(&self, cx: &mut TestAppContext) {
        let state = self.state(cx);
        state.update(cx, |state, cx| state.close(cx));
        self.frame(cx);
    }

    pub fn set_frame(&self, frame: SheetFrame, cx: &mut TestAppContext) {
        self.host.update(cx, |host, cx| {
            host.frame = frame;
            cx.notify();
        });
        self.frame(cx);
    }

    pub fn set_ringed(&self, is_ringed: bool, cx: &mut TestAppContext) {
        self.host.update(cx, |host, cx| {
            host.is_ringed = is_ringed;
            cx.notify();
        });
        self.frame(cx);
    }

    pub fn frame(&self, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| window.render_frame(cx))
            .expect("window");
        cx.run_until_parked();
    }

    pub fn wait(&self, millis: u64, cx: &mut TestAppContext) {
        cx.executor().advance_clock(Duration::from_millis(millis));
        cx.run_until_parked();
        self.frame(cx);
    }

    pub fn settle(&self, cx: &mut TestAppContext) {
        self.wait(400, cx);
    }

    pub fn press(&self, key: &str, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| window.press(key, cx))
            .expect("window");
        cx.run_until_parked();
    }

    pub fn click(&self, id: &'static str, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.within(SHEET).click(id, cx);
        })
        .expect("window");
        cx.run_until_parked();
    }

    pub fn drag_grip_by(&self, dx: f32, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            let from = window
                .within(SHEET)
                .find("side-sheet-grip")
                .bounds()
                .center();
            window.drag(from, point(from.x + px(dx), from.y), cx);
        })
        .expect("window");
        cx.run_until_parked();
        self.frame(cx);
    }

    pub fn bounds(&self, cx: &mut TestAppContext) -> Option<Bounds<Pixels>> {
        cx.update_window(self.window, |_, window, _| {
            window.try_find(SHEET).map(|sheet| sheet.bounds())
        })
        .expect("window")
    }

    pub fn sheet(&self, cx: &mut TestAppContext) -> Bounds<Pixels> {
        self.bounds(cx).expect("the sheet is on screen")
    }

    pub fn label(
        &self,
        id: impl Into<gpui_kit::ElementId>,
        cx: &mut TestAppContext,
    ) -> Option<String> {
        let id = id.into();
        cx.update_window(self.window, |_, window, _| {
            window.within(SHEET).find(id).label().map(str::to_owned)
        })
        .expect("window")
    }

    pub fn sheet_label(&self, cx: &mut TestAppContext) -> Option<String> {
        cx.update_window(self.window, |_, window, _| {
            window.find(SHEET).label().map(str::to_owned)
        })
        .expect("window")
    }

    pub fn is_selected(&self, cx: &mut TestAppContext) -> Option<bool> {
        cx.update_window(self.window, |_, window, _| window.find(SHEET).selected())
            .expect("window")
    }
}
