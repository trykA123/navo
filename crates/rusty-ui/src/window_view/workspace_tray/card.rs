use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::component::Selectable;
use gpui_kit::component::button::{Button as KitButton, ButtonVariants};
use gpui_kit::{FocusHandle, KeyDownEvent, RenderOnce, Role};

use super::foot::{Foot, card_foot, run as click};
use crate::prelude::*;
use crate::primitives::{IconButton, IconName, Kbd, icon};
use crate::tokens::{self, Theme};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayAction {
    Switch(usize),
    New,
    Close,
    Rename(usize),
    Move { n: usize, by: i8 },
    Delete(usize),
    Save,
    Discard(usize),
}

pub type OnTrayAction = Rc<dyn Fn(&TrayAction, &mut Window, &mut App)>;

pub struct TrayTile {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub label: SharedString,
}

pub struct TrayCard {
    pub name: SharedString,
    pub tiles: Vec<TrayTile>,
    pub active: bool,
    pub preview: bool,
    pub focus: Option<FocusHandle>,
    pub rename: Option<AnyElement>,
}

#[derive(IntoElement)]
pub struct WorkspaceTray {
    cards: Vec<TrayCard>,
    new_preview: bool,
    on_action: OnTrayAction,
}

impl WorkspaceTray {
    pub fn new(
        cards: Vec<TrayCard>,
        on_action: impl Fn(&TrayAction, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            cards,
            new_preview: false,
            on_action: Rc::new(on_action),
        }
    }

    #[must_use]
    pub fn new_preview(mut self, is_previewed: bool) -> Self {
        self.new_preview = is_previewed;
        self
    }
}

fn miniature(t: &Theme, tiles: &[TrayTile]) -> Div {
    div()
        .relative()
        .w_full()
        .h(t.spacing.xxl * 5.)
        .children(tiles.iter().map(|tile| {
            div()
                .absolute()
                .left(relative(tile.x))
                .top(relative(tile.y))
                .w(relative(tile.width))
                .h(relative(tile.height))
                .p(t.spacing.xxs)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size_full()
                        .px(t.spacing.xs)
                        .rounded(t.radii.sm)
                        .border_1()
                        .border_color(t.surfaces.card_edge)
                        .bg(t.surfaces.field)
                        .text_size(t.typography.small)
                        .text_color(t.colors.muted)
                        .child(div().min_w_0().truncate().child(tile.label.clone())),
                )
        }))
}

fn metadata(t: &Theme, n: usize, card: &TrayCard) -> Div {
    let open = rusty_l10n::tr("workspace-open");
    div()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .w_full()
        .children(Kbd::new(("workspace-key", n), &n.to_string()))
        .child(div().flex_1().min_w_0().truncate().child(card.name.clone()))
        .when(card.active, |row| {
            row.child(
                div()
                    .id("workspace-open")
                    .role(Role::Label)
                    .aria_label(open.clone())
                    .test_support()
                    .flex_none()
                    .text_size(t.typography.small)
                    .text_color(t.colors.accent_text)
                    .child(open),
            )
        })
}

pub(super) fn card_width(t: &Theme) -> Pixels {
    t.spacing.xxl * 14.
}

fn card_main(t: &Theme, n: usize, card: &TrayCard, act: &OnTrayAction) -> AnyElement {
    KitButton::new(("workspace-switch", n))
        .ghost()
        .accessibility_label(card.name.clone())
        .w_full()
        .flex_1()
        .min_h_0()
        .p(t.spacing.xs)
        .rounded(t.radii.sm)
        .text_color(t.colors.text)
        .selected(card.preview)
        .on_click(click(act, TrayAction::Switch(n)))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(t.spacing.md)
                .size_full()
                .child(miniature(t, &card.tiles))
                .child(metadata(t, n, card)),
        )
        .into_any_element()
}

fn renaming(t: &Theme, card: &TrayCard, row: AnyElement) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.md)
        .w_full()
        .flex_1()
        .min_h_0()
        .p(t.spacing.xs)
        .child(miniature(t, &card.tiles))
        .child(row)
        .into_any_element()
}

