use super::Fuzzy;

#[test]
fn subsequence_matches() {
    let mut f = Fuzzy::new("rdm", false);
    let m = f.score("README.md").expect("match");
    assert!(m.score > 0);
    assert_eq!(m.indices.len(), 3);
}

#[test]
fn no_match_is_none() {
    let mut f = Fuzzy::new("xyz", false);
    assert!(f.score("README.md").is_none());
}

#[test]
fn smart_case_lowercase_ignores_case() {
    let mut f = Fuzzy::new("readme", false);
    assert!(f.score("README.md").is_some());
}

#[test]
fn smart_case_uppercase_is_exact() {
    let mut f = Fuzzy::new("Readme", false);
    assert!(f.score("readme.md").is_none());
    assert!(f.score("Readme.md").is_some());
}

#[test]
fn indices_count_characters_in_multibyte_names() {
    let mut f = Fuzzy::new("ünd", false);
    let m = f.score("Über_ünd").expect("match");
    assert_eq!(m.indices, vec![5, 6, 7]);
}

#[test]
fn path_config_prefers_last_segment() {
    let mut f = Fuzzy::new("foo", true);
    let in_name = f.score("xfoo/other/foo.rs").expect("match").score;
    let in_dir = f.score("xfoo/other/bar.rs").expect("match").score;
    assert!(in_name > in_dir);
}

#[test]
fn empty_query_matches_everything_with_zero() {
    let mut f = Fuzzy::new("", false);
    let m = f.score("anything").expect("match");
    assert_eq!(m.score, 0);
    assert!(m.indices.is_empty());
}
