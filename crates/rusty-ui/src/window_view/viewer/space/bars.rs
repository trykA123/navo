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
const SWATCH: f32 = 10.;
const SHOWN_KINDS: usize = 5;

fn percent(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.
    } else {
        used as f64 * 100. / total as f64
    }
}

pub(super) fn percent_text(used: u64, total: u64) -> String {
    let value = percent(used, total);
    if value < 10. {
        format!("{value:.1}")
    } else {
        format!("{value:.0}")
    }
}

pub(super) fn usage_label(region: &Region) -> String {
    match region.free() {
        Some(free) => t!(
            "space-usage-label",
            name = region.name.clone(),
            percent = percent_text(region.used, region.total.unwrap_or_default()),
            free = format::size(free)
        ),
        None => t!(
            "space-usage-label-unknown",
            name = region.name.clone(),
            used = format::size(region.used)
        ),
    }
}

pub(super) fn usage_lines(region: &Region, reserved: Option<u64>) -> Vec<String> {
    let used = format::size(region.used);
    let (Some(free), Some(total)) = (region.free(), region.total) else {
        return vec![
            t!("space-usage-used", used = used),
            tr("space-usage-unknown"),
        ];
    };
    let (free, total) = (format::size(free), format::size(total));
    vec![match reserved {
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
        let shown: u64 = region.by_kind[..SHOWN_KINDS].iter().sum();
        let fill = region.by_kind[Kind::Fill.index()];
        let whole = region.total.unwrap_or(shown).max(shown + fill).max(1) as f64;
        let label = usage_label(region);
        let track = div()
            .id(self.id)
            .role(Role::ProgressIndicator)
            .aria_label(label)
            .test_support()
            .flex()
            .w_full()
            .h(px(BAR_HEIGHT))
            .overflow_hidden()
            .rounded(t.radii.xs)
            .border_1()
            .border_color(t.mem.free_line)
            .bg(t.mem.free);
        let used = (0..SHOWN_KINDS)
            .filter(|index| region.by_kind[*index] > 0)
            .fold(track, |bar, index| {
                let color = kind_colours(&t, index).strong;
                bar.child(segment(region.by_kind[index] as f64 / whole, color))
            });
        if fill > 0 {
            used.child(fill_segment(&t, fill as f64 / whole))
        } else {
            used
        }
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
        let figure = match region.total {
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
}

impl Legend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_fill(mut self, fill: bool) -> Self {
        self.fill = fill;
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
            .child(entry(swatch(&t, t.mem.free, true), "space-kind-free"))
    }
}

#[cfg(test)]
mod tests;
