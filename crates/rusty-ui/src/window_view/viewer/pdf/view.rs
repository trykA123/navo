use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{Role, Stateful};

use super::keys::{
    ActualSize, AllHits, CopyText, FitWidth, LinkFollow, LinkNext, LinkPrevious, PagesDrawer,
    TrailBack, TrailForward, WholeWord, ZoomIn, ZoomOut,
};
use super::layout::Zoom;
use super::rail::hit_rail;
use super::{PDF_CONTEXT, PdfView, Phase};
use crate::format;
use crate::prelude::*;
use crate::tokens::{self, Theme};
use crate::window_view::viewer::{
    Back, GoEnd, GoStart, LeftDrawer, MatchCase, OpenFind, OpenGoTo, PageDown, PageUp, RightDrawer,
    RowDown, RowUp, ScrollLeft, ScrollRight,
};

fn small(t: &Theme, color: Hsla) -> Div {
    div()
        .flex_none()
        .text_size(t.typography.small)
        .text_color(color)
}

fn labelled(
    t: &Theme,
    id: &'static str,
    color: Hsla,
    text: SharedString,
) -> ObservedElement<Stateful<Div>> {
    small(t, color)
        .id(id)
        .role(Role::Status)
        .aria_label(text.clone())
        .test_support()
        .child(text)
}

fn and_more(count: usize) -> String {
    match count.saturating_sub(1) {
        0 => String::new(),
        more => format!(" (and {more} more)"),
    }
}

impl PdfView {
    fn meta(&self) -> SharedString {
        let size = format::size(self.len);
        match self.doc() {
            Some(doc) => format!(
                "PDF · {} · {size}",
                format::count(u64::from(doc.count()), "page", "pages")
            ),
            None => format!("PDF · {size}"),
        }
        .into()
    }

