use gpui_kit::SharedString;

use super::layer::Entry;

const SHORTEST_RUN: usize = 3;

fn numbered(label: &str) -> Option<(&str, u32)> {
    let (prefix, number) = label.rsplit_once(' ')?;
    Some((prefix, number.parse().ok()?))
}

fn digit_key(entry: &Entry) -> Option<u32> {
    match entry.keys.as_slice() {
        [key] if key.len() == 1 => key.parse().ok(),
        _ => None,
    }
}

fn is_consecutive(values: &[u32]) -> bool {
    values.windows(2).all(|pair| pair[1] == pair[0] + 1)
}

fn run_at(entries: &[Entry], start: usize) -> Option<usize> {
    let (prefix, _) = numbered(&entries[start].label)?;
    let family = |entry: &Entry| numbered(&entry.label).is_some_and(|(p, _)| p == prefix);
    let len = entries[start..].iter().take_while(|e| family(e)).count();
    let members = &entries[start..start + len];
    let is_whole = entries.iter().filter(|e| family(e)).count() == len;
    let numbers: Option<Vec<u32>> = members
        .iter()
        .map(|e| Some(numbered(&e.label)?.1))
        .collect();
    let keys: Option<Vec<u32>> = members.iter().map(digit_key).collect();
    let is_run =
        numbers.is_some_and(|n| is_consecutive(&n)) && keys.is_some_and(|k| is_consecutive(&k));
    (len >= SHORTEST_RUN && is_whole && is_run).then_some(len)
}

fn fold(members: &[Entry]) -> Option<Entry> {
    let (first, last) = (members.first()?, members.last()?);
    let (prefix, from) = numbered(&first.label)?;
    let (_, to) = numbered(&last.label)?;
    let keys = format!("{}–{}", first.keys.first()?, last.keys.first()?);
    Some(Entry {
        command: first.command,
        label: SharedString::from(format!("{prefix} {from}–{to}")),
        keys: vec![keys.into()],
    })
}

pub(super) fn folded(entries: &[Entry]) -> Vec<Entry> {
    let mut out = Vec::with_capacity(entries.len());
    let mut at = 0;
    while at < entries.len() {
        let run = run_at(entries, at).and_then(|len| Some((len, fold(&entries[at..at + len])?)));
        match run {
            Some((len, entry)) => {
                out.push(entry);
                at += len;
            }
            None => {
                out.push(entries[at].clone());
                at += 1;
            }
        }
    }
    out
}
