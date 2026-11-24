//! Rules for the find tokens that carry a typed value.

use super::{FindQuery, TokenKind};
use crate::Pattern;

const NAME_HELP: &str = "name: use masks like *.rs;!test* or /regex/";
const DEPTH_HELP: &str = "depth: use a number from 0 to 64";

/// Whether [`apply`] owns this token kind.
pub(super) fn handles(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Name | TokenKind::Depth | TokenKind::Word | TokenKind::Not | TokenKind::Case
    )
}

/// Applies one of the typed token kinds; other kinds are ignored.
pub(super) fn apply(query: &mut FindQuery, kind: TokenKind, value: &str) -> Result<(), String> {
    match kind {
        TokenKind::Name => name(query, value),
        TokenKind::Depth => depth(query, value),
        TokenKind::Word => on_off(value, "word", &mut query.word),
        TokenKind::Not => on_off(value, "not", &mut query.invert),
        TokenKind::Case => on_off(value, "case", &mut query.case),
        _ => Ok(()),
    }
}

fn name(query: &mut FindQuery, value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(NAME_HELP.to_owned());
    }
    if value.len() > 2 && value.starts_with('/') && value.ends_with('/') {
        let pattern = &value[1..value.len() - 1];
        regex::Regex::new(&format!("(?i){pattern}")).map_err(|_| NAME_HELP.to_owned())?;
    } else {
        for part in value.split(';') {
            let part = part.strip_prefix('!').unwrap_or(part);
            if part.is_empty() || Pattern::parse(part).is_err() {
                return Err(NAME_HELP.to_owned());
            }
        }
    }
    query.name = Some(value.to_owned());
    Ok(())
}

fn depth(query: &mut FindQuery, value: &str) -> Result<(), String> {
    let depth = value.parse::<u32>().map_err(|_| DEPTH_HELP.to_owned())?;
    if depth > 64 {
        return Err(DEPTH_HELP.to_owned());
    }
    query.depth = Some(depth);
    Ok(())
}

fn on_off(value: &str, key: &str, target: &mut bool) -> Result<(), String> {
    *target = match value {
        "on" => true,
        "off" => false,
        _ => return Err(format!("{key}: use on or off")),
    };
    Ok(())
}
