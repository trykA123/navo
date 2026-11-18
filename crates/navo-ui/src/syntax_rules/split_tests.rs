//! Split-safety tests for the rule-lexer adapter (D7): a line lexed in chunks keeps its colours.
//! The chunks follow the paged view: `lex` for a chunk that may continue, `lex_last` for the end.

mod check;
mod hold;

use std::ops::Range;
use std::sync::OnceLock;

use navo_syntax::{ChunkState, Delimiter, Inside, Language, Registry};
use proptest::prelude::*;
use sidewise_kit::languages::lines::{LineLexer, LineState};

use super::state::pack;
use super::{RuleLexer, rules};
use check::Line;

const MIX: &str = "[[language]]\nname = \"mix\"\nextensions = [\"mix\"]\ncase_ignored = true\n\
    line_comments = [\"#\", \"--\"]\nblock_comments = [[\"(*\", \"*)\"], [\"{\", \"}\"]]\n\
    operators = [\"+\", \"==\"]\n\
    [[language.keywords]]\nkind = \"keyword\"\nwords = [\"BEGIN\", \"end\"]\n\
    [[language.delimiters]]\nopen = '\"'\nclose = '\"'\nescape = '\\'\n\
    [[language.delimiters]]\nopen = \"'\"\nclose = \"'\"\n\
    [[language.delimiters]]\nopen = \"`\"\nclose = \"`\"\nmultiline = true\n\
    [[language.delimiters]]\nopen = \"~\"\nkind = \"constant\"\n\
    [[language.delimiters]]\nopen = \"S\"\nclose = \"!\"\n\
    [[language.delimiters]]\nopen = \"s\"\nclose = \"?\"\n";

/// A language with a multiline escaped string, M4-style word openers and `TASK`.
const WORDY: &str = "[[language]]\nname = \"wordy\"\nextensions = [\"wordy\"]\n\
    line_comments = [\"dnl\", \"rem\"]\nblock_comments = [[\"begin\", \"end.\"]]\n\
    operators = [\"==\"]\n\
    [[language.keywords]]\nkind = \"keyword\"\nwords = [\"TASK\", \"if\"]\n\
    [[language.delimiters]]\nopen = '\"'\nclose = '\"'\nescape = '\\'\nmultiline = true\n\
    [[language.delimiters]]\nopen = \"q\"\nclose = \"z\"\n";

/// Leaks a test language parsed from TOML so a lexer can borrow it for the process.
fn leaked(toml: &str) -> &'static Language {
    let language = navo_syntax::from_toml(toml)
        .expect("test language parses")
        .remove(0);
    Box::leak(Box::new(language))
}

