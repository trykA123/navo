use gpui_kit::Stateful;

use super::{LeftTab, PdfView};
use crate::prelude::*;
use crate::side_sheet::{SheetFrame, Side, SideSheet};
use crate::tokens::Theme;

const LEFT_WIDTH: f32 = 320.;
const RIGHT_WIDTH: f32 = 400.;

impl PdfView {
    fn sheet_of(&self, side: Side) -> &Entity<crate::side_sheet::SheetState> {
        match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
        }
    }

    pub(super) fn is_open(&self, side: Side, cx: &App) -> bool {
        self.sheet_of(side).read(cx).is_open()
    }

    fn open_sheet(&mut self, side: Side, window: &mut Window, cx: &mut Context<Self>) {
        self.sheet_of(side).update(cx, |sheet, cx| sheet.open(cx));
        self.drawers.retain(|open| *open != side);
        self.drawers.push(side);
        let focus = match side {
            Side::Left => self.pages_focus.clone(),
            Side::Right => self.hits_focus.clone(),
        };
        window.focus(&focus, cx);
        self.pump(cx);
        cx.notify();
    }

    pub(super) fn close_sheet(&mut self, side: Side, window: &mut Window, cx: &mut Context<Self>) {
        self.sheet_of(side).update(cx, |sheet, cx| sheet.close(cx));
        self.drawers.retain(|open| *open != side);
        match self.drawers.last() {
            Some(Side::Left) => window.focus(&self.pages_focus, cx),
            Some(Side::Right) => window.focus(&self.hits_focus, cx),
            None => self.focus_pages(window, cx),
        }
        cx.notify();
    }

    pub(super) fn close_top(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(side) = self.drawers.last().copied() else {
            return false;
        };
        self.close_sheet(side, window, cx);
        true
    }

    pub(super) fn toggle_left(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_open(Side::Left, cx) {
            self.close_sheet(Side::Left, window, cx);
        } else {
            self.show_pages_row();
            self.open_sheet(Side::Left, window, cx);
        }
    }

    pub(super) fn open_pages(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.left_tab = LeftTab::Pages;
        self.show_pages_row();
        self.open_sheet(Side::Left, window, cx);
    }

    pub fn open_outline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pick_tab(LeftTab::Outline, cx);
        self.open_sheet(Side::Left, window, cx);
    }

    pub(super) fn pick_tab(&mut self, tab: LeftTab, cx: &mut Context<Self>) {
        self.left_tab = tab;
        if tab == LeftTab::Outline
            && let Some(doc) = self.doc_mut()
        {
            doc.show_chapter();
        }
        self.pump(cx);
        cx.notify();
    }

    pub(super) fn toggle_hits(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_open(Side::Right, cx) {
            self.close_sheet(Side::Right, window, cx);
        } else {
            self.open_sheet(Side::Right, window, cx);
        }
    }

    fn frame(&self, side: Side, width: f32) -> SheetFrame {
        let body = px(self.measured.get().size.width.max(width));
        SheetFrame::new(body, px(width), side.other()).covering_passive_card()
    }

    fn placed(t: &Theme, sheet: SideSheet) -> AnyElement {
        div()
            .absolute()
            .top(t.spacing.xs)
            .bottom(t.spacing.xs)
            .left(t.spacing.xs)
            .right(t.spacing.xs)
            .child(sheet)
            .into_any_element()
    }

    fn closer(
        &self,
        side: Side,
        cx: &mut Context<Self>,
    ) -> impl Fn(&mut Window, &mut App) + 'static {
        let this = cx.weak_entity();
        move |window, cx| {
            let _ = this.update(cx, |view, cx| view.close_sheet(side, window, cx));
        }
    }

    pub(super) fn left_sheet(&self, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.left.read(cx).is_shown(cx) {
            return None;
        }
        let sheet = SideSheet::new(
            "pdf-left-drawer",
            &self.left,
            self.frame(Side::Left, LEFT_WIDTH),
        )
        .label("Pages and outline")
        .header(self.left_tabs(t, cx))
        .close("escape", self.closer(Side::Left, cx))
        .body(self.left_body(t, cx))
        .hint(&["up", "down"], "Move")
        .hint(&["enter"], "Go")
        .hint(&["escape"], "Close");
        Some(Self::placed(t, sheet))
    }

    pub(super) fn right_sheet(&self, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.right.read(cx).is_shown(cx) {
            return None;
        }
        let sheet = SideSheet::new(
            "pdf-right-drawer",
            &self.right,
            self.frame(Side::Right, RIGHT_WIDTH),
        )
        .label("All hits")
        .header(self.hits_title(t))
        .close("escape", self.closer(Side::Right, cx))
        .body(self.hits_body(t, cx))
        .hint(&["enter"], "Go")
        .hint(&["f3"], "Next")
        .hint(&["escape"], "Close");
        Some(Self::placed(t, sheet))
    }

    pub(super) fn drawer_layer(&self, t: &Theme, cx: &mut Context<Self>) -> Vec<AnyElement> {
        [self.left_sheet(t, cx), self.right_sheet(t, cx)]
            .into_iter()
            .flatten()
            .collect()
    }
}

pub(super) fn list_frame(id: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .key_context(crate::window_view::viewer::search::FIND_ALL_CONTEXT)
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
}
