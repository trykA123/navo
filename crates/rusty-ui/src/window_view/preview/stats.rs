use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{ClickEvent, Role, Stateful};
use rusty_fs::FolderProfile;

use super::folder::FolderPreview;
use super::scan::{Phase, Scan};
use crate::file_list::row::inset_ring;
use crate::format;
use crate::prelude::*;
use crate::primitives::{IconName, icon};
use crate::tokens::Theme;
use crate::window_view::FerriteWindow;

const MINI_HEIGHT: f32 = 22.;

fn card(t: &Theme, label: &str, value: String) -> Div {
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .py(t.spacing.lg)
        .px(t.spacing.xl)
        .rounded(t.radii.card)
        .bg(t.surfaces.field)
        .shadow(inset_ring(t.surfaces.card_edge))
        .child(
            div()
                .truncate()
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(label.to_owned()),
        )
        .child(
            div()
                .truncate()
                .font_weight(FontWeight::SEMIBOLD)
                .font_family(t.typography.numbers_font.clone())
                .font_features(t.typography.numbers_font_features.clone())
                .child(value),
        )
}

fn sub(t: &Theme, text: String) -> Div {
    div()
        .truncate()
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(text)
}

fn mini(
    t: &Theme,
    id: &'static str,
    glyph: IconName,
    label: &'static str,
) -> ObservedElement<Stateful<Div>> {
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(label)
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xs)
        .h(px(MINI_HEIGHT))
        .px(t.spacing.sm)
        .rounded(t.radii.pill)
        .cursor_pointer()
        .bg(t.surfaces.sheet)
        .text_color(t.colors.text)
        .shadow(inset_ring(t.surfaces.card_edge))
        .child(icon(glyph, px(12.), t.colors.muted))
        .child(label)
}

fn seconds(took: std::time::Duration) -> String {
    format!("{:.1} s", took.as_secs_f64())
}

fn scan_words(scan: &Scan) -> String {
    match &scan.phase {
        Phase::Done { .. } if scan.is_recounting => "Recounting".to_owned(),
        Phase::Counting { .. } => "Counting".to_owned(),
        Phase::Done { took: Some(took) } => format!("Done in {} · cached", seconds(*took)),
        Phase::Done { took: None } => "Done · cached".to_owned(),
        Phase::Stopped => "Stopped, partial".to_owned(),
        Phase::Failed => "Could not read every folder".to_owned(),
    }
}

fn live_marks(t: &Theme, scan: &Scan) -> Option<Div> {
    if !matches!(scan.phase, Phase::Done { .. }) {
        return None;
    }
    let counted_ms = scan.profile.as_ref()?.counted_ms?;
    let counted = format!("Counted at {}", format::clock_seconds(counted_ms));
    let live = scan.is_live().then(|| {
        div()
            .id("preview-live")
            .aria_label("Live")
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.xs)
            .text_color(t.colors.ok)
            .child(div().size(px(6.)).rounded(t.radii.full).bg(t.colors.ok))
            .child("Live")
    });
    let line = div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .min_h(px(MINI_HEIGHT))
        .text_size(t.typography.small)
        .text_color(t.colors.muted)
        .child(
            div()
                .id("preview-counted")
                .aria_label(counted.clone())
                .test_support()
                .truncate()
                .child(counted),
        )
        .children(live);
    Some(line)
}

impl FerriteWindow {
    fn scan_line(t: &Theme, scan: &Scan, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let words = scan_words(scan);
        let phase = &scan.phase;
        let (glyph, color) = match phase {
            Phase::Done { .. } if scan.is_recounting => {
                (IconName::LoaderCircle, t.colors.accent_text)
            }
            Phase::Done { .. } => (IconName::Check, t.colors.ok),
            Phase::Counting { .. } => (IconName::LoaderCircle, t.colors.accent_text),
            Phase::Stopped | Phase::Failed => (IconName::CircleAlert, t.colors.muted),
        };
        let action = match phase {
            Phase::Counting { .. } => Some(
                mini(t, "preview-stop", IconName::Square, "Stop").on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| {
                        this.stop_scan(cx);
                    },
                )),
            ),
            Phase::Stopped => Some(
                mini(t, "preview-resume", IconName::Play, "Resume").on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| {
                        this.resume_scan(cx);
                    },
                )),
            ),
            Phase::Done { .. } | Phase::Failed => None,
        };
        div()
            .id("preview-scan")
            .role(Role::Status)
            .aria_label(words.clone())
            .test_support()
            .flex()
            .items_center()
            .gap(t.spacing.sm)
            .min_h(px(MINI_HEIGHT))
            .mt(t.spacing.xs)
            .text_size(t.typography.small)
            .text_color(color)
            .child(icon(glyph, px(12.), color))
            .child(div().truncate().child(words))
            .children(action)
    }

    pub(super) fn stats(
        folder: &FolderPreview,
        is_row: bool,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let profile = folder.scan.profile.as_ref();
        let (totals, is_final) = folder.shown_totals();
        let is_done = matches!(folder.scan.phase, Phase::Done { .. });
        let size_label = if is_final {
            "Total size"
        } else {
            "Total size so far"
        };
        let size = card(t, size_label, format::size(totals.bytes))
            .id("preview-total")
            .aria_label(format::bytes(totals.bytes))
            .test_support()
            .text_size(t.typography.body * 1.5)
            .child(Self::scan_line(t, &folder.scan, cx))
            .children(live_marks(t, &folder.scan));
        let mut folders_sub = format!("{} at top", folder.contents.top_folders());
        if totals.skipped > 0 {
            folders_sub.push_str(&format!(" · {} skipped", format::bytes(totals.skipped)));
        }
        let level = if is_final { "all levels" } else { "so far" };
        let files = card(t, "Files", format::bytes(totals.files)).child(sub(t, level.to_owned()));
        let folders = card(t, "Folders", format::bytes(totals.folders)).child(sub(t, folders_sub));
        let changed = Self::changed_card(folder, profile, is_done, t);
        let rest = [files, folders, changed].map(|card| card.flex_1().flex_basis(px(0.)));
        let row = div().flex().gap(t.spacing.md);
        if is_row {
            row.child(size.flex_grow(1.6).flex_basis(px(0.)).min_w_0())
                .children(rest)
        } else {
            div()
                .flex()
                .flex_col()
                .gap(t.spacing.md)
                .child(size)
                .child(row.children(rest))
        }
    }

    fn changed_card(
        folder: &FolderPreview,
        profile: Option<&FolderProfile>,
        is_done: bool,
        t: &Theme,
    ) -> Div {
        let newest = profile.and_then(|profile| profile.newest_ms);
        let (moment, source) = match (newest, is_done) {
            (Some(newest), true) => (Some(newest), "newest file"),
            _ => (folder.target.entry.modified_ms, "folder date"),
        };
        let text = moment.map(format::date).unwrap_or_default();
        let (day, time) = text.split_once(' ').unwrap_or((text.as_str(), ""));
        let detail = if time.is_empty() {
            source.to_owned()
        } else {
            format!("{time} · {source}")
        };
        card(t, "Last change", day.to_owned()).child(sub(t, detail))
    }
}
