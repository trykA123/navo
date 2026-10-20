use std::collections::BTreeSet;

use super::ftl::defined;
use super::scan::{is_capitalised_multi_word, scan};

pub(super) const ALLOW_FILE: &str = "xtask/l10n-allow.txt";
const COMMAND_PREFIX: &str = "cmd-";

pub(super) struct Source {
    pub path: String,
    pub text: String,
    pub in_ratchet: bool,
}

#[derive(Debug, Default)]
pub(super) struct Outcome {
    pub messages: usize,
    pub errors: Vec<String>,
    pub literals: Vec<String>,
}

pub(super) fn evaluate(ftl: &[(String, String)], sources: &[Source], allowed: &str) -> Outcome {
    let known = defined(ftl);
    let mut outcome = Outcome {
        messages: known.ids.len(),
        errors: known.problems,
        literals: Vec::new(),
    };
    let mut strings = BTreeSet::new();
    for source in sources {
        let found = scan(&source.text);
        for call in found
            .calls
            .iter()
            .filter(|c| !known.ids.contains_key(&c.text))
        {
            outcome.errors.push(format!(
                "{}:{}: unknown message id {}",
                source.path, call.line, call.text
            ));
        }
        for string in found.strings {
            if source.in_ratchet && is_capitalised_multi_word(&string.text) {
                outcome
                    .literals
                    .push(format!("{}:{}: {}", source.path, string.line, string.text));
            }
            strings.insert(string.text);
        }
    }
    for (id, file) in &known.ids {
        if !id.starts_with(COMMAND_PREFIX) && !strings.contains(id) {
            outcome.errors.push(format!("{file}: {id} is never used"));
        }
    }
    outcome
        .errors
        .extend(ratchet(outcome.literals.len(), allowed));
    outcome
}

fn ratchet(count: usize, allowed: &str) -> Option<String> {
    let Ok(allowed) = allowed.trim().parse::<usize>() else {
        return Some(format!("{ALLOW_FILE} must hold one number"));
    };
    match count.cmp(&allowed) {
        std::cmp::Ordering::Greater => Some(format!(
            "{count} hard-coded texts, {allowed} allowed: move the new ones into the .ftl files"
        )),
        std::cmp::Ordering::Less => Some(format!(
            "{count} hard-coded texts, {allowed} allowed: lower {ALLOW_FILE} to {count}"
        )),
        std::cmp::Ordering::Equal => None,
    }
}
