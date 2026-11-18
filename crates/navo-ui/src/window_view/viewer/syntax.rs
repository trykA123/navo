//! Selects syntax highlighting for viewer text.

use std::sync::{Arc, OnceLock};

use super::paged::PagedBody;

use sidewise_kit::languages::{
    canonical, detect,
    lines::{LineLanguage, LineLexer},
};

use crate::syntax_rules::{RuleLexer, rules};

pub(in crate::window_view::viewer) enum Highlight {
    Grammar(&'static str),
    Line(Arc<dyn LineLexer>),
    Plain,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::window_view::viewer) enum LanguageChoice {
    #[default]
    Auto,
    Plain,
    Named(String),
}
pub(in crate::window_view::viewer) struct Pick<'a> {
    pub name: &'a str,
    pub head: &'a [u8],
    pub choice: &'a LanguageChoice,
    /// The file's byte length when known; a head shorter than it is a sample, not the file.
    pub len: Option<u64>,
}
/// Selects highlighting, reading the support switch once per process.
pub(in crate::window_view::viewer) fn choose(pick: &Pick<'_>) -> Highlight {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    let enabled =
        *ENABLED.get_or_init(|| std::env::var_os("NAVO_SYNTAX").is_none_or(|v| v != "off"));
    choose_with(pick, enabled)
}
/// Selects highlighting for the file and choice using the supplied switch state.
fn choose_with(pick: &Pick<'_>, enabled: bool) -> Highlight {
    if !enabled || *pick.choice == LanguageChoice::Plain {
        return Highlight::Plain;
    }
    if let LanguageChoice::Named(name) = pick.choice {
        if let Some(language) = rules().get(name) {
            return Highlight::Line(RuleLexer::shared(language));
        }
        if name == "log" {
            return Highlight::Line(Arc::new(LineLanguage::Log));
        }
        if let Some(grammar) = canonical(name) {
            return Highlight::Grammar(grammar);
        }
    }
    // Auto, or a Named choice nothing knows: binary guard first, then the extension rules.
    let head_len = u64::try_from(pick.head.len()).unwrap_or(u64::MAX);
    let truncated = pick.len.is_none_or(|len| len > head_len);
    if looks_binary(pick.head, truncated) {
        return Highlight::Plain;
    }
    if let Some(language) = rules().for_name(pick.name) {
        return Highlight::Line(RuleLexer::shared(language));
    }
    match detect(pick.name, pick.head) {
        "text" => Highlight::Line(Arc::new(LineLanguage::Log)),
        grammar => Highlight::Grammar(grammar),
    }
}
/// True when the head is not text: a BOM-less head with a NUL byte, or a head with a
/// UTF-16 or UTF-32LE BOM that is not plausible UTF-16 (shared with the viewer's classifier).
fn looks_binary(head: &[u8], truncated: bool) -> bool {
    if !(head.starts_with(b"\xff\xfe") || head.starts_with(b"\xfe\xff")) {
        return head.contains(&0);
    }
    navo_core::encoding::looks_binary(head) || has_bad_surrogates(head, truncated)
}

/// True when the UTF-16 body after the BOM has a lone surrogate; a high surrogate cut off at
/// the end of a `truncated` head (a sample shorter than the file) is fine.
fn has_bad_surrogates(head: &[u8], truncated: bool) -> bool {
    let little = head.starts_with(b"\xff\xfe");
    let units: Vec<u16> = head[2..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            if little {
                u16::from_le_bytes(*pair)
            } else {
                u16::from_be_bytes(*pair)
            }
        })
        .collect();
    let body = match units.split_last() {
        Some((last, rest)) if truncated && (0xd800..0xdc00).contains(last) => rest,
        _ => &units,
    };
    char::decode_utf16(body.iter().copied()).any(|unit| unit.is_err())
}
impl Highlight {
    /// The language name this highlight stands for (`plain` for none).
    pub fn name(&self) -> &str {
        match self {
            Self::Line(lexer) => lexer.name(),
            Self::Grammar(name) => name,
            Self::Plain => "plain",
        }
    }

    /// Returns the line lexer for paged text, or None for plain text.
    pub fn paged_lexer(self) -> Option<Arc<dyn LineLexer>> {
        match self {
            Self::Line(lexer) => Some(lexer),
            Self::Grammar(name) => Some(Arc::new(
                LineLanguage::for_language(name).unwrap_or(LineLanguage::Log),
            )),
            Self::Plain => None,
        }
    }
}
impl super::Viewer {
    /// Sets the paged body's lexer from this viewer's own name, head and language choice.
    ///
    /// Does nothing for other bodies; an unchanged lexer is free (`set_lexer`'s same-lexer rule).
    pub(in crate::window_view::viewer) fn relex(&self, cx: &mut gpui_kit::App) {
        let Some(paged) = self
            .body
            .as_ref()
            .and_then(|body| PagedBody::of(body.as_ref()))
        else {
            return;
        };
        let sniff = self.sniff();
        let lexer = choose(&Pick {
            name: sniff.name,
            head: sniff.head,
            choice: &self.language,
            len: Some(sniff.len),
        })
        .paged_lexer();
        paged.set_lexer(lexer, cx);
    }
}

#[cfg(test)]
mod tests;
