//! Maps lexer spans onto the text shown in a paged viewer row.

use std::collections::BTreeSet;
use std::ops::Range;

use gpui_kit::HighlightStyle;
use navo_core::lines::Span;

/// Layers paint runs over syntax runs in input order; a paint replaces only the fields it sets.
///
/// Cuts the shown string at every run edge; pieces without a style are dropped. `syntax` must be
/// sorted and not overlapping. One left-to-right pass: a syntax cursor and the set of live paints.
pub(super) fn overlay(
    syntax: &[(Range<usize>, HighlightStyle)],
    paints: &[(Range<usize>, HighlightStyle)],
) -> Vec<(Range<usize>, HighlightStyle)> {
    let mut edges: Vec<usize> = syntax
        .iter()
        .chain(paints)
        .flat_map(|(range, _)| [range.start, range.end])
        .collect();
    edges.sort_unstable();
    edges.dedup();
    let mut by_start: Vec<usize> = (0..paints.len()).collect();
    by_start.sort_by_key(|&index| paints[index].0.start);
    let (mut cursor, mut entered) = (0, 0);
    let mut live = BTreeSet::new();
    let mut pieces = Vec::new();
    for pair in edges.windows(2) {
        let piece = pair[0]..pair[1];
        while cursor < syntax.len() && syntax[cursor].0.end <= piece.start {
            cursor += 1;
        }
        while entered < by_start.len() && paints[by_start[entered]].0.start <= piece.start {
            live.insert(by_start[entered]);
            entered += 1;
        }
        live.retain(|&index| paints[index].0.end > piece.start);
        let mut style = syntax
            .get(cursor)
            .filter(|(range, _)| range.start <= piece.start && piece.end <= range.end)
            .map(|(_, style)| *style);
        for &index in &live {
            let paint = paints[index].1;
            style = Some(style.map_or(paint, |base| set_over(base, paint)));
        }
        if let Some(style) = style {
            pieces.push((piece, style));
        }
    }
    pieces
}

/// Returns `base` with every field `paint` sets replaced.
fn set_over(base: HighlightStyle, paint: HighlightStyle) -> HighlightStyle {
    HighlightStyle {
        color: paint.color.or(base.color),
        background_color: paint.background_color.or(base.background_color),
        font_weight: paint.font_weight.or(base.font_weight),
        font_style: paint.font_style.or(base.font_style),
        underline: paint.underline.or(base.underline),
        strikethrough: paint.strikethrough.or(base.strikethrough),
        fade_out: paint.fade_out.or(base.fade_out),
    }
}

