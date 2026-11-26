use navo_icons::spec::{BrandColour, IconSpec, Shape, Tone};

use super::{ParsedItem, ParsedKind, parse};
use crate::user_commands::model::Console;

// BB-D3 example (with `args = "-d ."` for `wt.exe`, per the D10 terminal placeholder rule).
const EXAMPLE_TOML: &str = r#"
# Navo button bar. Each [[item]] is one button, separator or menu.
[[item]]
name = "Edit"
icon = "lucide:file-pen"
user = "Open in Notepad"          # a command from user-commands.toml

[[item]]
name = "Terminal here"
icon = "tabler:terminal-2"
program = "wt.exe"                # inline program: user-commands rules D7–D13
args = "-d ."
console = "hide"

[[item]]
separator = true

[[item]]
name = "Compare"
icon = { letters = "C", shape = "soft", tone = "accent" }
navo = "compare.with_other_pane"  # a Navo command id

[[item]]
name = "Tools"
icon = "lucide:archive"
  [[item.menu]]
  name = "Hash file"
  icon = { glyph = "hash", tone = "kind-code", dot = "ok" }
  program = "certutil.exe"
  args = "-hashfile {file} SHA256"
  [[item.menu]]
  name = "GitHub folder"
  icon = { set = "brands", name = "github", colour = "brand" }
  program = "explorer.exe"
  args = "{folder}"
"#;

#[test]
fn button_bar_reads_the_example_file() {
    let (items, warnings) = parse(EXAMPLE_TOML, None).expect("example file should parse");
    assert!(warnings.is_empty());
    assert_eq!(items.len(), 5);

    assert_eq!(items[0].name, "Edit");
    assert_eq!(
        items[0].icon,
        Some(IconSpec::Set {
            set: "lucide".to_owned(),
            name: "file-pen".to_owned(),
            tone: None,
        })
    );
    assert_eq!(
        items[0].kind,
        ParsedKind::User("Open in Notepad".to_owned())
    );

    assert_eq!(items[1].name, "Terminal here");
    assert_eq!(
        items[1].icon,
        Some(IconSpec::Set {
            set: "tabler".to_owned(),
            name: "terminal-2".to_owned(),
            tone: None,
        })
    );
    let ParsedKind::Program(cmd) = &items[1].kind else {
        panic!("expected Program, got {:?}", items[1].kind);
    };
    assert_eq!(cmd.program, "wt.exe");
    assert_eq!(cmd.console, Console::Hide);

    assert_eq!(items[2].name, "");
    assert_eq!(items[2].icon, None);
    assert_eq!(items[2].kind, ParsedKind::Separator);

    assert_eq!(items[3].name, "Compare");
    assert_eq!(
        items[3].icon,
        Some(IconSpec::Letters {
            letters: "C".to_owned(),
            shape: Shape::Soft,
            tone: Tone::Accent,
        })
    );
    assert_eq!(
        items[3].kind,
        ParsedKind::Navo("compare.with_other_pane".to_owned())
    );

    assert_tools_menu(&items[4]);
}

fn assert_tools_menu(item: &ParsedItem) {
    assert_eq!(item.name, "Tools");
    assert_eq!(
        item.icon,
        Some(IconSpec::Set {
            set: "lucide".to_owned(),
            name: "archive".to_owned(),
            tone: None,
        })
    );
    let ParsedKind::Menu(children) = &item.kind else {
        panic!("expected Menu, got {:?}", item.kind);
    };
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].name, "Hash file");
    assert_eq!(
        children[0].icon,
        Some(IconSpec::Glyph {
            glyph: "hash".to_owned(),
            tone: Tone::KindCode,
            dot: Some(Tone::Ok),
        })
    );
    assert!(matches!(children[0].kind, ParsedKind::Program(_)));

    assert_eq!(children[1].name, "GitHub folder");
    assert_eq!(
        children[1].icon,
        Some(IconSpec::Brand {
            name: "github".to_owned(),
            colour: BrandColour::Brand,
        })
    );
    assert!(matches!(children[1].kind, ParsedKind::Program(_)));
}

#[test]
fn button_bar_refuses_two_actions() {
    let text = r#"
[[item]]
name = "Both"
navo = "compare.with_other_pane"
user = "Open in Notepad"
"#;
    let errors = parse(text, None).expect_err("two actions must fail");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].line, 2);
}

