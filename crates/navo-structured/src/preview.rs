pub const PREVIEW_CHARS: usize = 160;
pub const KEY_CHARS: usize = 512;

pub fn clip(text: &str, max_chars: usize) -> (&str, bool) {
    match text.char_indices().nth(max_chars) {
        Some((at, _)) => (text.get(..at).unwrap_or(text), true),
        None => (text, false),
    }
}

pub fn one_line(text: &str, max_chars: usize) -> String {
    let (kept, clipped) = clip(text, max_chars);
    let mut line: String = kept
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    if clipped {
        line.push('…');
    }
    line
}

pub fn squeeze_into(out: &mut String, text: &str, max_chars: usize) {
    let mut count = out.chars().count();
    let mut was_space = out.is_empty() || out.ends_with(' ');
    for ch in text.chars() {
        if count >= max_chars {
            if !out.ends_with('…') {
                out.push('…');
            }
            return;
        }
        let is_space = ch.is_whitespace();
        if is_space && was_space {
            continue;
        }
        out.push(if is_space { ' ' } else { ch });
        was_space = is_space;
        count += 1;
    }
}
