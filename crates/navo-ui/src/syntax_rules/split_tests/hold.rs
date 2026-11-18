//! RC3: a pattern a chunk end may only have started is held back for the next chunk (D7). Each
//! language here makes a shorter pattern, a consumed byte or an escape look like the end of a
//! comment or string; every one-cut and two-cut split of its line must colour like the whole.

use navo_syntax::{ChunkState, Inside, Language};
use sidewise_kit::languages::lines::{LineLexer, LineState};

use super::super::state::pack;
use super::super::{RuleLexer, rules};
use super::check::Line;
use super::{WORDY, leaked, paged};

const TASK: &str = "[[language.keywords]]\nkind = \"keyword\"\nwords = [\"TASK\"]\n";

/// A language: its extension, its TOML rules, and lines that every split must keep.
fn cases() -> Vec<(String, Vec<&'static str>)> {
    let head = |name: &str| format!("[[language]]\nname = \"{name}\"\nextensions = [\"{name}\"]\n");
    vec![
        (
            format!(
                "{}line_comments = [\"d\"]\nblock_comments = [[\"dnl\", \"z\"]]\n{TASK}",
                head("dnl")
            ),
            vec!["dnlzTAIL", "dnl", "d TASK", "xdnlzTAIL", "dnlz TASK dnl"],
        ),
        (
            format!(
                "{}line_comments = [\"--\"]\nblock_comments = [[\"--[[\", \"]]\"]]\n{TASK}",
                head("lua")
            ),
            vec!["--[[ x ]] TASK", "-- x TASK", "--[[ x", "--[ TASK"],
        ),
        (
            format!(
                "{}line_comments = [\"//\"]\nblock_comments = [[\"#\", \"/\"]]\n{TASK}",
                head("hash")
            ),
            vec!["#x// TASK", "#x/ TASK", "#x/", "// TASK"],
        ),
        (
            format!(
                "{}{TASK}[[language.delimiters]]\nopen = \"<\"\nclose = \"a!b\"\nescape = \"!\"\n",
                head("lt")
            ),
            vec!["<a!b TASK", "<x!a!b TASK", "<a!", "<a!!b TASK"],
        ),
        (
            format!(
                "{}operators = [\"@#!\"]\nblock_comments = [[\"#\", \"z\"], [\"#!\", \"L\"]]\n{TASK}",
                head("op")
            ),
            vec!["@#! TASK", "@#!#xz TAIL", "@#!#xzTAIL"],
        ),
    ]
}

/// The held-back test languages, so the random split test covers them too.
pub(super) fn languages() -> Vec<&'static Language> {
    cases().iter().map(|(toml, _)| leaked(toml)).collect()
}

/// Every one-cut and two-cut split of `text`, cut on char boundaries.
fn splits(text: &str) -> Vec<Vec<usize>> {
    let ends: Vec<usize> = (1..text.len())
        .filter(|&at| text.is_char_boundary(at))
        .collect();
    let pairs = ends.iter().flat_map(|&first| {
        ends.iter()
            .filter(move |&&at| at > first)
            .map(move |&second| vec![first, second])
    });
    ends.iter().map(|&cut| vec![cut]).chain(pairs).collect()
}

#[test]
fn a_held_back_pattern_keeps_every_split_of_the_line_exact() {
    for (toml, lines) in cases() {
        let language = leaked(&toml);
        let lexer = RuleLexer::new(language);
        for text in lines {
            let line = Line::new(&lexer, language, text, LineState(0));
            for cuts in splits(text) {
                let result = line.check(&cuts);
                assert!(result.is_ok(), "{text:?} cuts {cuts:?}: {result:?}");
            }
        }
    }
}

/// The checker names a leak past the cut token and a block lost across the line end.
#[test]
fn the_checker_flags_bytes_past_the_cut_token_and_a_lost_end_state() {
    let (toml, _) = cases().remove(0);
    let language = leaked(&toml);
    let lexer = RuleLexer::new(language);
    for text in ["dnlzTAIL", "dnl"] {
        let result = Line::new(&lexer, language, text, LineState(0)).check(&[1]);
        assert!(result.is_ok(), "{text:?} cut 1: {result:?}");
    }
}

/// RC2-G2: a cut inside `*/` or `/*` must not leave the block state wrong for the next lines.
#[test]
fn a_cut_inside_a_block_opener_or_closer_keeps_the_block_state() {
    let lexer = RuleLexer::new(rules().for_name("x.oil").expect("oil"));
    let (spans, end) = paged(&lexer, &["CPU /* a *", "/ TASK"]);
    assert_eq!(
        spans,
        [
            (0..3, "keyword"),
            (4..10, "comment"),
            (10..11, "comment"),
            (12..16, "keyword")
        ]
    );
    assert_eq!(end, LineState(0), "the block closed in the second chunk");
    let (spans, end) = paged(&lexer, &["CPU /", "* open TASK"]);
    assert_eq!(spans, [(0..3, "keyword"), (5..16, "comment")]);
    assert_eq!(
        end,
        pack(ChunkState {
            inside: Inside::Block(0),
            ..ChunkState::default()
        })
    );
    assert_eq!(
        lexer.lex_last("TASK */ TASK", end).0,
        [(0..7, "comment"), (8..12, "keyword")]
    );
}

/// RC2-G2: a word-start opener cut off behind a word byte of an earlier chunk is not an opener.
#[test]
fn a_chunk_that_starts_mid_word_does_not_open_a_word_opener() {
    let language = leaked(WORDY);
    let lexer = RuleLexer::new(language);
    let text = "xbegin tail  TASK ";
    let line = Line::new(&lexer, language, text, LineState(0));
    for cuts in [[1, 2], [1, 3], [2, 3], [2, 4]] {
        let result = line.check(&cuts);
        assert!(result.is_ok(), "cuts {cuts:?}: {result:?}");
    }
}

/// RC4-G4: a cut exactly on a token's edge frees no byte, so the plain word next to it is
/// checked: here `TAIL` must stay plain when the cut after `#xz` follows a cut inside `@#!`.
#[test]
fn a_cut_on_a_token_edge_does_not_free_the_plain_word_beside_it() {
    let (toml, _) = cases().pop().expect("the operator case");
    let language = leaked(&toml);
    let lexer = RuleLexer::new(language);
    let line = Line::new(&lexer, language, "@#!#xzTAIL", LineState(0));
    let result = line.check(&[2, 6]);
    assert!(result.is_ok(), "cuts [2, 6]: {result:?}");
}
