use super::mask::{CaseMode, CounterParts, Mask, MaskError, MaskErrorKind, Span, Token};

impl Mask {
    pub fn parse(text: &str) -> Result<Self, MaskError> {
        let chars: Vec<char> = text.chars().collect();
        let mut tokens = Vec::new();
        let mut literal = String::new();
        let mut i = 0;
        while i < chars.len() {
            let fail = |kind| MaskError { at: i + 1, kind };
            match (chars[i], chars.get(i + 1)) {
                ('[', Some('[')) | (']', Some(']')) => {
                    literal.push(chars[i]);
                    i += 2;
                }
                (']', _) => return Err(fail(MaskErrorKind::StrayClose)),
                ('[', _) => {
                    let (parsed, used) = bracket(&chars[i..]).map_err(fail)?;
                    flush(&mut tokens, &mut literal);
                    tokens.push(parsed);
                    i += used;
                }
                (c, _) => {
                    literal.push(c);
                    i += 1;
                }
            }
        }
        flush(&mut tokens, &mut literal);
        Ok(Self { tokens })
    }
}

fn flush(tokens: &mut Vec<Token>, literal: &mut String) {
    if !literal.is_empty() {
        tokens.push(Token::Text(std::mem::take(literal)));
    }
}

fn bracket(chars: &[char]) -> Result<(Token, usize), MaskErrorKind> {
    let len = chars[1..]
        .iter()
        .position(|&c| c == ']')
        .ok_or(MaskErrorKind::Unclosed)?;
    let body: String = chars[1..=len].iter().collect();
    Ok((token(&body)?, len + 2))
}

fn token(body: &str) -> Result<Token, MaskErrorKind> {
    let fixed = match body {
        "P" => Some(Token::Parent),
        "G" => Some(Token::Grand),
        "Y" => Some(Token::Year),
        "y" => Some(Token::ShortYear),
        "M" => Some(Token::Month),
        "D" => Some(Token::Day),
        "h" => Some(Token::Hour),
        "i" => Some(Token::Minute),
        "s" => Some(Token::Second),
        "d" => Some(Token::Date),
        "t" => Some(Token::Time),
        "U" => Some(Token::Case(CaseMode::Upper)),
        "L" => Some(Token::Case(CaseMode::Lower)),
        "F" => Some(Token::Case(CaseMode::Words)),
        "n" => Some(Token::Case(CaseMode::Original)),
        _ => None,
    };
    if let Some(token) = fixed {
        return Ok(token);
    }
    let mut chars = body.chars();
    let head = chars.next();
    let rest = chars.as_str();
    match head {
        Some('N') => span(rest).map(Token::Name),
        Some('E') => span(rest).map(Token::Ext),
        Some('C') => counter(rest).map(Token::Counter),
        Some('=') => field(rest),
        _ => Err(MaskErrorKind::UnknownToken),
    }
}

fn field(text: &str) -> Result<Token, MaskErrorKind> {
    let valid = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    };
    match text.split_once('.') {
        Some((group, field)) if valid(group) && valid(field) => Ok(Token::Field {
            group: group.to_owned(),
            field: field.to_owned(),
        }),
        _ => Err(MaskErrorKind::BadField),
    }
}

fn span(text: &str) -> Result<Span, MaskErrorKind> {
    if text.is_empty() {
        return Ok(Span::All);
    }
    let bad = || MaskErrorKind::BadSpan;
    let (start, tail) = signed(text).ok_or_else(bad)?;
    if tail.is_empty() {
        return Ok(Span::At(start));
    }
    if let Some(end) = tail.strip_prefix('-') {
        if end.is_empty() {
            return Ok(Span::From(start));
        }
        return match signed(end) {
            Some((end, "")) => Ok(Span::Range(start, end)),
            _ => Err(bad()),
        };
    }
    let count = tail.strip_prefix(',').ok_or_else(bad)?;
    match digits(count) {
        Some((count, "")) => Ok(Span::Len(start, u32::try_from(count).map_err(|_| bad())?)),
        _ => Err(bad()),
    }
}

fn counter(text: &str) -> Result<CounterParts, MaskErrorKind> {
    let bad = || MaskErrorKind::BadCounter;
    let (start, rest) = leading_number(text).ok_or_else(bad)?;
    let (step, rest) = marked_number(rest, '+').ok_or_else(bad)?;
    let (width, rest) = marked_number(rest, ':').ok_or_else(bad)?;
    let width = width.map(u8::try_from).transpose().map_err(|_| bad())?;
    let step_ok = step.is_none_or(|step| step >= 1);
    let width_ok = width.is_none_or(|width| (1..=10).contains(&width));
    if !rest.is_empty() || !step_ok || !width_ok {
        return Err(bad());
    }
    Ok(CounterParts {
        start,
        step,
        digits: width,
    })
}

fn leading_number(text: &str) -> Option<(Option<u64>, &str)> {
    if text.starts_with(|c: char| c.is_ascii_digit()) {
        let (value, rest) = digits(text)?;
        return Some((Some(value), rest));
    }
    Some((None, text))
}

fn marked_number(text: &str, mark: char) -> Option<(Option<u64>, &str)> {
    match text.strip_prefix(mark) {
        Some(rest) => digits(rest).map(|(value, rest)| (Some(value), rest)),
        None => Some((None, text)),
    }
}

fn digits(text: &str) -> Option<(u64, &str)> {
    let end = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    let value = text[..end].parse().ok()?;
    Some((value, &text[end..]))
}

fn signed(text: &str) -> Option<(i32, &str)> {
    let (negative, rest) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (value, rest) = digits(rest)?;
    let value = i32::try_from(value).ok()?;
    Some((if negative { -value } else { value }, rest))
}
