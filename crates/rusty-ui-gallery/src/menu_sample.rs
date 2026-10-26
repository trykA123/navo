use rusty_core::shell_menu::{Group, Icon, ItemId, ItemKind, MenuItem, ShellMenu, Submenu};
use rusty_ui::Hsla;
use rusty_ui::file_list::SortKey;
use rusty_ui::window_view::{BackgroundLook, merged_background_menu, merged_file_menu};

pub const OPEN_WITH: usize = 1;
pub const SEVEN_ZIP: usize = 2;
pub const SEND_TO: usize = 5;
pub const NEW: usize = 9;
const ICON_SIDE: u32 = 16;

fn shell(command: u32, raw: &str, group: Group) -> MenuItem {
    MenuItem::new(
        ItemId::Shell {
            command,
            verb: None,
        },
        raw,
        group,
    )
}

fn submenu(item: MenuItem, submenu: Submenu) -> MenuItem {
    MenuItem {
        kind: ItemKind::Submenu(submenu),
        ..item
    }
}

fn badge(color: Hsla) -> Icon {
    let rgb = color.to_rgb();
    let channel = |value: f32| (value * 255.).round() as u8;
    let pixel = [channel(rgb.r), channel(rgb.g), channel(rgb.b), 255];
    let last = ICON_SIDE - 1;
    let rgba = (0..ICON_SIDE * ICON_SIDE)
        .flat_map(|at| {
            let (x, y) = (at % ICON_SIDE, at / ICON_SIDE);
            let is_corner = (x == 0 || x == last) && (y == 0 || y == last);
            if is_corner { [0; 4] } else { pixel }
        })
        .collect();
    Icon {
        width: ICON_SIDE,
        height: ICON_SIDE,
        rgba,
    }
}

fn seven_zip(target: &str, icon: Icon) -> MenuItem {
    let group = Group::Extensions;
    let checksums = vec![
        shell(40, "CRC-32", group),
        shell(41, "SHA-256", group),
        shell(42, "*", group),
    ];
    let children = vec![
        shell(30, "Open archive", group),
        shell(31, "Extract files…", group),
        shell(32, "Extract Here", group),
        shell(33, &format!("Extract to \"{target}\\\""), group),
        MenuItem::separator(group),
        shell(34, "Test archive", group),
        MenuItem::separator(group),
        shell(35, "Add to archive…", group),
        shell(36, "Compress and email…", group),
        submenu(shell(37, "CRC SHA", group), Submenu::Filled(checksums)),
    ];
    MenuItem {
        icon: Some(icon),
        ..submenu(shell(3, "7-Zip", group), Submenu::Filled(children))
    }
}

fn windows_items(target: &str, icon: Icon) -> Vec<MenuItem> {
    vec![
        MenuItem {
            default: true,
            ..shell(1, "&Open", Group::Open)
        },
        submenu(shell(2, "Open wit&h", Group::Open), Submenu::Lazy),
        seven_zip(target, icon),
        shell(4, "Scan with Microsoft Defender…", Group::Extensions),
        MenuItem::separator(Group::Extensions),
        submenu(shell(5, "&Send to", Group::Share), Submenu::Lazy),
        submenu(shell(6, "Give access to", Group::Share), Submenu::Lazy),
        MenuItem::separator(Group::Share),
        shell(8, "Cu&t", Group::Edit),
        shell(9, "&Copy", Group::Edit),
        MenuItem::separator(Group::Edit),
        shell(15, "Create &shortcut", Group::Edit),
        shell(10, "&Delete", Group::Edit),
        shell(11, "Rena&me", Group::Edit),
        MenuItem::separator(Group::Other),
        shell(12, "Restore previous versions", Group::Other),
        shell(13, "Pin to Quick access", Group::Other),
        MenuItem::separator(Group::Other),
        properties(14),
    ]
}

fn properties(command: u32) -> MenuItem {
    MenuItem::new(
        ItemId::Shell {
            command,
            verb: Some("properties".to_owned()),
        },
        "&Properties",
        Group::Other,
    )
}

pub fn zip_menu(icon_color: Hsla) -> ShellMenu {
    merged_file_menu(&windows_items("ferrite-0.4.2-win64", badge(icon_color)))
}

pub fn files_menu(icon_color: Hsla) -> ShellMenu {
    merged_file_menu(&windows_items("commander", badge(icon_color)))
}

fn disabled(item: MenuItem) -> MenuItem {
    MenuItem {
        enabled: false,
        ..item
    }
}

fn folder_items() -> Vec<MenuItem> {
    let new_types = vec![
        shell(60, "&Folder", Group::Other),
        shell(61, "&Shortcut", Group::Other),
        MenuItem::separator(Group::Other),
        shell(62, "Bitmap image", Group::Other),
        shell(63, "Rich Text Document", Group::Other),
        shell(64, "Text Document", Group::Other),
        shell(65, "Compressed (zipped) Folder", Group::Other),
    ];
    vec![
        disabled(shell(22, "&Paste", Group::Edit)),
        disabled(shell(23, "Paste &shortcut", Group::Edit)),
        MenuItem::separator(Group::Edit),
        shell(21, "Open in Terminal", Group::Open),
        submenu(shell(24, "Give access to", Group::Share), Submenu::Lazy),
        MenuItem::separator(Group::Share),
        submenu(shell(20, "Ne&w", Group::Other), Submenu::Filled(new_types)),
        MenuItem::separator(Group::Other),
        properties(25),
    ]
}

pub fn folder_menu() -> ShellMenu {
    let look = BackgroundLook {
        sort: SortKey::default(),
        can_paste: false,
        has_properties: false,
    };
    merged_background_menu(&folder_items(), &look)
}
