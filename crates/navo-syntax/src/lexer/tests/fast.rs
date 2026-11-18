//! Equivalence tests: the byte-class shortcuts and the packed keyword table give the same
//! tokens as the full probe and a plain `HashMap` reference.

use std::collections::HashMap;

use proptest::prelude::*;

use super::super::words::Words;
use super::*;

const PIECES: &[&str] = &[
    "if",
    "else",
    "TRUE",
    "dsGetX",
    "dsget",
    "x1",
    "_a",
    "12",
    "0x1F",
    "1e5",
    "\"",
    "'",
    "'''",
    "//",
    "#",
    "/*",
    "*/",
    "(*",
    "*)",
    "==",
    "=",
    "+",
    "->",
    "/",
    " ",
    "  ",
    "\t",
    "é",
    "ß",
    "\u{212A}",
    "İ",
    "ÀB",
    "w0_000",
    "abcdefghij",
    "ABCDEFGH",
    "IF",
    "While",
    "(",
    ")",
    ";",
    "\\",
    "a中b",
    "中",
    "日本",
    "ünï",
    "ä",
    "«",
    "»",
    "§§",
    "„",
    "“",
    "→",
    "≤",
];

fn languages() -> Vec<Language> {
    let mut folded = sample();
    folded.case_ignored = true;
    let mut all = crate::builtin();
    all.push(sample());
    all.push(folded);
    all.push(unicode_language());
    all
}

/// A language whose openers, operators and keywords are all non-ASCII.
fn unicode_language() -> Language {
    Language {
        name: "unicode".to_owned(),
        extensions: owned(&["uni"]),
        case_ignored: false,
        keywords: vec![
            KeywordGroup {
                kind: K::Keyword,
                words: owned(&["ünï", "a中b", "if"]),
                prefix: false,
            },
            KeywordGroup {
                kind: K::Function,
                words: owned(&["日"]),
                prefix: true,
            },
        ],
        line_comments: owned(&["§§"]),
        block_comments: vec![("«".to_owned(), "»".to_owned())],
        delimiters: vec![delimiter("„", "“", Some('ä'), true)],
        operators: owned(&["→", "≤"]),
        numbers: true,
    }
}

/// The largest char boundary of `text` at or below `cut % (len + 1)`.
fn char_cut(text: &str, cut: usize) -> usize {
    let mut cut = cut % (text.len() + 1);
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    cut
}

/// A copy of `lexer` that runs the full probe at every byte.
fn unshortcut(lexer: &Lexer) -> Lexer {
    Lexer {
        classes: [ByteClass::Full; 256],
        ..lexer.clone()
    }
}

proptest! {
    #[test]
    fn byte_class_shortcuts_match_the_full_probe(
        picks in prop::collection::vec(0..PIECES.len(), 0..40),
        state in 0_u8..3,
        cut in any::<usize>(),
    ) {
        let line: String = picks.iter().map(|&i| PIECES[i]).collect();
        let state = match state {
            0 => LineState::Normal,
            1 => LineState::Block(0),
            _ => LineState::Delimited(1),
        };
        for language in languages() {
            let lexer = Lexer::new(&language);
            let full = unshortcut(&lexer);
            prop_assert_eq!(lexer.line(&line, state), full.line(&line, state), "{}", language.name);
            let (head, tail) = line.split_at(char_cut(&line, cut));
            let (head_tokens, head_end) = lexer.line(head, state);
            prop_assert_eq!((head_tokens, head_end), full.line(head, state), "{} head {:?}", language.name, head);
            prop_assert_eq!(lexer.line(tail, head_end), full.line(tail, head_end), "{} tail {:?}", language.name, tail);
        }
    }
}

/// Independent reference for the production word scan: the run of `is_word` chars at the start.
fn reference_word_len(text: &str) -> usize {
    text.char_indices()
        .find(|(_, ch)| !(ch.is_alphanumeric() || *ch == '_'))
        .map_or(text.len(), |(at, _)| at)
}

/// Independent reference for whole lines of plain words: splits by chars, looks words up in a map.
fn reference_tokens<'a>(line: &'a str, keywords: &HashMap<&str, K>) -> Vec<(&'a str, K)> {
    let mut found = Vec::new();
    let mut rest = line;
    while !rest.is_empty() {
        let len = reference_word_len(rest);
        let skip = if len == 0 {
            rest.chars().next().map_or(1, char::len_utf8)
        } else {
            len
        };
        if let Some(kind) = keywords.get(&rest[..len]).filter(|_| len > 0) {
            found.push((&rest[..len], *kind));
        }
        rest = &rest[skip..];
    }
    found
}

