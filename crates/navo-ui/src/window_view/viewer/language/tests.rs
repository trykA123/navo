//! Tests for the language picker model: the rows, the checked row and the id round trip.
//! They pass a registry to the picker, so none installs the process-wide one.

use std::collections::HashSet;
use std::sync::OnceLock;

use super::*;
use crate::window_view::viewer::syntax::{Pick, choose};
use navo_core::shell_menu::ItemKind;
use navo_syntax::{Registry, from_toml};

/// The built-in registry, built once for every test that wants the shipped languages.
fn builtin() -> &'static Registry {
    static BUILTIN: OnceLock<Registry> = OnceLock::new();
    BUILTIN.get_or_init(Registry::builtin)
}

/// The picker rows for `current` over the built-in languages.
fn picker(current: &LanguageChoice) -> ShellMenu {
    language_picker(current, "log", builtin())
}

/// The action rows of a menu, separators skipped.
fn actions(menu: &ShellMenu) -> Vec<&MenuItem> {
    let rows = menu.items.iter();
    rows.filter(|item| item.kind != ItemKind::Separator)
        .collect()
}

/// The ids of the action rows of a menu.
fn ids(menu: &ShellMenu) -> Vec<ItemId> {
    actions(menu)
        .into_iter()
        .map(|row| row.id.clone())
        .collect()
}

/// The text of the one checked row of the menu built for `current`.
fn checked_text(current: &LanguageChoice) -> String {
    let menu = picker(current);
    let checked: Vec<_> = menu.items.iter().filter(|item| item.checked).collect();
    assert_eq!(checked.len(), 1, "{current:?}");
    checked[0].text.clone()
}

#[test]
fn rows_run_auto_plain_log_separator_then_sorted_names() {
    let menu = picker(&LanguageChoice::Auto);
    assert_eq!(menu.items[0].text, "Auto (detected: log)");
    assert_eq!(menu.items[1].text, "Plain text");
    assert_eq!(menu.items[2].text, "Log");
    assert_eq!(menu.items[3].kind, ItemKind::Separator);
    let names: Vec<_> = menu.items[4..]
        .iter()
        .map(|row| row.text.as_str())
        .collect();
    assert!(
        names.windows(2).all(|pair| pair[0] < pair[1]),
        "sorted, no repeats"
    );
}

#[test]
fn xml_and_a_rule_name_appear_once() {
    let menu = picker(&LanguageChoice::Auto);
    let texts: Vec<_> = actions(&menu).iter().map(|row| row.text.clone()).collect();
    assert_eq!(texts.iter().filter(|text| *text == "xml").count(), 1);
    assert_eq!(texts.iter().filter(|text| *text == "a2l").count(), 1);
}

#[test]
fn only_the_current_choice_is_checked() {
    assert!(checked_text(&LanguageChoice::Auto).starts_with("Auto"));
    assert_eq!(checked_text(&LanguageChoice::Plain), "Plain text");
    assert_eq!(checked_text(&LanguageChoice::Named("log".into())), "Log");
    for name in ["xml", "a2l"] {
        assert_eq!(checked_text(&LanguageChoice::Named(name.into())), name);
    }
}

#[test]
fn choice_of_round_trips_every_row_to_its_own_row() {
    let menu = picker(&LanguageChoice::Auto);
    let all = ids(&menu);
    let unique: HashSet<_> = all.iter().collect();
    assert_eq!(all.len(), unique.len(), "every row id is unique");
    for (at, id) in all.iter().enumerate() {
        let ItemId::Action(id) = id else {
            panic!("a picker row is an action");
        };
        let back = picker(&choice_of(id));
        let checked: Vec<_> = ids(&back)
            .into_iter()
            .zip(actions(&back))
            .filter(|(_, row)| row.checked)
            .map(|(id, _)| id)
            .collect();
        assert_eq!(checked, [all[at].clone()], "row {at}");
    }
}

#[test]
fn rules_named_like_the_fixed_rows_keep_their_own_unique_ids() {
    let toml = |name: &str| {
        format!(
            "[[language]]\nname = \"{name}\"\nextensions = [\"zz{}\"]\n\
             [[language.keywords]]\nkind = \"keyword\"\nwords = [\"X\"]\n",
            name.len()
        )
    };
    let mut registry = Registry::builtin();
    let hostile = [
        "language.auto",
        "language.plain",
        "language.log",
        "lang:xml",
    ];
    for name in hostile {
        for language in from_toml(&toml(name)).expect("a hostile name still parses") {
            registry.add(language);
        }
    }
    let menu = language_picker(&LanguageChoice::Auto, "log", &registry);
    let all = ids(&menu);
    let unique: HashSet<_> = all.iter().collect();
    assert_eq!(unique.len(), all.len(), "no id is shared: {all:?}");
    for name in hostile {
        let wanted = LanguageChoice::Named(name.to_owned());
        let row = actions(&menu)
            .into_iter()
            .find(|row| row.text == name)
            .unwrap_or_else(|| panic!("a row for {name}"));
        let ItemId::Action(id) = &row.id else {
            panic!("an action row");
        };
        assert_eq!(choice_of(id), wanted, "{name} maps back to itself");
    }
}

#[test]
fn auto_names_what_the_detector_gives() {
    let auto = choose(&Pick {
        name: "a.json",
        head: b"{}",
        choice: &LanguageChoice::Auto,
        len: Some(2),
    });
    assert_eq!(auto.name(), "json");
}

#[test]
fn an_open_search_bar_hides_the_syntax_button_only_in_a_narrow_window() {
    assert!(bar_fits(false, px(480.)), "no bar, always room");
    assert!(!bar_fits(true, px(480.)), "minimum window with Find open");
    assert!(!bar_fits(true, px(999.)));
    assert!(bar_fits(true, px(1000.)));
    assert!(bar_fits(true, px(1440.)));
}
