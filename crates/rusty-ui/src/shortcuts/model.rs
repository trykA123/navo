use crate::commands::{CommandSpec, Keymap, all, normal_key, shown};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub spec: CommandSpec,
    pub keys: Vec<String>,
    pub is_changed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    Area(&'static str),
    Row(usize),
}

#[must_use]
pub fn rows(keymap: &Keymap) -> Vec<Row> {
    all()
        .map(|spec| Row {
            spec,
            keys: distinct(keymap.keys_of(spec.name)),
            is_changed: keymap.is_changed(spec.name),
        })
        .collect()
}

fn distinct<'a>(keys: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut seen: Vec<Option<String>> = Vec::new();
    let mut kept = Vec::new();
    for key in keys {
        let normal = normal_key(key);
        if !seen.contains(&normal) {
            seen.push(normal);
            kept.push(key.to_owned());
        }
    }
    kept
}

pub(super) fn area_of(spec: &CommandSpec) -> &'static str {
    spec.name
        .split_once('.')
        .map_or(spec.name, |(area, _)| area)
}

#[must_use]
pub fn lines(rows: &[Row], query: &str) -> Vec<Line> {
    let query = Query::new(query);
    let mut areas: Vec<&'static str> = Vec::new();
    for row in rows {
        let area = area_of(&row.spec);
        if !areas.contains(&area) {
            areas.push(area);
        }
    }
    let mut lines = Vec::new();
    for area in areas {
        let start = lines.len();
        lines.extend(
            rows.iter()
                .enumerate()
                .filter(|(_, row)| area_of(&row.spec) == area && query.matches(row))
                .map(|(index, _)| Line::Row(index)),
        );
        if lines.len() > start {
            lines.insert(start, Line::Area(area));
        }
    }
    lines
}

struct Query {
    text: String,
    key: Option<String>,
}

impl Query {
    fn new(text: &str) -> Self {
        let text = text.trim().to_lowercase();
        let key = normal_key(&text.replace('+', "-"));
        Self { text, key }
    }

    fn matches(&self, row: &Row) -> bool {
        if self.text.is_empty() {
            return true;
        }
        [row.spec.label, row.spec.name]
            .iter()
            .any(|word| word.to_lowercase().contains(&self.text))
            || row.keys.iter().any(|key| self.matches_key(key))
    }

    fn matches_key(&self, key: &str) -> bool {
        if self.key.is_some() && normal_key(key) == self.key {
            return true;
        }
        shown(key).is_some_and(|text| text.to_lowercase() == self.text)
    }
}

#[must_use]
pub fn area_label(area: &str) -> String {
    let spaced = area.replace('_', " ");
    let mut chars = spaced.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}