fn keyword_words() -> Vec<String> {
    [
        "if",
        "Else",
        "TRUE",
        "a",
        "_x9",
        "ABCDEFGH",
        "abcdefghi",
        "straße",
        "\u{212A}",
        "ünï",
        "w0_000",
    ]
    .iter()
    .map(|word| (*word).to_owned())
    .chain(["Q".repeat(70), "z".repeat(9)])
    .collect()
}

proptest! {
    #[test]
    fn packed_keyword_table_matches_a_hash_map(
        picks in prop::collection::vec(0..PIECES.len() + 4, 1..6),
        tail in prop::sample::select(vec!["", " ", "+x", "é", ".", "_"]),
        case_ignored in any::<bool>(),
        cut in any::<usize>(),
    ) {
        let words = keyword_words();
        let entries = words.iter().map(|word| (word, TokenKind::Keyword));
        let table = Words::new(case_ignored, entries);
        let mut reference = HashMap::new();
        for word in &words {
            let key = if case_ignored { word.to_lowercase() } else { word.clone() };
            reference.entry(key).or_insert(TokenKind::Keyword);
        }
        let extra = ["ABCDEFGH", "ABCDEFGHI", &"q".repeat(70), "k"];
        let text: String = picks
            .iter()
            .map(|&i| PIECES.get(i).copied().unwrap_or_else(|| extra[i - PIECES.len()]))
            .chain([tail])
            .collect();
        let text = &text[char_cut(&text, cut)..];
        let len = reference_word_len(text);
        let key = if case_ignored { text[..len].to_lowercase() } else { text[..len].to_owned() };
        let expected = reference.get(&key).copied().filter(|_| len > 0);
        prop_assert_eq!(table.find(text), (len, expected), "{:?}", text);
    }
}

#[test]
fn trie_lists_every_prefix_longest_first_past_the_inline_hits() {
    let mut trie = super::super::trie::Trie::new(false);
    for len in 1..=7 {
        trie.insert(&"a".repeat(len), len);
    }
    trie.insert("aaa", 30);
    let found: Vec<_> = trie.matches("aaaaaaaab").collect();
    assert_eq!(
        found,
        [
            (7, 7),
            (6, 6),
            (5, 5),
            (4, 4),
            (3, 3),
            (3, 30),
            (2, 2),
            (1, 1)
        ]
    );
}

#[test]
fn packing_a_byte_far_past_the_eighth_leaves_the_key_alone() {
    use super::super::words::pack_byte;
    let key = u64::from_le_bytes(*b"abcdefgh");
    for index in [
        8,
        9,
        536_870_912,
        536_870_913,
        1 << 32,
        (1 << 32) + 1,
        usize::MAX,
    ] {
        assert_eq!(pack_byte(key, b'z', index), key, "index {index}");
    }
    assert_eq!(pack_byte(0, b'z', 0), u64::from(b'z'));
    assert_eq!(pack_byte(0, b'z', 7), u64::from(b'z') << 56);
}

proptest! {
    #[test]
    fn plain_word_lines_match_a_char_based_reference(
        picks in prop::collection::vec(0..WORD_POOL.len(), 0..20),
        gap in prop::sample::select(vec![" ", "+", "  ", "é ", ";"]),
    ) {
        let keywords: HashMap<&str, K> = WORD_POOL
            .iter()
            .step_by(2)
            .map(|word| (*word, K::Keyword))
            .collect();
        let language = Language {
            name: "words".to_owned(),
            extensions: owned(&["w"]),
            case_ignored: false,
            keywords: vec![KeywordGroup {
                kind: K::Keyword,
                words: keywords.keys().map(|word| (*word).to_owned()).collect(),
                prefix: false,
            }],
            line_comments: Vec::new(),
            block_comments: Vec::new(),
            delimiters: Vec::new(),
            operators: Vec::new(),
            numbers: false,
        };
        let line = picks.iter().map(|&i| WORD_POOL[i]).collect::<Vec<_>>().join(gap);
        prop_assert_eq!(spans(&language, &line, LineState::Normal).0, reference_tokens(&line, &keywords), "{:?}", line);
    }
}

const WORD_POOL: &[&str] = &[
    "if",
    "else",
    "a中b",
    "a中",
    "中b",
    "ünï",
    "ünï2",
    "x_9",
    "abcdefgh",
    "abcdefghi",
    "日本語",
    "日本",
    "ab",
    "a",
    "Ünï",
    "résumé",
    "résum",
];
