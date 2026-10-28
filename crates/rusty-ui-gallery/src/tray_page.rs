use rusty_ui::prelude::*;
use rusty_ui::tokens::Theme;
use rusty_ui::window_view::{TrayCard, TrayTile, WorkspaceTray};

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

fn card(name: &str, tiles: Vec<TrayTile>, draft: u16) -> TrayCard {
    TrayCard {
        name: name.to_owned().into(),
        tiles,
        active: false,
        preview: false,
        draft,
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
            2,
        )
    };
    let reports = card("Reports", vec![tile(0., 1., full, "Downloads")], 0);
    let logs = TrayCard {
        preview: true,
        ..card(
            "Trace logs",
            vec![
                tile(0., 0.5, full, "traces"),
                tile(0.5, 0.5, (0., 0.5), "photos"),
                tile(0.5, 0.5, (0.5, 0.5), "archive"),
            ],
            0,
        )
    };
    vec![main, reports, logs]
}

pub fn element(page: Page, t: &Theme) -> Option<AnyElement> {
    let width = match page {
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
