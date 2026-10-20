use fluent_bundle::{FluentArgs, FluentResource, concurrent::FluentBundle};
use thiserror::Error;
use unic_langid::LanguageIdentifier;

pub const FILES: [(&str, &str); 10] = [
    ("app.ftl", include_str!("../locales/en-US/app.ftl")),
    (
        "commands.ftl",
        include_str!("../locales/en-US/commands.ftl"),
    ),
    ("files.ftl", include_str!("../locales/en-US/files.ftl")),
    ("jobs.ftl", include_str!("../locales/en-US/jobs.ftl")),
    ("preview.ftl", include_str!("../locales/en-US/preview.ftl")),
    ("viewer.ftl", include_str!("../locales/en-US/viewer.ftl")),
    (
        "settings.ftl",
        include_str!("../locales/en-US/settings.ftl"),
    ),
    ("errors.ftl", include_str!("../locales/en-US/errors.ftl")),
    ("dialogs.ftl", include_str!("../locales/en-US/dialogs.ftl")),
    ("rename.ftl", include_str!("../locales/en-US/rename.ftl")),
];

const LOCALE: &str = "en-US";

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
    #[must_use]
    pub fn build(sources: &[(&str, &str)]) -> (Self, Vec<CatalogError>) {
        let language: LanguageIdentifier = LOCALE.parse().unwrap_or_default();
        let mut bundle = FluentBundle::new_concurrent(vec![language]);
        bundle.set_use_isolating(false);
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

    #[must_use]
    pub fn has(&self, id: &str) -> bool {
        self.bundle.has_message(id)
    }
}
