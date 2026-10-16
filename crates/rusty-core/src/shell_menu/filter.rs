use std::ops::Range;

use super::{ItemRow, MenuItem, Row, ShellMenu};

pub(super) fn matches(menu: &ShellMenu, query: &str) -> Vec<Row> {
    let mut walk = Walk {
        needle: query.trim().chars().flat_map(char::to_lowercase).collect(),
        path: Vec::new(),
        crumb: Vec::new(),
        hits: Vec::new(),
    };
    walk.visit(&menu.items, false);
    let mut hits = walk.hits;
    hits.sort_by_key(|(is_ferrite, row)| (!row.enabled, !is_ferrite));
    hits.into_iter().map(|(_, row)| Row::Item(row)).collect()
}

struct Walk {
    needle: Vec<char>,
    path: Vec<usize>,
    crumb: Vec<String>,
    hits: Vec<(bool, ItemRow)>,
}

impl Walk {
    fn visit(&mut self, items: &[MenuItem], parent_hidden: bool) {
        for (index, item) in items.iter().enumerate() {
            if item.is_separator() {
                continue;
            }
            self.path.push(index);
            let hidden = parent_hidden || item.hidden;
            if let Some(range) = find(&item.text, &self.needle) {
                let row = ItemRow {
                    crumb: self.crumb.clone(),
                    matched: Some(range),
                    hidden,
                    ..ItemRow::new(self.path.clone(), item)
                };
                self.hits.push((item.is_ferrite(), row));
            }
            if let Some(children) = item.children() {
                self.crumb.push(item.text.clone());
                self.visit(children, hidden);
                self.crumb.pop();
            }
            self.path.pop();
        }
    }
}

fn find(text: &str, needle: &[char]) -> Option<Range<usize>> {
    if needle.is_empty() {
        return None;
    }
    text.char_indices()
        .find_map(|(start, _)| match_at(&text[start..], needle).map(|end| start..start + end))
}

fn match_at(rest: &str, needle: &[char]) -> Option<usize> {
    let mut remaining = needle;
    for (at, c) in rest.char_indices() {
        for lower in c.to_lowercase() {
            match remaining.split_first() {
                Some((want, tail)) if *want == lower => remaining = tail,
                Some(_) => return None,
                None => break,
            }
        }
        if remaining.is_empty() {
            return Some(at + c.len_utf8());
        }
    }
    None
}
