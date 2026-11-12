//! The facts the details drawer (Ctrl+I) shows for the pairs that are not small text: the
//! large text, hex and image bodies list their kind, both sizes, the engine step or alignment
//! mode, and progress.

use navo_l10n::{t, tr};

use super::FileScreen;
use super::drawers::{fact, frame};
use crate::commands::compare::PickToggle;
use crate::compare_diff::image_body::ImageMode;
use crate::compare_diff::{HexAlign, LargeStep, LargeStepView};
use crate::format;
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant};
use crate::settings::Field;
use crate::tokens::Theme;
use crate::window_view::ScreenOwner;

/// One "name: value" line.
fn line(name: &str, value: String) -> String {
    format!("{}: {value}", tr(name))
}

/// The two sizes as "left ↔ right".
fn sizes(left: String, right: String) -> String {
    line(
        "compare-drawer-sizes",
        t!("compare-drawer-sides", left = left, right = right),
    )
}

/// The most cutting step any section took; the whole-file step when none cut detail.
fn engine_step(steps: &[LargeStepView]) -> &'static str {
    let rank = |step: LargeStep| match step {
        LargeStep::Whole => 0,
        LargeStep::Sections => 1,
        LargeStep::Coarse => 2,
        LargeStep::PagedHunks => 3,
    };
    match steps
        .iter()
        .map(|view| view.step)
        .max_by_key(|step| rank(*step))
    {
        None | Some(LargeStep::Whole) => "compare-drawer-step-whole",
        Some(LargeStep::Sections) => "compare-drawer-step-sections",
        Some(LargeStep::Coarse) => "compare-drawer-step-coarse",
        Some(LargeStep::PagedHunks) => "compare-drawer-step-paged",
    }
}

impl FileScreen {
    /// The detail lines of a large, hex or image body; `None` for a small text pair, which has
    /// its own drawer.
    pub(super) fn body_facts(&self) -> Option<Vec<String>> {
        if let Some(pictures) = self.pictures.as_ref() {
            let mode = match pictures.mode() {
                ImageMode::SideBySide => "compare-image-side-by-side",
                ImageMode::Overlay => "compare-image-overlay",
                ImageMode::Mask => "compare-image-mask",
            };
            let size = |at| {
                pictures
                    .dimensions(at)
                    .map_or_else(String::new, |(w, h)| format!("{w} × {h}"))
            };
            let mut facts = vec![
                line("compare-drawer-kind", tr("compare-drawer-kind-image")),
                sizes(size(0), size(1)),
                line("compare-drawer-view", tr(mode)),
            ];
            facts.extend(
                pictures
                    .status()
                    .map(|status| line("compare-drawer-progress", status.to_string())),
            );
            return Some(facts);
        }
        if let Some(hex) = self.hex.as_ref() {
            let align = match hex.align() {
                HexAlign::Offset => "compare-hex-align-offset",
                HexAlign::Content => "compare-hex-align-content",
            };
            let (left, right) = hex.lengths();
            let mut facts = vec![
                line("compare-drawer-kind", tr("compare-drawer-kind-binary")),
                sizes(format::size(left), format::size(right)),
                line("compare-drawer-align", tr(align)),
            ];
            facts.extend(
                hex.status()
                    .map(|status| line("compare-drawer-progress", status.to_string())),
            );
            return Some(facts);
        }
        let large = self.large.as_ref()?;
        let (left, right) = self.large_sizes()?;
        Some(vec![
            line("compare-drawer-kind", tr("compare-drawer-kind-large")),
            sizes(format::size(left), format::size(right)),
            line("compare-drawer-step", tr(engine_step(&large.steps))),
            line("compare-drawer-progress", large.progress_text()),
        ])
    }
}

/// The drawer for those facts, in the same frame as the text pair's.
fn body_details_view(t: &Theme, facts: &[String]) -> AnyElement {
    frame(t, "compare-details", tr("compare-drawer-details"))
        .border_l_1()
        .children(facts.iter().map(|text| fact(t, text.clone())))
        .into_any_element()
}

/// A body with the details drawer (when given) on its right.
pub(super) fn beside(content: AnyElement, details: Option<AnyElement>) -> Div {
    let body = div().flex().flex_col().flex_1().min_w_0().child(content);
    let row = div().flex().flex_1().min_h_0().min_w_0();
    let row = row.child(body).children(details);
    div().flex().flex_col().flex_1().min_h_0().child(row)
}

impl FileScreen {
    /// The details drawer of a large, hex or image body while Ctrl+I has it open.
    pub(super) fn body_details(&self, t: &Theme) -> Option<AnyElement> {
        let facts = self.drawers.details.then(|| self.body_facts()).flatten()?;
        Some(body_details_view(t, &facts))
    }

    /// The link beside the large body's counter to the memory budget setting, when a step
    /// cut detail for the budget. Space on it opens Settings too: it would otherwise reach the
    /// diff's pick key.
    pub(super) fn budget_link_button(&self, cx: &mut Context<ScreenOwner>) -> Option<Div> {
        self.large.as_ref().filter(|large| large.budget_link())?;
        let button = Button::new(
            "compare-screen-budget-link",
            tr("compare-screen-budget-link"),
        )
        .variant(ButtonVariant::Ghost)
        .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
            view.open_settings_at(Field::CompareBudgetMib, window, cx);
        }));
        Some(
            div()
                .child(button)
                .on_action(cx.listener(|view, _: &PickToggle, window, cx| {
                    view.open_settings_at(Field::CompareBudgetMib, window, cx);
                })),
        )
    }
}

#[cfg(test)]
impl crate::window_view::ScreenOwner {
    /// The lines the open details drawer shows for a large, hex or image body; none when the
    /// drawer is closed or the body is a small text pair.
    pub(in crate::window_view) fn body_facts_probe(&self) -> Option<Vec<String>> {
        let file = self.compare.screen.as_ref()?.file.as_ref()?;
        file.drawers.details.then(|| file.body_facts()).flatten()
    }
}
