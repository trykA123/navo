use std::sync::Arc;
use std::time::Duration;

use navo_core::Location;
use navo_ui::clip_palette::{ClipMode, ClipPalette, ClipPaletteProps, FolderProbe, Place};
use navo_ui::prelude::*;
use navo_ui::tokens;

use crate::clip_sample::{GONE, NOW, PROJECT, SECRET_ROW, SNIPPET_ROW, SampleClipboard};
use crate::gallery::Page;

const DESKTOP: f32 = 1280.;
const PHONE: f32 = 390.;
const POP_UP: f32 = 720.;
const FILES_ROW: usize = 1;

/// What a page shows once it is on screen.
#[derive(Clone, Copy)]
pub enum Scene {
    Plain,
    Menu,
    SecretHidden,
    SecretShown,
    Copied,
    SecretMenu,
    /// The pop-up window's palette, filling a 720 × 400 window.
    PopUp,
    PopUpMenu,
    /// The library view in the pop-up window's frame.
    Manage,
    /// Notes mode in the window, with the folder note in the editor.
    Notes,
    /// F2 on a saved snippet: the edit form.
    Edit,
    /// F2 on a secret, then a new password typed: the secret form with its masked field.
    EditSecret,
}

const SCENES: [(Scene, f32); COUNT] = [
    (Scene::Plain, DESKTOP),
    (Scene::Plain, PHONE),
    (Scene::Menu, DESKTOP),
    (Scene::Menu, PHONE),
    (Scene::SecretHidden, DESKTOP),
    (Scene::SecretHidden, PHONE),
    (Scene::SecretShown, DESKTOP),
    (Scene::SecretShown, PHONE),
    (Scene::Copied, DESKTOP),
    (Scene::Copied, PHONE),
    (Scene::SecretMenu, DESKTOP),
    (Scene::SecretMenu, PHONE),
    (Scene::PopUp, POP_UP),
    (Scene::PopUp, PHONE),
    (Scene::PopUpMenu, POP_UP),
    (Scene::Manage, POP_UP),
    (Scene::Manage, PHONE),
    (Scene::Notes, DESKTOP),
    (Scene::Notes, PHONE),
    (Scene::Edit, DESKTOP),
    (Scene::Edit, PHONE),
    (Scene::EditSecret, DESKTOP),
    (Scene::EditSecret, PHONE),
];

pub const COUNT: usize = 23;

fn stage(
    scene: Scene,
    wants_files: bool,
    palette: &mut ClipPalette,
    (window, cx): (&mut Window, &mut Context<ClipPalette>),
) {
    if wants_files && matches!(scene, Scene::Plain | Scene::Menu | Scene::PopUpMenu) {
        palette.select(FILES_ROW, cx);
    }
    match scene {
        Scene::Plain | Scene::PopUp => {}
        Scene::Manage => palette.open_manage(cx),
        Scene::Notes => palette.pick_mode(ClipMode::Notes, cx),
        Scene::Menu | Scene::PopUpMenu => palette.open_actions(cx),
        Scene::SecretHidden => palette.select(SECRET_ROW, cx),
        Scene::SecretShown => {
            palette.select(SECRET_ROW, cx);
            palette.toggle_reveal(cx);
        }
        Scene::Copied => {
            palette.select(SECRET_ROW, cx);
            palette.copy_selected(cx);
        }
        Scene::SecretMenu => {
            palette.select(SECRET_ROW, cx);
            palette.open_actions(cx);
        }
        Scene::Edit => {
            palette.select(SNIPPET_ROW, cx);
            palette.edit_selected(window, cx);
        }
        Scene::EditSecret => {
            palette.select(SECRET_ROW, cx);
            palette.edit_selected(window, cx);
        }
    }
}

/// Tab to the password field and type a new password, as a user would.
fn type_password(window: &mut Window, cx: &mut App) {
    for key in ["tab", "tab"] {
        if let Ok(stroke) = Keystroke::parse_with_os(key, cx.platform_info().os) {
            window.dispatch_keystroke(stroke, cx);
        }
    }
    for typed in "k7#Qm2-vX9!pLr4e".chars() {
        let text = typed.to_string();
        if let Ok(mut stroke) = Keystroke::parse_with_os(&text, cx.platform_info().os) {
            stroke.key_char = Some(text);
            window.dispatch_keystroke(stroke, cx);
        }
    }
}

