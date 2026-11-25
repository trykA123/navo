use std::sync::atomic::{AtomicUsize, Ordering};

use resvg::usvg::{Group, ImageHrefResolver, Node, Tree};

use super::*;

/// No output may contain any of these.
const FORBIDDEN: [&str; 8] = [
    "script", "href", "url(", "style", "<image", "<text", "&", "<!",
];

/// Normalises `svg`, checks the shared output rules, and checks that the output is a fixed point.
fn clean(svg: &str, mono: bool) -> Cleaned {
    let cleaned = normalise(svg.as_bytes(), mono).unwrap();
    let text = out(&cleaned);
    for word in FORBIDDEN {
        assert!(!text.contains(word), "the output keeps `{word}`: {text}");
    }
    let again = normalise(&cleaned.svg, mono).unwrap();
    assert_eq!(
        out(&again),
        text,
        "normalising the output again must not change it"
    );
    assert!(
        again.removed.is_empty(),
        "a clean output has nothing left to remove"
    );
    cleaned
}

fn out(cleaned: &Cleaned) -> &str {
    std::str::from_utf8(&cleaned.svg).unwrap()
}

fn has_image(group: &Group) -> bool {
    group.children().iter().any(|node| match node {
        Node::Image(_) => true,
        Node::Group(group) => has_image(group),
        _ => false,
    })
}

const SCRIPT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" onload="alert(1)">
<script>alert(1)</script><script href="https://example.invalid/x.js"/>
<path d="M2 2h20v20H2z" onclick="steal()" onmouseover="steal()"/></svg>"#;

const FOREIGN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
<foreignObject width="24" height="24"><div xmlns="http://www.w3.org/1999/xhtml"><script>alert(1)</script>
<a href="https://example.invalid">x</a></div></foreignObject><path d="M0 0h4v4z"/></svg>"#;

