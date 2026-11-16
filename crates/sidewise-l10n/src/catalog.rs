//! Embedded Fluent resources and the catalog that resolves message ids.

use fluent_bundle::{FluentArgs, FluentResource, FluentValue, concurrent::FluentBundle};
use thiserror::Error;
use unic_langid::LanguageIdentifier;

pub const FILES: &[(&str, &str)] = &[("compare.ftl", include_str!("../locales/en-US/compare.ftl"))];

const LOCALE: &str = "en-US";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Other,
}

impl Platform {
    pub const CURRENT: Self = if cfg!(windows) {
        Self::Windows
    } else {
        Self::Other
    };

    fn name(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CatalogError {
    #[error("{file} does not parse: {detail}")]
    Parse { file: String, detail: String },
    #[error("{file} redefines an existing message: {detail}")]
    Redefined { file: String, detail: String },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LookupError {
    #[error("no message with this id")]
    Missing,
    #[error("the message has attributes only")]
    NoValue,
    #[error("formatting failed: {0}")]
    Format(String),
}

pub struct Catalog {
    bundle: FluentBundle<FluentResource>,
}

impl Catalog {
    /// Builds a catalog from `sources` for the current platform and returns the errors found.
    #[must_use]
    pub fn build(sources: &[(&str, &str)]) -> (Self, Vec<CatalogError>) {
        Self::build_on(sources, Platform::CURRENT)
    }

    /// Builds a catalog from `sources` for `platform` and returns the errors found.
    #[must_use]
    pub fn build_on(sources: &[(&str, &str)], platform: Platform) -> (Self, Vec<CatalogError>) {
        let _zone = tracing::debug_span!("l10n.catalog").entered();
        let language: LanguageIdentifier = LOCALE.parse().unwrap_or_default();
        let mut bundle = FluentBundle::new_concurrent(vec![language]);
        bundle.set_use_isolating(false);
        let name = platform.name();
        let added = bundle.add_function("PLATFORM", move |_, _| FluentValue::from(name));
        debug_assert!(added.is_ok(), "l10n: PLATFORM is registered once");
        let mut errors = Vec::new();
        for &(file, source) in sources {
            let resource = FluentResource::try_new(source.to_owned()).unwrap_or_else(|failed| {
                errors.push(CatalogError::Parse {
                    file: file.to_owned(),
                    detail: format!("{:?}", failed.1),
                });
                failed.0
            });
            if let Err(clashes) = bundle.add_resource(resource) {
                errors.push(CatalogError::Redefined {
                    file: file.to_owned(),
                    detail: format!("{clashes:?}"),
                });
            }
        }
        (Self { bundle }, errors)
    }

    /// Formats the message `id` with optional `args`; fails when it is missing or malformed.
    pub fn lookup(&self, id: &str, args: Option<&FluentArgs>) -> Result<String, LookupError> {
        let message = self.bundle.get_message(id).ok_or(LookupError::Missing)?;
        let pattern = message.value().ok_or(LookupError::NoValue)?;
        let mut problems = Vec::new();
        let text = self.bundle.format_pattern(pattern, args, &mut problems);
        if problems.is_empty() {
            Ok(text.into_owned())
        } else {
            Err(LookupError::Format(format!("{problems:?}")))
        }
    }

    /// Returns whether the catalog defines the message `id`.
    #[must_use]
    pub fn has(&self, id: &str) -> bool {
        self.bundle.has_message(id)
    }
}
