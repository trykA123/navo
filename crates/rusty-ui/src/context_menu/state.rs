use rusty_core::shell_menu::{
    Choice, FerriteAction, ItemId, ItemKind, MenuItem, Row, Selection, ShellMenu, Submenu,
    rare_rows, rows, submenu_rows,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextMenuEvent {
    Chosen(ItemId),
    OpenSubmenu(Vec<usize>),
    ShowClassic,
    Dismissed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LevelStatus {
    Ready,
    Loading,
    Failed,
}

#[derive(Debug, Clone)]
pub(super) struct Level {
    pub path: Vec<usize>,
    pub parent_row: usize,
    pub rows: Vec<Row>,
    pub selection: Selection,
    pub status: LevelStatus,
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
    ClosedSubmenu,
    ClearedQuery,
    Dismiss,
}

pub(super) struct MenuState {
    menu: ShellMenu,
    query: String,
    show_rare: bool,
    levels: Vec<Level>,
    active: usize,
}

impl MenuState {
    pub fn new(menu: ShellMenu) -> Self {
        let rows = rows(&menu, "");
        let root = Level {
            path: Vec::new(),
            parent_row: 0,
            selection: Selection::first(&rows),
            rows,
            status: LevelStatus::Ready,
        };
        Self {
            menu,
            query: String::new(),
            show_rare: false,
            levels: vec![root],
            active: 0,
        }
    }

    pub fn menu(&self) -> &ShellMenu {
        &self.menu
    }

    pub fn is_filtering(&self) -> bool {
        !self.query.trim().is_empty()
    }

    pub fn levels(&self) -> &[Level] {
        &self.levels
    }

    pub fn active(&self) -> usize {
        self.active
    }

    pub fn set_query(&mut self, query: &str) {
        if self.query == query {
            return;
        }
        self.query = query.to_owned();
        self.levels.truncate(1);
        self.active = 0;
        let rows = self.root_rows();
        self.levels[0].selection = Selection::first(&rows);
        self.levels[0].rows = rows;
    }

    pub fn set_submenu(&mut self, path: &[usize], submenu: Submenu) -> bool {
        let changed = self.menu.set_submenu(path, submenu);
        if changed {
            self.refresh();
        }
        changed
    }

    pub fn move_by(&mut self, step: Step) {
        let active = self.active;
        let level = &mut self.levels[active];
        let rows = &level.rows;
        level.selection = match step {
            Step::Next => level.selection.next(rows),
            Step::Previous => level.selection.previous(rows),
            Step::First => Selection::first(rows),
            Step::Last => Selection::default().previous(rows),
            Step::PageDown(count) => page(&level.selection, rows, count, true),
            Step::PageUp(count) => page(&level.selection, rows, count, false),
        };
        self.levels.truncate(active + 1);
    }

    pub fn enter(&mut self) -> Option<ContextMenuEvent> {
        let active = self.active;
        let level = &self.levels[active];
        if level.status == LevelStatus::Failed {
            return Some(ContextMenuEvent::OpenSubmenu(level.path.clone()));
        }
        match level.selection.enter(&level.rows)? {
            Choice::Run(id) => Some(run(id)),
            Choice::OpenSubmenu(path) => self.open(active, path, true),
            Choice::ShowMore => {
                self.expand_rare();
                None
            }
        }
    }

    pub fn right(&mut self) -> Option<ContextMenuEvent> {
        let active = self.active;
        let level = &self.levels[active];
        let path = level.selection.open_submenu(&level.rows)?;
        self.open(active, path, true)
    }

    pub fn left(&mut self) {
        if self.active > 0 {
            self.levels.truncate(self.active);
            self.active -= 1;
        } else {
            self.levels.truncate(1);
        }
    }

    pub fn escape(&mut self) -> Escape {
        if self.levels.len() > 1 {
            self.left();
            Escape::ClosedSubmenu
        } else if !self.query.is_empty() {
            self.set_query("");
            Escape::ClearedQuery
        } else {
            Escape::Dismiss
        }
    }

    pub fn hover(&mut self, depth: usize, row: usize) -> bool {
        let Some(level) = self.levels.get_mut(depth) else {
            return false;
        };
        let Some(selection) = Selection::pick(&level.rows, row) else {
            return false;
        };
        level.selection = selection;
        self.active = depth;
        true
    }

    pub fn settle(&mut self, depth: usize, row: usize) -> Option<ContextMenuEvent> {
        let level = self.levels.get(depth)?;
        if level.selection.index() != Some(row) {
            return None;
        }
        match level.selection.open_submenu(&level.rows) {
            Some(path) => self.open(depth, path, false),
            None => {
                self.levels.truncate(depth + 1);
                None
            }
        }
    }

    pub fn click(&mut self, depth: usize, row: usize) -> Option<ContextMenuEvent> {
        self.hover(depth, row);
        let level = self.levels.get(depth)?;
        if level.selection.index() != Some(row) {
            return None;
        }
        self.enter()
    }

    pub fn retry(&self, depth: usize) -> Option<ContextMenuEvent> {
        let level = self.levels.get(depth)?;
        (level.status == LevelStatus::Failed)
            .then(|| ContextMenuEvent::OpenSubmenu(level.path.clone()))
    }

    pub fn open_path(&mut self, path: &[usize]) -> Option<ContextMenuEvent> {
        let depth = self.levels.iter().position(|level| {
            level
                .rows
                .iter()
                .any(|row| matches!(row, Row::Item(item) if item.path == path))
        })?;
        let level = &mut self.levels[depth];
        let row = level.rows.iter().position(|row| is_item_at(row, path))?;
        level.selection = Selection::pick(&level.rows, row)?;
        self.open(depth, path.to_vec(), true)
    }

    pub fn hidden_count(&self) -> usize {
        count(&self.menu.items, &|item| item.hidden)
    }

    pub fn rare_count(&self) -> usize {
        rare_rows(&self.menu)
            .iter()
            .filter(|row| matches!(row, Row::Item(_)))
            .count()
    }

    fn root_rows(&self) -> Vec<Row> {
        let mut out = rows(&self.menu, &self.query);
        if self.show_rare
            && !self.is_filtering()
            && let Some(at) = out.iter().position(|row| matches!(row, Row::More { .. }))
        {
            out.splice(at..=at, rare_rows(&self.menu));
        }
        out
    }

    fn submenu_level(&self, path: &[usize]) -> Option<(Vec<Row>, LevelStatus)> {
        match &self.menu.item(path)?.kind {
            ItemKind::Submenu(Submenu::Filled(_)) => {
                Some((submenu_rows(&self.menu, path), LevelStatus::Ready))
            }
            ItemKind::Submenu(Submenu::Lazy | Submenu::Loading) => {
                Some((Vec::new(), LevelStatus::Loading))
            }
            ItemKind::Submenu(Submenu::Failed) => Some((Vec::new(), LevelStatus::Failed)),
            ItemKind::Command | ItemKind::Separator => None,
        }
    }

    fn open(&mut self, from: usize, path: Vec<usize>, focus: bool) -> Option<ContextMenuEvent> {
        if self
            .levels
            .get(from + 1)
            .is_some_and(|level| level.path == path)
        {
            self.levels.truncate(from + 2);
            self.active = if focus { from + 1 } else { from };
            return None;
        }
        let parent_row = self.levels[from]
            .rows
            .iter()
            .position(|row| is_item_at(row, &path))?;
        let (rows, status) = self.submenu_level(&path)?;
        let is_lazy = matches!(
            self.menu.item(&path).map(|item| &item.kind),
            Some(ItemKind::Submenu(Submenu::Lazy))
        );
        self.levels.truncate(from + 1);
        self.levels.push(Level {
            path: path.clone(),
            parent_row,
            selection: Selection::first(&rows),
            rows,
            status,
        });
        self.active = if focus { from + 1 } else { from };
        is_lazy.then_some(ContextMenuEvent::OpenSubmenu(path))
    }

    fn expand_rare(&mut self) {
        let Some(at) = self.levels[0]
            .rows
            .iter()
            .position(|row| matches!(row, Row::More { .. }))
        else {
            return;
        };
        self.show_rare = true;
        let rows = self.root_rows();
        let root = &mut self.levels[0];
        root.selection = Selection::pick(&rows, at).unwrap_or_else(|| Selection::first(&rows));
        root.rows = rows;
        self.levels.truncate(1);
        self.active = 0;
    }

    fn refresh(&mut self) {
        let rows = self.root_rows();
        let root = &mut self.levels[0];
        root.selection = root.selection.keep(&root.rows, &rows);
        root.rows = rows;
        for depth in 1..self.levels.len() {
            if !self.refresh_level(depth) {
                self.levels.truncate(depth);
                break;
            }
        }
        self.active = self.active.min(self.levels.len() - 1);
    }

    fn refresh_level(&mut self, depth: usize) -> bool {
        let path = self.levels[depth].path.clone();
        let parent = self.levels[depth - 1]
            .rows
            .iter()
            .position(|row| is_item_at(row, &path));
        let (Some(parent_row), Some((rows, status))) = (parent, self.submenu_level(&path)) else {
            return false;
        };
        let level = &mut self.levels[depth];
        level.selection = level.selection.keep(&level.rows, &rows);
        level.parent_row = parent_row;
        level.rows = rows;
        level.status = status;
        true
    }
}

fn run(id: ItemId) -> ContextMenuEvent {
    if id == ItemId::Ferrite(FerriteAction::ClassicMenu) {
        ContextMenuEvent::ShowClassic
    } else {
        ContextMenuEvent::Chosen(id)
    }
}

fn is_item_at(row: &Row, path: &[usize]) -> bool {
    matches!(row, Row::Item(item) if item.path == path)
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

fn count(items: &[MenuItem], wanted: &dyn Fn(&MenuItem) -> bool) -> usize {
    items
        .iter()
        .map(|item| {
            let own = usize::from(!item.is_separator() && !item.text.is_empty() && wanted(item));
            own + item
                .children()
                .map_or(0, |children| count(children, wanted))
        })
        .sum()
}
