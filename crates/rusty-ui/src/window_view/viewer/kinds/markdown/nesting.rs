pub(super) const MAX_NESTING: usize = 32;
const MAX_RUN: usize = 1_000;
const MAX_RUNS: usize = 10_000;
const CODE_INDENT: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fence {
    byte: u8,
    len: usize,
}

#[derive(Default)]
struct Scan {
    fence: Option<Fence>,
    items: Vec<usize>,
    brackets: usize,
    runs: usize,
    is_too_deep: bool,
}

fn indent_of(line: &str) -> usize {
    line.bytes()
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .map(|byte| if byte == b'\t' { CODE_INDENT } else { 1 })
        .sum()
}

fn without_quotes(line: &str) -> &str {
    line.trim_start_matches([' ', '\t', '>'])
}

fn fence_at(text: &str) -> Option<Fence> {
    let byte = *text.as_bytes().first()?;
    if !matches!(byte, b'`' | b'~') {
        return None;
    }
    let len = text.bytes().take_while(|at| *at == byte).count();
    let info = text.get(len..).unwrap_or_default();
    (len >= 3 && !(byte == b'`' && info.contains('`'))).then_some(Fence { byte, len })
}

fn closes(fence: Fence, line: &str) -> bool {
    let text = without_quotes(line);
    let len = text.bytes().take_while(|at| *at == fence.byte).count();
    len >= fence.len && text.get(len..).is_some_and(|rest| rest.trim().is_empty())
}

fn marker_len(rest: &[u8]) -> Option<usize> {
    let digits = rest.iter().take_while(|byte| byte.is_ascii_digit()).count();
    let marker = match rest.get(digits) {
        Some(b'.' | b')') if (1..=9).contains(&digits) => digits + 1,
        Some(b'-' | b'+' | b'*') if digits == 0 => 1,
        _ => return None,
    };
    matches!(rest.get(marker), Some(b' ' | b'\t')).then_some(marker)
}

fn is_flanking(before: Option<u8>, run: u8, after: Option<u8>) -> bool {
    let is_word = |byte: Option<u8>| byte.is_some_and(|byte| byte.is_ascii_alphanumeric());
    let is_text = |byte: Option<u8>| byte.is_some_and(|byte| !byte.is_ascii_whitespace());
    let is_intraword = run == b'_' && is_word(before) && is_word(after);
    !is_intraword && (is_text(before) || is_text(after))
}

impl Scan {
    fn line(&mut self, line: &str) {
        if let Some(fence) = self.fence {
            if closes(fence, line) {
                self.fence = None;
            }
            return;
        }
        if without_quotes(line).trim().is_empty() {
            self.brackets = 0;
            return;
        }
        let indent = indent_of(line);
        while self.items.last().is_some_and(|content| *content > indent) {
            self.items.pop();
        }
        if indent >= self.items.last().copied().unwrap_or(0) + CODE_INDENT {
            return;
        }
        let (quotes, rest) = self.prefix(line);
        if quotes + self.items.len() > MAX_NESTING {
            self.is_too_deep = true;
        }
        match fence_at(rest) {
            Some(fence) => self.fence = Some(fence),
            None => self.inline(rest),
        }
    }

    fn prefix<'a>(&mut self, line: &'a str) -> (usize, &'a str) {
        let bytes = line.as_bytes();
        let (mut at, mut column, mut quotes) = (0, 0, 0);
        while let Some(byte) = bytes.get(at) {
            let step = match byte {
                b' ' | b'\t' => 1,
                b'>' => {
                    quotes += 1;
                    1
                }
                _ => match marker_len(&bytes[at..]) {
                    Some(len) => {
                        self.items.push(column + len + 1);
                        len
                    }
                    None => break,
                },
            };
            column += if *byte == b'\t' { CODE_INDENT } else { step };
            at += step;
        }
        (quotes, line.get(at..).unwrap_or_default())
    }

    fn inline(&mut self, text: &str) {
        let bytes = text.as_bytes();
        let mut at = 0;
        while let Some(&byte) = bytes.get(at) {
            if !matches!(byte, b'*' | b'_' | b'~') {
                self.bracket(byte);
                at += 1;
                continue;
            }
            let len = bytes[at..].iter().take_while(|next| **next == byte).count();
            let before = at.checked_sub(1).and_then(|back| bytes.get(back).copied());
            if is_flanking(before, byte, bytes.get(at + len).copied()) {
                self.runs += 1;
            }
            if len > MAX_RUN || self.runs > MAX_RUNS {
                self.is_too_deep = true;
            }
            at += len;
        }
    }

    fn bracket(&mut self, byte: u8) {
        match byte {
            b'[' => {
                self.brackets += 1;
                if self.brackets > MAX_NESTING {
                    self.is_too_deep = true;
                }
            }
            b']' => self.brackets = self.brackets.saturating_sub(1),
            _ => {}
        }
    }
}

pub(super) fn is_too_deep(text: &str) -> bool {
    let mut scan = Scan::default();
    for line in text.lines() {
        scan.line(line);
        if scan.is_too_deep {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests;
