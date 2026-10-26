use std::collections::HashMap;

use gpui_kit::{
    App, Div, EventEmitter, Global, Pixels, SharedString, Styled, TextRun, Window, div, font, px,
};

use crate::tokens;

const FALLBACK_CHAR_WIDTH: f32 = 7.2;
const SAMPLE: &str = "0000000000";
const NAME_SAMPLE: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_0123456789";
const FIT_ROWS: usize = 200;
const SORT_SLOT: usize = 2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ColumnAlign {
    Start,
    End,
}

#[derive(Clone, PartialEq, Debug)]
pub struct ColumnSpec {
    pub id: SharedString,
    pub title: SharedString,
    pub align: ColumnAlign,
    pub min_chars: usize,
    pub max_chars: usize,
    pub flex: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ColumnEvent {
    Sort(usize),
    Resized,
}

struct DragStart {
    at: usize,
    x: Pixels,
    widths: Vec<Pixels>,
}

pub struct ColumnSet {
    pub(super) specs: Vec<ColumnSpec>,
    pub(super) char_width: Pixels,
    pub(super) name_char_width: Pixels,
    pub(super) padding: Pixels,
    fitted: Vec<Pixels>,
    initial: Vec<Pixels>,
    widths: Vec<Pixels>,
    measured: Pixels,
    drag: Option<DragStart>,
}

impl EventEmitter<ColumnEvent> for ColumnSet {}

type WidthKey = (SharedString, u32, &'static str);

#[derive(Default)]
pub(super) struct MeasuredWidths(pub(super) HashMap<WidthKey, Pixels>);

impl Global for MeasuredWidths {}

fn measure(family: SharedString, sample: &'static str, window: &Window, cx: &mut App) -> Pixels {
    let t = tokens::current(cx);
    let size = t.typography.body;
    let key = (family.clone(), f32::from(size).to_bits(), sample);
    if let Some(width) = cx
        .try_global::<MeasuredWidths>()
        .and_then(|cache| cache.0.get(&key))
    {
        return *width;
    }
    let run = TextRun {
        len: sample.len(),
        font: font(family),
        color: t.colors.text,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window.text_system().layout_line(sample, size, &[run], None);
    let width = line.width / sample.len() as f32;
    let width = if width > px(0.) && f32::from(width).is_finite() {
        width
    } else {
        px(FALLBACK_CHAR_WIDTH)
    };
    cx.default_global::<MeasuredWidths>().0.insert(key, width);
    width
}

impl ColumnSet {
    pub fn new(specs: Vec<ColumnSpec>, window: &Window, cx: &mut App) -> Self {
        let typography = tokens::current(cx).typography.clone();
        let mut set = Self {
            char_width: measure(typography.mono_font, SAMPLE, window, cx),
            name_char_width: measure(typography.ui_font, NAME_SAMPLE, window, cx),
            padding: tokens::current(cx).spacing.md * 2.,
            fitted: Vec::new(),
            initial: Vec::new(),
            widths: Vec::new(),
            measured: px(0.),
            drag: None,
            specs,
        };
        set.fit_content(std::iter::empty());
        set
    }

    pub fn fit_content(&mut self, rows: impl IntoIterator<Item = Vec<String>>) {
        let mut chars: Vec<usize> = self
            .specs
            .iter()
            .map(|spec| spec.title.chars().count() + SORT_SLOT)
            .collect();
        for row in rows.into_iter().take(FIT_ROWS) {
            for (widest, cell) in chars.iter_mut().zip(&row) {
                *widest = (*widest).max(cell.chars().count());
            }
        }
        self.fitted = chars
            .iter()
            .enumerate()
            .map(|(at, count)| self.clamp(at, self.chars_px(*count)))
            .collect();
        self.initial = self
            .specs
            .iter()
            .enumerate()
            .map(|(at, spec)| {
                if spec.flex {
                    self.min_px(at)
                } else {
                    self.fitted[at]
                }
            })
            .collect();
        self.widths.clone_from(&self.initial);
    }

    pub fn width(&self, at: usize) -> Pixels {
        self.widths.get(at).copied().unwrap_or_default()
    }

    pub fn set_measured(&mut self, width: Pixels) -> bool {
        if (self.measured - width).abs() < px(0.5) {
            return false;
        }
        self.measured = width;
        true
    }

    pub fn text_chars(&self, at: usize) -> usize {
        self.chars_in(at, self.char_width)
    }

    pub fn name_chars(&self, at: usize) -> usize {
        self.chars_in(at, self.name_char_width)
    }

    fn chars_in(&self, at: usize, char_width: Pixels) -> usize {
        let shown = if self.is_flex(at) && self.measured > px(0.) {
            self.measured
        } else {
            self.width(at)
        };
        let room = (shown - self.padding).max(px(0.)) / char_width;
        room.floor() as usize
    }

    pub fn set_width(&mut self, at: usize, width: Pixels) {
        let width = self.clamp(at, width);
        if let Some(slot) = self.widths.get_mut(at) {
            *slot = width;
        }
    }

    pub fn fit(&mut self, at: usize) {
        if let Some(width) = self.fitted.get(at).copied() {
            self.set_width(at, width);
        }
    }

    pub fn reset(&mut self) {
        self.widths.clone_from(&self.initial);
    }

    pub fn cell(&self, at: usize) -> Div {
        let base = self
            .sized(at)
            .flex()
            .items_center()
            .h_full()
            .px(self.padding / 2.)
            .overflow_hidden()
            .whitespace_nowrap();
        if self.align(at) == ColumnAlign::End {
            base.justify_end()
        } else {
            base
        }
    }

    pub(super) fn sized(&self, at: usize) -> Div {
        let width = self.width(at);
        if self.is_flex(at) {
            div().flex_1().min_w(width)
        } else {
            div().flex_none().w(width)
        }
    }

    pub(super) fn align(&self, at: usize) -> ColumnAlign {
        self.specs
            .get(at)
            .map_or(ColumnAlign::Start, |spec| spec.align)
    }

    pub(super) fn begin_drag(&mut self, at: usize, x: Pixels) {
        self.drag = Some(DragStart {
            at,
            x,
            widths: self.widths.clone(),
        });
    }

    pub(super) fn drag_to(&mut self, at: usize, x: Pixels) {
        let Some(start) = self.drag.as_ref().filter(|start| start.at == at) else {
            return;
        };
        let delta = x - start.x;
        let flex = self.specs.iter().position(|spec| spec.flex);
        let next = at + 1;
        let from = |column: usize| start.widths.get(column).copied().unwrap_or_default();
        let (own, neighbour) = (from(at), from(next));
        if flex == Some(at) {
            self.set_width(next, neighbour - delta);
            return;
        }
        self.set_width(at, own + delta);
        if flex.is_some_and(|flex| flex < at) {
            let grown = self.width(at) - own;
            self.set_width(next, neighbour - grown);
        }
    }

    pub(super) fn is_flex(&self, at: usize) -> bool {
        self.specs.get(at).is_some_and(|spec| spec.flex)
    }

    fn chars_px(&self, count: usize) -> Pixels {
        self.char_width * count as f32 + self.padding
    }

    fn min_px(&self, at: usize) -> Pixels {
        self.chars_px(self.specs.get(at).map_or(0, |spec| spec.min_chars))
    }

    fn clamp(&self, at: usize, width: Pixels) -> Pixels {
        let max = self.specs.get(at).map_or(width, |spec| {
            self.chars_px(spec.max_chars.max(spec.min_chars))
        });
        width.min(max).max(self.min_px(at))
    }
}