fn card_frame(t: &Theme, id: impl Into<ElementId>, is_marked: bool) -> gpui_kit::Stateful<Div> {
    div()
        .id(id)
        .role(Role::Group)
        .aria_selected(is_marked)
        .flex()
        .flex_col()
        .flex_none()
        .gap(t.spacing.sm)
        .w(card_width(t))
        .p(t.spacing.sm)
        .overflow_hidden()
        .rounded(t.radii.card)
        .border_1()
        .border_color(if is_marked {
            t.colors.accent
        } else {
            t.surfaces.card_edge
        })
        .bg(t.surfaces.card)
        .text_color(t.colors.text)
}

#[derive(Clone, Copy)]
struct Slot {
    n: usize,
    count: usize,
}

fn workspace_card(
    t: &Theme,
    Slot { n, count }: Slot,
    mut card: TrayCard,
    act: &OnTrayAction,
) -> AnyElement {
    let main = match card.rename.take() {
        Some(row) => renaming(t, &card, row),
        None => card_main(t, n, &card, act),
    };
    let foot = Foot {
        n,
        name: card.name.as_ref(),
        count,
    };
    let rename = act.clone();
    card_frame(t, ("workspace-card", n), card.preview)
        .test_support()
        .when_some(card.focus.as_ref(), |frame, focus| frame.track_focus(focus))
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            if event.keystroke.key == "f2" && !event.keystroke.modifiers.modified() {
                cx.stop_propagation();
                rename(&TrayAction::Rename(n), window, cx);
            }
        })
        .child(main)
        .child(card_foot(t, &foot, act))
        .into_any_element()
}

fn new_card(t: &Theme, is_previewed: bool, act: &OnTrayAction) -> AnyElement {
    let label = rusty_l10n::tr("workspace-new-from-layout");
    card_frame(t, "workspace-new", is_previewed)
        .test_support()
        .child(
            KitButton::new("workspace-new-switch")
                .ghost()
                .accessibility_label(label.clone())
                .size_full()
                .p(t.spacing.xs)
                .rounded(t.radii.sm)
                .text_color(t.colors.text)
                .selected(is_previewed)
                .on_click(click(act, TrayAction::New))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(t.spacing.md)
                        .size_full()
                        .child(icon(IconName::Plus, t.spacing.icon, t.colors.muted))
                        .child(label)
                        .children(Kbd::new("workspace-new-key", "alt-n")),
                ),
        )
        .into_any_element()
}

fn header(t: &Theme, act: &OnTrayAction) -> impl IntoElement {
    div()
        .id("workspace-tray-header")
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .child(
            div()
                .id("workspace-tray-title")
                .test_support()
                .flex_none()
                .font_weight(FontWeight::SEMIBOLD)
                .child(rusty_l10n::tr("workspace-tray-title")),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(rusty_l10n::tr("workspace-tray-hint")),
        )
        .child(
            IconButton::new(
                "workspace-close",
                IconName::X,
                rusty_l10n::tr("workspace-tray-close"),
            )
            .on_click(click(act, TrayAction::Close)),
        )
}

impl RenderOnce for WorkspaceTray {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let count = self.cards.len();
        let act = self.on_action;
        let cards = self
            .cards
            .into_iter()
            .enumerate()
            .map(|(index, card)| {
                let slot = Slot {
                    n: index + 1,
                    count,
                };
                workspace_card(&t, slot, card, &act)
            })
            .collect::<Vec<_>>();
        div()
            .id("workspace-tray")
            .role(Role::Group)
            .aria_label(rusty_l10n::tr("workspace-tray-title"))
            .test_support()
            .occlude()
            .flex()
            .flex_col()
            .gap(t.spacing.md)
            .w_full()
            .p(t.spacing.xl)
            .bg(t.surfaces.sheet)
            .border_t_1()
            .border_color(t.surfaces.card_edge)
            .shadow(t.shadows.up.clone())
            .child(header(&t, &act))
            .child(
                div()
                    .id("workspace-cards")
                    .test_support()
                    .flex()
                    .min_w_0()
                    .w_full()
                    .gap(t.spacing.md)
                    .overflow_x_scroll()
                    .children(cards)
                    .child(new_card(&t, self.new_preview, &act)),
            )
    }
}
