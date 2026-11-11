//! The labelled bar on a hovered change: "← Copy to left | Copy to right →" (D29, V27).
//! It sits on the right of the change's first row and fades in over the theme's fade time.

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::{Animation, AnimationExt, Role};
use navo_l10n::tr;

use super::hooks::CopyDirection;
use crate::prelude::*;
use crate::primitives::{IconName, icon};
use crate::tokens::Theme;

/// Called with the hover state of one row of a change.
pub type OnHover = Rc<dyn Fn(bool, &mut App)>;
/// Called with the side a button asked to copy to.
pub type OnCopy = Rc<dyn Fn(CopyDirection, &mut Window, &mut App)>;

/// Called with the item id, change number and hover state of a Stream card's change.
pub type OnCardHover = Rc<dyn Fn(u64, u32, bool, &mut App)>;
/// Called with the item id, change number and side of a Stream card's copy button.
pub type OnCardCopy = Rc<dyn Fn(u64, u32, CopyDirection, &mut Window, &mut App)>;

/// A row of a copyable change: where the bar goes and how it reports.
pub struct BarRow {
    /// Unique among the rows on screen.
    pub id: (&'static str, usize),
    /// Whether this is the change's first row, the one that carries the bar.
    pub first: bool,
    /// Whether the pointer is over the change.
    pub hovered: bool,
    /// Whether the bar fades in; false when the system asks for reduced motion.
    pub animate: bool,
    /// Whether the bar sits on the row's left end: the right end shows a "Show bytes here"
    /// button (V41e), which the bar must not cover.
    pub bar_left: bool,
    pub on_hover: OnHover,
    pub on_copy: OnCopy,
}

/// What the Stream's cards need for their bars: the hovered change (item id, change number) and
/// what the pointer and the buttons call.
#[derive(Clone)]
pub struct CardBars {
    pub hovered: Option<(u64, u32)>,
    pub animate: bool,
    pub on_hover: OnCardHover,
    pub on_copy: OnCardCopy,
}

/// Wraps a drawn `row` of a copyable change so it reports hover, and adds the bar to the
/// first row while the change is hovered.
pub fn hunk_row(t: &Theme, row: AnyElement, bar: BarRow) -> AnyElement {
    let BarRow {
        id,
        first,
        hovered,
        animate,
        bar_left,
        on_hover,
        on_copy,
    } = bar;
    let wrapper = div()
        .id(id)
        .test_support()
        .relative()
        .size_full()
        .on_hover(move |on, _, cx| on_hover(*on, cx))
        .child(row);
    match hovered && first {
        true => wrapper.child(bar_view(t, (id.1, bar_left), on_copy, animate)),
        false => wrapper,
    }
    .into_any_element()
}

/// One button: an arrow and a label, in the order `arrow_first` gives.
fn button(
    t: &Theme,
    (name, id): (&'static str, usize),
    (glyph, label, arrow_first): (IconName, String, bool),
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let arrow = icon(glyph, t.spacing.icon, t.colors.muted);
    div()
        .id((name, id))
        .role(Role::Button)
        .aria_label(label.clone())
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.xs)
        .h_full()
        .px(t.spacing.md)
        .rounded(t.radii.sm)
        .cursor_pointer()
        .hover(|button| button.bg(t.colors.hover))
        .on_click(move |_, window, cx| on_click(window, cx))
        .when(arrow_first, |button| button.child(arrow))
        .child(SharedString::from(label))
}

/// The bar itself, faded in unless the system asks for reduced motion. It sits on the row's
/// right end, or on its left when `at_left` says the right end shows a "Show bytes here" button.
fn bar_view(t: &Theme, (id, at_left): (usize, bool), on_copy: OnCopy, animate: bool) -> AnyElement {
    let (to_left, to_right) = (on_copy.clone(), on_copy);
    let left = button(
        t,
        ("compare-copy-left", id),
        (IconName::ArrowLeft, tr("compare-copy-bar-left"), true),
        move |window, cx| to_left(CopyDirection::ToLeft, window, cx),
    );
    let right = button(
        t,
        ("compare-copy-right", id),
        (IconName::ArrowRight, tr("compare-copy-bar-right"), false),
        move |window, cx| to_right(CopyDirection::ToRight, window, cx),
    );
    let bar = div()
        .absolute()
        .top(px(1.))
        .bottom(px(1.))
        .flex()
        .items_center()
        .gap(t.spacing.xxs)
        .p(t.spacing.xxs)
        .rounded(t.radii.md)
        .bg(t.surfaces.card)
        .shadow(t.shadows.menu.clone())
        .font_family(t.typography.ui_font.clone())
        .text_size(t.typography.small)
        .text_color(t.colors.text)
        .child(left)
        .child(div().w(px(1.)).h_full().bg(t.colors.line))
        .child(right);
    let bar = match at_left {
        true => bar.left(t.spacing.md),
        false => bar.right(t.spacing.md),
    };
    if !animate {
        return bar.into_any_element();
    }
    bar.with_animation(
        ("compare-copy-bar", id),
        Animation::new(t.motion.fade).with_easing(ease_out_cubic),
        |bar, delta| bar.opacity(delta),
    )
    .into_any_element()
}
