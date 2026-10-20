use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, BoxShadow, ClickEvent, Div, ElementId, Entity, FontWeight, InteractiveElement,
    IntoElement, ParentElement, RenderOnce, Role, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Window, black, div, point, px,
};
use rusty_l10n::tr;

use super::tokens::Token;
use crate::primitives::{TextField, TextFieldState};
use crate::tokens::{self, Theme};

type PickHandler = Rc<dyn Fn(usize, &ClickEvent, &mut Window, &mut App)>;

const WIDTH: f32 = 340.;
const RADIUS: f32 = 10.;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuRow {
    pub text: SharedString,
    pub is_current: bool,
}

impl MenuRow {
    #[must_use]
    pub fn from_token(token: &Token) -> Self {
        Self {
            text: token.description().into(),
            is_current: false,
        }
    }
}

#[derive(IntoElement)]
pub struct FieldMenu {
    id: ElementId,
    filter: Option<Entity<TextFieldState>>,
    rows: Vec<MenuRow>,
    selected: usize,
    empty: SharedString,
    on_pick: Option<PickHandler>,
}

impl FieldMenu {
    pub fn new(id: impl Into<ElementId>, rows: Vec<MenuRow>) -> Self {
        Self {
            id: id.into(),
            filter: None,
            rows,
            selected: 0,
            empty: tr("rename-token-menu-empty").into(),
            on_pick: None,
        }
    }

    pub fn filter(mut self, field: &Entity<TextFieldState>) -> Self {
        self.filter = Some(field.clone());
        self
    }

    pub fn empty(mut self, text: impl Into<SharedString>) -> Self {
        self.empty = text.into();
        self
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    pub fn on_pick(
        mut self,
        handler: impl Fn(usize, &ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_pick = Some(Rc::new(handler));
        self
    }
}

fn shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: black().opacity(0.18),
        offset: point(px(0.), px(8.)),
        blur_radius: px(24.),
        spread_radius: px(0.),
        inset: false,
    }]
}

fn picked(
    handler: Option<PickHandler>,
    index: usize,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) {
    move |event, window, cx| {
        if let Some(handler) = &handler {
            handler(index, event, window, cx);
        }
    }
}

fn line(t: &Theme, index: usize, row: MenuRow) -> Stateful<Div> {
    let weight = if row.is_current {
        FontWeight::SEMIBOLD
    } else {
        FontWeight::NORMAL
    };
    div()
        .id(("field-menu-row", index))
        .role(Role::MenuItem)
        .aria_label(row.text.clone())
        .px(t.spacing.md)
        .py(t.spacing.xs)
        .rounded(t.radii.sm)
        .font_weight(weight)
        .truncate()
        .hover(|line| line.bg(t.colors.hover))
        .child(row.text)
}

impl RenderOnce for FieldMenu {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let filter = self.filter.map(|field| {
            div().pb(t.spacing.sm).child(TextField::new(
                "field-menu-filter",
                &field,
                tr("rename-token-menu-filter"),
            ))
        });
        let is_empty = self.rows.is_empty();
        let rows: Vec<_> = self
            .rows
            .into_iter()
            .enumerate()
            .map(|(index, row)| {
                line(&t, index, row)
                    .when(index == self.selected, |line| line.bg(t.colors.selection))
                    .on_click(picked(self.on_pick.clone(), index))
            })
            .collect();
        div()
            .id(self.id)
            .role(Role::Menu)
            .test_support()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(WIDTH))
            .p(t.spacing.sm)
            .rounded(px(RADIUS))
            .border_1()
            .border_color(t.colors.line)
            .bg(t.colors.surface)
            .shadow(shadow())
            .text_color(t.colors.text)
            .text_size(t.typography.small)
            .children(filter)
            .children(rows)
            .when(is_empty, |menu| {
                menu.child(
                    div()
                        .px(t.spacing.md)
                        .py(t.spacing.xs)
                        .text_color(t.colors.muted)
                        .child(self.empty),
                )
            })
    }
}
