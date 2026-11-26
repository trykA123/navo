use crate::px;
use crate::tokens::{Density, Mode, Theme};

use super::{Slot, fit, metrics};

#[test]
fn fit_shows_all_when_room() {
    let slots = [Slot::Button(28.0), Slot::Separator, Slot::Button(28.0)];
    // Exact width: 28 + 2 + 9 + 2 + 28 = 69.
    assert_eq!(fit(&slots, 69.0, 28.0, 9.0, 2.0), 3);
    assert_eq!(fit(&slots, 120.0, 28.0, 9.0, 2.0), 3);
}

#[test]
fn fit_keeps_room_for_more() {
    let slots = [Slot::Button(28.0), Slot::Button(28.0), Slot::Button(28.0)];
    // All 3 need 28 + 2 + 28 + 2 + 28 = 88.
    // With room = 87, overflow is needed: 2 buttons + gap + more(28) = 28 + 2 + 28 + 2 + 28 = 88 > 87.
    // 1 button + gap + more(28) = 28 + 2 + 28 = 58 <= 87.
    assert_eq!(fit(&slots, 87.0, 28.0, 9.0, 2.0), 1);
    // With room = 57, not even 1 button + gap + more fits.
    assert_eq!(fit(&slots, 57.0, 28.0, 9.0, 2.0), 0);
    // With room = 58, 1 button + gap + more fits exactly.
    assert_eq!(fit(&slots, 58.0, 28.0, 9.0, 2.0), 1);
}

#[test]
fn fit_drops_trailing_separator() {
    let slots = [Slot::Button(28.0), Slot::Separator, Slot::Button(40.0)];
    // Total = 28 + 2 + 9 + 2 + 40 = 81. With room = 75, button(28) + sep(9) + more(28) = 69 fits,
    // but trailing separator before the cut is dropped.
    assert_eq!(fit(&slots, 75.0, 28.0, 9.0, 2.0), 1);

    let consecutive = [
        Slot::Button(28.0),
        Slot::Separator,
        Slot::Separator,
        Slot::Button(40.0),
    ];
    // Total = 28 + 2 + 9 + 2 + 9 + 2 + 40 = 92. With room = 90, both separators fit before cut
    // and both are dropped.
    assert_eq!(fit(&consecutive, 90.0, 28.0, 9.0, 2.0), 1);
}

#[test]
fn fit_zero_room_shows_none() {
    let slots = [Slot::Button(28.0), Slot::Button(28.0)];
    assert_eq!(fit(&slots, 0.0, 28.0, 9.0, 2.0), 0);
    assert_eq!(fit(&slots, -10.0, 28.0, 9.0, 2.0), 0);
    assert_eq!(fit(&[], 200.0, 28.0, 9.0, 2.0), 0);
}

#[test]
fn metrics_follow_density() {
    let spacing = Theme::new(Mode::Dark).spacing;

    let compact = metrics(&spacing, Density::Compact);
    let normal = metrics(&spacing, Density::Normal);
    let comfortable = metrics(&spacing, Density::Comfortable);

    assert_eq!(compact.height, spacing.button_bar_height - spacing.xs);
    assert_eq!(normal.height, spacing.button_bar_height);
    assert_eq!(comfortable.height, spacing.button_bar_height + spacing.xs);
    assert_eq!(compact.height, px(28.0));
    assert_eq!(normal.height, px(32.0));
    assert_eq!(comfortable.height, px(36.0));

    for bar in [compact, normal, comfortable] {
        assert_eq!(bar.button, spacing.button_size);
        assert_eq!(bar.icon, spacing.button_icon);
        assert_eq!(bar.separator, spacing.button_separator);
        assert_eq!(bar.gap, spacing.xxs);
    }
}
