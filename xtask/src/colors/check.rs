use super::scan::{Found, scan_rust, scan_svg};

pub(super) const ALLOW_FILE: &str = "xtask/colors-allow.txt";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Rust,
    Svg,
}

pub(super) struct Source {
    pub path: String,
    pub text: String,
    pub kind: Kind,
}

struct Allowed<'a> {
    pattern: &'a str,
    used: bool,
}

impl Allowed<'_> {
    fn covers(&self, path: &str) -> bool {
        match self.pattern.strip_suffix("/**") {
            Some(folder) => path
                .strip_prefix(folder)
                .is_some_and(|rest| rest.starts_with('/')),
            None => path == self.pattern,
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct Outcome {
    pub files: usize,
    pub errors: Vec<String>,
}

pub(super) fn evaluate(sources: &[Source], allow: &str) -> Outcome {
    let mut outcome = Outcome::default();
    let mut allowed = parse_allowed(allow, &mut outcome.errors);
    for source in sources {
        outcome.files += 1;
        if let Some(entry) = allowed.iter_mut().find(|entry| entry.covers(&source.path)) {
            entry.used = true;
            continue;
        }
        for found in found_in(source) {
            outcome.errors.push(format!(
                "{}:{}: {} builds a colour outside the token layer; use a theme token",
                source.path, found.line, found.what
            ));
        }
    }
    for entry in allowed.iter().filter(|entry| !entry.used) {
        outcome.errors.push(format!(
            "{ALLOW_FILE}: {} matches no scanned file; remove it",
            entry.pattern
        ));
    }
    outcome
}

fn found_in(source: &Source) -> Vec<Found> {
    match source.kind {
        Kind::Rust => scan_rust(&source.text),
        Kind::Svg => scan_svg(&source.text),
    }
}

fn parse_allowed<'a>(allow: &'a str, errors: &mut Vec<String>) -> Vec<Allowed<'a>> {
    let mut entries = Vec::new();
    for (at, line) in allow
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
    {
        match line.split_once('|') {
            Some((pattern, reason)) if !reason.trim().is_empty() => entries.push(Allowed {
                pattern: pattern.trim(),
                used: false,
            }),
            _ => errors.push(format!(
                "{ALLOW_FILE}:{}: write `path | reason`; every exception needs a reason",
                at + 1
            )),
        }
    }
    entries
}