#[test]
fn button_bar_refuses_key() {
    let text = r#"
[[item]]
name = "Compare"
navo = "compare.with_other_pane"
key = "ctrl-alt-c"
"#;
    let errors = parse(text, None).expect_err("key must be refused");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].line, 5);
    assert_eq!(
        errors[0].message,
        "a button has no key; bind a user command instead"
    );
}

#[test]
fn button_bar_refuses_nested_menu() {
    let text = r#"
[[item]]
name = "Outer"
  [[item.menu]]
  name = "Inner"
    [[item.menu.menu]]
    name = "Leaf"
    navo = "compare.with_other_pane"
"#;
    let errors = parse(text, None).expect_err("nested menu must be refused");
    assert!(
        errors.iter().any(|e| e.line == 4),
        "expected error on inner menu line 4, got {errors:?}"
    );
}

#[test]
fn button_bar_refuses_over_200_items() {
    let mut text = String::new();
    for i in 0..201 {
        text.push_str(&format!(
            "[[item]]\nname = \"Btn {i}\"\nnavo = \"compare.with_other_pane\"\n"
        ));
    }
    let errors = parse(&text, None).expect_err("over 200 items must be refused");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].line, 200 * 3 + 1);
}

#[test]
fn button_bar_program_errors_point_at_buttonbar_lines() {
    let text = r#"
[[item]]
name = "Edit"
user = "Open in Notepad"

[[item]]
name = "Bad shell"
program = "cmd.exe"
args = "/c echo {file}"
start-in = "C:\\"
"#;
    let errors = parse(text, None).expect_err("shell with placeholder must fail");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].line, 9);
}

#[test]
fn button_bar_separator_has_no_name() {
    let text = r#"
[[item]]
name = "Divider"
separator = true
"#;
    let errors = parse(text, None).expect_err("separator with name must fail");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].line, 3);
}

#[test]
fn button_bar_refuses_bad_navo_id() {
    for bad in ["compare", "Compare.pane", "a.b-c", "a..b", "_a.b", "a.b1"] {
        let text = format!("[[item]]\nname = \"Bad\"\nnavo = \"{bad}\"\n");
        let errors = parse(&text, None).expect_err(&format!("`{bad}` must fail"));
        assert_eq!(errors.len(), 1, "for `{bad}`: {errors:?}");
        assert_eq!(errors[0].line, 3, "for `{bad}`");
    }
}

#[test]
fn button_bar_icon_error_has_icon_line() {
    let text = r#"
[[item]]
name = "Compare"
icon = "not-a-valid-icon"
navo = "compare.with_other_pane"
"#;
    let errors = parse(text, None).expect_err("bad icon must fail");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].line, 4);
}

#[test]
fn button_bar_refuses_file_over_256_kib() {
    let text = " ".repeat(256 * 1024 + 1);
    let errors = parse(&text, None).expect_err("oversized file must fail");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].line, 1);
}

#[test]
fn button_bar_refuses_separator_with_icon() {
    let text = r#"
[[item]]
separator = true
icon = "lucide:star"
"#;
    let errors = parse(text, None).expect_err("separator with icon must fail");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].line, 4);
}

#[test]
fn button_bar_refuses_empty_menu_and_over_50_children() {
    let empty = "[[item]]\nname = \"Empty\"\nmenu = []\n";
    let errors = parse(empty, None).expect_err("empty menu must fail");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].line, 1);

    let mut big = String::from("[[item]]\nname = \"Big\"\n");
    for i in 0..51 {
        big.push_str(&format!(
            "[[item.menu]]\nname = \"Item {i}\"\nnavo = \"compare.with_other_pane\"\n"
        ));
    }
    let errors = parse(&big, None).expect_err("menu with 51 items must fail");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].line, 1);
}

#[test]
fn button_bar_refuses_unknown_field_and_bad_icon_type() {
    let unknown = "[[item]]\nname = \"A\"\nnavo = \"compare.with_other_pane\"\nextra = 1\n";
    let errors = parse(unknown, None).expect_err("unknown field must fail");
    assert_eq!(errors.len(), 1);

    let bad_icon = "[[item]]\nname = \"A\"\nicon = 42\nnavo = \"compare.with_other_pane\"\n";
    let errors = parse(bad_icon, None).expect_err("integer icon must fail");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].line, 3);
}
