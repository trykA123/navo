use std::collections::HashSet;
use std::sync::{LazyLock, Mutex, PoisonError};

use fluent_bundle::FluentArgs;

use crate::catalog::{Catalog, FILES, LookupError};

static CATALOG: LazyLock<Catalog> = LazyLock::new(|| {
    let (catalog, errors) = Catalog::build(&files());
    for error in &errors {
        tracing::error!(%error, "l10n: embedded locale file is broken");
    }
    debug_assert!(errors.is_empty(), "l10n: {errors:?}");
    catalog
});

static REPORTED: LazyLock<Reported> = LazyLock::new(Reported::default);

#[derive(Default)]
pub(crate) struct Reported(Mutex<HashSet<String>>);

impl Reported {
    pub(crate) fn first(&self, id: &str) -> bool {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id.to_owned())
    }
}

#[cfg(not(test))]
fn files() -> Vec<(&'static str, &'static str)> {
    FILES.to_vec()
}

#[cfg(test)]
fn files() -> Vec<(&'static str, &'static str)> {
    let mut files = FILES.to_vec();
    files.push(("fixture.ftl", crate::tests::FIXTURE));
    files
}

#[must_use]
pub fn catalog() -> &'static Catalog {
    &CATALOG
}

#[must_use]
pub fn tr(id: &str) -> String {
    resolve(id, None)
}

#[must_use]
pub fn tr_args(id: &str, args: &FluentArgs) -> String {
    resolve(id, Some(args))
}

fn resolve(id: &str, args: Option<&FluentArgs>) -> String {
    let result = catalog().lookup(id, args);
    if let Err(error) = &result {
        report(id, error);
    }
    debug_assert!(result.is_ok(), "l10n lookup of {id} failed: {result:?}");
    result.unwrap_or_else(|_| id.to_owned())
}

fn report(id: &str, error: &LookupError) {
    if REPORTED.first(id) {
        tracing::warn!(id, %error, "l10n: lookup failed, showing the id");
    }
}
