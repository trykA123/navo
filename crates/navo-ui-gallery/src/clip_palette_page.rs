use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use navo_core::Location;
use navo_ui::clip_palette::{
    ClipCounts, ClipId, ClipKind, ClipMode, ClipPalette, ClipPaletteProps, ClipResults, ClipRow,
    ClipboardHooks, How, Origin, Place, SavedDraft, Scope, SecretText,
};
use navo_ui::prelude::*;
use navo_ui::tokens;

use crate::gallery::Page;

const NOW: i64 = 1_793_000_000_000;
const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;
const DESKTOP: f32 = 1280.;
const PHONE: f32 = 390.;
const POP_UP: f32 = 720.;
const FILES_ROW: usize = 1;
const SECRET_ROW: usize = HISTORY.len() + SAVED.len() - 2;
const PROJECT: &str = r"D:\Projects\ecu-a2";
const OUT: &str = r"D:\Projects\ecu-a2\build\out";

type Sample = (ClipKind, &'static str, &'static str, &'static str, i64);

const HISTORY: [Sample; 10] = [
    (
        ClipKind::Text,
        "Flash with TASKING 6.3r1; 6.2 breaks the .mdf import",
        "",
        "Teams",
        MINUTE,
    ),
    (
        ClipKind::Files,
        "3 files · ecu_a2_main.elf, ecu_a2_main.map, ecu_a2.mdf",
        "D:\\Projects\\ecu-a2\\build\\out\\ecu_a2_main.elf\nD:\\Projects\\ecu-a2\\build\\out\\ecu_a2_main.map\nD:\\Projects\\ecu-a2\\build\\out\\ecu_a2.mdf",
        "Navo",
        4 * MINUTE,
    ),
    (
        ClipKind::Path,
        r"D:\Projects\ecu-a2\build\out\ecu_a2_main.elf",
        "",
        "Navo",
        6 * MINUTE,
    ),
    (
        ClipKind::Text,
        "cargo xtask check -p navo-index",
        "",
        "Terminal",
        12 * MINUTE,
    ),
    (
        ClipKind::Link,
        "https://ci.example.lan/job/ecu-a2/1482/console",
        "",
        "Edge",
        25 * MINUTE,
    ),
    (
        ClipKind::Text,
        "0x8000_4A10  .text.CanIf_RxIndication  1 284 B",
        "",
        "Navo F3",
        40 * MINUTE,
    ),
    (
        ClipKind::Files,
        "2 files · DBC_PT_CAN_v14.dbc, A2L_ecu_a2_r7.a2l",
        "\\\\fileserver\\calib\\r7\\DBC_PT_CAN_v14.dbc\n\\\\fileserver\\calib\\r7\\A2L_ecu_a2_r7.a2l",
        "Navo",
        HOUR,
    ),
    (
        ClipKind::Text,
        "git log --oneline -20 origin/main",
        "",
        "Terminal",
        2 * HOUR,
    ),
    (
        ClipKind::Text,
        "Review notes for the CAN gateway: timeouts at 50 ms, not 20.",
        "",
        "Outlook",
        DAY + 2 * HOUR,
    ),
    (
        ClipKind::Path,
        r"\\fileserver\calib\r7\A2L_ecu_a2_r7.a2l",
        "",
        "Navo",
        DAY + 5 * HOUR,
    ),
];

const SAVED: [Sample; 8] = [
    (
        ClipKind::Text,
        "Check the index crate",
        "cargo xtask check -p navo-index",
        "",
        0,
    ),
    (
        ClipKind::Text,
        "Flash ECU A2 over CAN",
        "tricore-flash --port PCAN_USB1 --baud 500k build\\out\\ecu_a2_main.hex",
        PROJECT,
        2 * DAY,
    ),
    (ClipKind::Path, "ECU A2 build output", OUT, "", 0),
    (
        ClipKind::Path,
        "Calibration share",
        r"\\fileserver\calib\r7",
        "",
        7 * DAY,
    ),
    (
        ClipKind::Text,
        "Review reply",
        "Thanks, merged. The flash script now checks the .mdf version first.",
        "",
        3 * DAY,
    ),
    (ClipKind::Text, "Signature", "Regards,\nadmin", "", 0),
    (ClipKind::Secret, "Build server SSH", "admin", "", 0),
    (ClipKind::Secret, "Artifactory token", "ci-bot", "", 7 * DAY),
];

/// What `reveal` gives back, by title; a secret row carries only its user name.
const SECRETS: [(&str, &str); 2] = [
    ("Build server SSH", "k7#Qm2-vX9!pLr4e"),
    ("Artifactory token", "AKCp8ihxR2b9TqWm4sLkZ7"),
];

fn clip_row(id: u64, from: Origin, (kind, title, detail, extra, ago): Sample) -> ClipRow {
    let (source_app, scope) = match from {
        Origin::Clipboard => (Some(extra.to_owned()), Scope::Everywhere),
        Origin::Saved => (
            None,
            Location::new(extra).map_or(Scope::Everywhere, Scope::Folder),
        ),
    };
    ClipRow {
        id: ClipId(id),
        kind,
        title: title.to_owned(),
        detail: detail.to_owned(),
        source_app,
        when_ms: NOW - ago,
        scope,
        from,
        ranges: Vec::new(),
    }
}

struct SampleClipboard {
    rows: Vec<ClipRow>,
    /// The first-open line shows once, as on a first run.
    intro: AtomicBool,
}

impl SampleClipboard {
    fn new(has_intro: bool) -> Arc<Self> {
        let history = HISTORY.iter().zip(0..);
        let saved = SAVED.iter().zip(100..);
        let rows = history
            .map(|(sample, id)| clip_row(id, Origin::Clipboard, *sample))
            .chain(saved.map(|(sample, id)| clip_row(id, Origin::Saved, *sample)))
            .collect();
        Arc::new(Self {
            rows,
            intro: AtomicBool::new(has_intro),
        })
    }
}

fn is_in(mode: ClipMode, row: &ClipRow) -> bool {
    match mode {
        ClipMode::All => true,
        ClipMode::Recent => row.from == Origin::Clipboard,
        ClipMode::Saved => row.from == Origin::Saved && row.kind != ClipKind::Secret,
        ClipMode::Secrets => row.kind == ClipKind::Secret,
        ClipMode::Notes => false,
    }
}

impl ClipboardHooks for SampleClipboard {
    fn counts(&self) -> Option<ClipCounts> {
        Some(ClipCounts::default())
    }

    fn modes(&self) -> Vec<ClipMode> {
        vec![
            ClipMode::All,
            ClipMode::Recent,
            ClipMode::Saved,
            ClipMode::Secrets,
        ]
    }

    fn search(&self, query: &str, mode: ClipMode, _: Option<&Location>) -> ClipResults {
        let query = query.to_lowercase();
        let rows = self
            .rows
            .iter()
            .filter(|row| is_in(mode, row) && row.title.to_lowercase().contains(&query))
            .cloned()
            .collect();
        ClipResults {
            rows,
            kept: HISTORY.len() as u32,
            revision: 1,
            notice: None,
        }
    }

    fn copy(&self, _: ClipId, _: How) -> Result<(), String> {
        Ok(())
    }

    fn delete(&self, _: ClipId) -> Result<(), String> {
        Ok(())
    }

    fn clear_history(&self) -> Result<(), String> {
        Ok(())
    }

    fn delete_all(&self) -> Result<(), String> {
        Ok(())
    }

    fn set_recording(&self, _: bool) -> Result<(), String> {
        Ok(())
    }

    fn subscribe(&self, _: Box<dyn Fn() + Send + Sync>) {}

    fn save_from_history(&self, id: ClipId) -> Result<ClipId, String> {
        Ok(id)
    }

    fn save(&self, _: SavedDraft) -> Result<ClipId, String> {
        Ok(ClipId(0))
    }

    fn note_for(&self, _: &Location) -> Option<String> {
        None
    }

    fn set_note(&self, _: Scope, _: String) -> Result<(), String> {
        Ok(())
    }

    fn reveal(&self, id: ClipId) -> Result<SecretText, String> {
        let row = self.rows.iter().find(|row| row.id == id);
        let secret = SECRETS
            .iter()
            .find(|(title, _)| Some(*title) == row.map(|r| r.title.as_str()));
        secret
            .map(|(_, text)| SecretText::new((*text).to_owned()))
            .ok_or_else(String::new)
    }

    fn copy_user(&self, _: ClipId) -> Result<(), String> {
        Ok(())
    }

    fn take_intro(&self) -> bool {
        self.intro.swap(false, Ordering::SeqCst)
    }
}

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
];

