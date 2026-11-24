//! The image viewer's eyedropper: reads the pixel under the pointer, copies its colour on a click,
//! and draws the status line with Hex, RGB and OKLCH buttons that copy one format each.

use gpui_kit::base::TestSupportExt;
use gpui_kit::{ClipboardItem, Role};
use navo_l10n::{t, tr};

use super::ImageView;
use super::colour::{ColourFormat, Rgba8};
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant};
use crate::tokens::Theme;
use crate::window_view::viewer::{CopyColourHex, CopyColourOklch, CopyColourRgb, MenuEntry};

/// Whether the eyedropper is on, the pixel under the pointer and the last clicked pixel.
#[derive(Default)]
pub(super) struct Eyedropper {
    pub is_on: bool,
    hovered: Option<Rgba8>,
    picked: Option<Rgba8>,
}

impl Eyedropper {
    /// The colour the status line shows and the copy actions take: the hovered pixel, else the picked one.
    fn colour(&self) -> Option<Rgba8> {
        self.hovered.or(self.picked)
    }
}

/// The localised name of a format, as the status line and its tooltips show it.
fn format_name(format: ColourFormat) -> String {
    tr(match format {
        ColourFormat::Hex => "viewer-image-colour-hex",
        ColourFormat::Rgb => "viewer-image-colour-rgb",
        ColourFormat::Oklch => "viewer-image-colour-oklch",
    })
}

/// The viewer-menu items that copy the shown colour, one per format (mouse path to the copy keys).
pub(super) fn copy_entries() -> [MenuEntry; 3] {
    let label = |format| -> SharedString {
        t!("viewer-image-colour-copy", format = format_name(format)).into()
    };
    [
        MenuEntry::Run(label(ColourFormat::Hex), Box::new(CopyColourHex), false),
        MenuEntry::Run(label(ColourFormat::Rgb), Box::new(CopyColourRgb), false),
        MenuEntry::Run(label(ColourFormat::Oklch), Box::new(CopyColourOklch), false),
    ]
}

impl ImageView {
    /// Turns the eyedropper on or off; turning it off forgets the sampled colours.
    pub(super) fn toggle_eyedropper(&mut self, cx: &mut Context<Self>) {
        let is_on = !self.eyedropper.is_on;
        self.eyedropper = Eyedropper {
            is_on,
            ..Eyedropper::default()
        };
        self.drag = None;
        cx.notify();
    }

    /// The image pixel under `position` (window px), or None outside the picture.
    fn pixel_at(&self, position: Point<Pixels>) -> Option<Rgba8> {
        let (origin, shown) = self.zoom.placement(&self.viewport());
        let u = (f32::from(position.x) - origin.x) / shown.width;
        let v = (f32::from(position.y) - origin.y) / shown.height;
        if !(0. ..1.).contains(&u) || !(0. ..1.).contains(&v) {
            return None;
        }
        let size = self.pixels.size(0);
        let (width, height) = (size.width.0.max(0) as usize, size.height.0.max(0) as usize);
        let (x, y) = ((u * width as f32) as usize, (v * height as f32) as usize);
        let at = (y.min(height.saturating_sub(1)) * width + x.min(width.saturating_sub(1))) * 4;
        let [b, g, r, a]: [u8; 4] = self.pixels.as_bytes(0)?.get(at..at + 4)?.try_into().ok()?;
        Some(Rgba8 { r, g, b, a })
    }

    /// Follows the pointer: the status line shows the pixel under it, or the picked one off the picture.
    pub(super) fn hover_pixel(&mut self, position: Option<Point<Pixels>>, cx: &mut Context<Self>) {
        let hovered = position.and_then(|position| self.pixel_at(position));
        if hovered != self.eyedropper.hovered {
            self.eyedropper.hovered = hovered;
            cx.notify();
        }
    }

    /// Picks the pixel under `position` and copies its Hex; a click off the picture does nothing.
    pub(super) fn pick_pixel(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(pixel) = self.pixel_at(position) else {
            return;
        };
        self.eyedropper.picked = Some(pixel);
        self.eyedropper.hovered = Some(pixel);
        self.copy_colour(ColourFormat::Hex, cx);
    }

    /// Copies the shown colour in `format` and says so in the header note.
    pub(super) fn copy_colour(&mut self, format: ColourFormat, cx: &mut Context<Self>) {
        let Some(pixel) = self.eyedropper.colour() else {
            return self.set_note(tr("viewer-image-colour-none").into(), cx);
        };
        let text = pixel.text(format);
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        self.set_note(t!("viewer-image-colour-copied", value = text).into(), cx);
    }

    /// One status-line button: the value in `format`; a click copies it.
    fn colour_button(&self, format: ColourFormat, pixel: Rgba8, cx: &mut Context<Self>) -> Button {
        let name = format_name(format);
        let label = format!("{name} {}", pixel.text(format));
        Button::new(("image-colour", format as usize), label)
            .variant(ButtonVariant::Ghost)
            .tooltip(t!("viewer-image-colour-copy", format = name))
            .on_click(cx.listener(move |view, _, _, cx| view.copy_colour(format, cx)))
    }

    /// The eyedropper status line under the stage, or None while the eyedropper is off.
    pub(super) fn colour_line(
        &self,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        if !self.eyedropper.is_on {
            return None;
        }
        let line = div()
            .id("image-colour-line")
            .test_support()
            .role(Role::Status)
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.sm)
            .px(t.spacing.xl)
            .h(t.spacing.status_height)
            .border_t_1()
            .border_color(t.colors.line)
            .bg(t.colors.rail)
            .text_size(t.typography.small)
            .text_color(t.colors.muted)
            .child(tr("viewer-image-eyedropper"));
        Some(match self.eyedropper.colour() {
            Some(pixel) => {
                line.children(ColourFormat::ALL.map(|format| self.colour_button(format, pixel, cx)))
            }
            None => line.child(tr("viewer-image-colour-hint")),
        })
    }

    /// The pointer position used by tests to aim at a pixel's centre, in window px.
    #[cfg(test)]
    pub(in crate::window_view::viewer) fn pixel_centre(&self, x: u32, y: u32) -> Point<Pixels> {
        let (origin, shown) = self.zoom.placement(&self.viewport());
        let step_x = shown.width / self.zoom.image.width;
        let step_y = shown.height / self.zoom.image.height;
        gpui_kit::point(
            px(origin.x + (x as f32 + 0.5) * step_x),
            px(origin.y + (y as f32 + 0.5) * step_y),
        )
    }
}
