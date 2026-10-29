#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SummonKey {
    pub win: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: char,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SummonKeyError {
    UnknownModifier,
    NoModifier,
    BadKey,
}

impl SummonKeyError {
    pub fn message(self) -> String {
        rusty_l10n::tr(match self {
            Self::UnknownModifier => "settings-summon-key-unknown-modifier",
            Self::NoModifier => "settings-summon-key-no-modifier",
            Self::BadKey => "settings-summon-key-bad-key",
        })
    }
}

pub fn parse_summon_key(text: &str) -> Result<Option<SummonKey>, SummonKeyError> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let mut parts: Vec<&str> = text.split('+').map(str::trim).collect();
    let last = parts.pop().unwrap_or_default();
    let mut chars = last.chars();
    let key = match (chars.next(), chars.next()) {
        (Some(key), None) if key.is_ascii_alphanumeric() => key.to_ascii_uppercase(),
        _ => return Err(SummonKeyError::BadKey),
    };
    let mut found = SummonKey {
        win: false,
        ctrl: false,
        alt: false,
        shift: false,
        key,
    };
    for part in parts {
        let flag = match part.to_ascii_lowercase().as_str() {
            "win" => &mut found.win,
            "ctrl" => &mut found.ctrl,
            "alt" => &mut found.alt,
            "shift" => &mut found.shift,
            _ => return Err(SummonKeyError::UnknownModifier),
        };
        *flag = true;
    }
    if found.win || found.ctrl || found.alt || found.shift {
        Ok(Some(found))
    } else {
        Err(SummonKeyError::NoModifier)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(mods: &str, key: char) -> SummonKey {
        SummonKey {
            win: mods.contains('w'),
            ctrl: mods.contains('c'),
            alt: mods.contains('a'),
            shift: mods.contains('s'),
            key,
        }
    }

    #[test]
    fn the_default_parses() {
        assert_eq!(parse_summon_key("Win+Alt+N"), Ok(Some(key("wa", 'N'))));
    }

    #[test]
    fn case_and_spaces_do_not_matter() {
        assert_eq!(
            parse_summon_key(" ctrl + shift + 1 "),
            Ok(Some(key("cs", '1')))
        );
    }

    #[test]
    fn empty_turns_it_off() {
        assert_eq!(parse_summon_key(""), Ok(None));
        assert_eq!(parse_summon_key("   "), Ok(None));
    }

    #[test]
    fn a_bare_key_needs_a_modifier() {
        assert_eq!(parse_summon_key("N"), Err(SummonKeyError::NoModifier));
    }

    #[test]
    fn function_keys_and_missing_keys_are_refused() {
        assert_eq!(parse_summon_key("Win+Alt+F13"), Err(SummonKeyError::BadKey));
        assert_eq!(parse_summon_key("Win+Alt"), Err(SummonKeyError::BadKey));
        assert_eq!(parse_summon_key("Win+"), Err(SummonKeyError::BadKey));
    }

    #[test]
    fn unknown_modifiers_are_refused() {
        assert_eq!(
            parse_summon_key("Super+N"),
            Err(SummonKeyError::UnknownModifier)
        );
    }
}
