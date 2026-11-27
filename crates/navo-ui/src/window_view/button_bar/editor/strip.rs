//! Interactive button strip inside the button bar editor: selection, drag-reorder, and add buttons.

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, AppContext, Context, Div, Empty, InteractiveElement, IntoElement, ParentElement,
    Render, Role, StatefulInteractiveElement, Styled, Window, div, px,
};
use navo_l10n::tr;

use super::actions::default_button_draft;
use crate::primitives::{Button, ButtonVariant};
use crate::tokens::{self, Theme};
use crate::window_view::NavoWindow;
use crate::window_view::button_bar::fit::{self, BarMetrics};
use crate::window_view::button_bar::icon::icon_element;
use crate::window_view::button_bar::state::ButtonBarEditorState;
use crate::window_view::button_bar::{ButtonAction, ButtonItem, DraftAction, ItemDraft, ItemPath};

const MSG_STRIP_LABEL: &str = "button-bar-editor-strip-label";
const MSG_ADD_BUTTON: &str = "button-bar-editor-add-button";
const MSG_ADD_SEPARATOR: &str = "button-bar-editor-add-separator";
const MSG_ADD_MENU: &str = "button-bar-editor-add-menu";
const MSG_SEPARATOR: &str = "button-bar-editor-separator";
const MSG_EMPTY: &str = "button-bar-editor-empty";
const MSG_DEFAULT_MENU: &str = "button-bar-editor-default-menu";

const SEPARATOR_TILE_WIDTH: f32 = 24.;
const TILE_MAX_LABEL_WIDTH: f32 = 120.;

#[derive(Clone, Copy)]
struct DraggedEditorTile(usize);

struct TileDragGhost;

impl Render for TileDragGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

impl NavoWindow {
    /// Builds the interactive strip of item tiles and add-item controls for the editor.
    pub(super) fn editor_strip(
        &self,
        editor: &ButtonBarEditorState,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let metrics = fit::metrics(&t.spacing, t.density);
        let tiles: Vec<AnyElement> = self
            .button_bar
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let selected = editor
                    .selected
                    .is_some_and(|path| path.index == index && path.child.is_none());
                self.editor_tile(index, item, selected, metrics, cx)
            })
            .collect();

        let tiles_row = div()
            .id("button-bar-editor-tiles")
            .test_support()
            .flex()
            .flex_1()
            .min_w_0()
            .flex_wrap()
            .items_center()
            .gap(t.spacing.xs)
            .when(tiles.is_empty(), |row| {
                row.child(
                    div()
                        .text_size(t.typography.small)
                        .text_color(t.colors.muted)
                        .child(tr(MSG_EMPTY)),
                )
            })
            .children(tiles);

        div()
            .id("button-bar-editor-strip")
            .role(Role::Group)
            .aria_label(tr(MSG_STRIP_LABEL))
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .gap(t.spacing.md)
            .px(t.spacing.xl)
            .py(t.spacing.md)
            .border_b_1()
            .border_color(t.surfaces.card_edge)
            .bg(t.colors.rail)
            .child(tiles_row)
            .child(self.editor_add_controls(t, cx))
            .into_any_element()
    }

    fn editor_tile(
        &self,
        index: usize,
        item: &ButtonItem,
        selected: bool,
        metrics: BarMetrics,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = tokens::current(cx).selected_content(selected);
        let hover = palette.colors.hover;
        let is_separator = matches!(item.action, ButtonAction::Separator);
        let label = if is_separator {
            tr(MSG_SEPARATOR)
        } else {
            item.name.clone()
        };

        let tile = div()
            .id(("button-bar-editor-tile", index))
            .role(Role::Button)
            .aria_label(label)
            .aria_selected(selected)
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .h(metrics.button)
            .rounded(palette.radii.sm)
            .border_1()
            .cursor_pointer()
            .when(selected, |d| {
                d.bg(palette.colors.selection)
                    .border_color(palette.colors.accent)
                    .text_color(palette.colors.on_selection)
            })
            .when(!selected, |d| {
                d.bg(palette.surfaces.card)
                    .border_color(palette.surfaces.card_edge)
                    .text_color(palette.colors.text)
                    .hover(move |s| s.bg(hover))
            })
            .on_drag(DraggedEditorTile(index), |_, _, _, cx| {
                cx.new(|_| TileDragGhost)
            })
            .on_drop(
                cx.listener(move |view, dragged: &DraggedEditorTile, _, cx| {
                    view.editor_move_item(dragged.0, index, cx);
                }),
            )
            .on_click(cx.listener(move |view, _, _, cx| {
                view.select_editor_item(ItemPath { index, child: None }, cx);
            }));

        if is_separator {
            return tile
                .w(px(SEPARATOR_TILE_WIDTH))
                .child(div().w(px(1.)).h(metrics.icon).bg(palette.colors.line))
                .into_any_element();
        }

        tile.px(palette.spacing.sm)
            .gap(palette.spacing.xs)
            .child(icon_element(&item.icon, metrics.icon, cx))
            .child(
                div()
                    .max_w(px(TILE_MAX_LABEL_WIDTH))
                    .truncate()
                    .text_size(palette.typography.small)
                    .child(item.name.clone()),
            )
            .into_any_element()
    }

    fn editor_add_controls(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let add_button = Button::new("button-bar-editor-add-button", tr(MSG_ADD_BUTTON))
            .variant(ButtonVariant::Secondary)
            .on_click(cx.listener(|view, _, _, cx| {
                view.editor_insert_item(default_button_draft(), cx);
            }));
        let add_separator = Button::new("button-bar-editor-add-separator", tr(MSG_ADD_SEPARATOR))
            .variant(ButtonVariant::Secondary)
            .on_click(cx.listener(|view, _, _, cx| {
                view.editor_insert_item(separator_draft(), cx);
            }));
        let add_menu = Button::new("button-bar-editor-add-menu", tr(MSG_ADD_MENU))
            .variant(ButtonVariant::Secondary)
            .on_click(cx.listener(|view, _, _, cx| {
                view.editor_insert_item(menu_draft(), cx);
            }));

        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.xs)
            .child(add_button)
            .child(add_separator)
            .child(add_menu)
    }
}

fn separator_draft() -> ItemDraft {
    ItemDraft {
        name: String::new(),
        action: DraftAction::Separator,
        icon: None,
    }
}

fn menu_draft() -> ItemDraft {
    ItemDraft {
        name: tr(MSG_DEFAULT_MENU),
        action: DraftAction::Menu,
        icon: None,
    }
}