pub const COUNT: usize = 17;

fn stage(
    scene: Scene,
    wants_files: bool,
    palette: &mut ClipPalette,
    cx: &mut Context<ClipPalette>,
) {
    if wants_files && matches!(scene, Scene::Plain | Scene::Menu | Scene::PopUpMenu) {
        palette.select(FILES_ROW, cx);
    }
    match scene {
        Scene::Plain | Scene::PopUp => {}
        Scene::Manage => palette.open_manage(cx),
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
            hooks: SampleClipboard::new(matches!(scene, Scene::Plain | Scene::PopUp)),
            place: match scene {
                Scene::PopUp | Scene::PopUpMenu | Scene::Manage => Place::Anywhere,
                _ => Place::Window,
            },
            folder: Location::new(PROJECT).ok(),
            key_cap: Some("ctrl-shift-v".into()),
            now_ms: Some(NOW),
            width: Some(px(width)),
        };
        let palette = cx.new(|cx| ClipPalette::new(props, window, cx));
        Self {
            palette,
            width,
            scene: Some(scene),
        }
    }

    /// Plays the scene once the page is first drawn, so a countdown starts when it is seen.
    fn play(&self, scene: Scene, cx: &mut Context<Self>) {
        let shown = self.palette.clone();
        let wants_files = self.width > PHONE;
        cx.spawn(async move |_, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(200))
                .await;
            shown.update(cx, |palette, cx| stage(scene, wants_files, palette, cx));
        })
        .detach();
    }
}

impl Render for ClipPalettePage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(scene) = self.scene.take() {
            self.play(scene, cx);
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
        _ => return None,
    };
    Some(pages[at].clone().into_any_element())
}
