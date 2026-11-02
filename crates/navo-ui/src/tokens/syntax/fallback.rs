use super::Style;

pub(super) struct Fallback {
    pub name: &'static str,
    pub from: &'static str,
    pub style: Option<Style>,
}

const fn same(name: &'static str, from: &'static str) -> Fallback {
    Fallback {
        name,
        from,
        style: None,
    }
}

const fn styled(name: &'static str, from: &'static str, style: Style) -> Fallback {
    Fallback {
        name,
        from,
        style: Some(style),
    }
}

pub(super) const FALLBACKS: [Fallback; 16] = [
    styled("emphasis", "variable", Style::Italic),
    styled("emphasis.strong", "variable", Style::Bold),
    same("enum", "type"),
    same("hint", "comment.doc"),
    same("label", "attribute"),
    same("predictive", "comment.doc"),
    same("preproc", "keyword"),
    same("primary", "variable"),
    same("punctuation", "punctuation.delimiter"),
    same("punctuation.list_marker", "punctuation.delimiter"),
    same("punctuation.special", "operator"),
    same("string.special", "string"),
    same("string.special.symbol", "constant"),
    same("tag.doctype", "tag"),
    same("text.code.span", "text.literal"),
    same("variant", "constructor"),
];
