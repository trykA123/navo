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
    Back,
    Dismiss,
}

pub(super) struct MenuState {
    menu: ShellMenu,
    query: String,
    page: Vec<usize>,
    rows: Vec<Row>,
    selection: Selection,
    status: PageStatus,
}

impl MenuState {
    pub fn new(menu: ShellMenu) -> Self {
        let rows = rows(&menu, &[], "");
        Self {
            selection: Selection::first(&rows),
            rows,
            menu,
            query: String::new(),
            page: Vec::new(),
            status: PageStatus::Ready,
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

    pub fn page(&self) -> &[usize] {
        &self.page
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

    pub fn set_query(&mut self, query: &str) {
        if self.query == query {
            return;
        }
        self.query = query.to_owned();
        self.rows = rows(&self.menu, &self.page, &self.query);
        self.selection = Selection::first(&self.rows);
    }

    pub fn set_menu(&mut self, menu: ShellMenu) {
        self.menu = menu;
        if status_of(&self.menu, &self.page).is_none() {
            self.page.clear();
        }
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
        let rows = &self.rows;
        self.selection = match step {
            Step::Next => self.selection.next(rows),
            Step::Previous => self.selection.previous(rows),
            Step::First => Selection::first(rows),
            Step::Last => Selection::default().previous(rows),
            Step::PageDown(count) => page(&self.selection, rows, count, true),
            Step::PageUp(count) => page(&self.selection, rows, count, false),
        };
    }

    pub fn enter(&mut self) -> Option<ContextMenuEvent> {
        if self.status == PageStatus::Failed {
            return self.retry();
        }
        match self.selection.enter(&self.rows)? {
            Choice::Run(id) => Some(run(id)),
            Choice::OpenSubmenu(path) => self.drill(path),
        }
    }

    /// `None` when the row is not a submenu, so the key can go on to the filter.
    pub fn right(&mut self) -> Option<Option<ContextMenuEvent>> {
        let path = self.selection.open_submenu(&self.rows)?;
        Some(self.drill(path))
    }

    /// Back one page; refused while the filter holds text, so the field keeps ← and Backspace.
    pub fn back(&mut self) -> bool {
        if !self.query.is_empty() || self.page.is_empty() {
            return false;
        }
        let from = self.page.clone();
        self.show(from[..from.len() - 1].to_vec());
        let row = self.rows.iter().position(|row| is_item_at(row, &from));
        self.selection = row
            .and_then(|row| Selection::pick(&self.rows, row))
            .unwrap_or_else(|| Selection::first(&self.rows));
        true
    }

    pub fn escape(&mut self) -> Escape {
        if !self.query.is_empty() {
            self.set_query("");
            Escape::ClearedQuery
        } else if self.back() {
            Escape::Back
        } else {
            Escape::Dismiss
        }
    }

    pub fn hover(&mut self, row: usize) -> bool {
        let Some(selection) = Selection::pick(&self.rows, row) else {
            return false;
        };
        let changed = selection != self.selection;
        self.selection = selection;
        changed
    }

    pub fn click(&mut self, row: usize) -> Option<ContextMenuEvent> {
        self.hover(row);
        if self.selection.index() != Some(row) {
            return None;
        }
        self.enter()
    }

    pub fn retry(&self) -> Option<ContextMenuEvent> {
        (self.status == PageStatus::Failed)
            .then(|| ContextMenuEvent::OpenSubmenu(self.page.clone()))
    }

    pub fn drill(&mut self, path: Vec<usize>) -> Option<ContextMenuEvent> {
        let is_lazy = match &self.menu.item(&path)?.kind {
            ItemKind::Submenu(Submenu::Lazy) => true,
            ItemKind::Submenu(_) => false,
            ItemKind::Command | ItemKind::Separator => return None,
        };
        if is_lazy {
            self.menu.set_submenu(&path, Submenu::Loading);
        }
        self.query.clear();
        self.show(path.clone());
        self.selection = Selection::first(&self.rows);
        is_lazy.then_some(ContextMenuEvent::OpenSubmenu(path))
    }

    fn show(&mut self, page: Vec<usize>) {
        self.status = status_of(&self.menu, &page).unwrap_or(PageStatus::Ready);
        self.page = page;
        self.rows = rows(&self.menu, &self.page, &self.query);
    }

    fn refresh(&mut self) {
        self.status = status_of(&self.menu, &self.page).unwrap_or(PageStatus::Ready);
        self.rows = rows(&self.menu, &self.page, &self.query);
        self.selection = self.selection.keep(&self.rows);
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
