use std::collections::HashMap;
use std::sync::PoisonError;

use super::Redactor;

#[derive(Default)]
pub(super) struct NetworkNames {
    servers: HashMap<String, usize>,
    shares: HashMap<String, usize>,
}

impl NetworkNames {
    pub(super) fn root(&mut self, server: Option<&str>, share: Option<&str>) -> String {
        let mut root = String::from(r"\\");
        if let Some(server) = server {
            if numbered_token(server, "server") {
                root.push_str(server);
            } else {
                let number = number(&mut self.servers, server);
                root.push_str(&format!("<server:{number}>"));
            }
        }
        if let Some(share) = share {
            if numbered_token(share, "share") {
                root.push_str(&format!(r"\{share}"));
            } else {
                let number = number(&mut self.shares, share);
                root.push_str(&format!(r"\<share:{number}>"));
            }
        }
        root
    }
}

fn number(names: &mut HashMap<String, usize>, name: &str) -> usize {
    let next = names.len() + 1;
    *names.entry(name.to_lowercase()).or_insert(next)
}

fn numbered_token(name: &str, tag: &str) -> bool {
    name.strip_prefix('<')
        .and_then(|rest| rest.strip_prefix(tag))
        .and_then(|rest| rest.strip_prefix(':'))
        .and_then(|rest| rest.strip_suffix('>'))
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

pub(super) struct Replacement {
    name: String,
    token: String,
    whole_word: bool,
}

impl Redactor {
    pub(super) fn replacements(&self) -> Vec<Replacement> {
        let identity = [
            (&self.identity.user, "<user>"),
            (&self.identity.computer, "<computer>"),
            (&self.identity.domain, "<domain>"),
        ];
        let mut replacements: Vec<_> = identity
            .into_iter()
            .filter_map(|(name, token)| {
                let name = name.as_ref().filter(|name| !name.is_empty())?;
                Some(Replacement {
                    name: name.to_lowercase(),
                    token: token.to_owned(),
                    whole_word: false,
                })
            })
            .collect();
        let names = self.servers.lock().unwrap_or_else(PoisonError::into_inner);
        replacements.extend(names.servers.iter().map(|(name, number)| Replacement {
            name: name.clone(),
            token: format!("<server:{number}>"),
            whole_word: false,
        }));
        replacements.extend(names.shares.iter().map(|(name, number)| Replacement {
            name: name.clone(),
            token: format!("<share:{number}>"),
            whole_word: true,
        }));
        replacements.sort_by_key(|replacement| std::cmp::Reverse(replacement.name.len()));
        replacements
    }
}

pub(super) fn token_len(text: &str) -> Option<usize> {
    let end = text.strip_prefix('<')?.find('>')? + 2;
    let inner = &text[1..end - 1];
    let valid = match inner.split_once(':') {
        None => matches!(inner, "user" | "computer" | "domain"),
        Some(("d" | "f", hex)) => {
            hex.len() == 4 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        }
        Some(("server" | "share", digits)) => all_digits(digits),
        Some(("text", count)) => count.strip_suffix(" chars").is_some_and(all_digits),
        Some(_) => false,
    };
    valid.then_some(end)
}

fn all_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

pub(super) fn replace(text: &str, replacements: &[Replacement]) -> String {
    let mut output = String::new();
    let mut end = 0;
    let mut before = None;
    for (index, ch) in text.char_indices() {
        let at_word_start = before.is_none_or(|b: char| !b.is_alphanumeric());
        before = Some(ch);
        if index < end {
            continue;
        }
        if let Some(length) = token_len(&text[index..]) {
            output.push_str(&text[index..index + length]);
            end = index + length;
            continue;
        }
        let found = replacements.iter().find_map(|replacement| {
            prefix_length(&text[index..], &replacement.name)
                .filter(|&length| {
                    !replacement.whole_word || at_word_start && word_ends(text, index + length)
                })
                .map(|length| (length, &replacement.token))
        });
        if let Some((length, token)) = found {
            output.push_str(token);
            end = index + length;
        } else {
            output.push(ch);
        }
    }
    output
}

fn word_ends(text: &str, end: usize) -> bool {
    text[end..]
        .chars()
        .next()
        .is_none_or(|ch| !ch.is_alphanumeric())
}

fn prefix_length(text: &str, lowercase: &str) -> Option<usize> {
    let expected = lowercase.chars().count();
    let mut length = 0;
    for (index, ch) in text.char_indices() {
        length += ch.to_lowercase().count();
        if length < expected {
            continue;
        }
        let end = index + ch.len_utf8();
        return (text[..end].to_lowercase() == lowercase).then_some(end);
    }
    None
}
