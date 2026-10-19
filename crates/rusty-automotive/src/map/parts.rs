#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum Part {
    #[default]
    Skip,
    Locate,
    Memory,
    Symbols,
    Link,
    Removed,
}

pub(super) fn header(text: &str) -> Option<String> {
    if !text.starts_with(['*', '=']) {
        return None;
    }
    let name = text.trim_matches(|c: char| c == '*' || c == '=' || c.is_whitespace());
    let name = name.trim_end_matches(':').trim();
    (!name.is_empty()).then(|| name.to_ascii_lowercase())
}

pub(super) fn part(name: &str) -> Part {
    match name {
        "locate result" | "locate result: sections" | "sections" => Part::Locate,
        "link result" => Part::Link,
        "removed sections" => Part::Removed,
        _ if name.starts_with("symbols") || name == "locate result: symbols" => Part::Symbols,
        _ if name.starts_with("memory usage") => Part::Memory,
        _ => Part::Skip,
    }
}

#[derive(Default)]
pub(super) struct Evidence {
    lines: usize,
    tool: bool,
    header: bool,
}

impl Evidence {
    pub(super) fn take(&mut self, text: &str) -> bool {
        if self.lines < 200 {
            self.lines += 1;
            let lower = text.to_ascii_lowercase();
            let words: Vec<_> = lower
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
                .filter(|s| !s.is_empty())
                .collect();
            self.tool |= words.contains(&"vx-toolset")
                || words.contains(&"ltc")
                || words.windows(2).any(|pair| pair == ["tasking", "vx"]);
            self.header |= header(text.trim().trim_start_matches('\u{feff}'))
                .is_some_and(|name| part(&name) != Part::Skip || name == "tool and invocation");
        }
        self.tool && self.header
    }
}
