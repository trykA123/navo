use super::*;

fn forms(needle: &str) -> Vec<Vec<u8>> {
    let percent: String = needle.bytes().map(|byte| format!("%{byte:02X}")).collect();
    let unicode: String = needle
        .encode_utf16()
        .map(|unit| format!("\\u{unit:04x}"))
        .collect();
    vec![
        needle.as_bytes().to_vec(),
        needle.encode_utf16().flat_map(u16::to_le_bytes).collect(),
        needle.encode_utf16().flat_map(u16::to_be_bytes).collect(),
        percent.as_bytes().to_vec(),
        percent.to_lowercase().into_bytes(),
        unicode.as_bytes().to_vec(),
        unicode.to_uppercase().replace("\\U", "\\u").into_bytes(),
        serde_json::to_vec(needle).unwrap(),
        needle.replace('\\', "\\\\").into_bytes(),
    ]
}

#[test]
fn all_required_byte_encodings_are_detected() {
    let needles = [
        r"C:\zqxjv folder\zqxjv-file.txt",
        "zqxjv-query",
        "zqxjv-雪-🦀",
        "zqxjv\n\"x",
    ];
    for needle in needles {
        let scanner = LeakScanner::new(&[needle]).unwrap();
        for bytes in forms(needle) {
            assert!(
                scanner.count_leaks(&bytes) > 0,
                "an encoded confidential value escaped detection"
            );
            assert!(assert_no_leaks(&bytes, &[needle]).is_err());
        }
    }
}

#[test]
fn path_components_and_case_variants_are_protected() {
    let scanner = LeakScanner::new(&[r"C:\zqxjv-owner\zqxjv-child"]).unwrap();
    for component in ["zqxjv-owner", "ZQXJV-CHILD", r"C:\zqxjv-owner\zqxjv-child"] {
        assert!(scanner.count_leaks(component.as_bytes()) > 0);
    }
}

#[test]
fn mixed_and_partial_escape_forms_are_detected() {
    let scanner = LeakScanner::new(&["zqxjv:雪", r"zqxjv\path"]).unwrap();
    for bytes in [
        b"zq%78jv%3a%E9%9b%AA".as_slice(),
        b"zqxjv:\\u96eA",
        b"zqxjv\\u005cpath",
        b"zqxjv\\\\path",
        b"zqxjv%5C%5Cpath",
        b"zqxjv\\u0025\\u0035\\u0043path",
    ] {
        assert!(
            scanner.count_leaks(bytes) > 0,
            "mixed escaping bypassed detection"
        );
    }
}

#[test]
fn repeated_escaping_cannot_hide_a_confidential_value() {
    let scanner = LeakScanner::with_identity(&["zqxjv"], &[]).unwrap();
    let mut encoded = "%7a%71%78%6a%76".to_owned();
    for _ in 0..20 {
        assert!(scanner.count_leaks(encoded.as_bytes()) > 0);
        encoded = encoded.replace('%', "%25");
    }
    let json = serde_json::to_vec(r"\u007a\u0071\u0078\u006a\u0076").unwrap();
    assert!(scanner.count_leaks(&json) > 0);
}

#[test]
fn real_windows_identity_and_profile_components_are_protected() {
    let scanner = LeakScanner::new(&[]).unwrap();
    for key in ["USERNAME", "COMPUTERNAME", "USERPROFILE"] {
        let Some(value) = std::env::var_os(key) else {
            continue;
        };
        let value = value.to_string_lossy();
        let parts = value
            .split(['/', '\\'])
            .filter(|part| !part.is_empty() && !is_drive(part));
        for part in parts {
            for bytes in forms(part) {
                assert!(
                    scanner.count_leaks(&bytes) > 0,
                    "an environment identity escaped detection"
                );
            }
        }
    }
}

#[test]
fn identity_fields_are_also_tested_without_windows_environment_variables() {
    let identity = [
        "zqxjv-user".into(),
        "zqxjv-machine".into(),
        r"C:\Users\zqxjv-profile".into(),
    ];
    let scanner = LeakScanner::with_identity(&[], &identity).unwrap();
    for value in ["zqxjv-user", "zqxjv-machine", "zqxjv-profile"] {
        for bytes in forms(value) {
            assert!(scanner.count_leaks(&bytes) > 0);
        }
    }
}

