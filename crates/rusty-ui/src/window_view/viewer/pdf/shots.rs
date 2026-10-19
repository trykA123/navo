use std::collections::VecDeque;
use std::ops::Range;
use std::sync::Arc;

use gpui_kit::RenderImage;

pub(super) const PAGE_BUDGET: usize = 256 * 1024 * 1024;
pub(super) const THUMB_BUDGET: usize = 32 * 1024 * 1024;
const SHARP_ENOUGH: f32 = 0.85;
const NOT_WASTEFUL: f32 = 1.6;

#[derive(Clone)]
pub(super) struct Shot {
    pub width: u32,
    pub image: Arc<RenderImage>,
    bytes: usize,
}

impl Shot {
    pub(super) fn new(width: u32, height: u32, image: Arc<RenderImage>) -> Self {
        Self {
            width,
            image,
            bytes: width as usize * height as usize * 4,
        }
    }
}

pub(super) struct Shots {
    kept: VecDeque<(u32, Shot)>,
    bytes: usize,
    budget: usize,
}

impl Shots {
    pub(super) fn new(budget: usize) -> Self {
        Self {
            kept: VecDeque::new(),
            bytes: 0,
            budget,
        }
    }

    pub(super) fn get(&self, page: u32) -> Option<&Shot> {
        self.kept
            .iter()
            .find(|(at, _)| *at == page)
            .map(|(_, shot)| shot)
    }

    pub(super) fn needs(&self, page: u32, width: u32) -> bool {
        self.get(page).is_none_or(|shot| {
            let ratio = shot.width as f32 / width.max(1) as f32;
            !(SHARP_ENOUGH..=NOT_WASTEFUL).contains(&ratio)
        })
    }

    pub(super) fn insert(
        &mut self,
        page: u32,
        shot: Shot,
        keep: &Range<u32>,
    ) -> Vec<Arc<RenderImage>> {
        let mut dropped = Vec::new();
        let replaced = self.kept.iter().position(|(kept, _)| *kept == page);
        if let Some((_, old)) = replaced.and_then(|at| self.kept.remove(at)) {
            self.bytes -= old.bytes;
            dropped.push(old.image);
        }
        self.bytes += shot.bytes;
        self.kept.push_back((page, shot));
        while self.bytes > self.budget {
            let Some(at) = self.kept.iter().position(|(kept, _)| !keep.contains(kept)) else {
                break;
            };
            if let Some((_, old)) = self.kept.remove(at) {
                self.bytes -= old.bytes;
                dropped.push(old.image);
            }
        }
        dropped
    }

    pub(super) fn drain(&mut self) -> Vec<Arc<RenderImage>> {
        self.bytes = 0;
        self.kept.drain(..).map(|(_, shot)| shot.image).collect()
    }

    #[cfg(test)]
    pub(super) fn pages(&self) -> Vec<u32> {
        self.kept.iter().map(|(page, _)| *page).collect()
    }
}

#[cfg(test)]
mod tests {
    use image::{Frame, RgbaImage};

    use super::*;

    fn shot(width: u32) -> Shot {
        let frame = Frame::new(RgbaImage::new(1, 1));
        Shot::new(width, 10, Arc::new(RenderImage::new(vec![frame])))
    }

    #[test]
    fn the_oldest_page_outside_the_view_leaves_first() {
        let mut shots = Shots::new(100 * 10 * 4 * 2);
        shots.insert(1, shot(100), &(0..0));
        shots.insert(2, shot(100), &(0..0));
        let dropped = shots.insert(3, shot(100), &(1..2));
        assert_eq!((dropped.len(), shots.pages()), (1, vec![1, 3]));
    }

    #[test]
    fn a_shot_is_redrawn_only_when_its_width_is_far_off() {
        let mut shots = Shots::new(PAGE_BUDGET);
        shots.insert(0, shot(1000), &(0..1));
        assert!(!shots.needs(0, 1100));
        assert!(shots.needs(0, 1300));
        assert!(shots.needs(0, 500));
        assert!(shots.needs(1, 1000));
    }
}
