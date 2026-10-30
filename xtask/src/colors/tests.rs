use super::check::{Kind, Source, evaluate};
use super::scan::{scan_rust, scan_svg};

fn whats(code: &str) -> Vec<String> {
    scan_rust(code)
        .into_iter()
        .map(|found| found.what)
        .collect()
}

fn rust(path: &str, text: &str) -> Source {
    Source {
        path: path.to_owned(),
        text: text.to_owned(),
        kind: Kind::Rust,
    }
}

#[test]
fn colour_constructors_and_named_helpers_are_found() {
    let code = "fn f() { let a = rgb(0x112233); let b = gpui_kit::black().opacity(0.4); let c = hsla(0., 0., 0., 1.); let d = white(); }";
    assert_eq!(whats(code), ["rgb(", "black()", "hsla(", "white()"]);
}

#[test]
fn struct_literals_are_found_but_return_types_are_not() {
    let code = "fn a() -> Hsla { t.text }\nfn b() { let x = Hsla { h: 0., s: 0., l: 0., a: 1. }; let y = Rgba { r: 0., g: 0., b: 0., a: 1. }; }";
    assert_eq!(whats(code), ["Hsla {", "Rgba {"]);
}

#[test]
fn methods_declarations_and_test_code_are_not_colour_construction() {
    let code = "fn rgb(x: u8) {}\nfn f(b: Bitmap) { b.rgba(); }\n#[cfg(test)]\nmod tests { fn g() { white(); } }";
    assert!(whats(code).is_empty());
}

#[test]
fn comments_and_ordinary_strings_are_ignored() {
    assert!(whats("// rgb(1)\nfn f() { let s = \"black()\"; }").is_empty());
}

#[test]
fn svg_colours_are_found_in_files_and_in_embedded_strings() {
    let svg = "<svg>\n<path fill=\"#fff\"/>\n<path fill=\"currentColor\"/></svg>";
    assert_eq!(scan_svg(svg)[0].line, 2);
    assert_eq!(scan_svg(svg).len(), 1);
    assert_eq!(
        whats("const S: &str = r##\"<path stroke=\"#000\"/>\"##;").len(),
        1
    );
}

#[test]
fn a_hit_names_file_line_and_the_token_rule() {
    let outcome = evaluate(
        &[rust("crates/a/src/x.rs", "fn f() {\n let c = white();\n}")],
        "",
    );
    assert_eq!(
        outcome.errors,
        ["crates/a/src/x.rs:2: white() builds a colour outside the token layer; use a theme token"]
    );
}

#[test]
fn allowed_paths_are_skipped_and_stale_or_reasonless_entries_fail() {
    let sources = [rust("crates/a/src/tokens/c.rs", "fn f() { white(); }")];
    assert!(
        evaluate(&sources, "crates/a/src/tokens/** | builds the tokens")
            .errors
            .is_empty()
    );
    let stale = evaluate(&sources, "crates/a/src/tokens/** | ok\ncrates/b/** | gone");
    assert_eq!(stale.errors.len(), 1);
    assert!(stale.errors[0].contains("crates/b/**"));
    let no_reason = evaluate(&sources, "crates/a/src/tokens/**");
    assert!(
        no_reason
            .errors
            .iter()
            .any(|e| e.contains("needs a reason"))
    );
}

#[test]
fn a_folder_pattern_does_not_cover_a_sibling_with_the_same_prefix() {
    let sources = [rust(
        "crates/a/src/tokens_extra/c.rs",
        "fn f() { white(); }",
    )];
    let outcome = evaluate(&sources, "crates/a/src/tokens/** | x");
    assert!(
        outcome
            .errors
            .iter()
            .any(|e| e.contains("tokens_extra/c.rs:1"))
    );
}
