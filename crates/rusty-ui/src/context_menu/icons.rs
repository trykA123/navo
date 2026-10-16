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
