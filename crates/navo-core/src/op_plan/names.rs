use crate::brand::TEMP_SUFFIX;
use crate::location::windows_style;
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

pub(super) fn temp_of(location: &Location) -> Option<Location> {
    let name = name_of(location)?.to_text();
    let temp = ProviderPath::from_text(&format!("{name}{TEMP_SUFFIX}"));
    location.parent()?.join(&temp).ok()
}

pub(super) fn shortcut_name(name: &ProviderPath, target: &Location) -> ProviderPath {
    if target.is_local() && windows_style(target.as_str()) {
        ProviderPath::from_text(&format!("{} - Shortcut.lnk", name.to_text()))
    } else {
        name.clone()
    }
}
