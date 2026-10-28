use rusty_ui::confirm_dialog::ConfirmDialog;
use rusty_ui::prelude::*;
use rusty_ui::prompt_dialog::PromptDialog;
use rusty_ui::tokens::Theme;
use rusty_ui::window_view::{
    LayoutEdit, LayoutSwitcher, TrayCard, TrayTile, WorkspaceTray, layout_ask_props,
    layout_presets, save_layout_props,
};

use crate::gallery::Page;

const WIDE: f32 = 1920.;
const PHONE: f32 = 390.;

fn tile(x: f32, width: f32, (y, height): (f32, f32), label: &str) -> TrayTile {
    TrayTile {
        x,
        y,
        width,
        height,
        label: label.to_owned().into(),
    }
}

fn card(name: &str, tiles: Vec<TrayTile>) -> TrayCard {
    TrayCard {
        name: name.to_owned().into(),
        tiles,
        active: false,
        preview: false,
        focus: None,
        rename: None,
    }
}

fn cards() -> Vec<TrayCard> {
    let full = (0., 1.);
    let main = TrayCard {
        active: true,
        ..card(
            "Firmware",
            vec![
                tile(0., 0.5, full, "commander"),
                tile(0.5, 0.5, full, "2026-10"),
            ],
        )
    };
    let reports = card("Reports", vec![tile(0., 1., full, "Downloads")]);
    let logs = TrayCard {
        preview: true,
        ..card(
            "Trace logs",
            vec![
                tile(0., 0.5, full, "traces"),
                tile(0.5, 0.5, (0., 0.5), "photos"),
                tile(0.5, 0.5, (0.5, 0.5), "archive"),
            ],
        )
    };
    vec![main, reports, logs]
}

pub fn element(page: Page, t: &Theme) -> Option<AnyElement> {
    let width = match page {
        Page::LayoutSwitcher
        | Page::LayoutSwitcherPhone
        | Page::LayoutSave
        | Page::LayoutRemove => return Some(layout_switcher(page, t)),
        Page::WorkspaceTray => WIDE,
        Page::WorkspaceTrayPhone => PHONE,
        _ => return None,
    };
    Some(
        div()
            .relative()
            .flex_none()
            .w(px(width))
            .h_full()
            .bg(t.colors.background)
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom(t.spacing.status_height + t.spacing.xs + t.spacing.md)
                    .child(WorkspaceTray::new(cards(), |_, _, _| {})),
            )
            .into_any_element(),
    )
}

fn preset_dialog(page: Page, t: &Theme) -> Option<AnyElement> {
    let dialog = match page {
        Page::LayoutSave => {
            PromptDialog::new("layout-save", save_layout_props(&user_presets(), None))
                .into_any_element()
        }
        Page::LayoutRemove => {
            let remove = LayoutEdit::Remove {
                name: "Review".to_owned(),
            };
            ConfirmDialog::new("layout-ask", layout_ask_props(&remove)?).into_any_element()
        }
        _ => return None,
    };
    Some(
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(t.surfaces.scrim)
            .child(dialog)
            .into_any_element(),
    )
}

fn layout_switcher(page: Page, t: &Theme) -> AnyElement {
    div()
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .flex_none()
        .w_full()
        .when(page == Page::LayoutSwitcherPhone, |sample| {
            sample.w(px(PHONE))
        })
        .h_full()
        .p(t.spacing.md)
        .bg(t.colors.background)
        .child(
            LayoutSwitcher::new(
                layout_presets(&user_presets()),
                rusty_core::Tiles::builtin()[0].tiles.clone(),
                |_, _, _| {},
            )
            .focused(if page == Page::LayoutRemove { 5 } else { 0 }),
        )
        .children(preset_dialog(page, t))
        .into_any_element()
}

fn user_presets() -> Vec<(SharedString, rusty_core::Tiles)> {
    [
        ("Review", "row(70, pane, pane)"),
        ("Logs", "col(40, pane, pane)"),
    ]
    .into_iter()
    .filter_map(|(name, tiles)| Some((name.into(), rusty_core::Tiles::parse(tiles).ok()?)))
    .collect()
}
