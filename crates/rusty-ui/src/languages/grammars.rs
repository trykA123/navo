use std::sync::Once;

use gpui_kit::component::highlighter::{GrammarConfig, LanguageRegistry};
use tree_sitter::Language;
use tree_sitter_language::LanguageFn;

use super::captures::remap;

pub(crate) struct Grammar {
    pub name: &'static str,
    pub language: Language,
    pub highlights: String,
}

const GRAMMARS: [(&str, LanguageFn, &[&str]); 13] = [
    (
        "xml",
        tree_sitter_xml::LANGUAGE_XML,
        &[tree_sitter_xml::XML_HIGHLIGHT_QUERY],
    ),
    (
        "sql",
        tree_sitter_sequel::LANGUAGE,
        &[tree_sitter_sequel::HIGHLIGHTS_QUERY],
    ),
    (
        "powershell",
        tree_sitter_powershell::LANGUAGE,
        &[tree_sitter_powershell::HIGHLIGHTS_QUERY],
    ),
    (
        "ini",
        tree_sitter_ini::LANGUAGE,
        &[tree_sitter_ini::HIGHLIGHTS_QUERY],
    ),
    (
        "properties",
        tree_sitter_properties::LANGUAGE,
        &[tree_sitter_properties::HIGHLIGHTS_QUERY],
    ),
    (
        "batch",
        tree_sitter_batch::LANGUAGE,
        &[tree_sitter_batch::HIGHLIGHTS_QUERY],
    ),
    (
        "dockerfile",
        tree_sitter_containerfile::LANGUAGE,
        &[tree_sitter_containerfile::HIGHLIGHTS_QUERY],
    ),
    (
        "json5",
        tree_sitter_json5_orchard::LANGUAGE,
        &[tree_sitter_json5_orchard::HIGHLIGHTS_QUERY],
    ),
    (
        "r",
        tree_sitter_r::LANGUAGE,
        &[tree_sitter_r::HIGHLIGHTS_QUERY],
    ),
    (
        "perl",
        ts_parser_perl::LANGUAGE,
        &[ts_parser_perl::HIGHLIGHTS_QUERY],
    ),
    (
        "latex",
        codebook_tree_sitter_latex::LANGUAGE,
        &[include_str!("queries/latex.scm")],
    ),
    (
        "objc",
        tree_sitter_objc::LANGUAGE,
        &[
            tree_sitter_c::HIGHLIGHT_QUERY,
            tree_sitter_objc::HIGHLIGHTS_QUERY,
        ],
    ),
    (
        "asm",
        tree_sitter_asm::LANGUAGE,
        &[tree_sitter_asm::HIGHLIGHTS_QUERY],
    ),
];

pub(crate) fn grammars() -> impl Iterator<Item = Grammar> {
    GRAMMARS.iter().map(|(name, language, queries)| Grammar {
        name,
        language: (*language).into(),
        highlights: remap(&queries.join("\n")),
    })
}

pub(super) struct GrammarRegistry {
    registered: Once,
}

impl GrammarRegistry {
    pub(super) const fn new() -> Self {
        Self {
            registered: Once::new(),
        }
    }

    pub(super) fn register(&self) {
        self.registered.call_once(|| {
            let registry = LanguageRegistry::singleton();
            for grammar in grammars() {
                let config = GrammarConfig::new(
                    grammar.name,
                    grammar.language,
                    Vec::new(),
                    &grammar.highlights,
                    "",
                    "",
                );
                registry.register(grammar.name, &config);
            }
        });
    }

    #[cfg(test)]
    pub(super) fn registered_count(&self) -> usize {
        if self.registered.is_completed() {
            GRAMMARS.len()
        } else {
            0
        }
    }
}

static LANGUAGES: GrammarRegistry = GrammarRegistry::new();

pub fn register_languages() {
    LANGUAGES.register();
}
