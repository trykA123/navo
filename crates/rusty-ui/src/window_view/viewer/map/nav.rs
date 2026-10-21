use super::loaded::Loaded;
use super::scope::Scope;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NavItem {
    All,
    Region(usize),
    Object(usize),
    NotRead,
    Unread(usize),
}

pub(super) fn items(loaded: &Loaded) -> Vec<NavItem> {
    let regions = (0..loaded.regions.len()).map(NavItem::Region);
    let objects = (0..loaded.objects.len()).map(NavItem::Object);
    let unread = (!loaded.model.skipped_parts.is_empty())
        .then_some(NavItem::NotRead)
        .into_iter()
        .chain((0..loaded.model.skipped_parts.len()).map(NavItem::Unread));
    std::iter::once(NavItem::All)
        .chain(regions)
        .chain(objects)
        .chain(unread)
        .collect()
}

pub(super) fn selectable(items: &[NavItem]) -> usize {
    items.iter().take_while(|item| item.is_pickable()).count()
}

pub(super) fn starts_group(items: &[NavItem], at: usize) -> bool {
    let (Some(item), Some(before)) = (
        items.get(at),
        at.checked_sub(1).and_then(|at| items.get(at)),
    ) else {
        return false;
    };
    match item {
        NavItem::Object(_) => matches!(before, NavItem::Region(_) | NavItem::All),
        NavItem::NotRead => true,
        _ => false,
    }
}

impl NavItem {
    pub(super) fn is_pickable(self) -> bool {
        !matches!(self, Self::NotRead | Self::Unread(_))
    }

    pub(super) fn kind(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Region(_) => "Region",
            Self::Object(_) => "Object",
            Self::NotRead => "Group",
            Self::Unread(_) => "Part",
        }
    }

    pub(super) fn scope(self, loaded: &Loaded) -> Option<Scope> {
        match self {
            Self::All => Some(Scope::All),
            Self::Region(at) => loaded
                .regions
                .get(at)
                .map(|region| Scope::Region(region.name.clone())),
            Self::Object(at) => loaded
                .objects
                .get(at)
                .map(|object| Scope::Object(object.name.clone())),
            Self::NotRead | Self::Unread(_) => None,
        }
    }
}
