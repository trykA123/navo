use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;

use super::fields::Field;
use super::steps::Nudge;
use super::values::{Change, Value};
use super::{Handler, SheetEvent};
use crate::git::ToolRow;
use crate::prelude::*;
use crate::tokens::Theme;

pub(super) const AUTO: &str = "auto";
pub(super) const OTHER: &str = "custom";
const GIT_CLI: &str = "git-cli";
const NONE: &str = "none";
const OPTION_MIN: f32 = 28.;
const RADIO: f32 = 14.;
const RADIO_DOT: f32 = 6.;

/// What Settings → Git needs from the Git hooks: the tools they found, and the command check.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GitPage {
    pub tools: Vec<ToolRow>,
    /// The tool Auto stands for.
    pub resolved: Option<String>,
    /// Why the command of Other is refused.
    pub problem: Option<String>,
}

impl GitPage {
    #[must_use]
    pub fn new(tools: Vec<ToolRow>, problem: Option<String>) -> Self {
        let resolved = tools
            .iter()
            .find(|tool| tool.selected)
            .map(|tool| tool.label.clone());
        Self {
            tools,
            resolved,
            problem,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ToolOption {
    pub id: String,
    pub label: String,
    pub path: String,
}

fn option(id: &str, label: String, path: String) -> ToolOption {
    ToolOption {
        id: id.to_owned(),
        label,
        path,
    }
}

impl GitPage {
    /// Auto, each tool found, the command line, Other and None, plus the stored tool when it is
    /// none of these. Auto names its tool only while chosen: `resolved` follows the setting.
    pub(super) fn options(&self, current: &str) -> Vec<ToolOption> {
        let auto = match self.resolved.as_ref().filter(|_| current == AUTO) {
            Some(tool) => navo_l10n::t!("git-tool-auto-resolved", tool = tool.clone()),
            None => navo_l10n::tr("git-tool-auto"),
        };
        let mut found = vec![option(AUTO, auto, String::new())];
        let rest = self
            .tools
            .iter()
            .filter(|tool| tool.id != GIT_CLI && tool.id != OTHER);
        found.extend(rest.map(|tool| option(&tool.id, tool.label.clone(), tool.path.clone())));
        let cli_path = self.tools.iter().find(|tool| tool.id == GIT_CLI);
        found.push(option(
            GIT_CLI,
            navo_l10n::tr("git-tool-git-cli"),
            cli_path.map(|tool| tool.path.clone()).unwrap_or_default(),
        ));
        found.push(option(
            OTHER,
            navo_l10n::tr("git-tool-other"),
            String::new(),
        ));
        found.push(option(NONE, navo_l10n::tr("git-tool-none"), String::new()));
        if !current.is_empty() && found.iter().all(|option| option.id != current) {
            found.insert(1, option(current, current.to_owned(), String::new()));
        }
        found
    }

    /// The tool a Left, Right or Space press moves to; `None` at the ends of the list.
    pub(super) fn nudged(&self, current: &str, nudge: Nudge) -> Option<Value> {
        let options = self.options(current);
        let at = options.iter().position(|option| option.id == current)?;
        let to = match nudge {
            Nudge::Back => at.checked_sub(1)?,
            Nudge::Forward => (at + 1 < options.len()).then_some(at + 1)?,
            Nudge::Flip => (at + 1) % options.len(),
        };
        Some(Value::Text(options.get(to)?.id.clone().into()))
    }
}

fn radio(t: &Theme, is_on: bool) -> impl IntoElement + use<> {
    let ring = if is_on {
        t.colors.accent
    } else {
        t.colors.faint
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(RADIO))
        .rounded(t.radii.full)
        .border_1()
        .border_color(ring)
        .when(is_on, |ring| {
            ring.child(
                div()
                    .size(px(RADIO_DOT))
                    .rounded(t.radii.full)
                    .bg(t.colors.accent),
            )
        })
}

fn pick_event(id: &str) -> SheetEvent {
    SheetEvent::Set(Change::text(Field::GitTool, id.to_owned()))
}

fn option_row(
    t: &Theme,
    (index, option): (usize, ToolOption),
    (is_on, send): (bool, Option<&Handler>),
) -> AnyElement {
    let label = SharedString::from(option.label);
    let path = SharedString::from(option.path);
    let row = div()
        .id(("settings-tool", index))
        .role(Role::RadioButton)
        .aria_label(label.clone())
        .aria_selected(is_on)
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .min_h(px(OPTION_MIN))
        .px(t.spacing.md)
        .rounded(t.radii.md)
        .cursor_pointer()
        .hover(|row| row.bg(t.colors.hover))
        .child(radio(t, is_on))
        .child(
            div()
                .flex_none()
                .whitespace_nowrap()
                .when(is_on, |name| name.font_weight(FontWeight::SEMIBOLD))
                .child(label),
        )
        .when(!path.is_empty(), |row| {
            row.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(t.typography.small)
                    .text_color(t.colors.muted)
                    .child(path),
            )
        });
    let Some(send) = send.cloned() else {
        return row.into_any_element();
    };
    let event = pick_event(&option.id);
    row.on_click(move |_, window, cx| send(&event, window, cx))
        .into_any_element()
}

/// The tool list under the Git tool row.
pub(super) fn tool_list(
    t: &Theme,
    page: &GitPage,
    current: &str,
    send: Option<&Handler>,
) -> AnyElement {
    let rows = page.options(current).into_iter().enumerate().map(|pair| {
        let is_on = pair.1.id == current;
        option_row(t, pair, (is_on, send))
    });
    div()
        .id("settings-tools")
        .role(Role::RadioGroup)
        .aria_label(navo_l10n::tr("settings-git-tool"))
        .test_support()
        .flex()
        .flex_col()
        .gap(t.spacing.xxs)
        .pt(t.spacing.md)
        .children(rows)
        .into_any_element()
}

/// The refusal of the Other command, shown under its row.
pub(super) fn problem_note(t: &Theme, problem: &str) -> AnyElement {
    let text = SharedString::from(problem.to_owned());
    div()
        .id("settings-git-command-problem")
        .role(Role::Alert)
        .aria_label(text.clone())
        .test_support()
        .text_size(t.typography.small)
        .text_color(t.colors.mark)
        .child(text)
        .into_any_element()
}
