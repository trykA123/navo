use gpui_kit::SharedString;
use rusty_core::Location;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathSegment {
    pub label: SharedString,
    pub location: Location,
}

const SEPARATORS: [char; 2] = ['/', '\\'];

fn root_label(text: &str) -> String {
    let trimmed = text.trim_end_matches(SEPARATORS);
    if trimmed.is_empty() {
        text.chars().take(1).collect()
    } else {
        trimmed.to_owned()
    }
}

fn label_below(parent: &str, child: &str) -> String {
    child
        .strip_prefix(parent)
        .unwrap_or(child)
        .trim_matches(SEPARATORS)
        .to_owned()
}

pub fn path_segments(location: &Location) -> Vec<PathSegment> {
    let mut chain = vec![location.clone()];
    while let Some(parent) = chain.last().and_then(Location::parent) {
        chain.push(parent);
    }
    chain.reverse();
    let mut segments = Vec::with_capacity(chain.len());
    let mut parent_text: Option<String> = None;
    for step in chain {
        let text = step.path().display().into_owned();
        let label = match &parent_text {
            Some(parent) => label_below(parent, &text),
            None => root_label(&text),
        };
        parent_text = Some(text);
        segments.push(PathSegment {
            label: label.into(),
            location: step,
        });
    }
    segments
}
