use super::*;
use crate::{Delimiter, TokenKind};

const BOARD: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<NotepadPlus>
    <UserLang name="Board Script" ext="bsc BSX" udlVersion="2.1">
        <Settings>
            <Global caseIgnored="yes" allowFoldOfComments="no" foldCompact="no" forcePureLC="0" decimalSeparator="0" />
            <Prefix Keywords1="no" Keywords2="yes" Keywords3="no" Keywords4="no" Keywords5="no" Keywords6="no" Keywords7="no" Keywords8="no" />
        </Settings>
        <KeywordLists>
            <Keywords name="Comments">00// /// 00# 01 02 03/* 04*/</Keywords>
            <Keywords name="Numbers, prefix1">0x</Keywords>
            <Keywords name="Numbers, prefix2"></Keywords>
            <Keywords name="Numbers, extras1">A B C D E F</Keywords>
            <Keywords name="Numbers, extras2"></Keywords>
            <Keywords name="Numbers, suffix1"></Keywords>
            <Keywords name="Numbers, suffix2"></Keywords>
            <Keywords name="Numbers, range"></Keywords>
            <Keywords name="Operators1">+ - = &lt; &amp;&amp; ( )</Keywords>
            <Keywords name="Operators2">AND OR</Keywords>
            <Keywords name="Folders in code1, open">{</Keywords>
            <Keywords name="Folders in code1, middle"></Keywords>
            <Keywords name="Folders in code1, close">}</Keywords>
            <Keywords name="Folders in code2, open">begin</Keywords>
            <Keywords name="Folders in code2, middle"></Keywords>
            <Keywords name="Folders in code2, close">end</Keywords>
            <Keywords name="Folders in comment, open"></Keywords>
            <Keywords name="Folders in comment, middle"></Keywords>
            <Keywords name="Folders in comment, close"></Keywords>
            <Keywords name="Keywords1">if else while</Keywords>
            <Keywords name="Keywords2">uint sint</Keywords>
            <Keywords name="Keywords3">print wait</Keywords>
            <Keywords name="Keywords4">ON OFF</Keywords>
            <Keywords name="Keywords5">@inline</Keywords>
            <Keywords name="Keywords6">board pin</Keywords>
            <Keywords name="Keywords7">speed mode</Keywords>
            <Keywords name="Keywords8">$PATH &quot;help all&quot;</Keywords>
            <Keywords name="Delimiters">00&quot; 01\ 02&quot; 03&apos; 04 05&apos; 06 07 08 09[[ 10 11]] 12 13 14 15 16 17 18 19 20 21 22 23</Keywords>
        </KeywordLists>
        <Styles>
            <WordsStyle name="DEFAULT" fgColor="000000" bgColor="FFFFFF" fontName="" fontStyle="0" nesting="0" />
            <WordsStyle name="COMMENTS" fgColor="008000" bgColor="FFFFFF" fontName="" fontStyle="0" nesting="0" />
            <WordsStyle name="KEYWORDS1" fgColor="0000FF" bgColor="FFFFFF" fontName="" fontStyle="1" nesting="0" />
            <WordsStyle name="DELIMITERS1" fgColor="808080" bgColor="FFFFFF" fontName="" fontStyle="0" nesting="0" />
        </Styles>
    </UserLang>
</NotepadPlus>
"#;

fn board() -> Option<Language> {
    from_udl(BOARD).ok()?.into_iter().next()
}

fn owned(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_owned()).collect()
}

fn sorted(mut words: Vec<String>) -> Vec<String> {
    words.sort();
    words
}

#[test]
fn name_and_space_separated_extensions_map() {
    let lang = board();
    assert_eq!(lang.as_ref().map(|l| l.name.as_str()), Some("Board Script"));
    assert_eq!(lang.map(|l| l.extensions), Some(owned(&["bsc", "bsx"])));
}

#[test]
fn case_ignored_yes_maps() {
    assert_eq!(board().map(|l| l.case_ignored), Some(true));
}

#[test]
fn eight_keyword_groups_map_in_order_with_prefix_flags() {
    let groups = board().map(|l| {
        l.keywords
            .into_iter()
            .map(|g| (g.kind, g.words, g.prefix))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        groups,
        Some(vec![
            (TokenKind::Keyword, owned(&["if", "else", "while"]), false),
            (TokenKind::Type, owned(&["uint", "sint"]), true),
            (TokenKind::Function, owned(&["print", "wait"]), false),
            (TokenKind::Constant, owned(&["ON", "OFF"]), false),
            (TokenKind::Attribute, owned(&["@inline"]), false),
            (TokenKind::Tag, owned(&["board", "pin"]), false),
            (TokenKind::Property, owned(&["speed", "mode"]), false),
            (TokenKind::Variable, owned(&["$PATH", "help all"]), false),
        ])
    );
}

#[test]
fn comment_codes_map_with_several_values_per_code() {
    let lang = board();
    assert_eq!(
        lang.as_ref().map(|l| l.line_comments.clone()),
        Some(owned(&["//", "///", "#"]))
    );
    assert_eq!(
        lang.map(|l| l.block_comments),
        Some(vec![("/*".to_owned(), "*/".to_owned())])
    );
}

#[test]
fn both_operator_lists_become_operators() {
    assert_eq!(
        board().map(|l| sorted(l.operators)),
        Some(sorted(owned(&[
            "+", "-", "=", "<", "&&", "(", ")", "AND", "OR"
        ])))
    );
}

#[test]
fn delimiter_codes_map_open_escape_close_and_skip_empty_groups() {
    let string = |open: &str, close: &str, escape| Delimiter {
        open: open.to_owned(),
        close: close.to_owned(),
        escape,
        multiline: true,
        kind: TokenKind::String,
    };
    assert_eq!(
        board().map(|l| l.delimiters),
        Some(vec![
            string("\"", "\"", Some('\\')),
            string("'", "'", None),
            string("[[", "]]", None),
        ])
    );
}

#[test]
fn file_with_two_user_langs_gives_two_languages() {
    let second = BOARD
        .replace("<NotepadPlus>", "")
        .replace("</NotepadPlus>", "")
        .replace("<?xml version=\"1.0\" encoding=\"UTF-8\" ?>", "")
        .replace("Board Script", "Board Lite");
    let both = BOARD.replace("</NotepadPlus>", &format!("{second}</NotepadPlus>"));
    let names = from_udl(&both).map(|langs| langs.into_iter().map(|l| l.name).collect::<Vec<_>>());
    assert_eq!(names, Ok(owned(&["Board Script", "Board Lite"])));
}

#[test]
fn quoted_keywords_keep_their_spaces() {
    let xml = BOARD.replace(
        "if else while",
        "&quot;end if&quot; then &quot;go to&quot; else",
    );
    let first = from_udl(&xml)
        .ok()
        .and_then(|langs| langs.into_iter().next())
        .and_then(|l| l.keywords.into_iter().next())
        .map(|g| g.words);
    assert_eq!(first, Some(owned(&["end if", "then", "go to", "else"])));
}

#[test]
fn malformed_xml_is_an_xml_error() {
    let xml = BOARD.replace("</KeywordLists>", "");
    let err = from_udl(&xml).map(|langs| langs.len());
    assert!(matches!(err, Err(RulesError::Xml { .. })), "{err:?}");
}

#[test]
fn user_lang_without_a_name_is_an_error() {
    let xml = BOARD.replace("name=\"Board Script\" ", "");
    assert_eq!(
        from_udl(&xml).map(|langs| langs.len()),
        Err(RulesError::MissingName)
    );
}