/// Maps row byte spans into shown byte ranges after character clipping and control replacement.
pub(super) fn syntax_runs(
    text: &str,
    shift: usize,
    limit: usize,
    spans: &[Span],
) -> Vec<(Range<usize>, &'static str)> {
    let mut runs: Vec<(Range<usize>, &'static str)> = Vec::new();
    let mut span_index = 0;
    let mut active = None;
    let mut shown_len = 0;
    for (position, (offset, ch)) in text
        .char_indices()
        .enumerate()
        .take(shift.saturating_add(limit))
    {
        if position < shift {
            continue;
        }
        while span_index < spans.len() && spans[span_index].0.end <= offset {
            span_index += 1;
        }
        let len = if ch.is_control() { 1 } else { ch.len_utf8() };
        if let Some((range, role)) = spans.get(span_index)
            && range.start <= offset
            && offset < range.end
        {
            if active == Some(span_index)
                && let Some((range, _)) = runs.last_mut()
            {
                range.end = shown_len + len;
            } else {
                runs.push((shown_len..shown_len + len, role));
                active = Some(span_index);
            }
        }
        shown_len += len;
    }
    runs
}

#[cfg(test)]
mod tests {
    use gpui_kit::{HighlightStyle, hsla};

    use std::ops::Range;

    use super::{overlay, set_over, syntax_runs};

    fn colour(hue: f32) -> HighlightStyle {
        HighlightStyle {
            color: Some(hsla(hue, 1.0, 0.5, 1.0)),
            ..HighlightStyle::default()
        }
    }

    fn fill(hue: f32) -> HighlightStyle {
        HighlightStyle {
            background_color: Some(hsla(hue, 1.0, 0.5, 1.0)),
            ..HighlightStyle::default()
        }
    }

    #[test]
    fn hit_colour_replaces_syntax_colour_exactly() {
        let hit = HighlightStyle {
            color: colour(0.9).color,
            background_color: fill(0.1).background_color,
            ..HighlightStyle::default()
        };
        assert_eq!(
            overlay(&[(0..4, colour(0.3))], &[(0..4, hit)]),
            vec![(0..4, hit)]
        );
    }

    #[test]
    fn match_background_keeps_syntax_colour() {
        let merged = HighlightStyle {
            color: colour(0.3).color,
            background_color: fill(0.6).background_color,
            ..HighlightStyle::default()
        };
        assert_eq!(
            overlay(&[(0..6, colour(0.3))], &[(2..4, fill(0.6))]),
            vec![(0..2, colour(0.3)), (2..4, merged), (4..6, colour(0.3))]
        );
    }

    #[test]
    fn later_paint_wins_and_output_is_sorted() {
        let out = overlay(&[], &[(0..6, colour(0.1)), (3..9, colour(0.2))]);
        assert_eq!(
            out,
            vec![
                (0..3, colour(0.1)),
                (3..6, colour(0.2)),
                (6..9, colour(0.2))
            ]
        );
        assert!(out.windows(2).all(|w| w[0].0.end <= w[1].0.start));
    }

    /// The plain quadratic overlay the one-pass version must match.
    fn reference(
        syntax: &[(Range<usize>, HighlightStyle)],
        paints: &[(Range<usize>, HighlightStyle)],
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let mut edges: Vec<usize> = (syntax.iter().chain(paints))
            .flat_map(|(range, _)| [range.start, range.end])
            .collect();
        edges.sort_unstable();
        edges.dedup();
        let mut out = Vec::new();
        for pair in edges.windows(2) {
            let covers = |r: &Range<usize>| r.start <= pair[0] && pair[1] <= r.end;
            let mut style = syntax.iter().find(|(r, _)| covers(r)).map(|(_, s)| *s);
            for (_, paint) in paints.iter().filter(|(r, _)| covers(r)) {
                style = Some(style.map_or(*paint, |base| set_over(base, *paint)));
            }
            out.extend(style.map(|style| (pair[0]..pair[1], style)));
        }
        out
    }

    /// `runs` syntax runs of width 3 every 4 bytes, and `paints` overlapping paints every 16 bytes.
    fn dense(runs: usize, paints: usize) -> Layers {
        let syntax = (0..runs)
            .map(|i| (i * 4..i * 4 + 3, colour((i % 7) as f32 / 7.0)))
            .collect();
        let paints = (0..paints)
            .map(|i| {
                let style = if i % 2 == 0 { fill(0.6) } else { colour(0.9) };
                (i * 16 + 1..i * 16 + 41, style)
            })
            .collect();
        (syntax, paints)
    }

    type Layers = (
        Vec<(Range<usize>, HighlightStyle)>,
        Vec<(Range<usize>, HighlightStyle)>,
    );

    #[test]
    fn a_dense_row_matches_the_plain_overlay() {
        let (syntax, paints) = dense(2_000, 500);
        assert_eq!(overlay(&syntax, &paints), reference(&syntax, &paints));
    }

    #[test]
    fn a_very_dense_row_overlays_in_one_pass() {
        let (syntax, paints) = dense(40_000, 10_000);
        let began = std::time::Instant::now();
        let out = overlay(&syntax, &paints);
        assert!(out.len() > 40_000);
        assert!(began.elapsed().as_secs() < 2, "{:?}", began.elapsed());
    }

    #[test]
    fn unstyled_pieces_are_dropped() {
        assert_eq!(
            overlay(&[(5..8, colour(0.3))], &[(0..2, fill(0.6))]),
            vec![(0..2, fill(0.6)), (5..8, colour(0.3))]
        );
    }

    #[test]
    fn ascii_spans_and_empty_results() {
        assert_eq!(
            syntax_runs("abc def", 0, 7, &[(0..3, "keyword"), (4..7, "string")]),
            vec![(0..3, "keyword"), (4..7, "string")]
        );
        assert!(syntax_runs("abc", 0, 0, &[(0..3, "keyword")]).is_empty());
    }

    #[test]
    fn shift_cuts_a_span() {
        assert_eq!(
            syntax_runs("abcdef", 2, 4, &[(1..5, "string")]),
            vec![(0..3, "string")]
        );
    }

    #[test]
    fn control_before_span_shrinks_shown_bytes() {
        assert_eq!(
            syntax_runs("a\u{85}bc", 0, 4, &[(3..5, "string")]),
            vec![(2..4, "string")]
        );
    }

    #[test]
    fn limit_cuts_a_span() {
        assert_eq!(
            syntax_runs("abcdef", 0, 3, &[(1..5, "string")]),
            vec![(1..3, "string")]
        );
    }

    #[test]
    fn multibyte_character_before_span_keeps_byte_offsets() {
        assert_eq!(
            syntax_runs("éabc", 0, 4, &[(2..5, "string")]),
            vec![(2..5, "string")]
        );
    }
}
