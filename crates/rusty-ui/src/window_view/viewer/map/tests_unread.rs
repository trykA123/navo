use super::nav::{NavItem, items, selectable};
use super::select::Table;
use super::tests::{SAMPLE, loaded_from};
use super::text::{cells, heads, status};

fn texts(text: &str) -> Vec<String> {
    status(&loaded_from(text))
        .into_iter()
        .map(|check| check.text)
        .collect()
}

fn with_parts(titles: &[&str]) -> String {
    let parts: String = titles
        .iter()
        .map(|title| format!("* {title}\n======\ninvented line\n"))
        .collect();
    format!("{SAMPLE}{parts}")
}

fn without_sizes() -> String {
    let sample = SAMPLE.replace(
        "| Name | Address | Size | Space |",
        "| Name | Address | Space |",
    );
    sample
        .lines()
        .map(
            |line| match line.split(" | ").collect::<Vec<_>>().as_slice() {
                [name, address, _, space] if line.contains("0x8") || line.contains("0x7") => {
                    format!("{name} | {address} | {space}")
                }
                _ => line.to_owned(),
            },
        )
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_status_lists_the_first_three_unread_titles_and_counts_the_rest() {
    let few = texts(&with_parts(&["Invented Alpha", "Invented Beta"]));
    assert!(
        few.contains(&"Not read: Tool and Invocation, Invented Alpha, Invented Beta".to_owned()),
        "{few:?}"
    );
    let many = texts(&with_parts(&[
        "Invented Alpha",
        "Invented Beta",
        "Invented Gamma",
    ]));
    assert!(
        many.contains(
            &"Not read: Tool and Invocation, Invented Alpha, Invented Beta +1 more".to_owned()
        ),
        "{many:?}"
    );
}

#[test]
fn the_status_says_when_the_map_lists_no_symbol_sizes() {
    assert!(!texts(SAMPLE).contains(&"This map lists no symbol sizes".to_owned()));
    let bare = texts(&without_sizes());
    assert!(
        bare.contains(&"This map lists no symbol sizes".to_owned()),
        "{bare:?}"
    );
}

#[test]
fn the_symbols_table_drops_size_only_when_the_map_lacks_sizes() {
    let names = |text: &str| {
        let loaded = loaded_from(text);
        let heads: Vec<_> = heads(&loaded, Table::Symbols)
            .iter()
            .map(|h| h.name)
            .collect();
        (heads, cells(&loaded, Table::Symbols, 0))
    };
    let (with, row) = names(SAMPLE);
    assert_eq!(with, ["Symbol", "Address", "Size", "Space"]);
    assert_eq!(row.len(), 4);
    let (without, row) = names(&without_sizes());
    assert_eq!(without, ["Symbol", "Address", "Space"]);
    assert_eq!(row.len(), 3, "{row:?}");
}

#[test]
fn the_navigator_lists_every_unread_title_after_the_selectable_items() {
    let loaded = loaded_from(&with_parts(&["Invented Alpha", "Invented Beta"]));
    let items = items(&loaded);
    let at = items
        .iter()
        .position(|item| *item == NavItem::NotRead)
        .expect("a Not read group");
    assert_eq!(selectable(&items), at);
    assert_eq!(items.len() - at - 1, loaded.model.skipped_parts.len());
    assert!(
        items[at + 1..]
            .iter()
            .all(|item| matches!(item, NavItem::Unread(_)))
    );
    assert!(items[at..].iter().all(|item| item.scope(&loaded).is_none()));
}

#[test]
fn a_map_with_everything_read_has_no_not_read_group() {
    let model = SAMPLE.replace("* Tool and Invocation\ninvented invocation\n", "");
    let loaded = loaded_from(&model);
    assert!(loaded.model.skipped_parts.is_empty());
    let items = items(&loaded);
    assert_eq!(selectable(&items), items.len());
}
