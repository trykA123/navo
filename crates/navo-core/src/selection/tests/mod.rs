use crate::{Entry, Listing, Pattern, Selection};

mod bulk;
mod marks;
mod memory;
mod pattern;
mod props;
mod span;

fn entry(name: &str, is_folder: bool) -> Entry {
    Entry {
        name: name.to_owned(),
        is_folder,
    }
}

fn files(len: usize) -> Listing {
    Listing::new(
        (0..len)
            .map(|i| entry(&format!("f{i}.txt"), false))
            .collect(),
    )
}

fn mixed() -> Listing {
    Listing::new(vec![
        entry("docs", true),
        entry("a.rs", false),
        entry("b.txt", false),
        entry("src", true),
        entry("c.rs", false),
        entry("lib.rs", true),
    ])
}

fn pat(text: &str) -> Pattern {
    Pattern::parse(text).unwrap()
}

fn marked(rows: &[u64], len: u64) -> Selection {
    let mut selection = Selection::new(len);
    for &row in rows {
        selection.toggle(row);
    }
    selection
}
