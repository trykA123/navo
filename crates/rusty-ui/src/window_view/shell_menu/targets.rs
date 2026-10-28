use gpui_kit::{Pixels, Point, Window};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use rusty_core::{Location, Tab};
use rusty_fs::FsEntry;

use super::provider::ScreenSpot;
use crate::format;

pub(super) struct Targets {
    pub items: Vec<Location>,
    pub context: String,
}

pub(super) fn targets(tab: &Tab, details: &[FsEntry], adds_cursor: bool) -> Targets {
    let mut rows = tab.selection().marked();
    if rows.is_empty() || adds_cursor {
        rows.push(tab.view().cursor);
        rows.sort_unstable();
        rows.dedup();
    }
    let chosen: Vec<(Location, &FsEntry)> = rows
        .into_iter()
        .filter_map(|row| {
            let entry = details.get(usize::try_from(row).ok()?)?;
            let location = tab.entry_location(&entry.name.to_text()).ok()?;
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

#[cfg(test)]
mod tests {
    use rusty_core::{AppState, Location, ProviderPath};
    use rusty_fs::{Attributes, EntryKind, FsEntry};

    #[test]
    fn context_menu_resolves_a_results_entry_under_its_base() {
        let state = AppState::new(Location::new(r"C:\shown").unwrap());
        let mut tab = state.active_tab().clone();
        tab.set_base(Some(Location::new(r"C:\base").unwrap()));
        let entry = FsEntry {
            name: ProviderPath::from_text(r"sub\a.txt"),
            kind: EntryKind::File,
            size: 1,
            modified_ms: None,
            attributes: Attributes::default(),
        };
        assert_eq!(
            super::targets(&tab, &[entry], false).items,
            [Location::new(r"C:\base\sub\a.txt").unwrap()]
        );
    }
}
