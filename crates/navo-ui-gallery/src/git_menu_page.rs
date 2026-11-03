use navo_ui::git::menu::{Model, Source, build};
use navo_ui::git::menu_view::{Look, preview};
use navo_ui::git::{BadgeKind, BadgeReason, GitAction, GitMenu, GitMenuItem, RepoBadge};
use navo_ui::prelude::*;
use navo_ui::tokens;

use crate::gallery::Page;

const DESKTOP: f32 = 1280.;
const MINIMUM: f32 = 480.;
const FETCHED_MS: i64 = 1_791_920_400_000;
const PULL_ASK: &str = "Pull main from origin: fast-forward 2 commits";
const PUSH_ASK: &str = "Push main to origin: 3 commits";

fn root(kind: BadgeKind) -> RepoBadge {
    RepoBadge {
        kind,
        is_root: true,
        stale: false,
        branch: Some("main".to_owned()),
        upstream: Some("origin/main".to_owned()),
        ahead: 0,
        behind: 0,
        changed: 0,
        untracked: 0,
        conflicted: 0,
        fetched_ms: Some(FETCHED_MS),
        reason: None,
    }
}

fn item(action: GitAction, label: &str, confirm: Option<&str>) -> GitMenuItem {
    GitMenuItem {
        action,
        label: label.to_owned(),
        confirm: confirm.map(str::to_owned),
        confirm_id: None,
        pending: false,
    }
}

fn gui_tool() -> GitMenu {
    GitMenu {
        head: "navo".to_owned(),
        items: vec![
            item(GitAction::Commit, "Commit…", None),
            item(GitAction::Pull, "Pull", None),
            item(GitAction::Push, "Push", None),
            item(GitAction::Log, "Log", None),
            item(GitAction::Diff, "Diff", None),
        ],
    }
}

fn command_line() -> GitMenu {
    GitMenu {
        head: "flasher".to_owned(),
        items: vec![
            item(GitAction::Commit, "Commit…", None),
            item(GitAction::Pull, "Pull…", Some(PULL_ASK)),
            item(GitAction::Push, "Push…", Some(PUSH_ASK)),
            item(GitAction::Log, "Log", None),
        ],
    }
}

fn open_only() -> GitMenu {
    GitMenu {
        head: "notes".to_owned(),
        items: vec![item(GitAction::Open, "Open in GitKraken", None)],
    }
}

fn no_tool() -> GitMenu {
    GitMenu {
        head: "docs-site".to_owned(),
        items: vec![
            item(GitAction::Diff, "Diff", None),
            item(GitAction::ChooseTool, "Choose a Git tool…", None),
        ],
    }
}

struct Scene {
    caption: &'static str,
    model: Model,
}

fn scene(caption: &'static str, menu: &GitMenu, badge: &RepoBadge, tool: Option<&str>) -> Scene {
    let source = Source {
        menu,
        badge,
        tool,
        can_diff: true,
        previews: None,
    };
    Scene {
        caption,
        model: build(&source),
    }
}

fn modified() -> RepoBadge {
    RepoBadge {
        changed: 3,
        untracked: 1,
        ahead: 2,
        behind: 1,
        ..root(BadgeKind::Modified)
    }
}

fn below() -> RepoBadge {
    RepoBadge {
        is_root: false,
        branch: None,
        upstream: None,
        fetched_ms: None,
        changed: 12,
        ..root(BadgeKind::Modified)
    }
}

fn desktop_scenes() -> Vec<Scene> {
    let behind = RepoBadge {
        behind: 2,
        ..root(BadgeKind::Behind)
    };
    let diverged = RepoBadge {
        ahead: 3,
        behind: 5,
        ..root(BadgeKind::Diverged)
    };
    let unknown = RepoBadge {
        reason: Some(BadgeReason::NotTrusted),
        ..root(BadgeKind::Unknown)
    };
    let tortoise = Some("TortoiseGit");
    let cli = Some("Git command line");
    vec![
        scene(
            "Changes, ahead and behind",
            &gui_tool(),
            &modified(),
            tortoise,
        ),
        scene("Behind, command line", &command_line(), &behind, cli),
        scene("Diverged, command line", &command_line(), &diverged, cli),
        scene("Unknown", &gui_tool(), &unknown, tortoise),
        scene(
            "Open-only tool",
            &open_only(),
            &root(BadgeKind::Clean),
            Some("GitKraken"),
        ),
        scene("No tool", &no_tool(), &root(BadgeKind::Clean), None),
    ]
}

fn minimum_scenes() -> Vec<Scene> {
    vec![
        scene(
            "Changes, ahead and behind",
            &gui_tool(),
            &modified(),
            Some("TortoiseGit"),
        ),
        scene(
            "A folder inside a repository",
            &gui_tool(),
            &below(),
            Some("TortoiseGit"),
        ),
    ]
}

pub struct GitMenuPage {
    width: f32,
    scenes: Vec<Scene>,
}

impl Render for GitMenuPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let cards = self.scenes.iter().map(|scene| {
            let look = Look {
                theme: &t,
                selected: Some(0),
                hint: Some("alt-g"),
            };
            div()
                .flex()
                .flex_col()
                .gap(t.spacing.md)
                .child(
                    div()
                        .text_size(t.typography.small)
                        .text_color(t.colors.muted)
                        .child(scene.caption),
                )
                .child(preview(&scene.model, &look))
        });
        div()
            .flex_none()
            .w(px(self.width))
            .h_full()
            .flex()
            .flex_wrap()
            .content_start()
            .gap(t.spacing.xl)
            .p(t.spacing.xl)
            .bg(t.colors.background)
            .children(cards)
    }
}

pub fn pages(cx: &mut App) -> [Entity<GitMenuPage>; 2] {
    [(DESKTOP, desktop_scenes()), (MINIMUM, minimum_scenes())]
        .map(|(width, scenes)| cx.new(|_| GitMenuPage { width, scenes }))
}

pub fn element(page: Page, pages: &[Entity<GitMenuPage>; 2]) -> Option<AnyElement> {
    let shown = match page {
        Page::GitMenu => &pages[0],
        Page::GitMenuMinimum => &pages[1],
        _ => return None,
    };
    Some(shown.clone().into_any_element())
}
