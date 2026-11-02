use std::ops::Range;

pub struct Widths<'a> {
    pub crumbs: &'a [f32],
    pub separator: f32,
    pub ellipsis: f32,
}

fn needed(widths: &Widths, hidden: &Range<usize>) -> f32 {
    let shown = widths.crumbs.len() - hidden.len();
    let items = shown + usize::from(!hidden.is_empty());
    let text: f32 = widths
        .crumbs
        .iter()
        .enumerate()
        .filter(|(index, _)| !hidden.contains(index))
        .map(|(_, width)| width)
        .sum();
    let ellipsis = if hidden.is_empty() {
        0.
    } else {
        widths.ellipsis
    };
    text + ellipsis + widths.separator * items.saturating_sub(1) as f32
}

pub fn hidden_segments(widths: &Widths, available: f32) -> Range<usize> {
    let count = widths.crumbs.len();
    if count < 3 || needed(widths, &(0..0)) <= available {
        return 0..0;
    }
    (2..count)
        .map(|end| 1..end)
        .find(|hidden| needed(widths, hidden) <= available)
        .unwrap_or(1..count - 1)
}
