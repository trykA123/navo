use super::tests::{good_timings, passing_reports, profile_reports};
use super::*;
use crate::fixture;
use crate::privacy::{LeakScanner, SafeText, assert_no_leaks};
use crate::safe_text;

fn outline(page: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for part in page.split("<h").skip(1) {
        let (level, rest) = part.split_at(1);
        if matches!(level, "1" | "2" | "3") {
            let text = rest
                .trim_start_matches('>')
                .split("</h")
                .next()
                .unwrap_or("");
            lines.push(format!("h{level} {text}"));
        }
    }
    lines
}

fn page_text(report: &RunReport) -> String {
    String::from_utf8(report.page().as_bytes().to_vec()).unwrap()
}

#[test]
fn html_has_the_fixed_sections_in_order() {
    let mut reports = passing_reports();
    reports.extend(profile_reports());
    let page = page_text(&RunReport::new(reports, good_timings()));
    let sections: Vec<&str> = page
        .match_indices("<section id=\"")
        .map(|(at, _)| &page[at + 13..])
        .map(|s| s.split('"').next().unwrap())
        .collect();
    assert_eq!(
        sections,
        [
            "summary",
            "safety",
            "targets",
            "start-idle",
            "index",
            "timings",
            "profile",
            "machine",
            "baselines"
        ]
    );
    let headings = outline(&page);
    assert_eq!(
        &headings[..4],
        [
            "h1 Navo bench report",
            "h2 Summary verdict",
            "h2 Data safety",
            "h2 Targets"
        ]
    );
    assert!(headings.contains(&"h3 Cold start".to_string()));
    assert!(headings.contains(&"h3 file-size".to_string()));
    assert_eq!(headings.last().unwrap(), "h2 Baselines");
}

#[test]
fn html_golden_skeleton_for_an_empty_run() {
    let report = RunReport::new(
        Vec::new(),
        Timings::NotMeasured {
            reason: safe_text!("no folder chosen"),
        },
    );
    let page = page_text(&report);
    assert!(page.starts_with("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">"));
    assert!(page.ends_with("</main></body></html>\n"));
    assert_eq!(
        outline(&page),
        [
            "h1 Navo bench report",
            "h2 Summary verdict",
            "h2 Data safety",
            "h2 Targets",
            "h2 Start and idle",
            "h3 Cold start",
            "h3 Warm start",
            "h3 Idle",
            "h3 Exe size",
            "h2 Index",
            "h2 Folder timings",
            "h2 Data profile",
            "h2 Machine",
            "h2 Baselines"
        ]
    );
    assert!(page.contains("<p class=\"muted\">Not measured: no folder chosen.</p>"));
    assert!(page.contains("Library baselines are not measured in v1: later."));
    assert!(page.contains("<span class=\"badge\">Not measured</span>"));
}

#[test]
fn html_is_self_contained_and_follows_the_colour_scheme() {
    let page = page_text(&RunReport::new(passing_reports(), good_timings()));
    for banned in [
        "<script", "http://", "https://", "src=", "<link", "<img", "url(", "@import", "onclick",
    ] {
        assert!(!page.contains(banned), "{banned}");
    }
    assert!(page.contains("@media (prefers-color-scheme:dark)"));
    assert!(page.contains("name=\"viewport\""));
    assert!(page.contains("role=\"region\" tabindex=\"0\""));
}

#[test]
fn histogram_bars_are_inline_percent_widths() {
    let mut reports = Vec::new();
    for (bucket, files) in [(safe_text!("a"), 50), (safe_text!("b"), 25)] {
        reports.push(Report::Bucket {
            group: safe_text!("g"),
            bucket,
            files,
            bytes: 0,
        });
    }
    let page = page_text(&RunReport::new(reports, good_timings()));
    assert!(page.contains("<i style=\"width:100%\"></i>"));
    assert!(page.contains("<i style=\"width:50%\"></i>"));
}

#[test]
fn text_is_escaped_in_the_page() {
    let reason: SafeText = safe_text!("<b>&\"x\"");
    let page = page_text(&RunReport::new(Vec::new(), Timings::NotMeasured { reason }));
    assert!(page.contains("&lt;b&gt;&amp;&quot;x&quot;"));
    assert!(!page.contains("<b>&"));
}

#[test]
fn planted_names_never_reach_the_html_or_json() {
    let planted = [
        fixture::MARKER,
        fixture::PRIVATE_FOLDER,
        fixture::PRIVATE_FILE,
        fixture::PRIVATE_EXTENSION,
        fixture::PRIVATE_QUERY,
        fixture::PRIVATE_CONTENT,
        fixture::TARGET_A,
        fixture::LINK_NAME,
        fixture::KIND_DIR,
        fixture::REPO_DIR,
    ];
    let mut reports = passing_reports();
    reports.extend(profile_reports());
    let report = RunReport::new(reports, good_timings());
    let (mut html, mut json) = (Vec::new(), Vec::new());
    let mut html_writer = SafeWriter::new(&mut html, LeakScanner::new(&planted).unwrap());
    report.write_html(&mut html_writer).unwrap();
    let mut json_writer = SafeWriter::new(&mut json, LeakScanner::new(&planted).unwrap());
    report.write_json(&mut json_writer).unwrap();
    assert!(
        html.len() > 4000 && json.len() > 2000,
        "not a redaction stub"
    );
    assert_no_leaks(&html, &planted).unwrap();
    assert_no_leaks(&json, &planted).unwrap();
}

#[test]
fn a_leak_in_the_page_becomes_a_count_only_stub() {
    let report = RunReport::new(passing_reports(), good_timings());
    let mut out = Vec::new();
    let mut writer = SafeWriter::new(&mut out, LeakScanner::new(&["Summary verdict"]).unwrap());
    report.write_html(&mut writer).unwrap();
    assert_eq!(String::from_utf8(out).unwrap().trim(), "1");
}

#[test]
fn bars_never_overflow_and_empty_groups_are_safe() {
    let reports = vec![Report::Bucket {
        group: safe_text!("g"),
        bucket: safe_text!("a"),
        files: 0,
        bytes: 0,
    }];
    let page = page_text(&RunReport::new(reports, good_timings()));
    assert!(page.contains("width:0%"));
}
