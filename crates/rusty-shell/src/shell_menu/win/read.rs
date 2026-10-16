use windows::Win32::UI::Shell::{GCS_VERBW, IContextMenu};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMenuItemCount, GetMenuItemInfoW, HMENU, MENUITEMINFOW, MFS_CHECKED, MFS_DEFAULT,
    MFS_DISABLED, MFT_SEPARATOR, MIIM_BITMAP, MIIM_CHECKMARKS, MIIM_FTYPE, MIIM_ID, MIIM_STATE,
    MIIM_STRING, MIIM_SUBMENU,
};
use windows::core::{PSTR, PWSTR};

use super::icon;
use crate::shell_menu::classify::RawItem;

pub(super) const ID_FIRST: u32 = 1;
pub(super) const ID_LAST: u32 = 0x7FFF;

pub(super) struct Entry {
    pub raw: RawItem,
    pub submenu: Option<HMENU>,
}

pub(super) struct Verbs<'a> {
    pub menu: &'a IContextMenu,
    pub command_count: u32,
}

impl Verbs<'_> {
    pub(super) fn offset(&self, command: u32) -> Option<u32> {
        command
            .checked_sub(ID_FIRST)
            .filter(|offset| *offset < self.command_count)
    }

    fn verb(&self, command: u32) -> Option<String> {
        let offset = usize::try_from(self.offset(command)?).ok()?;
        let mut buf = [0u16; 256];
        // SAFETY: GCS_VERBW writes at most cchmax UTF-16 units into buf.
        unsafe {
            self.menu
                .GetCommandString(offset, GCS_VERBW, None, PSTR(buf.as_mut_ptr().cast()), 256)
                .ok()?;
        }
        let verb = from_wide(&buf);
        (!verb.is_empty()).then_some(verb)
    }
}

pub(super) fn read_level(hmenu: HMENU, verbs: &Verbs<'_>) -> Vec<Entry> {
    (0..item_count(hmenu))
        .map(|position| read_entry(hmenu, position, verbs))
        .collect()
}

pub(super) fn has_named_item(hmenu: HMENU) -> bool {
    (0..item_count(hmenu))
        .filter_map(|position| read_item(hmenu, position, MIIM_FTYPE | MIIM_STRING))
        .any(|(item, text)| !item.fType.contains(MFT_SEPARATOR) && !text.trim().is_empty())
}

fn read_entry(hmenu: HMENU, position: u32, verbs: &Verbs<'_>) -> Entry {
    let mask = MIIM_FTYPE
        | MIIM_STATE
        | MIIM_ID
        | MIIM_SUBMENU
        | MIIM_STRING
        | MIIM_BITMAP
        | MIIM_CHECKMARKS;
    let Some((item, text)) = read_item(hmenu, position, mask) else {
        return Entry {
            raw: unreadable(),
            submenu: None,
        };
    };
    let separator = item.fType.contains(MFT_SEPARATOR);
    let submenu = (!item.hSubMenu.is_invalid()).then_some(item.hSubMenu);
    let is_command = !separator && submenu.is_none();
    Entry {
        raw: RawItem {
            command: item.wID,
            verb: is_command.then(|| verbs.verb(item.wID)).flatten(),
            enabled: !item.fState.contains(MFS_DISABLED),
            checked: item.fState.contains(MFS_CHECKED),
            default: item.fState.contains(MFS_DEFAULT),
            separator,
            icon: (!separator).then(|| icon::of_item(&item)).flatten(),
            text,
        },
        submenu,
    }
}

fn unreadable() -> RawItem {
    RawItem {
        command: 0,
        text: String::new(),
        verb: None,
        enabled: false,
        checked: false,
        default: false,
        separator: false,
        icon: None,
    }
}

fn item_count(hmenu: HMENU) -> u32 {
    // SAFETY: read-only query on a menu handle; an invalid handle returns -1.
    let count = unsafe { GetMenuItemCount(Some(hmenu)) };
    u32::try_from(count).unwrap_or(0)
}

fn read_item(
    hmenu: HMENU,
    position: u32,
    mask: windows::Win32::UI::WindowsAndMessaging::MENU_ITEM_MASK,
) -> Option<(MENUITEMINFOW, String)> {
    let mut item = MENUITEMINFOW {
        cbSize: u32::try_from(size_of::<MENUITEMINFOW>()).ok()?,
        fMask: mask,
        ..Default::default()
    };
    // SAFETY: the first call sizes the text (dwTypeData null), the second fills a buffer of cch + 1.
    unsafe {
        GetMenuItemInfoW(hmenu, position, true, &mut item).ok()?;
        let mut buf = vec![0u16; item.cch as usize + 1];
        item.dwTypeData = PWSTR(buf.as_mut_ptr());
        item.cch = u32::try_from(buf.len()).ok()?;
        GetMenuItemInfoW(hmenu, position, true, &mut item).ok()?;
        item.dwTypeData = PWSTR::null();
        Some((item, from_wide(&buf)))
    }
}

fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|unit| *unit == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}
