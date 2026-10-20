use gpui_kit::base::TestSupportExt;
use gpui_kit::{ElementId, Point};
use rusty_l10n::t;
use rusty_pdf::protocol::{CharBox, LinkTarget, PageLink};

use super::doc::Doc;
use super::select::page_point;
use crate::prelude::*;
use crate::primitives::Tooltip;
use crate::tokens::Theme;

const TIP_MAX: usize = 80;
const OPENED: [&str; 3] = ["http", "https", "mailto"];
const MAX_LINKS: usize = 4096;

pub(super) fn middle_cut(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_owned();
    }
    let keep = max.saturating_sub(1);
    let head: String = text.chars().take(keep - keep / 2).collect();
    let tail: String = text.chars().skip(count - keep / 2).collect();
    format!("{head}…{tail}")
}

pub(super) fn describe(target: &LinkTarget) -> String {
    match target {
        LinkTarget::Page { page, .. } => t!("pdf-link-page", page = (page + 1).to_string()),
        LinkTarget::Uri(uri) => middle_cut(uri, TIP_MAX),
        LinkTarget::Unsupported => t!("pdf-link-unsupported"),
    }
}

pub(super) fn scheme_of(uri: &str) -> Option<String> {
    let (scheme, _) = uri.split_once(':')?;
    let is_scheme = scheme.starts_with(|ch: char| ch.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'));
    is_scheme.then(|| scheme.to_ascii_lowercase())
}

pub(super) fn is_opened(scheme: &str) -> bool {
    OPENED.contains(&scheme)
}

fn holds(bounds: &CharBox, x: f32, y: f32) -> bool {
    (bounds.left..=bounds.right).contains(&x) && (bounds.top..=bounds.bottom).contains(&y)
}

pub(super) fn link_at(doc: &Doc, at: Point<f32>) -> Option<&PageLink> {
    let (page, x, y) = page_point(doc, at)?;
    doc.links_on(page)?
        .links()
        .iter()
        .find(|link| holds(&link.bounds, x, y))
}

fn region(t: &Theme, scale: f32, id: ElementId, link: &PageLink) -> AnyElement {
    let bounds = link.bounds;
    let tip = describe(&link.target);
    let shown = tip.clone();
    div()
        .id(id)
        .aria_label(tip)
        .test_support()
        .absolute()
        .left(px(bounds.left * scale))
        .top(px(bounds.top * scale))
        .w(px((bounds.right - bounds.left) * scale))
        .h(px((bounds.bottom - bounds.top) * scale))
        .rounded(t.radii.xs)
        .when(link.target != LinkTarget::Unsupported, |region| {
            region.cursor_pointer()
        })
        .tooltip(move |window, cx| Tooltip::new(shown.clone()).build(window, cx))
        .into_any_element()
}

pub(super) fn link_regions(t: &Theme, doc: &Doc, page: u32) -> Vec<AnyElement> {
    let Some(links) = doc.links_on(page) else {
        return Vec::new();
    };
    let scale = doc.scale();
    links
        .links()
        .iter()
        .enumerate()
        .map(|(index, link)| {
            let id = ElementId::from(("pdf-link", page as usize * MAX_LINKS + index));
            region(t, scale, id, link)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_loses_its_middle() {
        let long = format!("https://example.org/{}", "a".repeat(200));
        let cut = middle_cut(&long, TIP_MAX);
        assert_eq!(cut.chars().count(), TIP_MAX);
        assert!(cut.starts_with("https://") && cut.ends_with("aaa") && cut.contains('…'));
        assert_eq!(middle_cut("short", TIP_MAX), "short");
    }

    #[test]
    fn only_web_and_mail_schemes_are_opened() {
        let opened = |uri: &str| scheme_of(uri).is_some_and(|scheme| is_opened(&scheme));
        assert!(opened("https://example.org") && opened("HTTP://x") && opened("mailto:a@b.c"));
        assert!(!opened("file:///etc/passwd") && !opened("javascript:alert(1)"));
        assert!(!opened("//example.org") && !opened("c:\\dir"));
        assert_eq!(scheme_of("no scheme"), None);
    }
}
