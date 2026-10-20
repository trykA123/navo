use rusty_l10n::tr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub insert: &'static str,
    pub text_id: &'static str,
}

const fn token(insert: &'static str, text_id: &'static str) -> Token {
    Token { insert, text_id }
}

pub const TOKENS: [Token; 26] = [
    token("[N]", "rename-token-name"),
    token("[N3]", "rename-token-name-char"),
    token("[N2-5]", "rename-token-name-range"),
    token("[N2-]", "rename-token-name-from"),
    token("[N2,5]", "rename-token-name-count"),
    token("[E]", "rename-token-extension"),
    token("[E2-3]", "rename-token-extension-range"),
    token("[P]", "rename-token-parent"),
    token("[G]", "rename-token-grandparent"),
    token("[C]", "rename-token-counter"),
    token("[C10+5:3]", "rename-token-counter-own"),
    token("[Y]", "rename-token-year-full"),
    token("[y]", "rename-token-year"),
    token("[M]", "rename-token-month"),
    token("[D]", "rename-token-day"),
    token("[h]", "rename-token-hour"),
    token("[i]", "rename-token-minute"),
    token("[s]", "rename-token-second"),
    token("[d]", "rename-token-date"),
    token("[t]", "rename-token-time"),
    token("[U]", "rename-token-upper"),
    token("[L]", "rename-token-lower"),
    token("[F]", "rename-token-first"),
    token("[n]", "rename-token-original"),
    token("[[", "rename-token-open"),
    token("]]", "rename-token-close"),
];

impl Token {
    #[must_use]
    pub fn description(&self) -> String {
        tr(self.text_id)
    }
}

#[must_use]
pub fn matching(query: &str) -> Vec<Token> {
    let query = query.trim().to_lowercase();
    TOKENS
        .into_iter()
        .filter(|token| query.is_empty() || token.description().to_lowercase().contains(&query))
        .collect()
}
