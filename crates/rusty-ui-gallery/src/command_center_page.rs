use std::rc::Rc;

use rusty_core::{Location, Selection as Marks, ViewState};
use rusty_fs::FsEntry;
use rusty_ui::FocusHandle;
use rusty_ui::command_center::{
    ArgKind, Argument, CommandCenter, CommandCenterProps, Group, HintKeys, Hit, Indexing, Results,
    Row, Scope, Selection, Target,
};
use rusty_ui::commands::Keymap;
use rusty_ui::file_list::{FileList, FileListProps, SortKey};
use rusty_ui::prelude::*;
use rusty_ui::tokens;

use crate::pages::Page;
use crate::pane_sample;

const USER: &str = "C:\\Users\\admin";
const FIELD_WIDTH: f32 = 520.;
const FIELD_TOP: f32 = 7.;
const FIELD_HEIGHT: f32 = 32.;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shows {
    Mixed,
    Recent,
    Commands,
    Path,
    Argument,
    NoMatch,
    Indexing,
}

pub const PAGES: [(Page, Shows); 7] = [
    (Page::CommandCenter, Shows::Mixed),
    (Page::CommandCenterRecent, Shows::Recent),
    (Page::CommandCenterCommands, Shows::Commands),
    (Page::CommandCenterPath, Shows::Path),
    (Page::CommandCenterArgument, Shows::Argument),
    (Page::CommandCenterNoMatch, Shows::NoMatch),
    (Page::CommandCenterIndexing, Shows::Indexing),
];

const COMMANDS: [(&str, &str, Option<&str>); 9] = [
    ("Copy path", "File list", Some("ctrl-shift-c")),
    ("Copy", "File list", Some("f5")),
    ("Copy name only", "File list", Some("ctrl-alt-c")),
    ("Copy to clipboard", "File list", Some("ctrl-c")),
    ("Compare folders", "File list", Some("shift-f2")),
    ("Compare files by content", "File list", None),
    ("Copy folder sizes", "File list", None),
    ("Close other tabs", "Window", None),
    ("Close tab", "Window", Some("ctrl-w")),
];

const FOLDERS: [(&str, &str); 4] = [
    ("copper-gauge", "C:\\Projects"),
    ("Copenhagen trip 2025", "C:\\Users\\admin\\Pictures"),
    ("copies 2024", "D:\\Backup"),
    ("scope-logger", "C:\\Projects"),
];

const BUDGET_FILES: [(&str, &str); 7] = [
    ("budget_template.xlsx", "C:\\Users\\admin\\Downloads"),
    ("budget_notes.md", "C:\\Users\\admin\\Documents\\Budget"),
    ("budget_2026.xlsx", "C:\\Users\\admin\\Documents\\Budget"),
    ("budget_2025.xlsx", "C:\\Users\\admin\\Documents\\Budget"),
    ("budget_review.pdf", "C:\\Projects\\project-atlas\\reports"),
    (
        "trip_budget.xlsx",
        "C:\\Users\\admin\\Pictures\\Copenhagen trip 2025",
    ),
    (
        "household_budget_q3.xlsx",
        "C:\\Users\\admin\\Documents\\Budget",
    ),
];

fn matched(title: &str, query: &str) -> Vec<u32> {
    let lower = title.to_lowercase();
    lower
        .find(query)
        .map(|start| {
            let first = lower[..start].chars().count() as u32;
            (first..first + query.chars().count() as u32).collect()
        })
        .unwrap_or_default()
}

fn command(index: usize, query: &str) -> Hit {
    let (title, context, keys) = COMMANDS[index];
    Hit {
        source: "commands",
        group: Group::Commands,
        title: title.into(),
        detail: context.into(),
        score: 100 - index as i64,
        indices: matched(title, query),
        target: Target::Action("file_list.copy"),
        shortcut: keys.map(SharedString::from),
    }
}

fn location(path: &str) -> Option<Location> {
    Location::new(path).ok()
}

fn place(title: &str, parent: &str, query: &str, score: i64) -> Option<Hit> {
    let folder = !title.contains('.');
    Some(Hit {
        source: "recent",
        group: if folder { Group::Folders } else { Group::Files },
        title: title.to_owned().into(),
        detail: parent.to_owned().into(),
        score,
        indices: matched(title, query),
        target: Target::Open {
            location: location(&format!("{parent}\\{title}"))?,
            folder,
        },
        shortcut: None,
    })
}

fn folders(query: &str, base: i64) -> Vec<Hit> {
    FOLDERS
        .iter()
        .zip(0..)
        .filter_map(|((title, parent), n)| place(title, parent, query, base - n))
        .collect()
}

