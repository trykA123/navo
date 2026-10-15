use crate::{Location, ProviderPath};

pub(super) fn name_of(location: &Location) -> Option<ProviderPath> {
    let parent = location.parent()?;
    let rest = location.as_str().strip_prefix(parent.as_str())?;
    let name = rest.trim_start_matches(['\\', '/']);
    (!name.is_empty()).then(|| ProviderPath::from_text(name))
}

pub(super) fn numbered(name: &ProviderPath, number: u32) -> ProviderPath {
    let text = name.to_text();
    let (stem, extension) = match text.rfind('.') {
        Some(dot) if dot > 0 => text.split_at(dot),
        _ => (text.as_str(), ""),
    };
    ProviderPath::from_text(&format!("{stem} ({number}){extension}"))
}

pub(super) fn key(location: &Location) -> String {
    location.as_str().to_lowercase()
}
