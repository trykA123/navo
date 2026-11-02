#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NameError {
    #[error("Type a name.")]
    Empty,
    #[error("A name cannot contain {0}")]
    Character(char),
    #[error("A name cannot be . or ..")]
    DotName,
    #[error("A name cannot end with a space or a dot.")]
    Trailing,
    #[error("Windows reserves this name.")]
    Reserved,
    #[error("The name is too long.")]
    TooLong,
}

const FORBIDDEN: [char; 9] = ['\\', '/', ':', '*', '?', '"', '<', '>', '|'];
const DEVICES: [&str; 6] = ["con", "prn", "aux", "nul", "conin$", "conout$"];
const PORT_DIGITS: [char; 13] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '¹', '²', '³',
];
const MAX_UTF16: usize = 255;

pub fn check_name(name: &str) -> Result<(), NameError> {
    if name.trim().is_empty() {
        return Err(NameError::Empty);
    }
    if let Some(ch) = name
        .chars()
        .find(|ch| FORBIDDEN.contains(ch) || ch.is_control())
    {
        return Err(NameError::Character(ch));
    }
    if name == "." || name == ".." {
        return Err(NameError::DotName);
    }
    if name.ends_with([' ', '.']) {
        return Err(NameError::Trailing);
    }
    if is_device(name) {
        return Err(NameError::Reserved);
    }
    if name.encode_utf16().count() > MAX_UTF16 {
        return Err(NameError::TooLong);
    }
    Ok(())
}

fn is_device(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or(name)
        .trim_end()
        .to_lowercase();
    if DEVICES.contains(&stem.as_str()) {
        return true;
    }
    let numbered = |prefix: &str| {
        stem.strip_prefix(prefix).is_some_and(|rest| {
            let mut chars = rest.chars();
            matches!((chars.next(), chars.next()), (Some(digit), None) if PORT_DIGITS.contains(&digit))
        })
    };
    numbered("com") || numbered("lpt")
}

#[cfg(test)]
mod tests;
