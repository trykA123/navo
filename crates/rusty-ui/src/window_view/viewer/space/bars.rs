mod file;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{RenderOnce, Role, pattern_slash, relative};
use rusty_automotive::space::{Kind, Region};
use rusty_l10n::{t, tr};

pub use file::{FileBytes, FileBytesBar, FilePart};

use crate::format;
use crate::prelude::*;
use crate::tokens::{self, MemKind, Theme};

const BAR_HEIGHT: f32 = 14.;
const THIN_BAR_HEIGHT: f32 = 6.;
const SWATCH: f32 = 10.;
const SHOWN_KINDS: usize = 5;

pub(super) fn percent_text(used: u64, total: u64) -> String {
    if used == 0 {
        return "0".into();
    }
    if total == 0 {
        return "100".into();
    }
    match u128::from(used) * 100 / u128::from(total) {
        0 => "<1".into(),
        whole => whole.to_string(),
    }
}

fn sized_total(region: &Region) -> Option<u64> {
    region.total.filter(|total| *total > 0)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Part {
    Kind(usize),
    Fill,
    Reserved,
}

pub(super) fn bar_parts(region: &Region) -> Vec<(Part, f64)> {
    let shown = region.by_kind[..SHOWN_KINDS]
        .iter()
        .fold(0u64, |sum, bytes| sum.saturating_add(*bytes));
    let fill = region.by_kind[Kind::Fill.index()];
    let reserved = region.reserved.filter(|_| sized_total(region).is_some());
    let reserved = reserved.unwrap_or_default();
    let taken = shown.saturating_add(fill).saturating_add(reserved);
    let whole = sized_total(region).unwrap_or(taken).max(taken).max(1) as f64;
    let kinds = (0..SHOWN_KINDS).map(|index| (Part::Kind(index), region.by_kind[index]));
    kinds
        .chain([(Part::Fill, fill), (Part::Reserved, reserved)])
        .filter(|(_, bytes)| *bytes > 0)
        .map(|(part, bytes)| (part, bytes as f64 / whole))
        .collect()
}

pub(super) fn usage_label(region: &Region) -> String {
    let name = region.name.clone();
    match (sized_total(region), region.free()) {
        (Some(total), Some(free)) => t!(
            "space-usage-label",
            name = name,
            percent = percent_text(region.used, total),
            free = format::size(free)
        ),
        _ if region.total == Some(0) => t!(
            "space-usage-label-sizeless",
            name = name,
            used = format::size(region.used)
        ),
        _ => t!(
            "space-usage-label-unknown",
            name = name,
            used = format::size(region.used)
        ),
    }
}

pub(super) fn usage_lines(region: &Region, reserved: Option<u64>) -> Vec<String> {
    let used = format::size(region.used);
    let (Some(free), Some(total)) = (region.free(), sized_total(region)) else {
        let used_line = t!("space-usage-used", used = used);
        return if region.total == Some(0) {
            vec![used_line]
        } else {
            vec![used_line, tr("space-usage-unknown")]
        };
    };
    let (free, total) = (format::size(free), format::size(total));
    vec![match reserved.filter(|reserved| *reserved > 0) {
        Some(reserved) => t!(
            "space-usage-line-reserved",
            used = used,
            free = free,
            reserved = format::size(reserved),
            total = total
        ),
        None => t!("space-usage-line", used = used, free = free, total = total),
    }]
}

fn kind_colours(t: &Theme, index: usize) -> MemKind {
    t.mem.kinds()[index].1
}

pub(super) fn segment(share: f64, color: Hsla) -> Div {
    div()
        .h_full()
        .flex_none()
        .w(relative(share.clamp(0., 1.) as f32))
        .min_w(px(1.))
        .bg(color)
}

fn hatch(t: &Theme) -> Div {
    div()
        .size_full()
        .bg(pattern_slash(t.mem.fill.strong, 1., 3.))
}

fn fill_segment(t: &Theme, share: f64) -> Div {
    segment(share, t.mem.fill.tint).child(hatch(t))
}

fn reserved_hatch(t: &Theme) -> Div {
    div().size_full().bg(pattern_slash(t.mem.free_line, 1., 4.))
}

fn reserved_segment(t: &Theme, share: f64) -> Div {
    segment(share, t.mem.free).child(reserved_hatch(t))
}

#[derive(IntoElement)]
pub struct UsageBar {
    id: ElementId,
    region: Region,
}

impl UsageBar {
    pub fn new(id: impl Into<ElementId>, region: &Region) -> Self {
        Self {
            id: id.into(),
            region: Region {
                blocks: Vec::new(),
                ..region.clone()
            },
        }
    }
}

impl RenderOnce for UsageBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let region = &self.region;
        let known = sized_total(region).is_some();
        let track = div()
            .id(self.id)
            .role(Role::ProgressIndicator)
            .aria_label(usage_label(region))
            .test_support()
            .flex()
            .w_full()
            .overflow_hidden()
            .rounded(t.radii.xs);
        let track = if known {
            track
                .h(px(BAR_HEIGHT))
                .border_1()
                .border_color(t.mem.free_line)
                .bg(t.mem.free)
        } else {
            track.h(px(THIN_BAR_HEIGHT))
        };
        bar_parts(region)
            .into_iter()
            .fold(track, |bar, (part, share)| match part {
                Part::Kind(index) => bar.child(segment(share, kind_colours(&t, index).strong)),
                Part::Fill => bar.child(fill_segment(&t, share)),
                Part::Reserved => bar.child(reserved_segment(&t, share)),
            })
    }
}

