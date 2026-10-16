use std::collections::HashMap;
use std::sync::Arc;

use gpui_kit::RenderImage;
use image::{Frame, RgbaImage};
use rusty_core::shell_menu::{FerriteAction, Icon, MenuItem, ShellMenu};

use crate::primitives::IconName;

pub(super) fn ferrite_icon(action: FerriteAction) -> IconName {
    match action {
        FerriteAction::CopyPath => IconName::Copy,
        FerriteAction::CopyName => IconName::Type,
        FerriteAction::Rename => IconName::Pencil,
        FerriteAction::Compare => IconName::GitCompare,
        FerriteAction::OpenInOtherPane => IconName::PanelRight,
        FerriteAction::Properties => IconName::Info,
        FerriteAction::ClassicMenu => IconName::SquareMenu,
    }
}

#[derive(Default)]
pub(super) struct ShellIcons(HashMap<Vec<usize>, Arc<RenderImage>>);

impl ShellIcons {
    pub fn build(menu: &ShellMenu) -> Self {
        let mut icons = Self::default();
        icons.collect(&menu.items, &mut Vec::new());
        icons
    }

    pub fn refill(&mut self, menu: &ShellMenu, path: &[usize]) -> Vec<Arc<RenderImage>> {
        let stale = self
            .0
            .extract_if(|key, _| key.len() > path.len() && key.starts_with(path))
            .map(|(_, image)| image)
            .collect();
        if let Some(children) = menu.item(path).and_then(MenuItem::children) {
            self.collect(children, &mut path.to_vec());
        }
        stale
    }

    pub fn drain(&mut self) -> impl Iterator<Item = Arc<RenderImage>> + '_ {
        self.0.drain().map(|(_, image)| image)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn get(&self, path: &[usize]) -> Option<Arc<RenderImage>> {
        self.0.get(path).cloned()
    }

    fn collect(&mut self, items: &[MenuItem], path: &mut Vec<usize>) {
        for (index, item) in items.iter().enumerate() {
            path.push(index);
            if let Some(image) = item.icon.as_ref().and_then(to_image) {
                self.0.insert(path.clone(), image);
            }
            if let Some(children) = item.children() {
                self.collect(children, path);
            }
            path.pop();
        }
    }
}

fn to_image(icon: &Icon) -> Option<Arc<RenderImage>> {
    let mut pixels = RgbaImage::from_raw(icon.width, icon.height, icon.rgba.clone())?;
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(vec![Frame::new(pixels)])))
}