/// Every built-in rule language plus the `mix` and `wordy` test languages, as leaked lexers.
fn lexers() -> &'static [(RuleLexer, &'static Language)] {
    static ALL: OnceLock<Vec<(RuleLexer, &'static Language)>> = OnceLock::new();
    ALL.get_or_init(|| {
        let tests = [leaked(MIX), leaked(WORDY)];
        Registry::builtin()
            .iter()
            .map(|language| &*Box::leak(Box::new(language.clone())))
            .chain(tests)
            .chain(hold::languages())
            .map(|language| (RuleLexer::new(language), language))
            .collect()
    })
}

/// Picks a plain keyword (letters, digits, `_` only) of the language by index.
fn word(language: &Language, n: u16) -> String {
    let words: Vec<&String> = language
        .keywords
        .iter()
        .filter(|group| !group.prefix)
        .flat_map(|group| &group.words)
        .filter(|w| !w.is_empty() && w.chars().all(|c| c.is_alphanumeric() || c == '_'))
        .collect();
    words
        .get(usize::from(n) % words.len().max(1))
        .map_or_else(|| "zq".to_owned(), |w| (*w).clone())
}

/// Appends the body of a comment or string: keywords, non-ASCII text and spaces.
fn body(line: &mut String, language: &Language, n: u16) {
    line.push_str(&format!(
        " {} \u{e9} \u{65e5}\u{672c} {} ",
        word(language, n),
        word(language, n / 7)
    ));
}

/// Appends one delimited token; `n` picks a closed or an open string, an escape or a last escape.
fn delimited(line: &mut String, language: &Language, d: &Delimiter, n: u16) -> bool {
    line.push_str(&d.open);
    body(line, language, n);
    if let Some(escape) = d.escape.filter(|_| n.is_multiple_of(5)) {
        line.push(escape);
        return true;
    }
    if let Some(escape) = d.escape.filter(|_| n.is_multiple_of(3)) {
        let escaped = if n.is_multiple_of(2) {
            d.close.chars().next().unwrap_or('x')
        } else {
            'x'
        };
        line.push_str(&format!("{escape}{escaped} "));
    }
    let closed = !n.is_multiple_of(4) && !d.close.is_empty();
    if closed {
        line.push_str(&d.close);
        line.push(' ');
    }
    !closed
}

/// Appends a block comment, closed or open (`n` picks); true when it ends the line.
fn block(line: &mut String, language: &Language, n: u16) -> bool {
    let (open, close) = &language.block_comments[usize::from(n) % language.block_comments.len()];
    line.push_str(open);
    body(line, language, n);
    let open_end = n.is_multiple_of(3);
    if !open_end {
        line.push_str(close);
        line.push(' ');
    }
    open_end
}

/// Appends a letter glued to an opener (`xdnl tail`): a word, or a real opener, by the rules.
fn glued(line: &mut String, language: &Language, n: u16) {
    let openers: Vec<&String> = language
        .line_comments
        .iter()
        .chain(language.block_comments.iter().map(|(open, _)| open))
        .chain(language.delimiters.iter().map(|d| &d.open))
        .collect();
    let open = openers.get(usize::from(n) % openers.len().max(1));
    line.push('x');
    line.push_str(open.map_or("", |open| open.as_str()));
    line.push_str(" tail ");
}

/// Appends one item of kind `kind` (0 to 10); true when the item ends the line.
fn item(line: &mut String, language: &Language, kind: u8, n: u16) -> bool {
    match kind % 11 {
        0 => line.push_str(&word(language, n)),
        1 if language.numbers => line.push_str(&format!("{}", u32::from(n) + 1)),
        2 if !language.operators.is_empty() => {
            line.push_str(&language.operators[usize::from(n) % language.operators.len()]);
        }
        3 => line.push_str(" \u{65e5}\u{672c} \u{e9}"),
        4 if !language.line_comments.is_empty() => {
            line.push_str(&language.line_comments[usize::from(n) % language.line_comments.len()]);
            body(line, language, n);
            return true;
        }
        5 if !language.block_comments.is_empty() => return block(line, language, n),
        6 if !language.delimiters.is_empty() => {
            let d = &language.delimiters[usize::from(n) % language.delimiters.len()];
            return delimited(line, language, d, n);
        }
        7 => line.push_str(&" ".repeat(usize::from(n % 70))),
        9 => glued(line, language, n),
        _ => line.push_str("zzq"),
    }
    line.push(' ');
    false
}

/// Builds a line from generated item choices; a line comment or an open token ends it.
fn build(language: &Language, items: &[(u8, u16)]) -> String {
    let mut line = String::new();
    for &(kind, n) in items {
        if item(&mut line, language, kind, n) {
            break;
        }
    }
    line
}

/// Turns raw numbers into strictly rising char-boundary cuts anywhere, inside words and openers
/// too, and inside closers.
fn cuts_of(text: &str, raw: &[u16]) -> Vec<usize> {
    let mut cuts: Vec<usize> = raw
        .iter()
        .map(|r| {
            let mut at = usize::from(*r) % (text.len() + 1);
            while !text.is_char_boundary(at) {
                at += 1;
            }
            at
        })
        .filter(|&at| at > 0 && at < text.len())
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    cuts
}

/// Converts a rule index to the `u16` the lexer states use.
fn index(i: usize) -> u16 {
    u16::try_from(i).expect("test languages have few rules")
}

/// The start states worth trying: a fresh line, or inside each block comment or long string.
fn starts(language: &Language) -> Vec<LineState> {
    let blocks = (0..language.block_comments.len()).map(|i| Inside::Block(index(i)));
    let multi = language
        .delimiters
        .iter()
        .enumerate()
        .filter(|(_, d)| d.multiline && !d.close.is_empty())
        .map(|(i, _)| Inside::Delimited {
            index: index(i),
            escaped: false,
        });
    std::iter::once(Inside::Code)
        .chain(blocks)
        .chain(multi)
        .map(|inside| {
            pack(ChunkState {
                inside,
                mid_word: false,
                partial: 0,
            })
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(8000))]

    #[test]
    fn chunked_lexing_matches_whole_line_lexing(
        pick in any::<usize>(),
        items in prop::collection::vec((any::<u8>(), any::<u16>()), 1..14),
        raw in prop::collection::vec(any::<u16>(), 1..5),
        start in any::<usize>(),
    ) {
        let (lexer, language) = &lexers()[pick % lexers().len()];
        let text = build(language, &items);
        let starts = starts(language);
        let state = starts[start % starts.len()];
        let line = Line::new(lexer, language, &text, state);
        let cuts = cuts_of(&text, &raw);
        let result = line.check(&cuts);
        prop_assert!(result.is_ok(), "{} {text:?} cuts {cuts:?} from {state:?}: {result:?}", lexer.name());
    }
}

/// Lexes `chunks` of one line like the paged view, returning every span shifted to the line.
fn paged(lexer: &RuleLexer, chunks: &[&str]) -> (Vec<(Range<usize>, &'static str)>, LineState) {
    let (mut spans, mut state, mut start) = (Vec::new(), LineState(0), 0);
    for (i, chunk) in chunks.iter().enumerate() {
        let (found, next) = if i + 1 == chunks.len() {
            lexer.lex_last(chunk, state)
        } else {
            lexer.lex(chunk, state)
        };
        spans.extend(
            found
                .into_iter()
                .map(|(r, role)| (start + r.start..start + r.end, role)),
        );
        state = next;
        start += chunk.len();
    }
    (spans, lexer.line_end(state))
}

/// The roles a whole-line lex gives `chunks` joined, to compare with `paged`.
fn whole(lexer: &RuleLexer, chunks: &[&str]) -> Vec<(Range<usize>, &'static str)> {
    lexer.lex_last(&chunks.concat(), LineState(0)).0
}

#[test]
fn a_line_comment_cut_by_a_chunk_stays_a_comment_in_the_next_chunk() {
    let lexer = RuleLexer::new(rules().for_name("x.oil").expect("oil"));
    let head = format!("//{}", " ".repeat(4094));
    let (spans, end) = paged(&lexer, &[&head, "TASK"]);
    assert_eq!(spans, [(0..4096, "comment"), (4096..4100, "comment")]);
    assert_eq!(end, LineState(0), "the comment ends with its line");
    let (next_line, _) = lexer.lex_last("TASK", end);
    assert_eq!(next_line, [(0..4, "keyword")]);
}

#[test]
fn a_string_cut_by_a_chunk_keeps_going_and_an_escape_at_the_cut_holds() {
    let lexer = RuleLexer::new(rules().for_name("x.oil").expect("oil"));
    let (spans, end) = paged(&lexer, &["\"ab TA", "SK TASK\" TASK"]);
    assert_eq!(
        spans,
        [(0..6, "string"), (6..14, "string"), (15..19, "keyword")]
    );
    assert_eq!(end, LineState(0));
    let (spans, _) = paged(&lexer, &["\"a\\", "\" TASK\" TASK"]);
    assert_eq!(
        spans,
        [(0..3, "string"), (3..10, "string"), (11..15, "keyword")]
    );
}

/// RC2 Must A: a multiline string with an escape char; the escape ends the first chunk.
#[test]
fn a_multiline_string_keeps_an_escape_at_the_cut() {
    let language = leaked(
        "[[language]]\nname = \"esc\"\nextensions = [\"esc\"]\n\
         [[language.keywords]]\nkind = \"keyword\"\nwords = [\"TASK\"]\n\
         [[language.delimiters]]\nopen = '\"'\nclose = '\"'\nescape = '\\'\nmultiline = true\n",
    );
    let lexer = RuleLexer::new(language);
    let chunks = ["\"abc\\", "\" TASK\""];
    let (spans, end) = paged(&lexer, &chunks);
    assert_eq!(spans, [(0..5, "string"), (5..12, "string")]);
    assert_eq!(whole(&lexer, &chunks), [(0..12, "string")]);
    assert_eq!(end, LineState(0));
}

/// RC2 Must B: delimiters `S...!` and `s...?` on a case-ignored language are told apart exactly.
#[test]
fn the_delimiter_a_chunk_ends_in_is_the_one_the_lexer_matched() {
    let language = leaked(
        "[[language]]\nname = \"cs\"\nextensions = [\"cs\"]\ncase_ignored = true\n\
         [[language.keywords]]\nkind = \"keyword\"\nwords = [\"TASK\"]\n\
         [[language.delimiters]]\nopen = \"S\"\nclose = \"!\"\n\
         [[language.delimiters]]\nopen = \"s\"\nclose = \"?\"\n",
    );
    let lexer = RuleLexer::new(language);
    let chunks = ["sabc", "? TASK"];
    let (spans, _) = paged(&lexer, &chunks);
    assert_eq!(
        spans,
        [(0..4, "string"), (4..5, "string"), (6..10, "keyword")]
    );
    assert_eq!(
        whole(&lexer, &chunks),
        [(0..5, "string"), (6..10, "keyword")]
    );
}

/// RC2 Should C: a chunk cut inside a word does not start a word-opened comment in the next chunk.
#[test]
fn a_word_cut_by_a_chunk_does_not_open_a_comment_in_the_next_chunk() {
    let lexer = RuleLexer::new(rules().for_name("x.m4").expect("m4"));
    let head = format!("{}x", " ".repeat(4095));
    let chunks = [head.as_str(), "dnl tail"];
    let (spans, _) = paged(&lexer, &chunks);
    assert_eq!(spans, whole(&lexer, &chunks));
    assert_eq!(spans, []);
    let line = format!("{head}dnl tail");
    let m4 = rules().for_name("x.m4").expect("m4");
    Line::new(&lexer, m4, &line, LineState(0))
        .check(&[4096])
        .expect("the cut word stays one word");
}

/// A cut that lands inside a token changes only that token, whatever the chunk state says.
#[test]
fn cuts_inside_a_keyword_or_an_opener_change_only_that_token() {
    let language = rules().for_name("x.oil").expect("oil");
    let lexer = RuleLexer::new(language);
    let text = "CPU /* TASK */ TASK // TASK";
    let line = Line::new(&lexer, language, text, LineState(0));
    for cut in 1..text.len() {
        let result = line.check(&[cut]);
        assert!(result.is_ok(), "cut {cut}: {result:?}");
    }
    let whole = lexer.lex_last(text, LineState(0)).0;
    assert_ne!(
        whole,
        line.chunked(&[2]).0,
        "the cut keyword loses its colour"
    );
}
