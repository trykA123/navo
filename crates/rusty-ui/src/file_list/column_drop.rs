use gpui_kit::{App, Entity, Pixels, px};

use super::row::check_column;
use super::widths::{ColumnWidths, min_name};
use super::{KnownWidth, ListParts, ListState, SortBy};
use crate::tokens::Theme;

const NAME_MIN_CHARS: f32 = 12.;
const CHAR_ADVANCE_EM: f32 = 0.55;
const DROP_ORDER: [SortBy; 3] = [SortBy::Ext, SortBy::Date, SortBy::Size];

pub(super) fn name_min_width(t: &Theme, parts: ListParts) -> Pixels {
    let text = t.typography.body * (NAME_MIN_CHARS * CHAR_ADVANCE_EM);
    let grip_to_zone = t.spacing.md;
    let content = if parts.icons {
        text + t.spacing.icon + t.spacing.md + grip_to_zone
    } else {
        text + grip_to_zone
    };
    content.max(min_name(t))
}

pub(super) fn needed(t: &Theme, parts: ListParts, widths: ColumnWidths) -> Pixels {
    let s = &t.spacing;
    let frame = s.xs * 2. + t.rows.inset + s.xxl + check_column(t) + s.md;
    let columns = DROP_ORDER
        .into_iter()
        .filter(|by| parts.shows(*by))
        .map(|by| s.md + widths.of(by).unwrap_or_default())
        .fold(px(0.), |sum, width| sum + width);
    frame + name_min_width(t, parts) + columns
}

fn hide(parts: &mut ListParts, by: SortBy) {
    match by {
        SortBy::Ext => parts.ext = false,
        SortBy::Size => parts.size = false,
        SortBy::Date => parts.date = false,
        SortBy::Name => {}
    }
}

pub(super) fn fit(
    width: Option<Pixels>,
    parts: ListParts,
    (t, widths): (&Theme, ColumnWidths),
) -> ListParts {
    let Some(width) = width else {
        return parts;
    };
    let mut fitted = parts;
    for by in DROP_ORDER {
        if needed(t, fitted, widths) <= width {
            break;
        }
        hide(&mut fitted, by);
    }
    fitted
}

pub(super) fn list_width(
    state: &Entity<ListState>,
    known: Option<&KnownWidth>,
    cx: &App,
) -> Option<Pixels> {
    state
        .read(cx)
        .list_bounds
        .map(|bounds| bounds.size.width)
        .or_else(|| known.and_then(|known| known.get()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::Mode;

    fn theme() -> Theme {
        Theme::new(Mode::Dark)
    }

    fn widths() -> ColumnWidths {
        ColumnWidths::fallback(&theme())
    }

    fn at(width: Pixels, parts: ListParts) -> ListParts {
        fit(Some(width), parts, (&theme(), widths()))
    }

    fn shown(parts: ListParts) -> [bool; 3] {
        [parts.ext, parts.date, parts.size]
    }

    fn threshold(parts: ListParts) -> Pixels {
        needed(&theme(), parts, widths())
    }

    fn without(parts: ListParts, by: SortBy) -> ListParts {
        let mut parts = parts;
        hide(&mut parts, by);
        parts
    }

    #[test]
    fn before_the_first_measure_every_column_shows() {
        assert_eq!(
            fit(None, ListParts::default(), (&theme(), widths())),
            ListParts::default()
        );
    }

    #[test]
    fn a_wide_pane_keeps_every_column() {
        let all = ListParts::default();
        assert_eq!(at(threshold(all), all), all);
        assert_eq!(at(px(2000.), all), all);
    }

    #[test]
    fn ext_drops_first_just_below_the_full_width() {
        let all = ListParts::default();
        let fitted = at(threshold(all) - px(1.), all);
        assert_eq!(shown(fitted), [false, true, true]);
    }

    #[test]
    fn date_drops_second() {
        let all = ListParts::default();
        let no_ext = without(all, SortBy::Ext);
        assert_eq!(at(threshold(no_ext), all), no_ext);
        let fitted = at(threshold(no_ext) - px(1.), all);
        assert_eq!(shown(fitted), [false, false, true]);
    }

    #[test]
    fn size_drops_last() {
        let all = ListParts::default();
        let size_only = without(without(all, SortBy::Ext), SortBy::Date);
        assert_eq!(at(threshold(size_only), all), size_only);
        let fitted = at(threshold(size_only) - px(1.), all);
        assert_eq!(shown(fitted), [false, false, false]);
    }

    #[test]
    fn name_stays_in_a_tiny_pane_and_keeps_its_minimum() {
        let fitted = at(px(10.), ListParts::default());
        assert_eq!(shown(fitted), [false, false, false]);
        assert!(fitted.shows(SortBy::Name));
        let none = threshold(fitted);
        let size_only = without(without(ListParts::default(), SortBy::Ext), SortBy::Date);
        let step = theme().spacing.md + widths().of(SortBy::Size).unwrap_or_default();
        assert!((f32::from(threshold(size_only) - none) - f32::from(step)).abs() < 0.01);
        assert!(none > name_min_width(&theme(), fitted));
    }

    #[test]
    fn the_name_minimum_counts_the_gap_between_name_and_drag_zone() {
        let t = theme();
        let text = t.typography.body * (NAME_MIN_CHARS * CHAR_ADVANCE_EM);
        let no_icons = ListParts {
            icons: false,
            ..ListParts::default()
        };
        assert!(name_min_width(&t, no_icons) >= text + t.spacing.md);
        assert!(
            name_min_width(&t, ListParts::default()) >= text + t.spacing.icon + t.spacing.md * 2.
        );
    }

    #[test]
    fn the_name_keeps_at_least_120_px_at_100_percent() {
        let t = theme();
        for icons in [true, false] {
            let parts = ListParts {
                icons,
                ..ListParts::default()
            };
            assert!(name_min_width(&t, parts) >= px(120.));
        }
    }

    #[test]
    fn a_wider_user_column_drops_columns_sooner_and_never_overflows() {
        let all = ListParts::default();
        let wide = widths().with(SortBy::Date, px(300.));
        let width = threshold(all);
        let fitted = fit(Some(width), all, (&theme(), wide));
        assert!(!fitted.ext && fitted.size, "{fitted:?}");
        assert!(needed(&theme(), fitted, wide) <= width);
        let tiny = fit(Some(px(200.)), all, (&theme(), wide));
        assert_eq!(shown(tiny), [false, false, false]);
    }

    #[test]
    fn a_column_the_user_hid_stays_hidden() {
        let hidden = ListParts {
            date: false,
            ..ListParts::default()
        };
        assert_eq!(shown(at(px(2000.), hidden)), [true, false, true]);
        let fitted = at(threshold(hidden) - px(1.), hidden);
        assert_eq!(shown(fitted), [false, false, true]);
        assert!(!at(px(10.), hidden).date);
    }

    #[test]
    fn a_hidden_ext_does_not_use_up_a_drop_step() {
        let hidden = ListParts {
            ext: false,
            ..ListParts::default()
        };
        let fitted = at(threshold(hidden) - px(1.), hidden);
        assert_eq!(shown(fitted), [false, false, true]);
    }

    #[test]
    fn dropping_leaves_icons_alone() {
        let no_icons = ListParts {
            icons: false,
            ..ListParts::default()
        };
        assert!(!at(px(10.), no_icons).icons);
        assert!(at(px(10.), ListParts::default()).icons);
    }
}
