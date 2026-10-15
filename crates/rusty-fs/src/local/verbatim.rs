const BACKSLASH: u16 = 0x5C;
const SLASH: u16 = 0x2F;
const DOT: u16 = 0x2E;
const COLON: u16 = 0x3A;

pub(super) fn verbatim(units: &[u16]) -> Vec<u16> {
    match units {
        [a, b, mark, c, ..]
            if is_separator(*a)
                && is_separator(*b)
                && matches!(*mark, 0x3F | DOT)
                && is_separator(*c) =>
        {
            units.to_vec()
        }
        [a, b, rest @ ..] if is_separator(*a) && is_separator(*b) => unc(rest),
        [letter, COLON] => drive(*letter, &[]),
        [letter, COLON, separator, rest @ ..] if is_separator(*separator) => drive(*letter, rest),
        _ => units.to_vec(),
    }
}

fn drive(letter: u16, rest: &[u16]) -> Vec<u16> {
    if !u8::try_from(letter).is_ok_and(|byte| byte.is_ascii_alphabetic()) {
        let mut units = vec![letter, COLON];
        units.extend_from_slice(rest);
        return units;
    }
    let mut out = encode(r"\\?\");
    out.extend([letter, COLON, BACKSLASH]);
    join(&mut out, &normalized(rest, 0));
    out
}

fn unc(rest: &[u16]) -> Vec<u16> {
    let mut out = encode(r"\\?\UNC\");
    join(&mut out, &normalized(rest, 2));
    out
}

fn normalized(rest: &[u16], fixed: usize) -> Vec<&[u16]> {
    let mut segments: Vec<&[u16]> = Vec::new();
    for segment in rest.split(|&unit| is_separator(unit)) {
        match segment {
            [] | [DOT] => {}
            [DOT, DOT] if segments.len() > fixed => {
                segments.pop();
            }
            [DOT, DOT] => {}
            _ => segments.push(segment),
        }
    }
    segments
}

fn join(out: &mut Vec<u16>, segments: &[&[u16]]) {
    for (index, segment) in segments.iter().enumerate() {
        if index > 0 {
            out.push(BACKSLASH);
        }
        out.extend_from_slice(segment);
    }
}

fn is_separator(unit: u16) -> bool {
    unit == BACKSLASH || unit == SLASH
}

fn encode(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
