//! Selects syntax highlighting for viewer text.

use std::sync::{Arc, OnceLock};

use super::paged::PagedBody;

use sidewise_kit::languages::{
    canonical, detect,
    lines::{LineLanguage, LineLexer},
};

pub(in crate::window_view::viewer) enum Highlight {
    Grammar(&'static str),
    Line(Arc<dyn LineLexer>),
    Plain,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "L3b's choice_of builds Plain and Named")
)]
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
        if name == "log" {
            return Highlight::Line(Arc::new(LineLanguage::Log));
        }
        if let Some(grammar) = canonical(name) {
            return Highlight::Grammar(grammar);
        }
    }
    if *pick.choice == LanguageChoice::Auto && looks_binary(pick.head) {
        return Highlight::Plain;
    }
    match detect(pick.name, pick.head) {
        "text" => Highlight::Line(Arc::new(LineLanguage::Log)),
        grammar => Highlight::Grammar(grammar),
    }
}
/// True when the head holds a NUL byte and starts with no UTF-16 BOM (UTF-16 text has NULs).
fn looks_binary(head: &[u8]) -> bool {
    let utf16 = head.starts_with(b"\xff\xfe") || head.starts_with(b"\xfe\xff");
    !utf16 && head.contains(&0)
}
impl Highlight {
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
    /// Sets the paged body's lexer from this viewer's own name and head (Auto choice).
    ///
    /// Does nothing for other bodies; an unchanged lexer is free (`set_lexer`'s same-name rule).
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
            choice: &LanguageChoice::Auto,
        })
        .paged_lexer();
        paged.set_lexer(lexer, cx);
    }
}

#[cfg(test)]
mod tests;
