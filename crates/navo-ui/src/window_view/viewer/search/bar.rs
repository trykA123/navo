use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{ClickEvent, Role, Stateful};

use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, TextField, TextFieldState};
use crate::tokens::Theme;
use crate::window_view::viewer::body::Key;

pub(in crate::window_view::viewer) const FIND_KEYS: [Key; 6] = [
    ("Enter", "Next"),
    ("Shift+Enter", "Previous"),
    ("Alt+Enter", "Find all"),
    ("Up", "History"),
    ("Alt+C", "Match case"),
    ("Esc", "Close search"),
];
pub(in crate::window_view::viewer) const LIST_KEYS: [Key; 3] = [
    ("Up/Down", "Move"),
    ("Enter", "Go there"),
    ("Esc", "Close list"),
];
pub(in crate::window_view::viewer) const GO_TO_KEYS: [Key; 2] =
    [("Enter", "Go there"), ("Esc", "Close")];
pub(in crate::window_view::viewer) const MARK_KEYS: [Key; 3] = [
    ("Ctrl+1…8", "Mark"),
    ("Ctrl+F2", "Bookmark"),
    ("F2", "Next bookmark"),
];

pub(in crate::window_view::viewer) struct BarLook<'a> {
    pub id: &'static str,
    pub input_id: &'static str,
    pub error_id: &'static str,
    pub label: &'static str,
    pub input: &'a Entity<TextFieldState>,
    pub is_find: bool,
    pub match_case: Option<bool>,
    pub count: Option<SharedString>,
    pub error: Option<SharedString>,
}

pub(in crate::window_view::viewer) fn parse_line(text: &str) -> Result<u64, String> {
    let trimmed = text.trim();
    trimmed
        .parse::<u64>()
        .ok()
        .filter(|line| *line > 0)
        .ok_or_else(|| format!("Not a line number: {trimmed}"))
}

fn small(t: &Theme, color: Hsla) -> Div {
    div()
        .flex_none()
        .text_size(t.typography.small)
        .text_color(color)
}

pub(in crate::window_view::viewer) fn bar_row(
    t: &Theme,
    look: BarLook<'_>,
    on_case: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> ObservedElement<Stateful<Div>> {
    let row = div()
        .id(look.id)
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.sm)
        .when(look.is_find, |row| row.key_context(super::FIND_CONTEXT))
        .child(small(t, t.colors.muted).child(look.label))
        .child(div().w(t.spacing.rail_width).child(TextField::new(
            look.input_id,
            look.input,
            look.label,
        )));
    row.when_some(look.count, |row, count| {
        row.child(
            small(t, t.colors.muted)
                .id("find-count")
                .role(Role::Status)
                .aria_label(count.clone())
                .test_support()
                .font_family(t.typography.numbers_font.clone())
                .font_features(t.typography.numbers_font_features.clone())
                .child(count),
        )
    })
    .when_some(look.match_case, |row, is_on| {
        let variant = if is_on {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Ghost
        };
        row.child(
            Button::new("match-case", "Match case")
                .variant(variant)
                .tooltip("Alt+C")
                .on_click(on_case),
        )
    })
    .when_some(look.error, |row, error| {
        row.child(
            small(t, t.colors.mark)
                .id(look.error_id)
                .role(Role::Alert)
                .aria_label(error.clone())
                .test_support()
                .child(error),
        )
    })
}
