use navo_icons::spec::{IconSpec, Shape, Tone};
use navo_ui::window_view::{BarEdit, DraftAction, ItemDraft, ItemPath};

use super::{EditError, apply};
use crate::button_bar::model;

fn path(index: usize) -> ItemPath {
    ItemPath { index, child: None }
}

fn child_path(index: usize, child: usize) -> ItemPath {
    ItemPath {
        index,
        child: Some(child),
    }
}

fn navo_draft(name: &str, id: &str) -> ItemDraft {
    ItemDraft {
        name: name.to_owned(),
        action: DraftAction::Navo(id.to_owned()),
        icon: None,
    }
}

#[test]
fn edit_insert_keeps_comments() {
    let source = "\
# Top comment for my bar.

# Refresh button comment
[[item]]
name = \"Refresh\" # inline comment
navo = \"pane.refresh\"
";
    let edited = apply(
        source,
        &BarEdit::Insert {
            at: path(0),
            draft: navo_draft("Copy", "file.copy"),
        },
    )
    .expect("insert at 0 succeeds");

    assert!(edited.contains("# Top comment for my bar."), "{edited}");
    assert!(edited.contains("# Refresh button comment"), "{edited}");
    assert!(edited.contains("# inline comment"), "{edited}");

    let (items, _) = model::parse(&edited, None).expect("edited file parses");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].name, "Copy");
    assert_eq!(items[1].name, "Refresh");
}

#[test]
fn edit_update_keeps_unknown_order() {
    let source = "\
[[item]]
first_extra = \"keep-first\"
name = \"Old\" # keep comment
navo = \"pane.refresh\"
last_extra = 42
";
    let edited = apply(
        source,
        &BarEdit::Update {
            at: path(0),
            draft: ItemDraft {
                name: "New".to_owned(),
                action: DraftAction::User("deploy".to_owned()),
                icon: Some(IconSpec::Set {
                    set: "lucide".to_owned(),
                    name: "rocket".to_owned(),
                    tone: None,
                }),
            },
        },
    )
    .expect("update succeeds");

    let first_pos = edited.find("first_extra").expect("first_extra kept");
    let name_pos = edited.find("name = \"New\"").expect("name updated");
    let last_pos = edited.find("last_extra").expect("last_extra kept");
    assert!(first_pos < name_pos, "{edited}");
    assert!(name_pos < last_pos, "{edited}");
    assert!(edited.contains("# keep comment"), "{edited}");
    assert!(!edited.contains("navo ="), "{edited}");
    assert!(edited.contains("user = \"deploy\""), "{edited}");
    assert!(edited.contains("icon = \"lucide:rocket\""), "{edited}");
}

#[test]
fn edit_move_into_menu() {
    let source = "\
[[item]]
name = \"Refresh\"
navo = \"pane.refresh\"

[[item]]
name = \"Tools\"

[[item.menu]]
name = \"Terminal\"
user = \"term\"
";
    let edited = apply(
        source,
        &BarEdit::Move {
            from: path(0),
            to: child_path(1, 0),
        },
    )
    .expect("move into menu succeeds");

    let (items, _) = model::parse(&edited, None).expect("edited file parses");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "Tools");
    let model::ParsedKind::Menu(children) = &items[0].kind else {
        panic!("expected menu, got {:?}", items[0].kind);
    };
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].name, "Refresh");
    assert_eq!(children[1].name, "Terminal");
}

#[test]
fn edit_remove_last_child_removes_menu_array() {
    let source = "\
[[item]]
name = \"Tools\"

[[item.menu]]
name = \"Only\"
navo = \"pane.refresh\"
";
    let edited = apply(
        source,
        &BarEdit::Remove {
            at: child_path(0, 0),
        },
    )
    .expect("remove last child succeeds");

    assert!(!edited.contains("menu"), "{edited}");
    assert!(edited.contains("name = \"Tools\""), "{edited}");
}

#[test]
fn edit_duplicate_names_copy() {
    let source = "\
[[item]]
name = \"Build\"
program = \"cargo\"
args = \"build\"
";
    let edited = apply(source, &BarEdit::Duplicate { at: path(0) }).expect("duplicate succeeds");

    let (items, _) = model::parse(&edited, None).expect("edited file parses");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].name, "Build");
    assert_eq!(items[1].name, "Build copy");
}