pub struct ClipPalettePage {
    palette: Entity<ClipPalette>,
    width: f32,
    scene: Option<Scene>,
}

impl ClipPalettePage {
    pub fn new((scene, width): (Scene, f32), window: &mut Window, cx: &mut Context<Self>) -> Self {
        let props = ClipPaletteProps {
            hooks: SampleClipboard::new(
                matches!(scene, Scene::Plain | Scene::PopUp),
                matches!(scene, Scene::Notes),
            ),
            place: match scene {
                Scene::PopUp | Scene::PopUpMenu | Scene::Manage => Place::Anywhere,
                _ => Place::Window,
            },
            folder: Location::new(PROJECT).ok(),
            key_cap: Some("ctrl-shift-v".into()),
            now_ms: Some(NOW),
            width: Some(px(width)),
            folder_probe: Some(Arc::new(|at: &Location| at.as_str() == GONE) as FolderProbe),
        };
        let palette = cx.new(|cx| ClipPalette::new(props, window, cx));
        Self {
            palette,
            width,
            scene: Some(scene),
        }
    }

    /// Plays the scene once the page is first drawn, so a countdown starts when it is seen.
    fn play(&self, scene: Scene, window: &Window, cx: &mut Context<Self>) {
        let shown = self.palette.clone();
        let wants_files = self.width > PHONE;
        cx.spawn_in(window, async move |_, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(200))
                .await;
            let _ = shown.update_in(cx, |palette, window, cx| {
                stage(scene, wants_files, palette, (window, cx));
            });
            if matches!(scene, Scene::Notes) {
                cx.background_executor()
                    .timer(Duration::from_millis(400))
                    .await;
                shown.update(cx, |palette, cx| palette.focus_note(cx));
            }
            if matches!(scene, Scene::EditSecret) {
                cx.background_executor()
                    .timer(Duration::from_millis(400))
                    .await;
                let _ = cx.update(type_password);
            }
        })
        .detach();
    }
}

impl Render for ClipPalettePage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(scene) = self.scene.take() {
            self.play(scene, window, cx);
        }
        let t = tokens::current(cx).clone();
        div()
            .flex()
            .flex_none()
            .justify_center()
            .pt(px(48.))
            .w(px(self.width.min(DESKTOP)))
            .h_full()
            .bg(t.colors.surface)
            .child(self.palette.clone())
    }
}

pub fn pages(window: &mut Window, cx: &mut App) -> [Entity<ClipPalettePage>; COUNT] {
    SCENES.map(|scene| cx.new(|cx| ClipPalettePage::new(scene, window, cx)))
}

pub fn element(page: Page, pages: &[Entity<ClipPalettePage>; COUNT]) -> Option<AnyElement> {
    let at = match page {
        Page::ClipPalette => 0,
        Page::ClipPalette390 => 1,
        Page::ClipPaletteMenu => 2,
        Page::ClipPaletteMenu390 => 3,
        Page::ClipPaletteSecret => 4,
        Page::ClipPaletteSecret390 => 5,
        Page::ClipPaletteShown => 6,
        Page::ClipPaletteShown390 => 7,
        Page::ClipPaletteCountdown => 8,
        Page::ClipPaletteCountdown390 => 9,
        Page::ClipPaletteSecretMenu => 10,
        Page::ClipPaletteSecretMenu390 => 11,
        Page::ClipPopUp => 12,
        Page::ClipPopUp390 => 13,
        Page::ClipPopUpMenu => 14,
        Page::ClipManage => 15,
        Page::ClipManage390 => 16,
        Page::ClipNotes => 17,
        Page::ClipNotes390 => 18,
        Page::ClipEdit => 19,
        Page::ClipEdit390 => 20,
        Page::ClipEditSecret => 21,
        Page::ClipEditSecret390 => 22,
        _ => return None,
    };
    Some(pages[at].clone().into_any_element())
}
