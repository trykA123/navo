const PREFIX: &str = "layout.presets[";

pub(super) fn message(lines: &[String]) -> Option<String> {
    let details: Vec<String> = lines
        .iter()
        .filter(|line| line.starts_with(PREFIX))
        .map(|line| detail(line))
        .collect();
    if details.is_empty() {
        return None;
    }
    Some(rusty_l10n::t!(
        "settings-layout-presets-skipped",
        count = details.len(),
        details = details.join("; ")
    ))
}

fn detail(line: &str) -> String {
    parts(line).map_or_else(
        || line.to_owned(),
        |(name, at, problem)| {
            rusty_l10n::t!(
                "settings-layout-preset-skipped",
                line = at.to_owned(),
                name = name.to_owned(),
                problem = problem.to_owned()
            )
        },
    )
}

fn parts(line: &str) -> Option<(&str, &str, &str)> {
    let (_, rest) = line.split_once("] ")?;
    let (name, rest) = rest.split_once(" (line ")?;
    let (at, problem) = rest.split_once(") was dropped: ")?;
    let name = name.strip_prefix('"')?.strip_suffix('"')?;
    Some((name, at, problem))
}
