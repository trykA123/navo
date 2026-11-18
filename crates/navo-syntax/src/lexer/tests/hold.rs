//! Hold-back at a chunk end: a comment or string pattern the chunk may only have started is
//! carried to the next chunk, so a shorter pattern never wins early and a consumed byte never
//! starts a new one.

use super::chunk::{chunked, lexer, whole};
use super::*;
use crate::{ChunkState, Inside};

/// A language with the given comments and strings and a `TASK` keyword.
fn lang(
    lines: &[&str],
    blocks: &[(&str, &str)],
    delimiters: Vec<Delimiter>,
    operators: &[&str],
) -> Language {
    Language {
        name: "hold".to_owned(),
        extensions: owned(&["hld"]),
        case_ignored: false,
        keywords: vec![KeywordGroup {
            kind: K::Keyword,
            words: owned(&["TASK"]),
            prefix: false,
        }],
        line_comments: owned(lines),
        block_comments: blocks
            .iter()
            .map(|(open, close)| ((*open).to_owned(), (*close).to_owned()))
            .collect(),
        delimiters,
        operators: owned(operators),
        numbers: false,
    }
}

/// The colour of every byte of a line, from its tokens.
fn colours(tokens: &[(usize, usize, K)], len: usize) -> Vec<Option<K>> {
    let mut roles = vec![None; len];
    for &(from, to, kind) in tokens {
        roles[from..to].fill(Some(kind));
    }
    roles
}

/// Checks that `chunks` colour like the whole line and end in the whole line's state. Bytes of a
/// whole-line token that lie before a cut inside it belong to an earlier chunk and may differ.
fn same(lang: &Language, chunks: &[&str]) {
    let line = chunks.concat();
    let (split, state) = chunked(lang, chunks);
    let whole = whole(lang, &line);
    let (a, b) = (colours(&split, line.len()), colours(&whole, line.len()));
    let exempt = |byte: usize| {
        let mut at = 0;
        chunks[..chunks.len() - 1].iter().any(|chunk| {
            at += chunk.len();
            whole.iter().any(|t| t.0 <= byte && byte < at && at < t.1)
        })
    };
    for byte in 0..line.len() {
        assert!(
            a[byte] == b[byte] || exempt(byte),
            "{chunks:?} byte {byte}: split {split:?}, whole {whole:?}"
        );
    }
    let (_, end) = lexer(lang).line(&line, LineState::Normal);
    assert_eq!(state.line_state(), end, "{chunks:?}: the state at the end");
}

#[test]
fn a_shorter_line_comment_does_not_win_over_a_longer_block_opener_cut_after_it() {
    let lua = lang(&["--"], &[("--[[", "]]")], vec![], &[]);
    same(&lua, &["--", "[[ x ]] TASK"]);
    same(&lua, &["-", "-[[ x ]] TASK"]);
    same(&lua, &["--[", "[ x ]] TASK"]);
    same(&lua, &["--[[ x ]", "] TASK"]);
}

#[test]
fn a_closer_byte_is_not_the_start_of_the_next_opener() {
    let hash = lang(&["//"], &[("#", "/")], vec![], &[]);
    same(&hash, &["#x", "/", "/ TASK"]);
    same(&hash, &["#x/", "/ TASK"]);
    same(&hash, &["#x", "//", " TASK"]);
}

#[test]
fn a_pending_escape_does_not_erase_a_pending_closer() {
    let lt = lang(&[], &[], vec![delimiter("<", "a!b", Some('!'), false)], &[]);
    same(&lt, &["<a!", "b TASK"]);
    same(&lt, &["<a", "!b TASK"]);
    same(&lt, &["<", "a!b TASK"]);
    same(&lt, &["<x!", "a!b TASK"]);
    same(&lt, &["<a!", "b", " TASK"]);
}

#[test]
fn a_line_opener_cut_in_front_of_a_longer_block_opener_holds_back_the_cut_bytes() {
    let dnl = lang(&["d"], &[("dnl", "z")], vec![], &[]);
    same(&dnl, &["d", "nlzTAIL"]);
    same(&dnl, &["d", "nl"]);
    let (_, next) = lexer(&dnl).chunk("d", ChunkState::default(), false);
    assert_eq!(
        next,
        ChunkState {
            inside: Inside::Opening(0),
            mid_word: false,
            partial: 1
        }
    );
}

#[test]
fn prefix_bytes_of_a_held_back_opener_do_not_count_against_the_token_cap() {
    let plus = lang(&["+?"], &[], vec![], &["+"]);
    let lexer = lexer(&plus);
    let (_, state) = lexer.chunk("+", ChunkState::default(), false);
    let (tokens, _) = lexer.chunk(&"+".repeat(MAX_TOKENS), state, true);
    assert_eq!(tokens.len(), MAX_TOKENS);
}

/// RC4-G4: an operator the chunk end cut counts as a pattern that may still be completing, so a
/// block opener inside its bytes is not replayed on its own.
#[test]
fn a_cut_operator_is_held_back_whole_and_hides_the_opener_inside_it() {
    let at = lang(&[], &[("#", "!"), ("#!", "z")], vec![], &["@#!"]);
    same(&at, &["@#", "! TASK"]);
    same(&at, &["@", "#! TASK"]);
    same(&at, &["@#!", " TASK"]);
    same(&at, &["@#!#xz", "TAIL"]);
    let (_, next) = lexer(&at).chunk("@#", ChunkState::default(), false);
    assert_eq!((next.inside, next.partial), (Inside::Opening(1), 2));
}

/// RC4-G4: a special keyword with a non-word byte may also be cut by the chunk end.
#[test]
fn a_cut_special_keyword_is_held_back_whole_and_hides_the_opener_inside_it() {
    let mut dollar = lang(&[], &[("#", "!"), ("#!", "z")], vec![], &[]);
    dollar.keywords.push(KeywordGroup {
        kind: K::Variable,
        words: owned(&["@#!"]),
        prefix: false,
    });
    same(&dollar, &["@#", "! TASK"]);
    same(&dollar, &["@", "#! TASK"]);
}

/// RC4-G4: a pattern over the 64-byte limit is not part of a directly built lexer, so the chunk
/// state never has to carry more than 64 bytes of it.
#[test]
fn a_pattern_over_the_byte_limit_is_dropped_by_the_lexer() {
    let long = format!("{}!", "a".repeat(64));
    assert_eq!(long.len(), 65);
    let big = lang(&[], &[("#", &long)], vec![], &[]);
    let line = format!("#{long} TASK");
    let cut = line.len() - " TASK".len() - 1;
    same(&big, &[&line[..cut], &line[cut..]]);
    let held = lang(&["x"], &[("#", &long)], vec![], &[]);
    same(&held, &[&line[..cut], &line[cut..]]);
}
