use gpui_kit::component::Rope;
use gpui_kit::component::highlighter::{HighlightTheme, LanguageRegistry, SyntaxHighlighter};

use super::super::captures::is_known;
use super::super::grammars::grammars;
use super::super::{ambiguous, names, register_languages, sniff};

const SAMPLES: [(&str, &str); 13] = [
    (
        "xml",
        "<?xml version=\"1.0\"?>\n<!-- part -->\n<ecu name=\"main\">text</ecu>\n",
    ),
    ("sql", "SELECT name, 42 FROM users WHERE id = 'x';\n"),
    (
        "powershell",
        "# note\nfunction Get-Thing { param($Name) Write-Host \"hi $Name\" }\n",
    ),
    ("ini", "; note\n[main]\nlevel = 2\n"),
    ("properties", "# note\nserver.port=8080\n"),
    (
        "batch",
        "@echo off\nREM note\nset NAME=world\necho %NAME%\n",
    ),
    ("dockerfile", "# note\nFROM alpine:3.20\nRUN echo \"hi\"\n"),
    ("json5", "// note\n{ name: 'x', count: 42, }\n"),
    (
        "r",
        "# note\nsquare <- function(x) { x * 2 }\nprint(\"hi\")\n",
    ),
    (
        "perl",
        "# note\nmy $name = \"world\";\nprint \"hi $name\\n\";\n",
    ),
    (
        "latex",
        "% note\n\\documentclass{article}\n\\begin{document}\n\\section{Intro}\n\\end{document}\n",
    ),
    (
        "objc",
        "#import <Foundation/Foundation.h>\n@interface Thing : NSObject\n@end\nint main() { return 0; }\n",
    ),
    ("asm", "; note\nstart:\n    mov eax, 42\n    ret\n"),
];

fn sample(name: &str) -> &'static str {
    SAMPLES
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, sample)| *sample)
        .unwrap_or_else(|| panic!("no sample for {name}"))
}

fn detectable() -> Vec<&'static str> {
    let marked = ambiguous::MARKED
        .iter()
        .flat_map(|(_, marks)| marks.iter().map(|(_, language)| *language));
    names::all()
        .chain(sniff::INTERPRETERS.iter().map(|(_, language)| *language))
        .chain(names::ALIASES.iter().map(|(_, language)| *language))
        .chain(names::SPECIAL.iter().map(|(_, language)| *language))
        .chain(marked)
        .chain(ambiguous::CONF_LANGUAGES)
        .chain(["xml", "dockerfile"])
        .collect()
}

fn colours(language: &str, text: &str) -> Vec<(std::ops::Range<usize>, bool)> {
    register_languages();
    let mut highlighter = SyntaxHighlighter::new(language);
    highlighter.update(None, &Rope::from(text), None);
    let theme = HighlightTheme::default_dark();
    highlighter
        .styles(&(0..text.len()), &*theme)
        .into_iter()
        .map(|(range, style)| (range, style.color.is_some()))
        .collect()
}

#[test]
fn every_detected_language_has_a_grammar() {
    register_languages();
    let registry = LanguageRegistry::singleton();
    for name in detectable() {
        let config = registry.language(name);
        assert!(
            config.is_some_and(|config| config.has_grammar()),
            "{name} has no grammar"
        );
    }
}

#[test]
fn every_remapped_capture_is_a_known_highlight_name() {
    for grammar in grammars() {
        let query = tree_sitter::Query::new(&grammar.language, &grammar.highlights)
            .unwrap_or_else(|error| panic!("{}: {error}", grammar.name));
        for name in query.capture_names() {
            assert!(is_known(name), "{}: @{name} is unknown", grammar.name);
        }
    }
}

#[test]
fn a_powershell_assignment_colours_its_value() {
    let text = "$y = \"hi\"\n";
    let start = text.find('"').expect("a quote");
    let coloured = colours("powershell", text)
        .into_iter()
        .any(|(range, coloured)| coloured && range.contains(&start));
    assert!(coloured);
}

#[test]
fn a_perl_comment_on_the_first_line_is_a_comment() {
    let text = "# note\nmy $x = 1;\n";
    let theme = HighlightTheme::default_dark();
    register_languages();
    let mut highlighter = SyntaxHighlighter::new("perl");
    highlighter.update(None, &Rope::from(text), None);
    let comment = theme.style("comment").and_then(|style| style.color);
    let first = highlighter
        .styles(&(0..text.len()), &*theme)
        .into_iter()
        .find(|(range, _)| range.contains(&0))
        .and_then(|(_, style)| style.color);
    assert_eq!(first, comment);
}

#[test]
fn every_new_highlights_query_compiles() {
    for grammar in grammars() {
        if let Err(error) = tree_sitter::Query::new(&grammar.language, &grammar.highlights) {
            panic!("{}: {error}", grammar.name);
        }
    }
}

#[test]
fn every_new_grammar_colours_a_sample() {
    register_languages();
    let theme = HighlightTheme::default_dark();
    for grammar in grammars() {
        let mut highlighter = SyntaxHighlighter::new(grammar.name);
        assert_eq!(highlighter.language().as_ref(), grammar.name);
        let sample = sample(grammar.name);
        highlighter.update(None, &Rope::from(sample), None);
        let styles = highlighter.styles(&(0..sample.len()), &*theme);
        assert!(
            styles.iter().any(|(_, style)| style.color.is_some()),
            "{} shows no colour",
            grammar.name
        );
    }
}