const STYLED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
<style>.a { fill: url(#g); } @import url(https://example.invalid/x.css);</style>
<defs><linearGradient id="g"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/></linearGradient>
<pattern id="p" width="2" height="2"><path d="M0 0h1v1z"/></pattern></defs>
<path class="a" d="M0 0h10v10z"/><path style="fill: url(#p); stroke: #00ff00" d="M12 12h10v10z"/></svg>"##;

const LUCIDE_CIRCLE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/></svg>"#;

const RICH: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48" width="96" height="96">
<g transform="translate(10 0)" opacity="0.5"><path d="M0 0h2v2h-2z" fill="#336699"/></g>
<path d="M1 1Q5 9 9 1C12 4 14 4 17 1" fill="none" stroke="#ff8800" stroke-width="1.5" stroke-linejoin="bevel"/>
<path d="M20 20h8v8h-8zM22 22h4v4h-4z" fill-rule="evenodd" fill="#123abc" stroke="#000" opacity="0.25"/>
<g filter="url(#f)" mask="url(#m)" clip-path="url(#c)"><rect x="30" y="30" width="4" height="4"/></g>
<defs><filter id="f"><feGaussianBlur stdDeviation="2"/></filter><mask id="m"><rect width="48" height="48" fill="#fff"/></mask>
<clipPath id="c"><rect width="40" height="40"/></clipPath></defs></svg>"##;

#[test]
fn sanitise_drops_script_and_handlers() {
    let cleaned = clean(SCRIPT, false);
    let text = out(&cleaned);
    assert!(
        !text.contains("alert") && !text.contains("steal") && !text.contains("on"),
        "{text}"
    );
    assert!(
        text.contains(r##"<path d="M2 2L22 2L22 22L2 22Z" fill="#000000"/>"##),
        "{text}"
    );
}

#[test]
fn sanitise_never_resolves_external_href() {
    let folder = tempfile::tempdir().unwrap();
    let inner = folder.path().join("inner.svg");
    std::fs::write(&inner, br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 4 4"><path d="M0 0h4v4z" fill="#123456"/></svg>"##).unwrap();
    let fixture = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 24 24">
<image href="file:///C:/Windows/win.ini" width="4" height="4"/><image xlink:href="{}" width="4" height="4"/>
<image href="data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCA0IDQiPjxwYXRoIGQ9Ik0wIDBoNHY0eiIgZmlsbD0iIzEyMzQ1NiIvPjwvc3ZnPg==" width="4" height="4"/>
<use href="https://example.invalid/x.svg#a"/><path d="M0 0h4v4z"/></svg>"#,
        inner.display()
    );
    let asked = AtomicUsize::new(0);
    let reader = ImageHrefResolver::default_string_resolver();
    let mut counting = Options::default();
    counting.image_href_resolver.resolve_string = Box::new(|href, options| {
        asked.fetch_add(1, Ordering::SeqCst);
        reader(href, options)
    });
    let control = Tree::from_str(&fixture, &counting).unwrap();
    assert!(
        asked.load(Ordering::SeqCst) >= 1 && has_image(control.root()),
        "the fixture names a readable file"
    );
    let ours = Tree::from_str(&fixture, &options()).unwrap();
    assert!(
        !has_image(ours.root()),
        "our options must never load an image, from a file or a data URL"
    );
    let cleaned = clean(&fixture, false);
    assert!(
        !out(&cleaned).contains("#123456"),
        "nothing from the linked file reaches the output"
    );
    assert_eq!(cleaned.removed, [Removed::Image]);
}

#[test]
fn sanitise_drops_foreign_object() {
    let text = out(&clean(FOREIGN, false)).to_owned();
    assert!(
        !text.contains("foreign") && !text.contains("div") && !text.contains("example"),
        "{text}"
    );
}

#[test]
fn sanitise_drops_style_and_url_paint() {
    let cleaned = clean(STYLED, false);
    let text = out(&cleaned);
    assert!(
        text.contains(r##"fill="#ff0000""##),
        "the class rule still paints: {text}"
    );
    assert!(
        text.contains(r##"fill="none" stroke="#00ff00""##),
        "a pattern fill becomes no paint: {text}"
    );
    assert_eq!(cleaned.removed, [Removed::Gradient, Removed::Pattern]);
}

#[test]
fn sanitise_refuses_entities() {
    let laughs = r#"<?xml version="1.0"?><!DOCTYPE svg [<!ENTITY a "lol"><!ENTITY b "&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;">
<!ENTITY c "&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;">]><svg xmlns="http://www.w3.org/2000/svg"><text>&c;</text></svg>"#;
    assert_eq!(normalise(laughs.as_bytes(), false), Err(SvgError::Doctype));
    let lower =
        r#"<!doctype svg><svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0h1v1z"/></svg>"#;
    assert_eq!(normalise(lower.as_bytes(), false), Err(SvgError::Doctype));
    let entity_only = r#"<svg xmlns="http://www.w3.org/2000/svg"><!-- <!eNtItY x "y"> --><path d="M0 0h1v1z"/></svg>"#;
    assert_eq!(
        normalise(entity_only.as_bytes(), false),
        Err(SvgError::Doctype)
    );
}

/// A valid one-path SVG padded with whitespace to exactly `len` bytes.
fn padded(len: usize) -> Vec<u8> {
    let head =
        br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 4 4"><path d="M0 0h4v4z"/>"#;
    let tail = b"</svg>";
    let mut svg = head.to_vec();
    svg.resize(len - tail.len(), b' ');
    svg.extend_from_slice(tail);
    svg
}

#[test]
fn sanitise_refuses_over_cap() {
    assert_eq!(
        normalise(&padded(SVG_CAP + 1), false),
        Err(SvgError::TooLarge)
    );
    for len in [SVG_CAP - 1, SVG_CAP] {
        assert!(
            normalise(&padded(len), false).is_ok(),
            "{len} bytes are within the cap"
        );
    }
}

#[test]
fn sanitise_refuses_gzip() {
    let gzip = [0x1F, 0x8B, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03];
    assert_eq!(normalise(&gzip, false), Err(SvgError::Compressed));
    let latin1 = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><title>na\xEFve</title></svg>";
    assert_eq!(normalise(latin1, false), Err(SvgError::NotUtf8));
    assert!(matches!(normalise(b"<svg", false), Err(SvgError::Parse(_))));
    let empty = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 4 4"><path d="M0 0h4v4z" fill="none"/></svg>"#;
    assert_eq!(normalise(empty.as_bytes(), false), Err(SvgError::Empty));
}

fn rects(count: usize) -> String {
    let body = r#"<rect x="0" y="0" width="1" height="1"/>"#.repeat(count);
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 4 4">{body}</svg>"#)
}

#[test]
fn sanitise_refuses_too_many_paths() {
    assert_eq!(
        normalise(rects(MAX_PATHS + 1).as_bytes(), false),
        Err(SvgError::TooManyPaths)
    );
    assert!(normalise(rects(MAX_PATHS).as_bytes(), false).is_ok());
}

#[test]
fn sanitise_turns_gradient_into_first_stop() {
    let radial = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 4 4"><radialGradient id="r">
<stop offset="0.2" stop-color="#abcdef"/><stop offset="1" stop-color="#000"/></radialGradient>
<path d="M0 0h4v4z" fill="url(#r)" stroke="url(#r)"/></svg>"##;
    let cleaned = clean(radial, false);
    assert!(
        out(&cleaned).contains(r##"fill="#abcdef" stroke="#abcdef""##),
        "{}",
        out(&cleaned)
    );
    assert_eq!(cleaned.removed, [Removed::Gradient]);
}

#[test]
fn sanitise_lists_removed_text() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><text x="0" y="12">hi</text><path d="M0 0h4v4z"/></svg>"#;
    let cleaned = clean(svg, false);
    assert_eq!(cleaned.removed, [Removed::Text]);
    assert_eq!(Removed::Image.label(), "embedded images");
}

#[test]
fn sanitise_keeps_lucide_stroke() {
    let text = out(&clean(LUCIDE_CIRCLE, false)).to_owned();
    assert!(
        text.starts_with(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M"#
        ),
        "{text}"
    );
    assert!(
        text.contains(r##" fill="none" stroke="#000000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>"##),
        "{text}"
    );
    assert_eq!(text.matches("<path").count(), 1);
}

#[test]
fn sanitise_mono_paints_black() {
    for fixture in [STYLED, RICH, LUCIDE_CIRCLE] {
        let text = out(&clean(fixture, true)).to_owned();
        let colours: Vec<&str> = text.split('#').skip(1).map(|rest| &rest[..6]).collect();
        assert!(
            !colours.is_empty() && colours.iter().all(|colour| *colour == "000000"),
            "{text}"
        );
    }
}

#[test]
fn sanitise_output_is_a_fixed_point() {
    let cleaned = clean(RICH, false);
    let text = out(&cleaned);
    assert_eq!(
        cleaned.removed,
        [Removed::Filter, Removed::Mask, Removed::ClipPath]
    );
    assert!(
        text.starts_with(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96">"#),
        "{text}"
    );
    assert!(
        text.contains(r##"<path d="M20 0L24 0L24 4L20 4Z" fill="#336699" opacity="0.5"/>"##),
        "{text}"
    );
    assert!(
        text.contains("Q") && text.contains("C"),
        "curves stay curves: {text}"
    );
    assert!(
        text.contains(r#"fill-rule="evenodd""#) && text.contains(r#"stroke-linejoin="bevel""#),
        "{text}"
    );
    assert!(
        text.contains(r#"stroke-width="3""#),
        "the stroke scales with the document: {text}"
    );
    assert!(
        text.contains(r##"<path d="M60 60L68 60L68 68L60 68Z" fill="#000000"/>"##),
        "a filtered group keeps its children: {text}"
    );
}

#[test]
fn sanitise_numbers_have_three_decimals_at_most() {
    assert_eq!(write::number(1.0), "1");
    assert_eq!(write::number(-0.0004), "0");
    assert_eq!(write::number(2.5), "2.5");
    assert_eq!(write::number(1.23456), "1.235");
    assert_eq!(write::number(f32::NAN), "0");
}

/// `<use>` levels that each repeat the level below ten times: 10^depth paths from ~1 KB.
fn use_fan_out(depth: usize) -> String {
    let mut defs = String::from(r#"<path id="l0" d="M0 0h1v1h-1z"/>"#);
    for level in 1..=depth {
        let uses = format!(r##"<use href="#l{}"/>"##, level - 1).repeat(10);
        defs.push_str(&format!(r#"<g id="l{level}">{uses}</g>"#));
    }
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 4 4"><defs>{defs}</defs><use href="#l{depth}"/></svg>"##
    )
}

#[test]
fn sanitise_refuses_use_fan_out() {
    for depth in [5, 6] {
        assert_eq!(
            normalise(use_fan_out(depth).as_bytes(), false),
            Err(SvgError::TooComplex),
            "the pre-pass stops 10^{depth} paths before parsing"
        );
    }
}

/// `M0 0` then `segments` line segments of two numbers each.
fn zigzag(segments: usize) -> String {
    let mut data = String::from("M0 0");
    for at in 0..segments {
        data.push_str(if at % 2 == 0 { "l1 1" } else { "l1-1" });
    }
    data
}

/// `levels` nested groups that each `<use>` the level below `fan` times, over one leaf path.
fn nested_uses(levels: usize, fan: usize, segments: usize) -> String {
    let mut defs = format!(r#"<path id="g0" fill="red" d="{}"/>"#, zigzag(segments));
    for level in 1..=levels {
        let uses = format!(r##"<use href="#g{}"/>"##, level - 1).repeat(fan);
        defs.push_str(&format!(r#"<g id="g{level}">{uses}</g>"#));
    }
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><defs>{defs}</defs><use href="#g{levels}"/></svg>"##
    )
}

/// One path `<use>`d `count` times side by side.
fn flat_uses(count: usize, data: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><defs><path id="a" fill="red" d="{data}"/></defs>{}</svg>"##,
        r##"<use href="#a"/>"##.repeat(count)
    )
}

/// A marker of `segments` segments on every vertex of a path of `segments` segments; with
/// `css` the marker is set by a style sheet instead of an attribute.
fn marker_mid(segments: usize, css: bool) -> String {
    let path = zigzag(segments);
    let (style, attribute) = if css {
        ("<style>path { marker-mid: url(#m) }</style>", "")
    } else {
        ("", r#" marker-mid="url(#m)""#)
    };
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">{style}<marker id="m" overflow="visible"><path fill="red" d="{path}"/></marker><path fill="none" stroke="#000"{attribute} d="{path}"/></svg>"##
    )
}

/// `count` nested `open` elements around one path.
fn nested(count: usize, open: &str, close: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">{}<path d="M0 0L5 5L0 5Z"/>{}</svg>"#,
        open.repeat(count),
        close.repeat(count)
    )
}

#[test]
fn sanitise_refuses_nested_use_bomb() {
    let bomb = nested_uses(4, 9, 100);
    assert!(bomb.len() < 2_000, "a small file: {}", bomb.len());
    assert_eq!(normalise(bomb.as_bytes(), false), Err(SvgError::TooComplex));
}

#[test]
fn sanitise_refuses_flat_use_bomb() {
    let bomb = flat_uses(100, &zigzag(3_000));
    assert_eq!(normalise(bomb.as_bytes(), false), Err(SvgError::TooComplex));
}

#[test]
fn sanitise_refuses_marker_bomb() {
    for css in [false, true] {
        let bomb = marker_mid(600, css);
        assert_eq!(
            normalise(bomb.as_bytes(), false),
            Err(SvgError::TooComplex),
            "css {css}"
        );
    }
}

#[test]
fn sanitise_refuses_deep_nesting() {
    for (open, close) in [("<g>", "</g>"), (r#"<svg viewBox="0 0 24 24">"#, "</svg>")] {
        let deep = nested(1_000, open, close);
        assert_eq!(
            normalise(deep.as_bytes(), false),
            Err(SvgError::TooDeep),
            "{open}"
        );
    }
    clean(&nested(60, "<g>", "</g>"), false);
}

#[test]
fn sanitise_refuses_output_over_cap() {
    let data: String = std::iter::once("M0 0")
        .chain(std::iter::repeat_n("h1v1", 1_000))
        .collect();
    let big = flat_uses(50, &data);
    assert!(big.len() < SVG_CAP);
    assert_eq!(normalise(big.as_bytes(), false), Err(SvgError::TooLarge));
}

#[test]
fn sanitise_keeps_small_uses_and_markers() {
    let cleaned = clean(&marker_mid(8, false), false);
    assert_eq!(out(&cleaned).matches("<path").count(), 1 + 7);
    let cleaned = clean(&nested_uses(2, 3, 4), false);
    assert_eq!(out(&cleaned).matches("<path").count(), 9);
}

#[test]
fn sanitise_refuses_a_viewbox_that_rounds_to_zero() {
    let tiny = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 0.0001 0.0001"><path d="M0 0L0.0001 0.0001L0 0.0001Z"/></svg>"#;
    assert_eq!(normalise(tiny.as_bytes(), false), Err(SvgError::Empty));
}

#[test]
fn sanitise_keeps_a_style_sheet_clip_path() {
    let paths = format!(r#"<path d="{}"/>"#, zigzag(20)).repeat(50);
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><defs><style>.a {{ clip-path: url(#c) }}</style><clipPath id="c"><rect width="24" height="24"/></clipPath></defs><g class="a">{paths}</g></svg>"##
    );
    let cleaned = normalise(svg.as_bytes(), false).unwrap();
    assert_eq!(cleaned.removed, [Removed::ClipPath]);
}

/// A pattern of `inside` squares filling `shapes` squares: through a `fill` attribute on each
/// shape, on their group, or through a style sheet.
fn pattern_fill(inside: usize, shapes: usize, via: &str) -> String {
    let square = r#"<path d="M0 0h1v1z"/>"#;
    let (sheet, group, shape) = match via {
        "group" => ("", r#" fill="url(#p)""#, square.to_owned()),
        "sheet" => (
            "<style>.q { fill: url(#p) }</style>",
            "",
            square.replace("<path", r#"<path class="q""#),
        ),
        _ => ("", "", square.replace("<path", r#"<path fill="url(#p)""#)),
    };
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">{sheet}<defs><pattern id="p" width="1" height="1">{}</pattern></defs><g{group}>{}</g></svg>"#,
        square.repeat(inside),
        shape.repeat(shapes)
    )
}

/// A gradient of `stops` stops that a second gradient inherits through `href`; `shapes`
/// squares fill with the second one.
fn gradient_fill(stops: usize, shapes: usize) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><defs><linearGradient id="g0">{}</linearGradient><linearGradient id="g" href="#g0"/></defs>{}</svg>"##,
        "<stop/>".repeat(stops),
        r#"<path fill="url(#g)" d="M0 0h1v1z"/>"#.repeat(shapes)
    )
}

#[test]
fn sanitise_refuses_paint_server_bombs() {
    for via in ["shape", "group", "sheet"] {
        let bomb = pattern_fill(600, 100, via);
        assert_eq!(
            normalise(bomb.as_bytes(), false),
            Err(SvgError::TooComplex),
            "{via}"
        );
    }
    let bomb = gradient_fill(3_000, 100);
    assert_eq!(normalise(bomb.as_bytes(), false), Err(SvgError::TooComplex));
    let cycle = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><pattern id="a" href="#b"/><pattern id="b" href="#a"/><path fill="url(#a)" d="M0 0h1v1z"/></svg>"##;
    assert_eq!(
        normalise(cycle.as_bytes(), false),
        Err(SvgError::TooComplex)
    );
}

/// A heavy group `<use>`d 100 times, with a `decoy` element that reuses its id after it.
fn heavy_first(decoy: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><defs><g id="a"><path d="{}"/></g></defs>{}{decoy}</svg>"##,
        zigzag(3_000),
        r##"<use href="#a"/>"##.repeat(100)
    )
}

#[test]
fn sanitise_refuses_duplicate_ids() {
    for decoy in [r#"<g id="a"/>"#, r#"<foo id="a"/>"#, r#"<g xml:id="a"/>"#] {
        assert_eq!(
            normalise(heavy_first(decoy).as_bytes(), false),
            Err(SvgError::DuplicateId),
            "{decoy}"
        );
    }
    let pattern = pattern_fill(600, 100, "shape").replace("</svg>", r#"<foo id="p"/></svg>"#);
    assert_eq!(
        normalise(pattern.as_bytes(), false),
        Err(SvgError::DuplicateId)
    );
    let masks = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><mask id="m"/><mask id="m"/><path mask="url(#m)" d="M0 0h1v1z"/></svg>"##;
    assert_eq!(
        normalise(masks.as_bytes(), false),
        Err(SvgError::DuplicateId)
    );
    let one = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path id="a" xml:id="a" d="M0 0h1v1z"/></svg>"##;
    assert!(
        normalise(one.as_bytes(), false).is_ok(),
        "one element, one id"
    );
}

/// An Illustrator-style export: `paths` long paths, each filled through a style sheet class
/// with one of eight five-stop gradients (or a solid colour).
fn illustrator(paths: usize, gradients: bool) -> String {
    let mut sheet = String::new();
    let mut defs = String::new();
    for at in 0..8 {
        defs.push_str(&format!(
            r#"<linearGradient id="SVGID_{at}_" x1="2" y1="2" x2="22" y2="22">{}</linearGradient>"#,
            r##"<stop offset="0.5" stop-color="#369"/>"##.repeat(5)
        ));
    }
    let mut body = String::new();
    for at in 0..paths {
        let fill = if gradients {
            format!("url(#SVGID_{}_)", at % 8)
        } else {
            "#369".to_owned()
        };
        sheet.push_str(&format!(".st{at}{{fill:{fill};}}"));
        body.push_str(&format!(r#"<path class="st{at}" d="{}"/>"#, zigzag(400)));
    }
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><style>{sheet}</style>{defs}{body}</svg>"#
    )
}

#[test]
fn sanitise_keeps_an_illustrator_export_with_shared_gradients() {
    for gradients in [false, true] {
        let svg = illustrator(40, gradients);
        let cleaned = normalise(svg.as_bytes(), false);
        assert!(cleaned.is_ok(), "gradients {gradients}: {cleaned:?}");
    }
    let bomb = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><style>.q { fill: url(#g) }</style><linearGradient id="g">STOPS</linearGradient>SHAPES</svg>"##
        .replace("STOPS", &"<stop/>".repeat(3_000))
        .replace("SHAPES", &r#"<path class="q" d="M0 0h1v1z"/>"#.repeat(100));
    assert_eq!(normalise(bomb.as_bytes(), false), Err(SvgError::TooComplex));
}

#[test]
fn sanitise_keeps_a_typical_icon_with_paint_and_effects() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#36f"/><stop offset="1" stop-color="#0c9"/></linearGradient>
<clipPath id="c"><rect width="24" height="24" rx="4"/></clipPath><mask id="m"><rect width="24" height="24" fill="#fff"/></mask>
<filter id="f"><feGaussianBlur stdDeviation="1"/></filter><style>.b { fill: url(#g) }</style></defs>
<g clip-path="url(#c)" mask="url(#m)"><path fill="url(#g)" d="M2 2h20v20H2z"/><path class="b" d="M4 4h8v8H4z"/></g>
<g fill="url(#g)" filter="url(#f)"><circle cx="12" cy="12" r="4"/><path stroke="url(#g)" d="M6 18l12-12"/></g></svg>"##;
    let cleaned = normalise(svg.as_bytes(), false).unwrap();
    assert!(
        out(&cleaned).matches("<path").count() >= 4,
        "{}",
        out(&cleaned)
    );
}
