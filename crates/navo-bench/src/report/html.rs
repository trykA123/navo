use super::results::Results;
use super::style::CSS;
use super::{Document, data, safety, sections};
use crate::privacy::Html;

pub(super) fn open_section(page: &mut Html, id: &'static str, title: &'static str) {
    page.markup("<section id=\"")
        .markup(id)
        .markup("\"><h2>")
        .markup(title)
        .markup("</h2>");
}

pub(super) fn close_section(page: &mut Html) {
    page.markup("</section>");
}

/// The whole report as one file: inline style, no script, no external resource.
pub(super) fn page(document: &Document, results: &Results) -> Html {
    let mut page = Html::new();
    page.markup("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">")
        .markup("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">")
        .markup("<meta name=\"color-scheme\" content=\"light dark\">")
        .markup("<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'\">")
        .markup("<title>Navo bench report</title><style>")
        .markup(CSS)
        .markup("</style></head><body><main><h1>Navo bench report</h1>")
        .markup("<p class=\"muted\">Schema version ")
        .uint(u64::from(document.schema_version))
        .markup(". Numbers only: no file names, paths or queries.</p>");
    sections::summary(&mut page, document.tally);
    safety::section(&mut page, results);
    sections::targets(&mut page, document);
    sections::start_and_idle(&mut page, results);
    data::index(&mut page, results);
    data::timings(&mut page, results);
    data::profile(&mut page, results);
    data::machine(&mut page, results);
    data::baselines(&mut page);
    page.markup("</main></body></html>\n");
    page
}
