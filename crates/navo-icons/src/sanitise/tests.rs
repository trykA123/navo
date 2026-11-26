use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use resvg::usvg::ImageHrefResolver;

use super::{MAX_PATHS, Removed, SVG_CAP, SvgError, normalise, normalise_with_resolver};

const FORBIDDEN_SUBSTRINGS: &[&str] = &[
    "script", "href", "url(", "style", "<image", "<text", "&", "<!",
];

fn assert_safe_output(svg: &[u8]) -> String {
    let text = std::str::from_utf8(svg).expect("normalised SVG must be UTF-8");
    let lower = text.to_ascii_lowercase();
    for needle in FORBIDDEN_SUBSTRINGS {
        assert!(
            !lower.contains(needle),
            "normalised SVG must not contain {needle:?}:\n{text}"
        );
    }
    assert!(
        text.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\""),
        "unexpected svg root header:\n{text}"
    );
    assert!(
        text.ends_with("</svg>"),
        "unexpected svg closing tag:\n{text}"
    );
    text.to_owned()
}

#[test]
fn sanitise_drops_script_and_handlers() {
    let input = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" onload="alert(1)">
        <script>alert('xss')</script>
        <rect x="2" y="2" width="20" height="20" fill="#112233" onclick="alert(2)" onmouseover="x()"/>
    </svg>"##;
    let cleaned = normalise(input, false).expect("valid SVG with script should normalise");
    let out = assert_safe_output(&cleaned.svg);
    assert!(!out.contains("onload"));
    assert!(!out.contains("onclick"));
    assert!(!out.contains("onmouseover"));
    assert!(out.contains("fill=\"#112233\""));
}

#[test]
fn sanitise_never_resolves_external_href() {
    let input = br##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 24 24">
        <image href="file:///C:/Windows/win.ini" width="24" height="24"/>
        <image xlink:href="https://example.invalid/pic.png" width="24" height="24"/>
        <use href="https://example.invalid/x.svg#a"/>
        <rect x="1" y="1" width="10" height="10" fill="#334455"/>
    </svg>"##;
    let calls = Arc::new(AtomicUsize::new(0));
    let data_calls = Arc::clone(&calls);
    let str_calls = Arc::clone(&calls);
    let resolver = ImageHrefResolver {
        resolve_data: Box::new(move |_, _, _| {
            data_calls.fetch_add(1, Ordering::SeqCst);
            None
        }),
        resolve_string: Box::new(move |_, _| {
            str_calls.fetch_add(1, Ordering::SeqCst);
            None
        }),
    };
    let cleaned = normalise_with_resolver(input, false, resolver)
        .expect("SVG with external hrefs should normalise");
    assert_safe_output(&cleaned.svg);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "resolver must never be called"
    );
    assert!(cleaned.removed.contains(&Removed::Image));
}

#[test]
fn sanitise_drops_foreign_object() {
    let input = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
        <foreignObject width="24" height="24">
            <div xmlns="http://www.w3.org/1999/xhtml"><script>alert(1)</script>hi</div>
        </foreignObject>
        <circle cx="12" cy="12" r="8" fill="#445566"/>
    </svg>"##;
    let cleaned = normalise(input, false).expect("SVG with foreignObject should normalise");
    let out = assert_safe_output(&cleaned.svg);
    assert!(!out.contains("foreignObject"));
    assert!(!out.contains("<div"));
    assert!(out.contains("<path"));
}

