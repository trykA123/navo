use super::loaded::Loaded;
use super::scope::Scope;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NavItem {
    All,
    Region(usize),
    Object(usize),
}

pub(super) fn items(loaded: &Loaded) -> Vec<NavItem> {
    let regions = (0..loaded.regions.len()).map(NavItem::Region);
    let objects = (0..loaded.objects.len()).map(NavItem::Object);
    std::iter::once(NavItem::All)
        .chain(regions)
        .chain(objects)
        .collect()
}

impl NavItem {
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
        }
    }
}
