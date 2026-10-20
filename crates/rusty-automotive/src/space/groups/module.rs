use crate::space::Kind;
use crate::space::plain_section;

const AUTOSAR: [&str; 2] = ["rte_", "schm_"];

pub(super) struct Modules {
    listed: Vec<String>,
}

pub(super) struct Module<'a> {
    pub(super) name: &'a str,
    pub(super) listed: bool,
}

impl Modules {
    pub(super) fn new(list: &[String]) -> Self {
        let mut listed: Vec<String> = list
            .iter()
            .map(|name| name.trim().to_ascii_lowercase())
            .filter(|name| !name.is_empty())
            .collect();
        listed.sort_by_key(|name| std::cmp::Reverse(name.len()));
        Self { listed }
    }

    pub(super) fn of<'a>(&self, candidates: [Option<&'a str>; 3]) -> Option<Module<'a>> {
        let [object, section, name] = candidates;
        let object = object.map(object_stem);
        [object, section.map(without_kind), name.map(without_kind)]
            .into_iter()
            .flatten()
            .map(|text| text.trim_start_matches(['.', '_']))
            .find(|text| !text.is_empty())
            .map(|text| self.module_in(text))
    }

    fn module_in<'a>(&self, text: &'a str) -> Module<'a> {
        if let Some(len) = self.listed_prefix(text) {
            return Module {
                name: &text[..len],
                listed: true,
            };
        }
        let len = autosar_len(text).unwrap_or_else(|| text.find(['_', '.']).unwrap_or(text.len()));
        Module {
            name: &text[..len],
            listed: false,
        }
    }

    fn listed_prefix(&self, text: &str) -> Option<usize> {
        let bytes = text.as_bytes();
        self.listed.iter().find_map(|name| {
            let head = bytes.get(..name.len())?;
            let ends = bytes
                .get(name.len())
                .is_none_or(|next| matches!(next, b'_' | b'.'));
            (head.eq_ignore_ascii_case(name.as_bytes()) && ends).then_some(name.len())
        })
    }
}

fn autosar_len(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let lead = AUTOSAR.iter().find(|lead| {
        bytes
            .get(..lead.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(lead.as_bytes()))
    })?;
    let tail = &text[lead.len()..];
    let part = tail.find(['_', '.']).unwrap_or(tail.len());
    (part > 0).then_some(lead.len() + part)
}

fn object_stem(object: &str) -> &str {
    let member = object
        .strip_suffix(')')
        .and_then(|rest| rest.rsplit_once('('))
        .map_or(object, |(_, member)| member);
    let file = member.rsplit(['/', '\\']).next().unwrap_or(member);
    file.rsplit_once('.').map_or(file, |(stem, _)| stem)
}

fn without_kind(name: &str) -> &str {
    let name = plain_section(name);
    if is_compiler_label(name) {
        return "";
    }
    let Some(rest) = name.strip_prefix('.') else {
        return name;
    };
    match rest.split_once('.') {
        Some((lead, tail)) if Kind::of_name(lead) != Kind::Other => tail,
        None if Kind::of_name(rest) != Kind::Other => "",
        _ => rest,
    }
}

fn is_compiler_label(name: &str) -> bool {
    name.strip_prefix(".L").is_some_and(|rest| {
        rest.ends_with(|c: char| c.is_ascii_digit())
            && rest
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    })
}
