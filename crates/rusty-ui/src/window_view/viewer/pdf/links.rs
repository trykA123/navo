use gpui_kit::base::TestSupportExt;
use gpui_kit::{ElementId, Point};
use rusty_l10n::t;
use rusty_pdf::protocol::{CharBox, LinkTarget, PageLink};
use url::{Host, Url};

use super::doc::Doc;
use super::select::page_point;
use crate::prelude::*;
use crate::primitives::Tooltip;
use crate::tokens::Theme;

const TIP_MAX: usize = 80;
const OPENED: [&str; 3] = ["http", "https", "mailto"];
const REFUSED: &str = "\"'<>\\^`{|}";
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

fn is_hidden(ch: char) -> bool {
    ch.is_control()
        || matches!(
            ch,
            '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}'
        )
        || matches!(ch, '\u{2066}'..='\u{2069}')
}

pub(super) fn describe(target: &LinkTarget) -> String {
    match target {
        LinkTarget::Page { page, .. } => t!("pdf-link-page", page = (page + 1).to_string()),
        LinkTarget::Uri(uri) => {
            let shown: String = uri.chars().filter(|ch| !is_hidden(*ch)).collect();
            middle_cut(&shown, TIP_MAX)
        }
        LinkTarget::Unsupported => t!("pdf-link-unsupported"),
    }
}

fn is_plain_host(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(domain)) => domain
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.')),
        Some(_) => true,
        None => url.scheme() == "mailto",
    }
}

pub(super) fn web_link(uri: &str) -> Result<Url, Option<String>> {
    let is_odd = |ch: char| is_hidden(ch) || ch.is_whitespace() || REFUSED.contains(ch);
    if uri.chars().any(is_odd) {
        return Err(None);
    }
    let url = Url::parse(uri).map_err(|_| None)?;
    if !OPENED.contains(&url.scheme()) {
        return Err(Some(url.scheme().to_owned()));
    }
    is_plain_host(&url).then_some(url).ok_or(None)
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

fn frame(scale: f32, bounds: CharBox) -> Div {
    div()
        .absolute()
        .left(px(bounds.left * scale))
        .top(px(bounds.top * scale))
        .w(px((bounds.right - bounds.left) * scale))
        .h(px((bounds.bottom - bounds.top) * scale))
}

fn region(t: &Theme, scale: f32, id: ElementId, link: &PageLink) -> AnyElement {
    let bounds = link.bounds;
    let tip = describe(&link.target);
    let shown = tip.clone();
    frame(scale, bounds)
        .id(id)
        .aria_label(tip)
        .test_support()
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
    let ring = doc
        .ring
        .filter(|(on, _)| *on == page)
        .and_then(|(_, index)| links.links().get(index))
        .map(|link| {
            frame(scale, link.bounds)
                .id("pdf-link-ring")
                .test_support()
                .rounded(t.radii.xs)
                .border_2()
                .border_color(t.colors.accent)
                .into_any_element()
        });
    let regions = links.links().iter().enumerate().map(|(index, link)| {
        let id = ElementId::from(("pdf-link", page as usize * MAX_LINKS + index));
        region(t, scale, id, link)
    });
    regions.chain(ring).collect()
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
    fn only_plain_web_and_mail_links_are_opened() {
        let opened = |uri: &str| web_link(uri).map(|url| url.as_str().to_owned());
        assert_eq!(
            opened("HTTPS://Example.org/a"),
            Ok("https://example.org/a".into())
        );
        assert_eq!(opened("mailto:a@b.c"), Ok("mailto:a@b.c".into()));
        assert_eq!(opened("file:///etc/passwd"), Err(Some("file".into())));
        assert_eq!(
            opened("javascript:alert(1)"),
            Err(Some("javascript".into()))
        );
        for hostile in [
            "https://example.org/\"a",
            "https://example.org/a b",
            "https://example.org,/select,C:\\Windows\\System32",
            "https://example.org,/root,/",
            "-https://example.org",
            "--help",
            "https://example.org/\u{7}",
            "https://example.org/\u{202e}gpj.exe",
            "//example.org",
            "c:\\dir",
            "no scheme",
        ] {
            assert_eq!(opened(hostile), Err(None), "{hostile}");
        }
    }

    #[test]
    fn tooltips_drop_control_and_direction_characters() {
        let target = LinkTarget::Uri("https://a.org/\u{202e}txt.exe\u{7}".into());
        assert_eq!(describe(&target), "https://a.org/txt.exe");
    }
}
