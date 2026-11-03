use std::collections::HashMap;
use std::sync::Arc;

use navo_core::{ProviderPath, Selection, ViewState};
use navo_fs::{Attributes, EntryKind, FsEntry};
use navo_ui::file_list::{FileList, FileListProps, SortKey};
use navo_ui::git::{BadgeKind, BadgeReason, FolderBadges, RepoBadge, marked_icon};
use navo_ui::prelude::*;
use navo_ui::primitives::{IconName, icon};
use navo_ui::tokens::{self, ColorTheme, ThemeLook};

use crate::gallery::Page;

const DESKTOP: f32 = 1280.;
const MINIMUM: f32 = 480.;
const NAME_WIDTH: f32 = 112.;
const MINIMUM_THEMES: [ColorTheme; 3] =
    [ColorTheme::OpenSea, ColorTheme::BlackCap, ColorTheme::Grain];
const MODIFIED_MS: i64 = 1_791_920_400_000;

fn folder(name: &str) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text(name),
        kind: EntryKind::Folder,
        size: 0,
        modified_ms: Some(MODIFIED_MS),
        attributes: Attributes::default(),
    }
}

fn file(name: &str, size: u64) -> FsEntry {
    FsEntry {
        kind: EntryKind::File,
        size,
        ..folder(name)
    }
}

fn badge(kind: BadgeKind, is_root: bool) -> RepoBadge {
    RepoBadge {
        kind,
        is_root,
        stale: false,
        branch: is_root.then(|| "main".to_owned()),
        upstream: is_root.then(|| "origin/main".to_owned()),
        ahead: 0,
        behind: 0,
        changed: 0,
        untracked: 0,
        conflicted: 0,
        fetched_ms: is_root.then_some(MODIFIED_MS),
        reason: None,
    }
}

fn root(kind: BadgeKind) -> RepoBadge {
    badge(kind, true)
}

fn sub(kind: BadgeKind) -> RepoBadge {
    badge(kind, false)
}

