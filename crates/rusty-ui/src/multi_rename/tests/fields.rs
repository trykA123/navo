use rusty_core::rename::{Compiled, ExtCase, NameCase, RenameRules};

use crate::multi_rename::view::fields::{TOKENS, ext_case_text, matching, name_case_text};

fn compiles(mask: &str) -> bool {
    let rules = RenameRules {
        name_mask: mask.to_owned(),
        ..RenameRules::default()
    };
    Compiled::new(&rules).is_ok()
}

#[test]
fn every_menu_token_is_a_valid_mask() {
    for token in TOKENS {
        assert!(compiles(token.insert), "{} does not compile", token.insert);
    }
}

#[test]
fn every_description_starts_with_the_text_it_inserts() {
    for token in TOKENS {
        assert!(
            token.description().starts_with(token.insert),
            "{}",
            token.text_id
        );
    }
}

#[test]
fn an_empty_filter_lists_every_token() {
    assert_eq!(matching("  ").len(), TOKENS.len());
}

#[test]
fn the_filter_matches_the_token_or_its_description_without_case() {
    let by_text: Vec<_> = matching("[n2").iter().map(|token| token.insert).collect();
    assert_eq!(by_text, ["[N2-5]", "[N2-]", "[N2,5]"]);
    let by_word: Vec<_> = matching("GRANDPARENT")
        .iter()
        .map(|token| token.insert)
        .collect();
    assert_eq!(by_word, ["[G]"]);
}

#[test]
fn a_filter_with_no_hit_lists_nothing() {
    assert!(matching("zzzz").is_empty());
}

#[test]
fn every_case_choice_has_its_own_text() {
    let names = [
        NameCase::Unchanged,
        NameCase::Lower,
        NameCase::Upper,
        NameCase::FirstLetter,
        NameCase::Title,
    ]
    .map(name_case_text);
    let mut sorted = names.to_vec();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len());
    assert_ne!(ext_case_text(ExtCase::Lower), ext_case_text(ExtCase::Upper));
}
