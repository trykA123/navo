use crate::command_center::rank::{self, Frecency};

const HOUR: i64 = 3_600_000;
const DAY: i64 = 24 * HOUR;
const NOW: i64 = 100 * DAY;

fn weight(score: f64, age_ms: i64) -> f64 {
    Frecency {
        score,
        last_at_ms: NOW - age_ms,
    }
    .weight(NOW)
}

#[test]
fn weight_tiers_follow_the_zoxide_rule() {
    assert_eq!(weight(2.0, 0), 8.0);
    assert_eq!(weight(2.0, HOUR - 1), 8.0);
    assert_eq!(weight(2.0, HOUR), 4.0);
    assert_eq!(weight(2.0, DAY - 1), 4.0);
    assert_eq!(weight(2.0, DAY), 1.0);
    assert_eq!(weight(2.0, 7 * DAY - 1), 1.0);
    assert_eq!(weight(2.0, 7 * DAY), 0.5);
}

#[test]
fn weight_of_a_future_use_counts_as_fresh() {
    assert_eq!(weight(1.0, -5 * HOUR), 4.0);
}

#[test]
fn exact_name_beats_prefix_beats_none() {
    assert_eq!(rank::name_bonus("Docs", "docs"), rank::BONUS_EXACT);
    assert_eq!(rank::name_bonus("Documents", "doc"), rank::BONUS_PREFIX);
    assert_eq!(rank::name_bonus("My docs", "doc"), 0);
}

#[test]
fn empty_query_earns_no_bonus() {
    assert_eq!(rank::name_bonus("anything", ""), 0);
}

#[test]
fn bonus_matches_unicode_case_insensitively() {
    assert_eq!(rank::name_bonus("Ärger", "ärger"), rank::BONUS_EXACT);
    assert_eq!(rank::name_bonus("ÄRGER.txt", "är"), rank::BONUS_PREFIX);
}

#[test]
fn score_without_history_or_bonus_is_the_match_score() {
    assert_eq!(rank::score(80, 0.0, 0), 80);
}

#[test]
fn score_adds_the_logarithmic_frecency_term() {
    let expected = 80 + (12.0 * 9.0_f64.ln()).round() as i64;
    assert_eq!(rank::score(80, 8.0, 0), expected);
}

#[test]
fn score_adds_the_name_bonus() {
    assert_eq!(rank::score(80, 0.0, rank::BONUS_EXACT), 110);
}

#[test]
fn frecency_lifts_a_weaker_match_above_a_stronger_one() {
    let used = rank::score(70, weight(5.0, 0), 0);
    let unused = rank::score(90, 0.0, 0);
    assert!(used > unused);
}

#[test]
fn old_frecency_lifts_less_than_fresh() {
    assert!(rank::score(50, weight(3.0, 0), 0) > rank::score(50, weight(3.0, 30 * DAY), 0));
}
