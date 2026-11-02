use std::time::{Duration, Instant};

use gpui_kit::{Context, ScrollHandle, Task, px};

use super::ContextMenu;
use super::place::{self, FLYOUT_MIN_WIDTH, Flyout, Frame, MIN_WIDTH, Panel, ROW, Side};
use super::state::PageStatus;

pub(super) const LOADING_DELAY: Duration = Duration::from_millis(150);

/// What the view keeps per open panel beside the menu state: when it opened, when it started
/// loading, and its scroll position.
pub(super) struct LevelView {
    pub path: Vec<usize>,
    pub opened: Instant,
    pub loading_since: Option<Instant>,
    pub scroll: ScrollHandle,
    pub selected: Option<usize>,
    wake: Option<Task<()>>,
}

impl LevelView {
    fn new(path: Vec<usize>, now: Instant) -> Self {
        Self {
            path,
            opened: now,
            loading_since: None,
            scroll: ScrollHandle::new(),
            selected: None,
            wake: None,
        }
    }

    pub fn is_loading_shown(&self, now: Instant) -> bool {
        self.loading_since
            .is_some_and(|since| now.saturating_duration_since(since) >= LOADING_DELAY)
    }
}

fn wake_after_delay(cx: &mut Context<ContextMenu>) -> Task<()> {
    cx.spawn(async move |this, cx| {
        cx.background_executor().timer(LOADING_DELAY).await;
        let _ = this.update(cx, |_, cx| cx.notify());
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Placed {
    pub frame: Frame,
    pub side: Side,
}

impl ContextMenu {
    pub(super) fn sync_levels(&mut self, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        let levels = self.state.levels();
        let kept = self
            .views
            .iter()
            .zip(levels)
            .take_while(|(view, level)| view.path == level.path())
            .count();
        self.views.truncate(kept);
        let opened = levels[kept..]
            .iter()
            .map(|level| LevelView::new(level.path().to_vec(), now));
        self.views.extend(opened);
        for (view, level) in self.views.iter_mut().zip(levels) {
            let is_loading = level.status() == PageStatus::Loading;
            if is_loading == view.loading_since.is_some() {
                continue;
            }
            view.loading_since = is_loading.then_some(now);
            view.wake = is_loading.then(|| wake_after_delay(cx));
        }
    }

    pub(super) fn root_width(&self) -> f32 {
        place::page_width(self.state.root().rows(), self.state.menu()).max(MIN_WIDTH)
    }

    /// The root keeps its top-left corner; each flyout sits beside the row that opened it.
    pub(super) fn frames(&self) -> Vec<Placed> {
        let levels = self.state.levels();
        let root = levels[0].rows();
        let panel = Panel {
            origin: self.origin,
            width: self.width,
            list: place::list_height(root, levels[0].status()),
        };
        let mut placed = vec![Placed {
            frame: place::panel_frame(panel, self.bounds),
            side: Side::Right,
        }];
        for (index, pair) in levels.windows(2).enumerate() {
            let [parent, level] = pair else { break };
            let Some(row) = parent.row_of(level.path()) else {
                break;
            };
            let above = placed[index];
            let list_top = above.frame.bounds.origin.y + px(place::list_top(index));
            let scrolled = self
                .views
                .get(index)
                .map_or(px(0.), |view| view.scroll.offset().y);
            let row_top = list_top + px(place::rows_height(&parent.rows()[..row])) + scrolled;
            let lowest = list_top + above.frame.list_height - px(ROW);
            let width = place::page_width(level.rows(), self.state.menu()).max(FLYOUT_MIN_WIDTH);
            let flyout = Flyout {
                parent: above.frame.bounds,
                row_top: row_top.min(lowest).max(list_top),
                width: px(width),
                list: place::list_height(level.rows(), level.status()),
                side: above.side,
            };
            let (frame, side) = place::flyout_frame(flyout, self.bounds);
            placed.push(Placed { frame, side });
        }
        placed
    }
}
