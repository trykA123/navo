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

fn picked(text: &str, indices: &[u32]) -> String {
    let wanted: Vec<char> = text
        .chars()
        .enumerate()
        .filter(|(at, _)| indices.contains(&u32::try_from(*at).unwrap()))
        .map(|(_, letter)| letter)
        .collect();
    wanted.into_iter().collect()
}

#[test]
fn indices_are_char_positions_after_a_combining_mark() {
    let name = "re\u{301}sume\u{301}.txt";
    let mut f = Fuzzy::new("sum", false);
    let m = f.score(name).expect("match");
    assert_eq!(picked(name, &m.indices), "sum");
}

#[test]
fn indices_are_char_positions_after_an_emoji_sequence() {
    let name = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467} family.txt";
    let mut f = Fuzzy::new("fam", false);
    let m = f.score(name).expect("match");
    assert_eq!(picked(name, &m.indices), "fam");
}

#[test]
fn operator_characters_match_literally() {
    let mut bang = Fuzzy::new("!jobs", false);
    assert!(bang.score("!jobs.txt").is_some());
    assert!(bang.score("jobs.txt").is_none());
    let mut caret = Fuzzy::new("^read", false);
    assert!(caret.score("^readme").is_some());
    assert!(caret.score("readme").is_none());
    let mut quote = Fuzzy::new("'a", false);
    assert!(quote.score("'a").is_some());
    assert!(quote.score("a").is_none());
    let mut dollar = Fuzzy::new("md$", false);
    assert!(dollar.score("a.md$").is_some());
    assert!(dollar.score("a.md").is_none());
}

#[test]
fn every_term_must_match() {
    let mut f = Fuzzy::new("tab new", false);
    assert!(f.score("New tab").is_some());
    assert!(f.score("New window").is_none());
}
