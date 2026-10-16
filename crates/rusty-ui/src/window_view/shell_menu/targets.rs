use gpui_kit::{Pixels, Point, Window};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use rusty_core::shell_menu::{Group, ItemId, MenuItem};
use rusty_core::{Location, Tab};
use rusty_fs::FsEntry;

use super::provider::ScreenSpot;
use crate::format;

pub(super) const LOADING: &str = "Loading Windows items…";

pub(super) struct Targets {
    pub items: Vec<Location>,
    pub context: String,
}

pub(super) fn targets(tab: &Tab, details: &[FsEntry]) -> Targets {
    let marked = tab.selection().marked();
    let rows = if marked.is_empty() {
        vec![tab.view().cursor]
    } else {
        marked
    };
    let chosen: Vec<(Location, &FsEntry)> = rows
        .into_iter()
        .filter_map(|row| {
            let entry = details.get(usize::try_from(row).ok()?)?;
            let location = tab.location().join(&entry.name).ok()?;
            Some((location, entry))
        })
        .collect();
    let bytes = chosen.iter().map(|(_, entry)| entry.size).sum();
    let context = match chosen.as_slice() {
        [(_, entry)] => format!("{} · {}", entry.name.display(), format::size(bytes)),
        many => {
            let count = u64::try_from(many.len()).unwrap_or(u64::MAX);
            let items = format::count(count, "item", "items");
            format!("{items} · {}", format::size(bytes))
        }
    };
    Targets {
        items: chosen.into_iter().map(|(location, _)| location).collect(),
        context,
    }
}

pub(super) fn loading_item() -> MenuItem {
    let id = ItemId::Shell {
        command: 0,
        verb: None,
    };
    MenuItem {
        enabled: false,
        ..MenuItem::new(id, LOADING, Group::Other)
    }
}

pub(super) fn screen_spot(window: &Window, anchor: Point<Pixels>) -> ScreenSpot {
    let origin = window.bounds().origin;
    let scale = window.scale_factor();
    let to_screen = |logical: Pixels| (f32::from(logical) * scale).round() as i32;
    ScreenSpot {
        window: native_handle(window),
        x: to_screen(origin.x + anchor.x),
        y: to_screen(origin.y + anchor.y),
    }
}

fn native_handle(window: &Window) -> Option<isize> {
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}