    fn header(&self, t: &Theme) -> Div {
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.md)
            .px(t.spacing.xl)
            .h(t.spacing.path_height)
            .border_b_1()
            .border_color(t.colors.line)
            .bg(t.colors.rail)
            .child(
                div()
                    .id("viewer-title")
                    .test_support()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.title.clone()),
            )
            .child(small(t, t.colors.muted).child(self.meta()))
            .child(div().flex_1())
            .children(
                self.note
                    .clone()
                    .map(|note| labelled(t, "pdf-note", t.colors.muted, note)),
            )
    }

    fn position(&self) -> SharedString {
        match &self.phase {
            Phase::Ready(doc) if doc.count() == 0 => "PDF · no pages".into(),
            Phase::Ready(doc) => format!(
                "PDF · page {} of {}",
                format::bytes(u64::from(doc.current_page() + 1)),
                format::bytes(u64::from(doc.count()))
            )
            .into(),
            Phase::Locked { .. } | Phase::Unlocking { .. } => {
                "PDF · locked, waiting for the password".into()
            }
            Phase::Refused(_) => "PDF · not opened".into(),
        }
    }

    fn problem(&self) -> Option<SharedString> {
        match &self.phase {
            Phase::Refused(error) => {
                let (headline, _) = super::card::refusal(error);
                Some(headline.into())
            }
            Phase::Ready(doc) => {
                let trouble = &doc.trouble;
                let drawn = trouble.broken.iter().next().map(|(page, error)| {
                    let others = and_more(trouble.broken.len());
                    format!("Page {} was not drawn: {error}{others}", page + 1)
                });
                let searched = trouble.unread.iter().next().map(|page| {
                    let others = and_more(trouble.unread.len());
                    format!("Page {} could not be searched{others}", page + 1)
                });
                let problems: Vec<String> = drawn.into_iter().chain(searched).collect();
                (!problems.is_empty()).then(|| problems.join(" · ").into())
            }
            _ => None,
        }
    }

    fn status_line(&self, t: &Theme) -> Div {
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.lg)
            .px(t.spacing.xl)
            .h(t.spacing.status_height)
            .border_t_1()
            .border_color(t.colors.line)
            .bg(t.colors.rail)
            .child(labelled(t, "pdf-status", t.colors.muted, self.position()))
            .child(div().flex_1())
            .children(
                self.indexing_label()
                    .map(|label| labelled(t, "pdf-indexing", t.colors.muted, label)),
            )
            .children(
                self.problem()
                    .map(|problem| labelled(t, "pdf-problem", t.colors.warn, problem)),
            )
    }

    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.clear_ring(cx) {
            return;
        }
        if self.leave_page_box(window, cx) {
            return self.focus_pages(window, cx);
        }
        if !self.close_top(window, cx) && !self.close_find(window, cx) {
            cx.propagate();
        }
    }

    fn page_keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &PageDown, _, cx| view.step_page(true, cx)))
            .on_action(cx.listener(|view, _: &PageUp, _, cx| view.step_page(false, cx)))
            .on_action(cx.listener(|view, _: &GoStart, _, cx| view.go_to_page(0, cx)))
            .on_action(cx.listener(|view, _: &GoEnd, _, cx| view.go_to_end(cx)))
            .on_action(cx.listener(|view, _: &RowUp, _, cx| view.scroll_lines(0., -1., cx)))
            .on_action(cx.listener(|view, _: &RowDown, _, cx| view.scroll_lines(0., 1., cx)))
            .on_action(cx.listener(|view, _: &ScrollLeft, _, cx| view.scroll_lines(-1., 0., cx)))
            .on_action(cx.listener(|view, _: &ScrollRight, _, cx| view.scroll_lines(1., 0., cx)))
            .on_action(cx.listener(|view, _: &OpenGoTo, window, cx| view.open_page_box(window, cx)))
            .on_action(cx.listener(|view, _: &FitWidth, _, cx| view.set_zoom(Zoom::Fit, cx)))
            .on_action(cx.listener(|view, _: &ActualSize, _, cx| view.actual_size(cx)))
            .on_action(cx.listener(|view, _: &ZoomIn, _, cx| view.zoom_step(true, None, cx)))
            .on_action(cx.listener(|view, _: &ZoomOut, _, cx| view.zoom_step(false, None, cx)))
    }

    fn find_keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &OpenFind, window, cx| view.open_find(window, cx)))
            .on_action(cx.listener(|view, _: &MatchCase, _, cx| view.toggle_case(cx)))
            .on_action(cx.listener(|view, _: &WholeWord, _, cx| view.toggle_word(cx)))
            .on_action(cx.listener(|view, _: &AllHits, window, cx| view.toggle_hits(window, cx)))
            .on_action(cx.listener(|view, _: &RightDrawer, window, cx| {
                view.toggle_hits(window, cx);
            }))
            .on_action(cx.listener(|view, _: &LeftDrawer, window, cx| view.toggle_left(window, cx)))
            .on_action(cx.listener(|view, _: &PagesDrawer, window, cx| view.open_pages(window, cx)))
            .on_action(cx.listener(|view, _: &CopyText, _, cx| view.copy_selection(cx)))
            .on_action(cx.listener(|view, _: &Back, window, cx| view.back(window, cx)))
            .on_action(cx.listener(|view, _: &LinkNext, _, cx| view.step_ring(false, cx)))
            .on_action(cx.listener(|view, _: &LinkPrevious, _, cx| view.step_ring(true, cx)))
            .on_action(cx.listener(|view, _: &LinkFollow, _, cx| view.follow_ring(cx)))
            .on_action(cx.listener(|view, _: &TrailBack, _, cx| view.go_back(cx)))
            .on_action(cx.listener(|view, _: &TrailForward, _, cx| view.go_forward(cx)))
    }

    fn content(&self, t: &Theme, cx: &mut Context<Self>) -> Vec<AnyElement> {
        match &self.phase {
            Phase::Refused(error) => vec![self.refused_card(t, error).into_any_element()],
            Phase::Locked { .. } | Phase::Unlocking { .. } => {
                vec![self.password_card(t, cx).into_any_element()]
            }
            Phase::Ready(_) => {
                let rail = self
                    .doc()
                    .filter(|_| self.look.hit_rail)
                    .and_then(|doc| hit_rail(t, doc));
                let stage = self
                    .stage(t, cx)
                    .children(rail)
                    .children(self.drawer_layer(t, cx));
                let mut parts = vec![
                    self.tool_row(t, cx).into_any_element(),
                    stage.into_any_element(),
                ];
                parts.extend(self.page_map(t, cx).map(IntoElement::into_any_element));
                parts
            }
        }
    }
}

impl Render for PdfView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        self.sync_viewport();
        self.sync_page_box(window, cx);
        self.pump(cx);
        let root = div()
            .id("pdf")
            .key_context(PDF_CONTEXT)
            .track_focus(&self.focus);
        let root = Self::find_keys(Self::page_keys(root, cx), cx);
        root.test_support()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .size_full()
            .bg(t.colors.surface)
            .child(self.header(&t))
            .children(self.content(&t, cx))
            .child(self.status_line(&t))
    }
}
