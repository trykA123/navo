use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{Action, Role, Stateful};
use rusty_pdf::protocol::PdfError;

use super::{PdfView, Phase};
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, IconName, TextField, icon};
use crate::tokens::Theme;
use crate::window_view::viewer::{CloseViewer, ShowHex, ShowText};

const CARD_WIDTH: f32 = 480.;

pub(super) fn refusal(error: &PdfError) -> (&'static str, String) {
    match error {
        PdfError::NoEngine(detail) => ("The PDF engine is missing.", detail.clone()),
        PdfError::NoHelper(detail) => ("The PDF helper did not start.", detail.clone()),
        PdfError::Corrupt => (
            "The file is damaged.",
            "Its structure cannot be read, so its pages cannot be found.".to_owned(),
        ),
        PdfError::TooLarge => (
            "The PDF is too large.",
            "It is over the limits of the PDF helper (1 GB per file).".to_owned(),
        ),
        PdfError::Timeout => (
            "The PDF took too long to open.",
            "The PDF helper stopped it. Navo and your files are fine.".to_owned(),
        ),
        PdfError::HelperDied => (
            "The PDF helper stopped while opening it.",
            "Navo and your files are fine.".to_owned(),
        ),
        PdfError::Unreadable(detail) => ("The file cannot be read.", detail.clone()),
        other => ("The PDF could not be opened.", other.to_string()),
    }
}

fn action_button(id: &'static str, label: &'static str, action: impl Action + Clone) -> Button {
    Button::new(id, label).on_click(move |_, window, cx| {
        window.dispatch_action(Box::new(action.clone()), cx);
    })
}

fn card(t: &Theme, id: &'static str) -> ObservedElement<Stateful<Div>> {
    div()
        .id(id)
        .test_support()
        .flex()
        .flex_col()
        .gap(t.spacing.lg)
        .w(px(CARD_WIDTH))
        .p(t.spacing.xxl)
        .rounded(t.radii.card)
        .border_1()
        .border_color(t.colors.line)
        .bg(t.surfaces.card)
        .shadow(t.shadows.card.clone())
}

fn title(t: &Theme, glyph: IconName, color: Hsla, text: &'static str) -> Div {
    div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .child(icon(glyph, t.spacing.icon, color))
        .child(div().font_weight(FontWeight::SEMIBOLD).child(text))
}

fn password_error(t: &Theme, error: SharedString) -> impl IntoElement {
    div()
        .id("pdf-password-error")
        .role(Role::Alert)
        .aria_label(error.clone())
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .text_size(t.typography.small)
        .text_color(t.colors.warn)
        .child(icon(IconName::TriangleAlert, t.spacing.icon, t.colors.warn))
        .child(error)
}

fn centred(t: &Theme, content: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_1()
        .min_h_0()
        .items_center()
        .justify_center()
        .p(t.spacing.xxl)
        .bg(t.colors.background)
        .child(content)
}

impl PdfView {
    pub(super) fn refused_card(&self, t: &Theme, error: &PdfError) -> Div {
        let (headline, detail) = refusal(error);
        let reason: SharedString = format!("{headline} {detail}").into();
        let buttons = div()
            .flex()
            .gap(t.spacing.md)
            .child(
                action_button("pdf-as-text", "View as Text", ShowText)
                    .variant(ButtonVariant::Primary),
            )
            .child(action_button("pdf-as-hex", "View as Hex", ShowHex))
            .child(action_button("pdf-close", "Close", CloseViewer).variant(ButtonVariant::Ghost));
        let body = card(t, "pdf-refused")
            .role(Role::Alert)
            .aria_label(reason)
            .child(title(
                t,
                IconName::TriangleAlert,
                t.colors.warn,
                "Navo did not open this PDF",
            ))
            .child(
                div()
                    .text_color(t.colors.muted)
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.colors.text)
                            .child(headline),
                    )
                    .child(detail),
            )
            .child(buttons);
        centred(t, body)
    }

    pub(super) fn password_card(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let (error, is_checking) = match &self.phase {
            Phase::Locked { error } => (error.clone(), false),
            _ => (None, true),
        };
        let open_label = if is_checking { "Checking…" } else { "Open" };
        let buttons = div()
            .flex()
            .gap(t.spacing.md)
            .child(
                Button::new("pdf-unlock", open_label)
                    .variant(ButtonVariant::Primary)
                    .disabled(is_checking)
                    .on_click(cx.listener(|view, _, window, cx| view.unlock(window, cx))),
            )
            .child(action_button("pdf-as-hex", "View as Hex", ShowHex))
            .child(action_button("pdf-close", "Cancel", CloseViewer).variant(ButtonVariant::Ghost));
        let body = card(t, "pdf-password")
            .child(title(
                t,
                IconName::Lock,
                t.colors.accent,
                "This PDF is protected",
            ))
            .child(div().text_color(t.colors.muted).child(format!(
                "{} needs a password to open. Navo keeps it in memory for this view only.",
                self.title
            )))
            .child(
                div()
                    .text_size(t.typography.small)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Password"),
            )
            .child(TextField::new(
                "pdf-password-input",
                &self.inputs.password,
                "Password",
            ))
            .children(error.map(|error| password_error(t, error)))
            .child(buttons);
        centred(t, body)
    }
}
