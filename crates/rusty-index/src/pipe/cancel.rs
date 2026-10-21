use std::{ops::Deref, sync::Arc};

use super::platform;

#[derive(Clone, Default)]
pub struct CancelToken {
    search: rusty_fs::CancelToken,
    pub(super) event: Arc<platform::CancelEvent>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.search.cancel();
        self.event.signal();
    }
}

impl Deref for CancelToken {
    type Target = rusty_fs::CancelToken;

    fn deref(&self) -> &Self::Target {
        &self.search
    }
}

pub(super) enum Cancellation<'a> {
    #[cfg(windows)]
    Never,
    Poll(&'a dyn Fn() -> bool),
    Tokens(&'a [&'a CancelToken]),
}

impl Cancellation<'_> {
    pub(super) fn is_cancelled(&self) -> bool {
        match self {
            #[cfg(windows)]
            Self::Never => false,
            Self::Poll(cancel) => cancel(),
            Self::Tokens(tokens) => tokens.iter().any(|token| token.is_cancelled()),
        }
    }
}