#[test]
fn json_backslash_decoder_recovers_the_entire_needle() {
    let needle = r"zqxjv\owner\file";
    let mut patterns = Vec::new();
    add_needle(&mut patterns, &mut 0, needle).unwrap();
    let scanner = LeakScanner {
        matcher: AhoCorasick::new(patterns).unwrap(),
    };
    for bytes in forms(needle) {
        assert!(scanner.count_leaks(&bytes) > 0);
    }
}

#[test]
fn empty_and_hostile_inputs_do_not_create_false_matches() {
    let scanner = LeakScanner::new(&["", "zqxjv-absent"]).unwrap();
    for bytes in [
        b"".as_slice(),
        b"%zz%0\\uD800\\uXXXX\\",
        &[0xff, 0xfe, 0, 0x80],
    ] {
        assert_eq!(scanner.count_leaks(bytes), 0);
    }
    let huge = vec![b'%'; 1024 * 1024];
    assert_eq!(scanner.count_leaks(&huge), 0);
}

#[test]
fn excessive_protection_data_fails_closed() {
    let needle = "x".repeat(MAX_PATTERN_BYTES / 5 + 1);
    let error = LeakScanner::new(&[&needle]).unwrap_err();
    assert_eq!(error.code, ErrorCode::ScanCapacity);
    let names = vec!["x"; MAX_NEEDLES + 1];
    assert_eq!(
        LeakScanner::new(&names).unwrap_err().code,
        ErrorCode::ScanCapacity
    );
}

#[test]
fn pattern_byte_budget_boundary() {
    for (budget, allowed) in [
        (MAX_PATTERN_BYTES - 6, true),
        (MAX_PATTERN_BYTES - 5, true),
        (MAX_PATTERN_BYTES - 4, false),
    ] {
        let mut patterns = Vec::new();
        let mut used = budget;
        assert_eq!(add_needle(&mut patterns, &mut used, "x").is_ok(), allowed);
    }
}

#[test]
fn needle_count_boundary() {
    for (count, allowed) in [
        (MAX_NEEDLES - 2, true),
        (MAX_NEEDLES - 1, true),
        (MAX_NEEDLES, false),
    ] {
        let mut patterns = vec![Vec::new(); count * 3];
        let mut budget = 0;
        assert_eq!(add_needle(&mut patterns, &mut budget, "x").is_ok(), allowed);
    }
}

#[test]
fn checker_fails_when_a_confidential_name_is_deliberately_inserted() {
    assert!(assert_no_leaks(b"zqxjv-deliberate-leak", &["zqxjv-deliberate-leak"]).is_err());
    assert!(assert_no_leaks(b"{\"files\":42}", &["zqxjv-deliberate-leak"]).is_ok());
}

#[test]
fn html_entities_cannot_hide_a_confidential_value() {
    let scanner = LeakScanner::with_identity(&["zqxjv&private", "zqxjv<tag>"], &[]).unwrap();
    for page in [
        "zqxjv&amp;private",
        "zqxjv&#38;private",
        "zqxjv&#x26;private",
        "ZQXJV&AMP;private",
        "zqxjv&amp;amp;private",
        "zqxjv&lt;tag&gt;",
    ] {
        assert!(scanner.count_leaks(page.as_bytes()) > 0, "{page}");
    }
    assert_eq!(
        scanner.count_leaks(b"zqxjv &amp; private &lt;b&gt; &#9731;"),
        0
    );
}

#[test]
fn drive_letters_of_a_path_are_not_needles_but_its_names_are() {
    let identity = [r"C:\Users\zqxjv".to_owned()];
    let scanner = LeakScanner::with_identity(&[r"D:\zqxjv-work"], &identity).unwrap();
    let css = b"td{font-variant-numeric:tabular-nums;background:none}";
    assert_eq!(scanner.count_leaks(css), 0);
    assert!(scanner.count_leaks(b"zqxjv-work") > 0);
    assert!(scanner.count_leaks(br"C:\Users\zqxjv") > 0);
}
