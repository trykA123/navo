pub const BONUS_EXACT: i64 = 30;
pub const BONUS_PREFIX: i64 = 15;
pub const FRECENCY_FACTOR: f64 = 12.0;

const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 24 * HOUR_MS;
const WEEK_MS: i64 = 7 * DAY_MS;
const WEIGHT_HOUR: f64 = 4.0;
const WEIGHT_DAY: f64 = 2.0;
const WEIGHT_WEEK: f64 = 0.5;
const WEIGHT_OLDER: f64 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frecency {
    pub score: f64,
    pub last_at_ms: i64,
}

impl Frecency {
    #[must_use]
    pub fn weight(self, now_ms: i64) -> f64 {
        let age = now_ms.saturating_sub(self.last_at_ms);
        let factor = [
            (HOUR_MS, WEIGHT_HOUR),
            (DAY_MS, WEIGHT_DAY),
            (WEEK_MS, WEIGHT_WEEK),
        ]
        .into_iter()
        .find_map(|(limit, factor)| (age < limit).then_some(factor))
        .unwrap_or(WEIGHT_OLDER);
        self.score * factor
    }
}

#[must_use]
pub fn name_bonus(name: &str, query: &str) -> i64 {
    if query.is_empty() {
        return 0;
    }
    let name = name.to_lowercase();
    let query = query.to_lowercase();
    if name == query {
        BONUS_EXACT
    } else if name.starts_with(&query) {
        BONUS_PREFIX
    } else {
        0
    }
}

#[must_use]
pub fn score(match_score: i64, frecency_weight: f64, bonus: i64) -> i64 {
    let lift = FRECENCY_FACTOR * frecency_weight.max(0.0).ln_1p();
    match_score + lift.round() as i64 + bonus
}
