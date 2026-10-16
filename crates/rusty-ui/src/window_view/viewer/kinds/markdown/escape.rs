use super::nesting::{Fence, closes, container_prefix, fence_at, without_quotes};

const CODE_INDENT: usize = 4;

#[derive(Default)]
struct Escape {
    out: Vec<u8>,
    fence: Option<Fence>,
    was_blank: bool,
    was_code: bool,
    in_list: bool,
}

fn indent_of(line: &str) -> usize {
    line.bytes()
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .map(|byte| if byte == b'\t' { CODE_INDENT } else { 1 })
        .sum()
}

fn closing_run(bytes: &[u8], from: usize, len: usize) -> Option<usize> {
    let mut at = from;
    while at < bytes.len() {
        if bytes[at] != b'`' {
            at += 1;
            continue;
        }
        let run = bytes[at..].iter().take_while(|byte| **byte == b'`').count();
        if run == len {
            return Some(at);
        }
        at += run;
    }
    None
}

fn escape_inline(text: &str, out: &mut Vec<u8>) {
    let bytes = text.as_bytes();
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        let step = match byte {
            b'\\' if bytes.get(at + 1).is_some_and(u8::is_ascii_punctuation) => 2,
            b'`' => {
                let run = bytes[at..].iter().take_while(|byte| **byte == b'`').count();
                closing_run(bytes, at + run, run).map_or(run, |close| close + run - at)
            }
            b'<' | b'>' | b'$' => {
                out.push(b'\\');
                1
            }
            _ => 1,
        };
        out.extend_from_slice(&bytes[at..at + step]);
        at += step;
    }
}

impl Escape {
    fn line(&mut self, line: &str) {
        let body = line.strip_suffix('\n').unwrap_or(line);
        if let Some(fence) = self.fence {
            if closes(fence, body) {
                self.fence = None;
            }
            self.out.extend_from_slice(line.as_bytes());
            return;
        }
        let is_blank = without_quotes(body).trim().is_empty();
        let prefix = container_prefix(body);
        let is_code = !prefix.has_quote
            && !prefix.has_marker
            && !is_blank
            && !self.in_list
            && indent_of(body) >= CODE_INDENT
            && (self.was_blank || self.was_code);
        if prefix.has_marker {
            self.in_list = true;
        } else if !is_blank && indent_of(body) == 0 {
            self.in_list = false;
        }
        (self.was_blank, self.was_code) = (is_blank, is_code);
        if is_code {
            self.out.extend_from_slice(line.as_bytes());
            return;
        }
        self.fence = fence_at(prefix.rest);
        if self.fence.is_some() {
            self.out.extend_from_slice(line.as_bytes());
            return;
        }
        let kept = body.len() - prefix.rest.len();
        self.out.extend_from_slice(&body.as_bytes()[..kept]);
        escape_inline(prefix.rest, &mut self.out);
        self.out.extend_from_slice(&line.as_bytes()[body.len()..]);
    }
}

pub(super) fn escape_raw(text: &str) -> String {
    let mut escape = Escape {
        was_blank: true,
        ..Escape::default()
    };
    for line in text.split_inclusive('\n') {
        escape.line(line);
    }
    String::from_utf8(escape.out)
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
}

#[cfg(test)]
mod tests;