#[test]
fn edit_icon_table_is_inline() {
    let edited = apply(
        "",
        &BarEdit::Insert {
            at: path(0),
            draft: ItemDraft {
                name: "Deploy".to_owned(),
                action: DraftAction::Navo("pane.refresh".to_owned()),
                icon: Some(IconSpec::Letters {
                    letters: "DP".to_owned(),
                    shape: Shape::Circle,
                    tone: Tone::Warn,
                }),
            },
        },
    )
    .expect("insert with table icon succeeds");

    assert!(
        edited.starts_with("# Navo button bar. Each [[item]] is one button, separator or menu.\n"),
        "{edited}"
    );
    assert!(edited.contains("icon = {"), "{edited}");
    assert!(!edited.contains("[item.icon]"), "{edited}");

    let (items, _) = model::parse(&edited, None).expect("edited file parses");
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].icon,
        Some(IconSpec::Letters {
            letters: "DP".to_owned(),
            shape: Shape::Circle,
            tone: Tone::Warn,
        })
    );
}

#[test]
fn edit_out_of_range_is_no_item() {
    let source = "\
[[item]]
name = \"One\"
navo = \"pane.refresh\"
";
    let bad = path(5);
    assert_eq!(
        apply(source, &BarEdit::Remove { at: bad }),
        Err(EditError::NoItem(bad))
    );
    assert_eq!(
        apply(
            source,
            &BarEdit::Insert {
                at: bad,
                draft: navo_draft("X", "pane.refresh"),
            }
        ),
        Err(EditError::NoItem(bad))
    );
    let bad_child = child_path(0, 2);
    assert_eq!(
        apply(source, &BarEdit::Remove { at: bad_child }),
        Err(EditError::NoItem(bad_child))
    );
}

#[test]
fn edit_rejects_nested_menus() {
    let source = "\
[[item]]
name = \"Tools\"

[[item.menu]]
name = \"Term\"
user = \"term\"

[[item]]
name = \"Other\"

[[item.menu]]
name = \"Build\"
user = \"build\"
";
    let nested_insert = apply(
        source,
        &BarEdit::Insert {
            at: child_path(0, 0),
            draft: ItemDraft {
                name: "Sub".to_owned(),
                action: DraftAction::Menu,
                icon: None,
            },
        },
    );
    assert_eq!(nested_insert, Err(EditError::BadTarget));

    let nested_move = apply(
        source,
        &BarEdit::Move {
            from: path(1),
            to: child_path(0, 0),
        },
    );
    assert_eq!(nested_move, Err(EditError::BadTarget));
}

#[test]
fn edit_move_out_of_menu_and_update_program_separator() {
    let source = "\
[[item]]
name = \"Tools\"

[[item.menu]]
name = \"Only\"
navo = \"pane.refresh\"
";
    let moved = apply(
        source,
        &BarEdit::Move {
            from: child_path(0, 0),
            to: path(1),
        },
    )
    .expect("move out of menu succeeds");
    assert!(!moved.contains("menu"), "{moved}");

    let updated = apply(
        &moved,
        &BarEdit::Update {
            at: path(1),
            draft: ItemDraft {
                name: "Run".to_owned(),
                action: DraftAction::Program {
                    program: "cargo".to_owned(),
                    args: String::new(),
                    start_in: "/tmp".to_owned(),
                    hidden_console: true,
                },
                icon: None,
            },
        },
    )
    .expect("update to program succeeds");
    assert!(updated.contains("program = \"cargo\""), "{updated}");
    assert!(!updated.contains("args ="), "{updated}");
    assert!(updated.contains("start-in = \"/tmp\""), "{updated}");
    assert!(updated.contains("console = \"hide\""), "{updated}");

    let sep = apply(
        &updated,
        &BarEdit::Update {
            at: path(1),
            draft: ItemDraft {
                name: "Ignored".to_owned(),
                action: DraftAction::Separator,
                icon: None,
            },
        },
    )
    .expect("update to separator succeeds");
    assert!(sep.contains("separator = true"), "{sep}");
    assert!(!sep.contains("program ="), "{sep}");
    assert!(!sep.contains("start-in ="), "{sep}");
    assert!(!sep.contains("console ="), "{sep}");
}

#[test]
fn edit_parse_error_and_menu_update_preserves_children() {
    assert!(matches!(
        apply("[[item\n", &BarEdit::Remove { at: path(0) }),
        Err(EditError::Parse(_))
    ));

    let source = "\
[[item]]
name = \"Tools\"

[[item.menu]]
name = \"Term\"
user = \"term\"
";
    let updated = apply(
        source,
        &BarEdit::Update {
            at: path(0),
            draft: ItemDraft {
                name: "Dev Tools".to_owned(),
                action: DraftAction::Menu,
                icon: None,
            },
        },
    )
    .expect("menu update keeps children");
    let (items, _) = model::parse(&updated, None).expect("parses");
    assert_eq!(items[0].name, "Dev Tools");
    let model::ParsedKind::Menu(children) = &items[0].kind else {
        panic!("expected menu");
    };
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "Term");
}
