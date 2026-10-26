use super::*;

mod actions;
mod filter;
mod props;
mod rows;
mod selection;
mod text;

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

fn submenu(command: u32, raw: &str, group: Group, children: Vec<MenuItem>) -> MenuItem {
    MenuItem {
        kind: ItemKind::Submenu(Submenu::Filled(children)),
        ..shell(command, raw, group)
    }
}

fn lazy(command: u32, raw: &str, group: Group) -> MenuItem {
    MenuItem {
        kind: ItemKind::Submenu(Submenu::Lazy),
        ..shell(command, raw, group)
    }
}

fn seven_zip() -> MenuItem {
    submenu(
        3,
        "7-Zip",
        Group::Extensions,
        vec![
            shell(30, "Open archive", Group::Extensions),
            shell(31, "Extract Here", Group::Extensions),
            MenuItem::separator(Group::Extensions),
            MenuItem {
                enabled: false,
                ..shell(32, "Extract to \"dist\\\"", Group::Extensions)
            },
        ],
    )
}

fn hidden_actions() -> Vec<MenuItem> {
    ferrite_actions()
        .into_iter()
        .map(|item| MenuItem {
            hidden: true,
            ..item
        })
        .collect()
}

const SEVEN_ZIP: usize = 2;
const SEND_TO: usize = 5;

fn zip_menu() -> ShellMenu {
    let mut items = vec![
        MenuItem {
            default: true,
            ..shell(1, "&Open", Group::Open)
        },
        lazy(2, "Open wit&h", Group::Open),
        seven_zip(),
        MenuItem {
            hidden: true,
            ..submenu(
                4,
                "CRC SHA",
                Group::Extensions,
                vec![shell(40, "SHA-256", Group::Extensions)],
            )
        },
        MenuItem::separator(Group::Extensions),
        lazy(5, "&Send to", Group::Share),
        MenuItem {
            hidden: true,
            ..shell(6, "Copy as path", Group::Share)
        },
        MenuItem::separator(Group::Share),
        shell(7, "Cu&t\tCtrl+X", Group::Edit),
        shell(8, "&Copy\tCtrl+C", Group::Edit),
        MenuItem::separator(Group::Edit),
        shell(9, "Create &shortcut", Group::Other),
        shell(10, "Restore previous versions", Group::Other),
    ];
    items.extend(hidden_actions());
    ShellMenu { items }
}

fn item_texts(rows: &[Row]) -> Vec<String> {
    rows.iter()
        .map(|row| match row {
            Row::Back { title } => format!("< {title}"),
            Row::Separator => "-".to_owned(),
            Row::Item(item) => item.breadcrumb(),
        })
        .collect()
}
