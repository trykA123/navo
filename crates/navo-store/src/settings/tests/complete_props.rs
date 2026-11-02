use std::collections::BTreeMap;

use proptest::collection::vec;
use proptest::prelude::*;
use toml_edit::{DocumentMut, TableLike};

use super::props::settings;
use super::*;

fn leaves(text: &str) -> BTreeMap<String, String> {
    let table: toml::Table = toml::from_str(text).unwrap();
    let mut found = BTreeMap::new();
    flatten("", &table, &mut found);
    found
}

fn flatten(prefix: &str, table: &toml::Table, found: &mut BTreeMap<String, String>) {
    for (key, value) in table {
        let path = format!("{prefix}{key}");
        match value {
            toml::Value::Table(inner) => flatten(&format!("{path}."), inner, found),
            other => {
                found.insert(path, other.to_string());
            }
        }
    }
}

fn thinned(text: &str, keep: &[bool], note: bool) -> String {
    let mut document: DocumentMut = text.parse().unwrap();
    let mut at = 0;
    for (_, item) in document.as_table_mut().iter_mut() {
        let Some(table) = item.as_table_like_mut() else {
            continue;
        };
        let keys: Vec<String> = table.iter().map(|(key, _)| key.to_owned()).collect();
        for key in keys {
            if !keep[at % keep.len()] {
                table.remove(&key);
            }
            at += 1;
        }
        if note {
            add_unknown(table);
        }
    }
    document.to_string()
}

fn add_unknown(table: &mut dyn TableLike) {
    table.insert("zoom-level", toml_edit::value(true));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn completing_never_changes_a_value_already_in_the_file(
        file in settings(),
        defaults in settings(),
        keep in vec(any::<bool>(), 1..20),
        note in any::<bool>(),
    ) {
        let text = thinned(&to_toml(&file), &keep, note);
        let before = leaves(&text);
        let done = complete_text(&text, &defaults)?;
        let after = leaves(&done.text);
        for (key, value) in &before {
            prop_assert_eq!(after.get(key), Some(value), "{} in {}", key, done.text);
        }
        for (key, value) in &done.written {
            prop_assert_eq!(after.get(key), Some(value));
            prop_assert!(!before.contains_key(key));
        }
        prop_assert!(parse(&done.text).is_ok());
        prop_assert_eq!(complete_text(&done.text, &defaults)?.written, Vec::new());
    }
}
