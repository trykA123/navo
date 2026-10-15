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
            let number = number(&mut self.servers, server);
            root.push_str(&format!("<server:{number}>"));
        }
        if let Some(share) = share {
            let number = number(&mut self.shares, share);
            root.push_str(&format!(r"\<share:{number}>"));
        }
        root
    }
}

fn number(names: &mut HashMap<String, usize>, name: &str) -> usize {
    let next = names.len() + 1;
    *names.entry(name.to_lowercase()).or_insert(next)
}

pub(super) struct Replacement {
    name: String,
    token: String,
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
                })
            })
            .collect();
        let names = self.servers.lock().unwrap_or_else(PoisonError::into_inner);
        replacements.extend(names.servers.iter().map(|(name, number)| Replacement {
            name: name.clone(),
            token: format!("<server:{number}>"),
        }));
        replacements.sort_by_key(|replacement| std::cmp::Reverse(replacement.name.len()));
        replacements
    }
}

pub(super) fn replace(text: &str, replacements: &[Replacement]) -> String {
    let mut output = String::new();
    let mut end = 0;
    for (index, ch) in text.char_indices() {
        if index < end {
            continue;
        }
        let found = replacements.iter().find_map(|replacement| {
            prefix_length(&text[index..], &replacement.name)
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
