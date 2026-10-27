use rusty_core::shell_menu::{
    Choice, FerriteAction, ItemId, ItemKind, Row, Selection, ShellMenu, Submenu, rows,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextMenuEvent {
    Chosen(ItemId),
    OpenSubmenu(Vec<usize>),
    ShowClassic,
    Dismissed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PageStatus {
    Ready,
    Loading,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Step {
    Next,
    Previous,
    First,
    Last,
    PageDown(usize),
    PageUp(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Escape {
    ClearedQuery,
    Closed,
    Dismiss,
}

/// A row in one panel: level 0 is the root, each open flyout adds one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Spot {
    pub level: usize,
    pub row: usize,
}

pub(super) struct Level {
    path: Vec<usize>,
    rows: Vec<Row>,
    selection: Selection,
    status: PageStatus,
}

impl Level {
    fn new(menu: &ShellMenu, path: Vec<usize>, query: &str) -> Self {
        let rows = rows(menu, &path, query);
        Self {
            selection: Selection::first(&rows),
            status: status_of(menu, &path).unwrap_or(PageStatus::Ready),
            rows,
            path,
        }
    }

    pub fn path(&self) -> &[usize] {
        &self.path
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    pub fn status(&self) -> PageStatus {
        self.status
    }

    pub fn row_of(&self, path: &[usize]) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| matches!(row, Row::Item(item) if item.path == path))
    }

    fn refresh(&mut self, menu: &ShellMenu, query: &str) {
        self.status = status_of(menu, &self.path).unwrap_or(PageStatus::Ready);
        self.rows = rows(menu, &self.path, query);
        self.selection = self.selection.keep(&self.rows);
    }
}

pub(super) struct MenuState {
    menu: ShellMenu,
    query: String,
    levels: Vec<Level>,
    active: usize,
}

impl MenuState {
    pub fn new(menu: ShellMenu) -> Self {
        let root = Level::new(&menu, Vec::new(), "");
        Self {
            levels: vec![root],
            menu,
            query: String::new(),
            active: 0,
        }
    }

    pub fn menu(&self) -> &ShellMenu {
        &self.menu
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn is_filtering(&self) -> bool {
        !self.query.trim().is_empty()
    }

    pub fn levels(&self) -> &[Level] {
        &self.levels
    }

    pub fn root(&self) -> &Level {
        &self.levels[0]
    }

    pub fn active(&self) -> usize {
        self.active
    }

    pub fn set_query(&mut self, query: &str) {
        if self.query == query {
            return;
        }
        self.query = query.to_owned();
        self.close_after(0);
        self.levels[0] = Level::new(&self.menu, Vec::new(), &self.query);
    }

    pub fn set_menu(&mut self, menu: ShellMenu) {
        self.menu = menu;
        self.refresh();
    }

    pub fn set_submenu(&mut self, path: &[usize], submenu: Submenu) -> bool {
        let changed = self.menu.set_submenu(path, submenu);
        if changed {
            self.refresh();
        }
        changed
    }

    pub fn move_by(&mut self, step: Step) {
        let level = &mut self.levels[self.active];
        let rows = &level.rows;
        level.selection = match step {
            Step::Next => level.selection.next(rows),
            Step::Previous => level.selection.previous(rows),
            Step::First => Selection::first(rows),
            Step::Last => Selection::default().previous(rows),
            Step::PageDown(count) => page(&level.selection, rows, count, true),
            Step::PageUp(count) => page(&level.selection, rows, count, false),
        };
        self.close_after(self.active);
    }

    pub fn enter(&mut self) -> Option<ContextMenuEvent> {
        let level = &self.levels[self.active];
        if level.status == PageStatus::Failed {
            return self.retry(self.active);
        }
        match level.selection.enter(&level.rows)? {
            Choice::Run(id) => Some(run(id)),
            Choice::OpenSubmenu(path) => self.open(self.active, path, true),
        }
    }

    /// `None` when the row is not a submenu, so the key can go on to the filter.
    pub fn right(&mut self) -> Option<Option<ContextMenuEvent>> {
        let level = &self.levels[self.active];
        let path = level.selection.open_submenu(&level.rows)?;
        Some(self.open(self.active, path, true))
    }

    /// Closes the focused flyout; on the root it closes flyouts only while the filter is empty,
    /// so the field keeps ← and Backspace.
    pub fn left(&mut self) -> bool {
        if self.active > 0 {
            self.close_after(self.active - 1);
            return true;
        }
        if self.levels.len() > 1 && self.query.is_empty() {
            self.close_after(0);
            return true;
        }
        false
    }

    pub fn close_innermost(&mut self) -> bool {
        if self.levels.len() <= 1 {
            return false;
        }
        self.close_after(self.levels.len() - 2);
        true
    }

    pub fn escape(&mut self) -> Escape {
        if self.close_innermost() {
            Escape::Closed
        } else if !self.query.is_empty() {
            self.set_query("");
            Escape::ClearedQuery
        } else {
            Escape::Dismiss
        }
    }

    /// Selects the row under the pointer and makes its panel the focused one.
    pub fn point(&mut self, spot: Spot) -> bool {
        let Some(level) = self.levels.get_mut(spot.level) else {
            return false;
        };
        let Some(selection) = Selection::pick(&level.rows, spot.row) else {
            return false;
        };
        let changed = selection != level.selection || self.active != spot.level;
        level.selection = selection;
        self.active = spot.level;
        changed
    }

    pub fn row_path(&self, spot: Spot) -> Option<&[usize]> {
        match self.levels.get(spot.level)?.rows.get(spot.row)? {
            Row::Item(item) => Some(&item.path),
            Row::Separator => None,
        }
    }

    pub fn is_open_parent(&self, spot: Spot) -> bool {
        let child = self.levels.get(spot.level + 1).map(Level::path);
        child.is_some_and(|child| self.row_path(spot) == Some(child))
    }

    /// Whether resting on this row changes anything: a flyout to close or one to open.
    pub fn needs_rest(&self, spot: Spot) -> bool {
        self.levels.len() > spot.level + 1 || self.submenu_at(spot).is_some()
    }

    /// The pointer rested on a row: deeper flyouts close and a submenu row opens its own.
    pub fn rest(&mut self, spot: Spot) -> Option<ContextMenuEvent> {
        self.point(spot);
        match self.submenu_at(spot) {
            Some(path) => self.open(spot.level, path, false),
            None => {
                self.close_after(spot.level);
                None
            }
        }
    }

    pub fn click(&mut self, spot: Spot) -> Option<ContextMenuEvent> {
        self.point(spot);
        if self.levels[spot.level].selection.index() != Some(spot.row) {
            return None;
        }
        match self.submenu_at(spot) {
            Some(path) => self.open(spot.level, path, false),
            None => self.enter(),
        }
    }

    pub fn retry(&self, level: usize) -> Option<ContextMenuEvent> {
        let level = self.levels.get(level)?;
        (level.status == PageStatus::Failed)
            .then(|| ContextMenuEvent::OpenSubmenu(level.path.clone()))
    }

    /// Opens every flyout along `path` and focuses the deepest; stops at a submenu still to load.
    pub fn open_path(&mut self, path: &[usize]) -> Option<ContextMenuEvent> {
        self.close_after(0);
        for depth in 1..=path.len() {
            if let Some(event) = self.open(depth - 1, path[..depth].to_vec(), true) {
                return Some(event);
            }
        }
        None
    }

    fn submenu_at(&self, spot: Spot) -> Option<Vec<usize>> {
        let level = self.levels.get(spot.level)?;
        match level.rows.get(spot.row)? {
            Row::Item(item) if item.enabled && item.opens_submenu => Some(item.path.clone()),
            _ => None,
        }
    }

    fn open(&mut self, parent: usize, path: Vec<usize>, focus: bool) -> Option<ContextMenuEvent> {
        let is_lazy = match &self.menu.item(&path)?.kind {
            ItemKind::Submenu(Submenu::Lazy | Submenu::Failed) => true,
            ItemKind::Submenu(_) => false,
            ItemKind::Command | ItemKind::Separator => return None,
        };
        if is_lazy {
            self.menu.set_submenu(&path, Submenu::Loading);
        }
        let is_open = self
            .levels
            .get(parent + 1)
            .is_some_and(|child| child.path == path);
        if is_open {
            self.close_after(parent + 1);
        } else {
            self.close_after(parent);
            self.levels.push(Level::new(&self.menu, path.clone(), ""));
        }
        self.active = if focus { parent + 1 } else { parent };
        is_lazy.then_some(ContextMenuEvent::OpenSubmenu(path))
    }

    fn close_after(&mut self, level: usize) {
        self.levels.truncate(level + 1);
        self.active = self.active.min(level);
    }

    /// Refills every panel; a flyout whose submenu or parent row is gone closes with its children.
    fn refresh(&mut self) {
        for (index, level) in self.levels.iter_mut().enumerate() {
            let query = if index == 0 { self.query.as_str() } else { "" };
            level.refresh(&self.menu, query);
        }
        let gone = (1..self.levels.len()).find(|&index| {
            let path = &self.levels[index].path;
            status_of(&self.menu, path).is_none() || self.levels[index - 1].row_of(path).is_none()
        });
        if let Some(index) = gone {
            self.close_after(index - 1);
        }
    }
}

fn status_of(menu: &ShellMenu, page: &[usize]) -> Option<PageStatus> {
    if page.is_empty() {
        return Some(PageStatus::Ready);
    }
    match &menu.item(page)?.kind {
        ItemKind::Submenu(Submenu::Filled(_)) => Some(PageStatus::Ready),
        ItemKind::Submenu(Submenu::Lazy | Submenu::Loading) => Some(PageStatus::Loading),
        ItemKind::Submenu(Submenu::Failed) => Some(PageStatus::Failed),
        ItemKind::Command | ItemKind::Separator => None,
    }
}

fn run(id: ItemId) -> ContextMenuEvent {
    if id == ItemId::Ferrite(FerriteAction::ClassicMenu) {
        ContextMenuEvent::ShowClassic
    } else {
        ContextMenuEvent::Chosen(id)
    }
}

fn page(selection: &Selection, rows: &[Row], count: usize, forward: bool) -> Selection {
    let mut current = selection.clone();
    for _ in 0..count {
        let next = if forward {
            current.next(rows)
        } else {
            current.previous(rows)
        };
        let wrapped = match (next.index(), current.index()) {
            (Some(to), Some(from)) => (to <= from) == forward,
            _ => true,
        };
        if wrapped {
            break;
        }
        current = next;
    }
    current
}
