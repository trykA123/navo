mod preview;
mod target;

use std::ops::ControlFlow;
use std::sync::Arc;

use gpui_kit::SharedString;
use rusty_core::Location;
use rusty_core::find::{FindQuery, TokenKind};
use rusty_fs::CancelToken;
use rusty_fs::find::{FindError, FindProgress, FoundBatch};

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
                message: token.message.clone().map(Into::into),
            })
            .collect();
        for (enabled, key) in [
            (case, "command-center-content-case"),
            (find.regex, "command-center-content-regex"),
        ] {
            if enabled {
                chips.push(ContentChip {
                    label: rusty_l10n::tr(key).into(),
                    message: None,
                });
            }
        }
        Self { find, chips }
    }
}

#[derive(Clone)]
pub struct ContentSource {
    walk: ContentWalk,
}

impl Default for ContentSource {
    fn default() -> Self {
        Self {
            walk: Arc::new(|base, query, cancel, sink| {
                rusty_fs::find::walk_files(base, query, cancel, sink)
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
