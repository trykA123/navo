#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Query {
    pub text: String,
    pub match_case: bool,
    pub whole_word: bool,
}

impl Query {
    pub(super) fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Hit {
    pub page: u32,
    pub start: u32,
    pub len: u32,
    pub y: f32,
}

impl Hit {
    pub(super) fn key(&self) -> (u32, u32) {
        (self.page, self.start)
    }
}

pub(super) struct Snippet {
    pub before: String,
    pub hit: String,
    pub after: String,
}

const CONTEXT: usize = 36;

fn fold(ch: char, match_case: bool) -> char {
    if match_case {
        return ch;
    }
    let mut lower = ch.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(single), None) => single,
        _ => ch,
    }
}

fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

pub(super) fn find(chars: &[char], query: &Query) -> Vec<(u32, u32)> {
    let needle: Vec<char> = query
        .text
        .chars()
        .map(|ch| fold(ch, query.match_case))
        .collect();
    if needle.is_empty() || needle.len() > chars.len() {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut at = 0;
    while at + needle.len() <= chars.len() {
        let fits = chars[at..at + needle.len()]
            .iter()
            .zip(&needle)
            .all(|(have, want)| fold(*have, query.match_case) == *want);
        let end = at + needle.len();
        let bounded = !query.whole_word
            || ((at == 0 || !is_word(chars[at - 1]))
                && chars.get(end).is_none_or(|ch| !is_word(*ch)));
        if fits && bounded {
            found.push((at as u32, needle.len() as u32));
            at = end;
        } else {
            at += 1;
        }
    }
    found
}

fn flat(chars: &[char]) -> String {
    let mut text = String::with_capacity(chars.len());
    for ch in chars {
        let ch = if ch.is_whitespace() { ' ' } else { *ch };
        if !(ch == ' ' && text.ends_with(' ')) {
            text.push(ch);
        }
    }
    text
}

pub(super) fn snippet(chars: &[char], hit: &Hit) -> Snippet {
    let start = (hit.start as usize).min(chars.len());
    let end = (start + hit.len as usize).min(chars.len());
    let from = start.saturating_sub(CONTEXT);
    let to = (end + CONTEXT).min(chars.len());
    let mut before = flat(&chars[from..start]).trim_start().to_owned();
    let mut after = flat(&chars[end..to]).trim_end().to_owned();
    if from > 0 {
        before.insert(0, '…');
    }
    if to < chars.len() {
        after.push('…');
    }
    Snippet {
        before,
        hit: flat(&chars[start..end]),
        after,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(text: &str) -> Vec<char> {
        text.chars().collect()
    }

    fn query(text: &str, match_case: bool, whole_word: bool) -> Query {
        Query {
            text: text.to_owned(),
            match_case,
            whole_word,
        }
    }

    #[test]
    fn find_ignores_case_unless_asked() {
        let page = chars("Volt volt VOLT");
        assert_eq!(find(&page, &query("volt", false, false)).len(), 3);
        assert_eq!(find(&page, &query("volt", true, false)), [(5, 4)]);
    }

    #[test]
    fn whole_word_skips_hits_inside_longer_words() {
        let page = chars("undervoltage volt volts");
        assert_eq!(find(&page, &query("volt", false, true)), [(13, 4)]);
    }

    #[test]
    fn a_snippet_keeps_context_on_one_line() {
        let page = chars("first line\r\nsecond   word here");
        let shown = snippet(
            &page,
            &Hit {
                page: 0,
                start: 21,
                len: 4,
                y: 0.,
            },
        );
        assert_eq!(
            (
                shown.before.as_str(),
                shown.hit.as_str(),
                shown.after.as_str()
            ),
            ("first line second ", "word", " here")
        );
    }
}