type Named = (&'static str, RepoBadge);

fn changed_repos() -> [Named; 3] {
    [
        (
            "ecu-tools",
            RepoBadge {
                conflicted: 2,
                ahead: 1,
                ..root(BadgeKind::Conflict)
            },
        ),
        (
            "navo",
            RepoBadge {
                changed: 3,
                untracked: 1,
                ahead: 2,
                behind: 1,
                ..root(BadgeKind::Modified)
            },
        ),
        (
            "scratch",
            RepoBadge {
                stale: true,
                changed: 5,
                ..root(BadgeKind::Modified)
            },
        ),
    ]
}

fn synced_repos() -> [Named; 6] {
    let counts = |kind, ahead, behind| RepoBadge {
        ahead,
        behind,
        ..root(kind)
    };
    [
        ("flasher", counts(BadgeKind::Ahead, 3, 0)),
        ("notes", counts(BadgeKind::Behind, 0, 4)),
        ("fork", counts(BadgeKind::Diverged, 2, 5)),
        ("docs-site", root(BadgeKind::Clean)),
        ("old-checkout", root(BadgeKind::Checking)),
        (
            "vendor-sdk",
            RepoBadge {
                reason: Some(BadgeReason::Filter),
                ..root(BadgeKind::Unknown)
            },
        ),
    ]
}

fn repositories() -> (Arc<[FsEntry]>, Arc<FolderBadges>) {
    let badges: Vec<Named> = changed_repos().into_iter().chain(synced_repos()).collect();
    let mut rows: Vec<FsEntry> = badges.iter().map(|(name, _)| folder(name)).collect();
    rows.extend([
        folder("Downloads"),
        file("todo.txt", 812),
        file("notes.md", 2_048),
    ]);
    (rows.into(), shared(badges))
}

fn inside() -> (Arc<[FsEntry]>, Arc<FolderBadges>) {
    let badges = [
        (
            "crates",
            RepoBadge {
                changed: 12,
                ..sub(BadgeKind::Modified)
            },
        ),
        (
            "design",
            RepoBadge {
                conflicted: 1,
                ..sub(BadgeKind::Conflict)
            },
        ),
        ("docs", sub(BadgeKind::Clean)),
        ("scripts", sub(BadgeKind::Clean)),
        (
            "target",
            RepoBadge {
                reason: Some(BadgeReason::Failed),
                ..sub(BadgeKind::Unknown)
            },
        ),
    ];
    let mut rows: Vec<FsEntry> = badges.iter().map(|(name, _)| folder(name)).collect();
    rows.extend([file("Cargo.toml", 812), file("README.md", 2_048)]);
    (rows.into(), shared(badges))
}

fn shared(badges: impl IntoIterator<Item = Named>) -> Arc<FolderBadges> {
    let by_name: HashMap<String, RepoBadge> = badges
        .into_iter()
        .map(|(name, badge)| (name.to_owned(), badge))
        .collect();
    Arc::new(FolderBadges {
        revision: 1,
        by_name,
        ..FolderBadges::default()
    })
}

fn props(name: &str, (rows, git): (Arc<[FsEntry]>, Arc<FolderBadges>)) -> FileListProps {
    FileListProps {
        folder: name.to_owned().into(),
        view: ViewState {
            cursor: 1,
            scroll: 0,
        },
        marked: Arc::new(Selection::new(rows.len() as u64)),
        rows,
        is_active: true,
        filter: None,
        sort: SortKey::default(),
        git: Some(git),
    }
}

fn themed(color_theme: ColorTheme, mode: tokens::Mode) -> tokens::Theme {
    let look = ThemeLook {
        color_theme,
        ..ThemeLook::default()
    };
    tokens::Theme::with_look(mode, look)
}

fn marked_folders(t: &tokens::Theme, ground: Hsla) -> Div {
    let kinds = [
        BadgeKind::Conflict,
        BadgeKind::Modified,
        BadgeKind::Ahead,
        BadgeKind::Diverged,
        BadgeKind::Clean,
        BadgeKind::Unknown,
    ];
    div()
        .flex()
        .items_center()
        .gap(t.spacing.xl)
        .h(t.spacing.row_height)
        .px(t.spacing.md)
        .bg(ground)
        .children(kinds.map(|kind| {
            let folder = icon(IconName::Folder, t.spacing.icon - px(1.), t.colors.folder);
            marked_icon(t, folder, &root(kind), ground)
        }))
}

fn theme_frame(color_theme: ColorTheme, mode: tokens::Mode) -> Div {
    let t = themed(color_theme, mode);
    div()
        .flex()
        .flex_none()
        .items_center()
        .rounded(t.radii.md)
        .border_1()
        .border_color(t.colors.line)
        .overflow_hidden()
        .bg(t.colors.background)
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(
            div()
                .w(px(NAME_WIDTH))
                .px(t.spacing.md)
                .child(color_theme.toml()),
        )
        .child(marked_folders(&t, t.surfaces.card))
        .child(marked_folders(&t, t.surfaces.current))
}

pub struct GitBadgesPage {
    width: f32,
    themes: &'static [ColorTheme],
}

impl GitBadgesPage {
    fn card(&self, t: &tokens::Theme, id: &'static str, list: FileListProps) -> Div {
        let height = t.spacing.row_height * (list.rows.len() + 1) as f32 + t.spacing.xs;
        div()
            .flex_none()
            .h(height)
            .flex()
            .flex_col()
            .rounded(t.radii.md)
            .border_1()
            .border_color(t.colors.line)
            .overflow_hidden()
            .child(FileList::new(id, list))
    }

    fn themes(&self, t: &tokens::Theme) -> Div {
        div().flex().flex_wrap().gap(t.spacing.sm).children(
            self.themes
                .iter()
                .map(|&color_theme| theme_frame(color_theme, t.mode)),
        )
    }

    fn caption(t: &tokens::Theme, text: &'static str) -> Div {
        div()
            .text_size(t.typography.small)
            .text_color(t.colors.muted)
            .child(text)
    }
}

impl Render for GitBadgesPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        div()
            .flex_none()
            .w(px(self.width))
            .h_full()
            .flex()
            .flex_col()
            .gap(t.spacing.md)
            .p(t.spacing.md)
            .bg(t.colors.background)
            .child(Self::caption(&t, "A folder that holds repositories"))
            .child(self.card(&t, "repos", props("Code", repositories())))
            .child(Self::caption(&t, "A folder inside a repository"))
            .child(self.card(&t, "inside", props("navo", inside())))
            .child(Self::caption(&t, "Each colour theme: row, selected row"))
            .child(self.themes(&t))
    }
}

pub fn pages(cx: &mut App) -> [Entity<GitBadgesPage>; 2] {
    [
        (DESKTOP, &ColorTheme::ALL[..]),
        (MINIMUM, &MINIMUM_THEMES[..]),
    ]
    .map(|(width, themes)| cx.new(|_| GitBadgesPage { width, themes }))
}

pub fn element(page: Page, pages: &[Entity<GitBadgesPage>; 2]) -> Option<AnyElement> {
    let shown = match page {
        Page::GitBadges => &pages[0],
        Page::GitBadgesMinimum => &pages[1],
        _ => return None,
    };
    Some(shown.clone().into_any_element())
}
