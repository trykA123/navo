use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FileKind {
    Object,
    Source,
}

pub(super) fn classify(name: &str) -> Option<FileKind> {
    let lower = name.to_ascii_lowercase();
    if [".o", ".obj"].iter().any(|ext| lower.ends_with(ext)) {
        Some(FileKind::Object)
    } else if [".c", ".cpp", ".cc", ".s", ".asm"]
        .iter()
        .any(|ext| lower.ends_with(ext))
    {
        Some(FileKind::Source)
    } else {
        None
    }
}

pub(in crate::space) fn plain_section(name: &str) -> &str {
    name.strip_suffix(')')
        .and_then(|rest| rest.rsplit_once(" ("))
        .filter(|(_, digits)| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
        .map_or(name, |(head, _)| head)
}

#[derive(Default)]
pub(super) struct Interner {
    pub(super) items: Vec<String>,
    index: HashMap<String, u32>,
}

impl Interner {
    pub(super) fn id(&mut self, text: &str) -> u32 {
        if let Some(found) = self.index.get(text) {
            return *found;
        }
        let at = self.items.len() as u32;
        self.items.push(text.to_owned());
        self.index.insert(text.to_owned(), at);
        at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_real_extensions_name_files() {
        assert_eq!(classify("Com_Main.o"), Some(FileKind::Object));
        assert_eq!(classify("lamp.OBJ"), Some(FileKind::Object));
        assert_eq!(classify("lamp.c"), Some(FileKind::Source));
        assert_eq!(classify("start.S"), Some(FileKind::Source));
        assert_eq!(classify("cc12345.src"), None);
        assert_eq!(classify("crtstuff"), None);
    }

    #[test]
    fn section_numbers_are_dropped() {
        assert_eq!(plain_section(".text.turn (12)"), ".text.turn");
        assert_eq!(plain_section(".text (x)"), ".text (x)");
        assert_eq!(plain_section(".text"), ".text");
    }
}
