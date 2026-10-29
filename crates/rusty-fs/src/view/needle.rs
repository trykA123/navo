use std::ops::Range;

pub struct Needle {
    parts: Vec<Part>,
}

struct Part {
    is_anchored: bool,
    opens_with_star: bool,
    segments: Vec<Vec<char>>,
}

#[derive(Default)]
pub struct Scratch {
    chars: Vec<char>,
    starts: Vec<usize>,
}

impl Scratch {
    fn fold(&mut self, name: &str) {
        self.chars.clear();
        self.starts.clear();
        for (at, ch) in name.char_indices() {
            for lower in ch.to_lowercase() {
                self.chars.push(lower);
                self.starts.push(at);
            }
        }
    }

    fn byte_range(&self, name: &str, folded: Range<usize>) -> Range<usize> {
        let Some(last) = folded.end.checked_sub(1).filter(|_| !folded.is_empty()) else {
            return 0..0;
        };
        let start = self.starts[folded.start];
        let at = self.starts[last];
        let width = name[at..].chars().next().map_or(0, char::len_utf8);
        start..at + width
    }
}

impl Needle {
    pub fn parse(text: &str) -> Self {
        let parts = text
            .split(';')
            .filter(|part| !part.is_empty())
            .map(Part::parse)
            .collect();
        Self { parts }
    }

    pub fn find(&self, name: &str, scratch: &mut Scratch) -> Option<Range<usize>> {
        self.first_match(name, scratch, false)
    }

    pub fn matches(&self, name: &str, scratch: &mut Scratch) -> bool {
        self.find(name, scratch).is_some()
    }

    pub fn starts(&self, name: &str, scratch: &mut Scratch) -> bool {
        self.first_match(name, scratch, true).is_some()
    }

    fn first_match(
        &self,
        name: &str,
        scratch: &mut Scratch,
        is_pinned: bool,
    ) -> Option<Range<usize>> {
        if self.parts.is_empty() {
            return Some(0..0);
        }
        self.parts
            .iter()
            .find_map(|part| part.locate(name, scratch, is_pinned || part.is_anchored))
    }
}

impl Part {
    fn parse(text: &str) -> Self {
        let (is_anchored, rest) = match text.strip_prefix('^') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let folded: Vec<char> = rest.chars().flat_map(char::to_lowercase).collect();
        let segments = folded
            .split(|&c| c == '*')
            .filter(|segment| !segment.is_empty())
            .map(<[char]>::to_vec)
            .collect();
        Self {
            is_anchored,
            opens_with_star: folded.first() == Some(&'*'),
            segments,
        }
    }

    fn locate(&self, name: &str, scratch: &mut Scratch, is_pinned: bool) -> Option<Range<usize>> {
        scratch.fold(name);
        let hay = scratch.chars.as_slice();
        let mut segments = self.segments.iter();
        let Some(first) = segments.next() else {
            return Some(0..0);
        };
        let first_at = if is_pinned && !self.opens_with_star {
            Some(0).filter(|&at| fits_at(first, hay, at))?
        } else {
            find_from(first, hay, 0)?
        };
        let mut end = first_at + first.len();
        for segment in segments {
            end = find_from(segment, hay, end)? + segment.len();
        }
        Some(scratch.byte_range(name, first_at..end))
    }
}

fn fits_at(segment: &[char], hay: &[char], at: usize) -> bool {
    hay.get(at..at + segment.len())
        .is_some_and(|window| window.iter().zip(segment).all(|(h, s)| *s == '?' || h == s))
}

fn find_from(segment: &[char], hay: &[char], from: usize) -> Option<usize> {
    let last = hay.len().checked_sub(segment.len())?;
    (from..=last).find(|&at| fits_at(segment, hay, at))
}

#[cfg(test)]
mod tests;