#[test]
fn sanitise_drops_style_and_url_paint() {
    let input = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
        <style>.a { fill: url(https://example.invalid/p.svg#x); }</style>
        <defs>
            <pattern id="pat" width="4" height="4" patternUnits="userSpaceOnUse">
                <rect width="4" height="4" fill="#ff0000"/>
            </pattern>
        </defs>
        <rect class="a" x="0" y="0" width="10" height="10" style="fill: red" fill="url(#pat)"/>
        <rect x="12" y="0" width="10" height="10" fill="#223344"/>
    </svg>"##;
    let cleaned = normalise(input, false).expect("SVG with style and pattern should normalise");
    let out = assert_safe_output(&cleaned.svg);
    assert!(cleaned.removed.contains(&Removed::Pattern));
    assert!(out.contains("fill=\"#223344\""));
}

#[test]
fn sanitise_refuses_entities() {
    let input = br#"<?xml version="1.0"?>
    <!DOCTYPE svg [
      <!ENTITY lol "lol">
      <!ENTITY lol2 "&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;">
    ]>
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
      <rect width="24" height="24"/>
    </svg>"#;
    assert_eq!(normalise(input, false), Err(SvgError::Doctype));

    let lower_doctype = b"<!doctype svg><svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>";
    assert_eq!(normalise(lower_doctype, false), Err(SvgError::Doctype));

    let entity_only = b"<!ENTITY a \"b\"><svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>";
    assert_eq!(normalise(entity_only, false), Err(SvgError::Doctype));
}

#[test]
fn sanitise_refuses_over_cap() {
    let oversized = vec![b' '; SVG_CAP + 1];
    assert_eq!(normalise(&oversized, false), Err(SvgError::TooLarge));
}

#[test]
fn sanitise_refuses_gzip() {
    let gzip = [0x1F, 0x8B, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(normalise(&gzip, false), Err(SvgError::Compressed));
}

#[test]
fn sanitise_refuses_too_many_paths() {
    let mut svg =
        String::from("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\">");
    for _ in 0..=MAX_PATHS {
        svg.push_str("<rect width=\"1\" height=\"1\"/>");
    }
    svg.push_str("</svg>");
    assert_eq!(
        normalise(svg.as_bytes(), false),
        Err(SvgError::TooManyPaths)
    );
}

#[test]
fn sanitise_turns_gradient_into_first_stop() {
    let input = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
        <defs>
            <linearGradient id="g1" x1="0" y1="0" x2="1" y2="1">
                <stop offset="0%" stop-color="#aa5500"/>
                <stop offset="100%" stop-color="#0055aa"/>
            </linearGradient>
        </defs>
        <rect x="2" y="2" width="20" height="20" fill="url(#g1)"/>
    </svg>"##;
    let cleaned = normalise(input, false).expect("gradient SVG should normalise");
    let out = assert_safe_output(&cleaned.svg);
    assert!(
        out.contains("fill=\"#aa5500\""),
        "expected first stop colour:\n{out}"
    );
    assert_eq!(cleaned.removed, vec![Removed::Gradient]);
}

#[test]
fn sanitise_lists_removed_text() {
    let input = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
        <rect x="2" y="2" width="20" height="20" fill="#123456"/>
        <text x="4" y="16">Hello</text>
    </svg>"##;
    let cleaned = normalise(input, false).expect("SVG with text should normalise");
    assert_safe_output(&cleaned.svg);
    assert_eq!(cleaned.removed, vec![Removed::Text]);
    assert_eq!(Removed::Text.label(), "text");
}

#[test]
fn sanitise_keeps_lucide_stroke() {
    let input = br#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="12" cy="12" r="10"/>
    </svg>"#;
    let cleaned = normalise(input, false).expect("Lucide circle should normalise");
    let out = assert_safe_output(&cleaned.svg);
    assert!(
        out.contains("stroke-width=\"2\""),
        "missing stroke-width:\n{out}"
    );
    assert!(
        out.contains("stroke-linecap=\"round\""),
        "missing round linecap:\n{out}"
    );
    assert!(
        out.contains("stroke-linejoin=\"round\""),
        "missing round linejoin:\n{out}"
    );
    assert!(out.contains("fill=\"none\""), "missing fill=none:\n{out}");
    assert!(cleaned.removed.is_empty());
}

#[test]
fn sanitise_mono_paints_black() {
    let input = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
        <rect x="2" y="2" width="10" height="10" fill="#ff3366" stroke="#00cc99" stroke-width="2"/>
    </svg>"##;
    let cleaned = normalise(input, true).expect("mono SVG should normalise");
    let out = assert_safe_output(&cleaned.svg);
    assert!(
        out.contains("fill=\"#000000\""),
        "expected black fill:\n{out}"
    );
    assert!(
        out.contains("stroke=\"#000000\""),
        "expected black stroke:\n{out}"
    );
    assert!(!out.contains("#ff3366"));
    assert!(!out.contains("#00cc99"));
}

#[test]
fn sanitise_output_is_a_fixed_point() {
    let input = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
        <g transform="translate(2, 3) scale(0.5)">
            <rect x="2" y="2" width="20" height="20" fill="#336699" fill-opacity="0.5"/>
            <circle cx="12" cy="12" r="6" fill="none" stroke="#996633" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
        </g>
    </svg>"##;
    let first = normalise(input, false).expect("first pass");
    assert_safe_output(&first.svg);
    let second = normalise(&first.svg, false).expect("second pass");
    assert_safe_output(&second.svg);
    assert_eq!(first.svg, second.svg, "normalise must be idempotent");
    assert!(second.removed.is_empty());
}

#[test]
fn sanitise_neutralises_use_and_marker_expansion_bombs() {
    let mut svg =
        String::from("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\"><defs>");
    svg.push_str("<g id=\"u0\"><rect width=\"1\" height=\"1\" fill=\"#000\"/></g>");
    for i in 1..=12 {
        let prev = i - 1;
        svg.push_str(&format!("<g id=\"u{i}\">"));
        for _ in 0..8 {
            svg.push_str(&format!("<use href=\"#u{prev}\"/>"));
        }
        svg.push_str("</g>");
    }
    svg.push_str(
        "<marker id=\"m1\" markerWidth=\"10\" markerHeight=\"10\"><use href=\"#u12\"/></marker>",
    );
    svg.push_str("</defs>");
    svg.push_str("<use href=\"#u12\"/>");
    svg.push_str("<path d=\"M0 0 L10 10 L20 0\" stroke=\"#111\" marker-start=\"url(#m1)\" marker-mid=\"url(#m1)\" marker-end=\"url(#m1)\"/>");
    svg.push_str("</svg>");

    let cleaned = normalise(svg.as_bytes(), false)
        .expect("use/marker bomb should be safely stripped without OOM");
    let out = assert_safe_output(&cleaned.svg);
    assert!(out.contains("<path"));
}

#[test]
fn sanitise_refuses_deep_nesting_without_stack_overflow() {
    let mut svg = String::from("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">");
    for _ in 0..1_000 {
        svg.push_str("<g>");
    }
    svg.push_str("<rect width=\"10\" height=\"10\"/>");
    for _ in 0..1_000 {
        svg.push_str("</g>");
    }
    svg.push_str("</svg>");

    let err = normalise(svg.as_bytes(), false).expect_err("deep nesting must be rejected");
    assert!(
        matches!(err, SvgError::Parse(_)),
        "expected Parse error for deep nesting, got {err:?}"
    );
}

#[test]
fn sanitise_neutralises_pattern_duplicate_id_and_whitespace_id_tricks() {
    let input = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
        <defs>
            <linearGradient id="  g1  ">
                <stop offset="0%" stop-color="#11aa22"/>
            </linearGradient>
            <linearGradient id="g1">
                <stop offset="0%" stop-color="#ff0000"/>
            </linearGradient>
            <pattern id="  p1  " width="2" height="2">
                <rect width="2" height="2" fill="#000"/>
            </pattern>
        </defs>
        <rect x="0" y="0" width="10" height="10" fill="url( #g1 )"/>
        <rect x="12" y="0" width="10" height="10" fill="url( #p1 )" stroke="#334455"/>
    </svg>"##;
    let cleaned = normalise(input, false).expect("duplicate/whitespace id SVG should normalise");
    let out = assert_safe_output(&cleaned.svg);
    assert!(
        out.contains("fill=\"#11aa22\""),
        "first gradient id must win:\n{out}"
    );
    assert!(
        !out.contains("#ff0000"),
        "duplicate gradient must be ignored:\n{out}"
    );
    assert!(cleaned.removed.contains(&Removed::Gradient));
    assert!(cleaned.removed.contains(&Removed::Pattern));
}

#[test]
fn sanitise_drops_inherit_mask_clip_filter_and_context_fill() {
    let input = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
        <defs>
            <filter id="f1"><feGaussianBlur stdDeviation="2"/></filter>
            <mask id="m1"><rect width="24" height="24" fill="#fff"/></mask>
            <clipPath id="c1"><rect width="12" height="12"/></clipPath>
        </defs>
        <g filter="inherit" mask="inherit" clip-path="inherit">
            <rect x="2" y="2" width="10" height="10" fill="context-fill" stroke="context-stroke"/>
            <rect x="12" y="12" width="10" height="10" fill="#224466"/>
        </g>
    </svg>"##;
    let cleaned = normalise(input, false).expect("inherit filter/mask/clipPath should normalise");
    let out = assert_safe_output(&cleaned.svg);
    assert!(out.contains("fill=\"#224466\""));
    assert!(cleaned.removed.contains(&Removed::Filter));
    assert!(cleaned.removed.contains(&Removed::Mask));
    assert!(cleaned.removed.contains(&Removed::ClipPath));
}