#[derive(IntoElement)]
pub struct UsageLine {
    lines: Vec<String>,
}

impl UsageLine {
    pub fn new(region: &Region, reserved: Option<u64>) -> Self {
        Self {
            lines: usage_lines(region, reserved),
        }
    }
}

impl RenderOnce for UsageLine {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        div()
            .flex()
            .flex_col()
            .text_size(t.typography.small)
            .text_color(t.colors.muted)
            .children(self.lines.into_iter().map(|line| div().child(line)))
    }
}

#[derive(IntoElement)]
pub struct RegionHead {
    name: SharedString,
    figure: String,
}

impl RegionHead {
    pub fn new(region: &Region) -> Self {
        let figure = match sized_total(region) {
            Some(total) => t!(
                "viewer-space-share",
                percent = percent_text(region.used, total)
            ),
            None => format::size(region.used),
        };
        Self {
            name: region.name.clone().into(),
            figure,
        }
    }
}

impl RenderOnce for RegionHead {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .truncate()
                    .text_size(t.typography.small)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.name),
            )
            .child(
                div()
                    .text_size(t.typography.body * 2.)
                    .font_family(t.typography.numbers_font.clone())
                    .font_features(t.typography.numbers_font_features.clone())
                    .child(self.figure),
            )
    }
}

pub(super) fn swatch(t: &Theme, color: Hsla, outlined: bool) -> Div {
    let dot = div()
        .flex_none()
        .size(px(SWATCH))
        .rounded(t.radii.xs)
        .bg(color);
    if outlined {
        dot.border_1().border_color(t.mem.free_line)
    } else {
        dot
    }
}

#[derive(IntoElement, Default)]
pub struct Legend {
    fill: bool,
    reserved: bool,
}

impl Legend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_fill(mut self, fill: bool) -> Self {
        self.fill = fill;
        self
    }

    pub fn with_reserved(mut self, reserved: bool) -> Self {
        self.reserved = reserved;
        self
    }
}

const KIND_TEXTS: [&str; 6] = [
    "space-kind-code",
    "space-kind-rodata",
    "space-kind-data",
    "space-kind-bss",
    "space-kind-other",
    "space-kind-fill",
];

impl RenderOnce for Legend {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let entry = |dot: Div, text: &str| {
            div()
                .flex()
                .items_center()
                .gap(t.spacing.xs)
                .child(dot)
                .child(tr(text))
        };
        let kinds = (0..SHOWN_KINDS).map(|index| {
            entry(
                swatch(&t, kind_colours(&t, index).strong, false),
                KIND_TEXTS[index],
            )
        });
        div()
            .id("space-legend")
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x(t.spacing.lg)
            .gap_y(t.spacing.xs)
            .text_size(t.typography.small)
            .text_color(t.colors.muted)
            .children(kinds)
            .when(self.fill, |row| {
                let dot = swatch(&t, t.mem.fill.tint, true)
                    .overflow_hidden()
                    .child(hatch(&t));
                row.child(entry(dot, KIND_TEXTS[Kind::Fill.index()]))
            })
            .when(self.reserved, |row| {
                let dot = swatch(&t, t.mem.free, true)
                    .overflow_hidden()
                    .child(reserved_hatch(&t));
                row.child(entry(dot, "space-kind-reserved"))
            })
            .child(entry(swatch(&t, t.mem.free, true), "space-kind-free"))
    }
}

#[cfg(test)]
mod tests;
