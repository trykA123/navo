use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use gpui_kit::{App, Global, RenderImage};
use image::{Frame, RgbaImage};
use rusty_core::shell_menu::{FerriteAction, Icon, MenuItem, ShellMenu};
use rusty_diag::components::{Held, MemoryUse, Tally};

use crate::primitives::IconName;

pub(super) const CACHED_ICONS: usize = 256;

static ICON_MEMORY: Tally = Tally::new("shell menu icons");

pub(super) fn ferrite_icon(action: FerriteAction) -> IconName {
    match action {
        FerriteAction::Open => IconName::ExternalLink,
        FerriteAction::View => IconName::Eye,
        FerriteAction::Edit => IconName::FilePen,
        FerriteAction::Copy => IconName::Files,
        FerriteAction::Move => IconName::FolderInput,
        FerriteAction::Rename => IconName::Pencil,
        FerriteAction::Delete => IconName::Trash,
        FerriteAction::CopyPath => IconName::Copy,
        FerriteAction::Properties => IconName::Info,
        FerriteAction::ClassicMenu => IconName::SquareMenu,
        FerriteAction::NewFolder => IconName::FolderPlus,
        FerriteAction::NewFile => IconName::FilePlus,
        FerriteAction::Paste => IconName::ClipboardPaste,
        FerriteAction::Refresh => IconName::RefreshCw,
        FerriteAction::SortBy => IconName::ArrowUpDown,
    }
}

/// Shell icons uploaded once per process, keyed by their pixels; the least recently used one leaves first.
#[derive(Default)]
pub(super) struct IconCache {
    images: HashMap<u64, Cached>,
    tick: u64,
}

struct Cached {
    image: Arc<RenderImage>,
    used: u64,
    _memory: Held,
}

impl Global for IconCache {}

impl IconCache {
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.images.len()
    }

    fn image(
        &mut self,
        icon: &Icon,
        evicted: &mut Vec<Arc<RenderImage>>,
    ) -> Option<Arc<RenderImage>> {
        let key = key_of(icon);
        self.tick += 1;
        if let Some(cached) = self.images.get_mut(&key) {
            cached.used = self.tick;
            return Some(cached.image.clone());
        }
        let image = to_image(icon)?;
        if self.images.len() >= CACHED_ICONS
            && let Some(oldest) = self
                .images
                .iter()
                .min_by_key(|(_, cached)| cached.used)
                .map(|(key, _)| *key)
        {
            evicted.extend(self.images.remove(&oldest).map(|cached| cached.image));
        }
        let cached = Cached {
            image: image.clone(),
            used: self.tick,
            _memory: ICON_MEMORY.hold(MemoryUse {
                bytes: icon.rgba.len() as u64,
                items: 1,
            }),
        };
        self.images.insert(key, cached);
        Some(image)
    }
}

fn key_of(icon: &Icon) -> u64 {
    let mut hasher = DefaultHasher::new();
    (icon.width, icon.height, &icon.rgba).hash(&mut hasher);
    hasher.finish()
}

#[derive(Default)]
pub(super) struct ShellIcons(HashMap<Vec<usize>, Arc<RenderImage>>);

impl ShellIcons {
    pub fn build(menu: &ShellMenu, cx: &mut App) -> Self {
        let mut found = Vec::new();
        collect(&menu.items, &mut Vec::new(), &mut found);
        Self(resolve(found, cx).into_iter().collect())
    }

    pub fn refill(&mut self, menu: &ShellMenu, path: &[usize], cx: &mut App) {
        self.0
            .retain(|key, _| !(key.len() > path.len() && key.starts_with(path)));
        let mut found = Vec::new();
        if let Some(children) = menu.item(path).and_then(MenuItem::children) {
            collect(children, &mut path.to_vec(), &mut found);
        }
        self.0.extend(resolve(found, cx));
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[cfg(test)]
    pub fn image(&self, path: &[usize]) -> Option<&Arc<RenderImage>> {
        self.0.get(path)
    }

    pub fn get(&self, path: &[usize]) -> Option<Arc<RenderImage>> {
        self.0.get(path).cloned()
    }
}

fn collect<'a>(
    items: &'a [MenuItem],
    path: &mut Vec<usize>,
    found: &mut Vec<(Vec<usize>, &'a Icon)>,
) {
    for (index, item) in items.iter().enumerate() {
        path.push(index);
        if let Some(icon) = &item.icon {
            found.push((path.clone(), icon));
        }
        if let Some(children) = item.children() {
            collect(children, path, found);
        }
        path.pop();
    }
}

fn resolve(found: Vec<(Vec<usize>, &Icon)>, cx: &mut App) -> Vec<(Vec<usize>, Arc<RenderImage>)> {
    let mut evicted = Vec::new();
    let cache = cx.default_global::<IconCache>();
    let images = found
        .into_iter()
        .filter_map(|(path, icon)| Some((path, cache.image(icon, &mut evicted)?)))
        .collect();
    for image in evicted {
        cx.drop_image(image, None);
    }
    images
}

fn to_image(icon: &Icon) -> Option<Arc<RenderImage>> {
    let mut pixels = RgbaImage::from_raw(icon.width, icon.height, icon.rgba.clone())?;
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(vec![Frame::new(pixels)])))
}
