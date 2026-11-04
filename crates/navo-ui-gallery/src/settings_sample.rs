use navo_ui::git::ToolRow;
use navo_ui::prelude::*;
use navo_ui::primitives::TextFieldState;
use navo_ui::settings::{
    self, Change, DefaultShift, Field, Filter, GitPage, IndexProgress, Moved, SearchPage,
    ServiceStatus, SettingsBody, Values, VolumeProgress, find_field,
};
use navo_ui::side_sheet::{SheetFrame, SheetState, SideSheet};
use navo_ui::tokens::{Mode, Theme, ThemeChoice};
use navo_ui::window_view::UpdateView;

#[derive(Clone, Default)]
pub struct Extras {
    pub has_notice: bool,
    pub find: Option<(Entity<TextFieldState>, &'static str)>,
    pub is_scrolled_to_end: bool,
    pub update: Option<UpdateView>,
}

pub struct Shown<'a> {
    pub field: Field,
    pub extras: &'a Extras,
}

fn notice() -> Vec<Moved> {
    [
        ("files.sort", "\"name\"", "\"date\""),
        ("files.folders-first", "false", "true"),
    ]
    .iter()
    .filter_map(|(key, old, new)| {
        Moved::of(&DefaultShift {
            key: (*key).to_owned(),
            old: (*old).to_owned(),
            new: (*new).to_owned(),
        })
    })
    .collect()
}

fn index_sample() -> IndexProgress {
    IndexProgress {
        volumes: vec![
            VolumeProgress::new("C:", 1_482_310, None),
            VolumeProgress::new("D:", 412_907, Some(100)),
            VolumeProgress {
                percent: Some(37),
                ..VolumeProgress::new("E: (sample)", 86_240, Some(0))
            },
            VolumeProgress::new("F:", 0, Some(0)),
        ],
        memory: Some(214 * 1024 * 1024),
        is_reachable: true,
    }
}

fn tool(id: &str, label: &str, path: &str, selected: bool) -> ToolRow {
    ToolRow {
        id: id.to_owned(),
        label: label.to_owned(),
        path: path.to_owned(),
        selected,
    }
}

const BAD_COMMAND: &str = "git-gui {action}";

/// The Git page with three tools found; the command-row page also picks Other with a bad command.
fn git_sample(values: Values, field: Field) -> (Values, GitPage) {
    let tools = vec![
        tool(
            "tortoisegit",
            "TortoiseGit",
            r"C:\Program Files\TortoiseGit\bin\TortoiseGitProc.exe",
            field != Field::GitToolCommand,
        ),
        tool(
            "fork",
            "Fork",
            r"C:\Users\admin\AppData\Local\Fork\Fork.exe",
            false,
        ),
        tool(
            "git-cli",
            "Git command line",
            r"C:\Program Files\Git\cmd\git.exe",
            false,
        ),
    ];
    if field != Field::GitToolCommand {
        return (values, GitPage::new(tools, None));
    }
    let problem = "The custom Git command needs a {path} argument".to_owned();
    let values = values
        .with(&Change::text(Field::GitTool, "custom"))
        .with(&Change::text(Field::GitToolCommand, BAD_COMMAND));
    (values, GitPage::new(tools, Some(problem)))
}

pub fn sheet(t: &Theme, state: &Entity<SheetState>, frame: SheetFrame, shown: &Shown) -> SideSheet {
    let theme = match t.mode {
        Mode::Light => ThemeChoice::Light,
        Mode::Dark => ThemeChoice::Dark,
    };
    let values = Values::default()
        .with(&Change::theme(theme))
        .with(&Change::text(Field::EditorPath, r"C:\Tools\code.exe"));
    let (values, git) = git_sample(values, shown.field);
    let row = shown.field.row();
    let page = SearchPage {
        status: ServiceStatus::NotAvailable,
        is_windows: true,
        is_cleared: false,
    };
    let body = SettingsBody::new(values, row)
        .search_page(page)
        .git_page(git)
        .index_progress(index_sample())
        .version("0.3.0".into())
        .update(
            shown.extras.update.clone().unwrap_or(UpdateView::Hidden),
            false,
        );
    let body = if shown.extras.is_scrolled_to_end {
        let scroll = ScrollHandle::new();
        scroll.set_offset(point(px(0.), px(-10_000.)));
        body.track_scroll(&scroll)
    } else {
        body
    };
    let body = match &shown.extras.find {
        Some((input, query)) => body
            .filter(Filter::new(query))
            .find(Some(find_field(t, input).into_any_element())),
        None => body,
    };
    let body = body.upgrade(if shown.extras.has_notice {
        notice()
    } else {
        Vec::new()
    });
    settings::sheet(state, frame, body).close("escape", |_, _| {})
}