fn go_to() -> Hit {
    Hit {
        title: "Go to folder".into(),
        detail: "Window".into(),
        indices: Vec::new(),
        target: Target::Arg("window.go_to", ArgKind::Path),
        shortcut: Some("ctrl-g".into()),
        ..command(0, "")
    }
}

fn recent_folders() -> Vec<Hit> {
    [
        ("ferrite-fw", "C:\\Projects"),
        ("Downloads", USER),
        ("Documents", USER),
        ("Invoices", "C:\\Users\\admin\\Documents"),
        ("firmware", "D:\\Backup"),
    ]
    .iter()
    .filter_map(|(title, parent)| place(title, parent, "", 0))
    .collect()
}

fn rows(shows: Shows) -> (&'static str, Vec<Row>) {
    match shows {
        Shows::Mixed => {
            let mut hits: Vec<Hit> = (0..7).map(|n| command(n, "cop")).collect();
            hits.extend(folders("cop", 60));
            ("cop", Results::merge(hits, Scope::Mixed))
        }
        Shows::Recent => {
            let commands = vec![go_to(), command(0, ""), command(4, "")];
            ("", Results::recent(commands, recent_folders()))
        }
        Shows::Commands => {
            let hits = (0..COMMANDS.len()).map(|n| command(n, "c")).collect();
            ("> c", Results::merge(hits, Scope::Commands))
        }
        Shows::Path => {
            let hits = ["admin", "Public", "Default"]
                .iter()
                .filter_map(|name| place(name, "C:\\Users", "", 0))
                .collect();
            ("C:\\Users\\", Results::merge(hits, Scope::Files))
        }
        Shows::Argument => ("C:\\Users\\admin\\Nowhere", Vec::new()),
        Shows::NoMatch => ("qzxv", Vec::new()),
        Shows::Indexing => {
            let hits = BUDGET_FILES
                .iter()
                .zip(0..)
                .filter_map(|((title, parent), n)| place(title, parent, "budget", 50 - n))
                .chain(place("Budget", "C:\\Users\\admin\\Documents", "budget", 90))
                .map(|hit| Hit {
                    source: "index",
                    ..hit
                })
                .collect();
            ("budget", Results::merge(hits, Scope::Mixed))
        }
    }
}

fn props(shows: Shows, rows: Vec<Row>, viewport: Bounds<Pixels>) -> CommandCenterProps {
    let selection = Selection::start(&rows);
    let center = viewport.size.width / 2.;
    CommandCenterProps {
        rows: rows.into(),
        selection,
        argument: (shows == Shows::Argument).then(|| Argument {
            label: "Go to folder".into(),
            kind: ArgKind::Path,
        }),
        note: (shows == Shows::Indexing).then(|| {
            Indexing {
                root: "C:".into(),
                count: 12_345,
            }
            .text()
        }),
        error: (shows == Shows::Argument).then(|| "Nothing at C:\\Users\\admin\\Nowhere".into()),
        anchor: Bounds::new(
            point(center - px(FIELD_WIDTH / 2.), px(FIELD_TOP)),
            size(px(FIELD_WIDTH), px(FIELD_HEIGHT)),
        ),
        key_hints: Some(HintKeys::of(Keymap::builtin())),
        is_leaving: false,
    }
}

pub struct CommandCenterPage {
    shows: Shows,
    rows: Rc<[FsEntry]>,
    focus: FocusHandle,
    center: Option<Entity<CommandCenter>>,
}

impl CommandCenterPage {
    pub fn new(shows: Shows, cx: &mut Context<Self>) -> Self {
        Self {
            shows,
            rows: pane_sample::rows(),
            focus: cx.focus_handle(),
            center: None,
        }
    }

    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.center.is_some() {
            return;
        }
        let (query, rows) = rows(self.shows);
        let viewport = Bounds::new(Point::default(), window.viewport_size());
        let props = props(self.shows, rows, viewport);
        self.center = Some(cx.new(|cx| {
            let mut center = CommandCenter::new(props, window, cx);
            center.set_query(query, window, cx);
            center
        }));
        cx.notify();
    }

    fn list(&self) -> FileList {
        let props = FileListProps {
            folder: "commander".into(),
            rows: self.rows.clone(),
            view: ViewState {
                cursor: 0,
                scroll: 0,
            },
            marked: Rc::new(Marks::new(self.rows.len() as u64)),
            is_active: true,
            filter: None,
            sort: SortKey::default(),
        };
        FileList::new("command-center-page-list", props).track_focus(&self.focus)
    }
}

impl Render for CommandCenterPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        if self.center.is_none() {
            cx.defer_in(window, Self::open);
        }
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .bg(t.colors.surface)
            .child(self.list())
            .children(self.center.clone())
    }
}
