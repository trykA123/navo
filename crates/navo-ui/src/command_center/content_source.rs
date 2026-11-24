mod preview;
mod target;

use std::ops::ControlFlow;
use std::sync::Arc;

use gpui_kit::SharedString;
use navo_core::Location;
use navo_core::find::{FindQuery, TokenError, TokenKind};
use navo_fs::CancelToken;
use navo_fs::find::{FindError, FindProgress, FoundBatch};

pub(crate) use preview::{Preview, PreviewEvent};

pub(crate) type ContentWalk = Arc<
    dyn Fn(
            &Location,
            &FindQuery,
            &CancelToken,
            &mut dyn FnMut(FoundBatch) -> ControlFlow<()>,
        ) -> Result<FindProgress, FindError>
        + Send
        + Sync,
>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentChip {
    pub label: SharedString,
    pub message: Option<SharedString>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContentQuery {
    pub find: FindQuery,
    pub chips: Vec<ContentChip>,
}

impl ContentQuery {
    #[must_use]
    pub fn parse(input: &str, case: bool, regex: bool) -> Self {
        let mut find = FindQuery::parse(input);
        find.case = case;
        find.regex |= regex;
        let mut chips: Vec<_> = find
            .tokens
            .iter()
            .filter(|token| token.kind != TokenKind::Text || token.message.is_some())
            .map(|token| ContentChip {
                label: input[token.span.clone()].to_owned().into(),
                message: token.message.as_ref().map(token_message),
            })
            .collect();
        for (enabled, key) in [
            (case, "command-center-content-case"),
            (find.regex, "command-center-content-regex"),
        ] {
            if enabled {
                chips.push(ContentChip {
                    label: navo_l10n::tr(key).into(),
                    message: None,
                });
            }
        }
        Self { find, chips }
    }
}

/// Resolves a rejected token's error through its Fluent message.
fn token_message(error: &TokenError) -> SharedString {
    match error {
        TokenError::UnclosedQuote => navo_l10n::t!("find-token-error-unclosed-quote"),
        TokenError::Size => navo_l10n::t!("find-token-error-size"),
        TokenError::Modified => navo_l10n::t!("find-token-error-modified"),
        TokenError::Attribute => navo_l10n::t!("find-token-error-attribute"),
        TokenError::Within => navo_l10n::t!("find-token-error-within"),
        TokenError::Archives => navo_l10n::t!("find-token-error-archives"),
        TokenError::Git => navo_l10n::t!("find-token-error-git"),
        TokenError::Regex => navo_l10n::t!("find-token-error-regex"),
        TokenError::RegexSyntax(error) => {
            navo_l10n::t!("find-token-error-regex-syntax", error = error.as_str())
        }
        TokenError::Extensions => navo_l10n::t!("find-token-error-extensions"),
        TokenError::Name => navo_l10n::t!("find-token-error-name"),
        TokenError::Depth { max } => {
            navo_l10n::t!("find-token-error-depth", max = i64::from(*max))
        }
        TokenError::Toggle { key } => navo_l10n::t!("find-token-error-toggle", key = *key),
        TokenError::Hex { max } => navo_l10n::t!("find-token-error-hex", max = i64::from(*max)),
        TokenError::Encoding => navo_l10n::t!("find-token-error-encoding"),
    }
    .into()
}

#[derive(Clone)]
pub struct ContentSource {
    walk: ContentWalk,
}

impl Default for ContentSource {
    fn default() -> Self {
        Self {
            walk: Arc::new(|base, query, cancel, sink| {
                navo_fs::find::walk_files(base, query, cancel, sink)
            }),
        }
    }
}

impl ContentSource {
    pub(crate) fn start(&self, base: Location, query: FindQuery, cancel: CancelToken) -> Preview {
        Preview::start(self.walk.clone(), base, query, cancel)
    }

    #[cfg(test)]
    pub(crate) fn with_walk(walk: ContentWalk) -> Self {
        Self { walk }
    }
}
