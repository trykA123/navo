use gpui_kit::Role;
use gpui_kit::base::TestSupportExt;

use super::index_progress::{IndexProgress, IndexState, VolumeProgress};
use super::search_page::{heading, row};
use crate::prelude::*;
use crate::tokens::Theme;

pub(super) const PROGRESS_WORDS: &str =
    "index progress building updating ready volume files percent memory service";

const DOT: f32 = 8.;
const BAR: f32 = 6.;

fn dot_color(t: &Theme, state: IndexState) -> Hsla {
    match state {
        IndexState::Ready => t.colors.ok,
        IndexState::Building | IndexState::Updating => t.colors.accent,
        IndexState::NotRunning => t.colors.muted,
    }
}

fn state_chip(t: &Theme, state: IndexState) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.sm)
        .text_size(t.typography.small)
        .child(
            div()
                .flex_none()
                .size(px(DOT))
                .rounded(t.radii.full)
                .bg(dot_color(t, state)),
        )
        .child(state.text())
}

fn muted(t: &Theme, text: impl Into<SharedString>) -> Div {
    div()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(text.into())
}

fn state_row(t: &Theme, progress: &IndexProgress) -> AnyElement {
    let state = progress.state();
    let chip = state_chip(t, state)
        .id("settings-index-state")
        .role(Role::Status)
        .aria_label(state.text())
        .test_support();
    let mut controls = vec![chip.into_any_element()];
    if let Some(current) = progress.current() {
        let text = navo_l10n::t!("settings-index-current", root = current.root.to_string());
        controls.push(muted(t, text).into_any_element());
    }
    if progress.is_reachable {
        let files = navo_l10n::t!(
            "settings-index-files",
            count = crate::format::bytes(progress.files())
        );
        controls.push(muted(t, files).into_any_element());
    }
    row(
        t,
        "settings-index-progress",
        ("settings-index-state", "settings-index-state-help"),
        controls,
    )
    .into_any_element()
}

fn memory_row(t: &Theme, progress: &IndexProgress) -> AnyElement {
    let text = match progress.memory {
        Some(bytes) => crate::format::size(bytes),
        None => navo_l10n::tr("settings-index-memory-unknown"),
    };
    let value = div()
        .id("settings-index-memory-value")
        .role(Role::Status)
        .aria_label(text.clone())
        .test_support()
        .text_size(t.typography.small)
        .child(text);
    row(
        t,
        "settings-index-memory",
        ("settings-index-memory", "settings-index-memory-help"),
        vec![value.into_any_element()],
    )
    .into_any_element()
}

fn bar(t: &Theme, index: usize, percent: u8) -> impl IntoElement + use<> {
    div()
        .id(("settings-index-bar", index))
        .role(Role::ProgressIndicator)
        .aria_label(format!("{percent}%"))
        .test_support()
        .flex_1()
        .min_w(px(48.))
        .h(px(BAR))
        .rounded(px(BAR / 2.))
        .overflow_hidden()
        .bg(t.surfaces.progress_track)
        .child(
            div()
                .h_full()
                .w(relative(f32::from(percent.min(100)) / 100.))
                .bg(t.surfaces.progress_fill),
        )
}

fn volume_row(t: &Theme, index: usize, volume: &VolumeProgress) -> AnyElement {
    let files = navo_l10n::t!(
        "settings-index-files",
        count = crate::format::bytes(volume.files)
    );
    let title = div()
        .flex()
        .items_center()
        .justify_between()
        .gap(t.spacing.lg)
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .child(volume.root.clone()),
        )
        .child(state_chip(t, volume.state));
    let meter = div()
        .flex()
        .items_center()
        .gap(t.spacing.lg)
        .when_some(volume.percent, |line, percent| {
            line.child(bar(t, index, percent))
                .child(muted(t, format!("{percent}%")))
        })
        .child(muted(t, files));
    div()
        .id(("settings-index-volume", index))
        .role(Role::ListItem)
        .aria_label(volume.root.clone())
        .test_support()
        .flex()
        .flex_col()
        .flex_none()
        .gap(t.spacing.sm)
        .px(t.spacing.xl)
        .py(t.spacing.md)
        .child(title)
        .child(meter)
        .into_any_element()
}

pub(super) fn progress_block(t: &Theme, progress: &IndexProgress) -> Vec<AnyElement> {
    let mut block = vec![
        heading(t, "settings-index-heading"),
        state_row(t, progress),
        memory_row(t, progress),
    ];
    block.extend(
        progress
            .volumes
            .iter()
            .enumerate()
            .map(|(index, volume)| volume_row(t, index, volume)),
    );
    block
}
