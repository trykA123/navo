use rusty_core::shell_menu::{
    ActionContext, Group, Icon, ItemId, ItemKind, MenuItem, ShellMenu, Submenu, ferrite_actions,
};
use rusty_ui::Hsla;

pub const SEVEN_ZIP: usize = 9;
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

fn hidden(item: MenuItem) -> MenuItem {
    MenuItem {
        hidden: true,
        ..item
    }
}

fn rare(item: MenuItem) -> MenuItem {
    MenuItem { rare: true, ..item }
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
        rare(shell(4, "Scan with Microsoft Defender…", Group::Extensions)),
        MenuItem::separator(Group::Extensions),
        submenu(shell(5, "&Send to", Group::Share), Submenu::Lazy),
        rare(shell(6, "Give access to", Group::Share)),
        hidden(shell(7, "Copy as path", Group::Share)),
        shell(8, "Cu&t\tCtrl+X", Group::Edit),
        shell(9, "&Copy\tCtrl+C", Group::Edit),
        shell(10, "&Delete\tDel", Group::Edit),
        hidden(shell(11, "Rena&me", Group::Edit)),
        rare(shell(12, "Restore previous versions", Group::Other)),
        rare(shell(13, "Pin to Quick access", Group::Other)),
        hidden(shell(14, "&Properties\tAlt+Enter", Group::Other)),
    ]
}

pub fn zip_menu(icon_color: Hsla) -> ShellMenu {
    let mut items = ferrite_actions(ActionContext {
        count: 1,
        one_per_pane: false,
    });
    items.extend(windows_items("ferrite-0.4.2-win64", badge(icon_color)));
    ShellMenu { items }
}

pub fn files_menu(icon_color: Hsla) -> ShellMenu {
    let mut items = ferrite_actions(ActionContext {
        count: 3,
        one_per_pane: false,
    });
    items.extend(windows_items("commander", badge(icon_color)));
    ShellMenu { items }
}
