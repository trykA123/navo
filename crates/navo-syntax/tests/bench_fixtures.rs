//! Builds every rule-lexer benchmark fixture, so its setup assertions run without Valgrind.

#[path = "../benches/support/mod.rs"]
mod support;

#[test]
fn every_rule_fixture_builds_and_passes_its_setup_assertions() {
    let a2l = support::a2l_page();
    let dense = support::udl_dense();
    let cap = support::udl_tokens();
    assert_eq!(a2l.rows.len(), 120);
    assert_eq!(dense.rows.len(), 120);
    assert_eq!(cap.rows.len(), 1);
    assert!(support::lex_page(&dense).1 > 20_000);
    assert_eq!(support::lex_page(&cap).1, support::MAX_TOKENS);
}
