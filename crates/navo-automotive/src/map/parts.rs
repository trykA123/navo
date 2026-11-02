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

pub(super) fn header(text: &str, previous: Option<&str>) -> Option<String> {
    let name = if !text.is_empty() && text.bytes().all(|byte| byte == b'=') {
        previous?
    } else if text.starts_with("***") && text.ends_with("***") {
        text.trim_matches('*').trim()
    } else if text.starts_with("===") && text.ends_with("===") {
        text.trim_matches('=').trim()
    } else {
        let name = title(text)?;
        known(name).then_some(name)?
    };
    let name = name.trim_end_matches(':').trim();
    (!name.is_empty()).then(|| name.to_owned())
}

pub(super) fn title(text: &str) -> Option<&str> {
    let name = text.strip_prefix('*')?;
    if !name.starts_with(char::is_whitespace) {
        return None;
    }
    let name = name.trim().trim_end_matches(':').trim();
    (!name.is_empty() && !call_graph(name)).then_some(name)
}

fn call_graph(name: &str) -> bool {
    let Some((_, counts)) = name.rsplit_once('[') else {
        return false;
    };
    let Some((first, second)) = counts
        .strip_suffix(']')
        .and_then(|text| text.split_once(','))
    else {
        return false;
    };
    [first, second].iter().all(|text| {
        let text = text.trim();
        !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn known(name: &str) -> bool {
    [
        "locate result",
        "locate result: sections",
        "sections",
        "link result",
        "removed sections",
        "symbols",
        "locate result: symbols",
        "symbols (sorted on name)",
        "symbols (sorted on address)",
        "memory usage",
        "memory usage in bytes",
        "tool and invocation",
        "processed files",
        "cross references",
        "call graph",
        "locate rules",
        "future extension",
    ]
    .iter()
    .any(|title| title.eq_ignore_ascii_case(name))
}

pub(super) fn part(name: &str) -> Part {
    let name = name.to_ascii_lowercase();
    match name.as_str() {
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
    previous_title: Option<String>,
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
            let text = text.trim().trim_start_matches('\u{feff}');
            let name = header(text, self.previous_title.take().as_deref());
            self.previous_title = title(text).filter(|_| name.is_none()).map(str::to_owned);
            self.header |= name.is_some_and(|name| {
                part(&name) != Part::Skip || name.eq_ignore_ascii_case("tool and invocation")
            });
        }
        self.tool && self.header
    }
}
