const MAX_RUN: usize = 1_000;
const MAX_RUNS: usize = 10_000;
const MAX_PREFIX: usize = 256;
const MAX_BRACKETS: usize = 20_000;
const MAX_OPEN_BRACKETS: usize = 1_000;
const TAB: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Fence {
    byte: u8,
    len: usize,
}

pub(super) struct Prefix<'a> {
    pub width: usize,
    pub has_quote: bool,
    pub has_marker: bool,
    pub rest: &'a str,
}

#[derive(Default)]
struct Scan {
    fence: Option<Fence>,
    runs: usize,
    brackets: usize,
    open_brackets: usize,
    fails: bool,
}

pub(super) fn without_quotes(line: &str) -> &str {
    line.trim_start_matches([' ', '\t', '>'])
}

pub(super) fn fence_at(text: &str) -> Option<Fence> {
    let byte = *text.as_bytes().first()?;
    if !matches!(byte, b'`' | b'~') {
        return None;
    }
    let len = text.bytes().take_while(|at| *at == byte).count();
    let info = text.get(len..).unwrap_or_default();
    (len >= 3 && !(byte == b'`' && info.contains('`'))).then_some(Fence { byte, len })
}

pub(super) fn closes(fence: Fence, line: &str) -> bool {
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

pub(super) fn container_prefix(line: &str) -> Prefix<'_> {
    let bytes = line.as_bytes();
    let mut prefix = Prefix {
        width: 0,
        has_quote: false,
        has_marker: false,
        rest: line,
    };
    let mut at = 0;
    while let Some(byte) = bytes.get(at) {
        let step = match byte {
            b' ' => 1,
            b'\t' => {
                prefix.width += TAB - 1;
                1
            }
            b'>' => {
                prefix.has_quote = true;
                1
            }
            _ => match marker_len(&bytes[at..]) {
                Some(len) => {
                    prefix.has_marker = true;
                    len
                }
                None => break,
            },
        };
        prefix.width += step;
        at += step;
    }
    prefix.rest = line.get(at..).unwrap_or_default();
    prefix
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
            self.open_brackets = 0;
        }
        let prefix = container_prefix(line);
        let opens = prefix.has_quote || prefix.has_marker;
        if opens && prefix.width > MAX_PREFIX {
            self.fails = true;
        }
        match fence_at(prefix.rest) {
            Some(fence) => self.fence = Some(fence),
            None => {
                self.emphasis(prefix.rest);
                self.brackets(prefix.rest);
            }
        }
    }

    fn brackets(&mut self, text: &str) {
        for byte in text.bytes() {
            match byte {
                b'[' => {
                    self.brackets += 1;
                    self.open_brackets += 1;
                }
                b']' => self.open_brackets = self.open_brackets.saturating_sub(1),
                _ => {}
            }
        }
        if self.brackets > MAX_BRACKETS || self.open_brackets > MAX_OPEN_BRACKETS {
            self.fails = true;
        }
    }

    fn emphasis(&mut self, text: &str) {
        let bytes = text.as_bytes();
        let mut at = 0;
        while let Some(&byte) = bytes.get(at) {
            if !matches!(byte, b'*' | b'_' | b'~') {
                at += 1;
                continue;
            }
            let len = bytes[at..].iter().take_while(|next| **next == byte).count();
            let before = at.checked_sub(1).and_then(|back| bytes.get(back).copied());
            if is_flanking(before, byte, bytes.get(at + len).copied()) {
                self.runs += 1;
            }
            if len > MAX_RUN || self.runs > MAX_RUNS {
                self.fails = true;
            }
            at += len;
        }
    }
}

pub(super) fn fails_fast(text: &str) -> bool {
    let mut scan = Scan::default();
    for line in text.lines() {
        scan.line(line);
        if scan.fails {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests;
