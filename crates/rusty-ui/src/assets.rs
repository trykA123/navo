use std::borrow::Cow;

use gpui_kit::assets::{Assets as KitAssets, icon_assets};
use gpui_kit::{App, AssetSource, SharedString};

use crate::UiError;

macro_rules! font {
    ($file:literal) => {
        Cow::Borrowed(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/fonts/",
                $file,
                ".ttf"
            ))
            .as_slice(),
        )
    };
}

icon_assets!(
    FerriteIcons,
    [
        Activity,
        AppWindow,
        ArrowRight,
        ArrowUp,
        ArrowUpDown,
        BookOpen,
        Brackets,
        CaseSensitive,
        ChevronDown,
        ChevronLeft,
        ChevronRight,
        CircleQuestionMark,
        CircuitBoard,
        Clipboard,
        ClipboardPaste,
        Cloud,
        CodeXml,
        Columns2,
        Columns3,
        Command,
        Contrast,
        Copy,
        Cpu,
        Database,
        Disc,
        EyeOff,
        FileArchive,
        FileBraces,
        FileCode,
        FileCog,
        FileImage,
        FileKey,
        FileMusic,
        FilePen,
        FilePlay,
        FilePlus,
        FileSpreadsheet,
        FileTerminal,
        FileText,
        FileType,
        Files,
        Folder,
        FolderInput,
        FolderPlus,
        GitCompare,
        HardDrive,
        Keyboard,
        Layers,
        LayoutGrid,
        Link,
        List,
        ListOrdered,
        Lock,
        Map,
        MemoryStick,
        MousePointer,
        Network,
        PanelLeft,
        Pencil,
        Pin,
        Plus,
        Power,
        Presentation,
        Regex,
        RefreshCw,
        Replace,
        RotateCcw,
        Rows3,
        Search,
        SlidersHorizontal,
        SquareChevronRight,
        SquareMenu,
        SquarePlus,
        SquareTerminal,
        Trash,
        TriangleAlert,
        Type,
        Usb,
        X,
    ]
);

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui_kit::Result<Option<Cow<'static, [u8]>>> {
        match FerriteIcons.load(path)? {
            Some(bytes) => Ok(Some(bytes)),
            None => KitAssets.load(path),
        }
    }

    fn list(&self, path: &str) -> gpui_kit::Result<Vec<SharedString>> {
        let mut names = FerriteIcons.list(path)?;
        names.extend(KitAssets.list(path)?);
        Ok(names)
    }
}

pub(crate) fn load_fonts(cx: &App) -> Result<(), UiError> {
    let fonts = vec![
        font!("Inter-Regular"),
        font!("Inter-Medium"),
        font!("Inter-SemiBold"),
        font!("Inter-Bold"),
        font!("SpaceGrotesk-SemiBold"),
        font!("JetBrainsMono-Regular"),
        font!("JetBrainsMono-Medium"),
        font!("JetBrainsMono-SemiBold"),
        font!("JetBrainsMono-Bold"),
    ];
    cx.text_system()
        .add_fonts(fonts)
        .map_err(|error| UiError::Fonts(format!("{error:#}")))
}

#[cfg(test)]
mod tests {
    use gpui_kit::TestAppContext;

    #[gpui_kit::test]
    #[cfg_attr(windows, ignore = "the headless Windows platform has no text system")]
    fn bundled_fonts_register_the_expected_families(cx: &mut TestAppContext) {
        let platform = gpui_kit::platform::current_platform(true);
        let cx = TestAppContext::build_with_text_system(
            cx.dispatcher.clone(),
            None,
            platform.text_system(),
        );
        cx.update(|cx| {
            super::load_fonts(cx).expect("bundled fonts load");
            let families = cx.text_system().all_font_names();
            for family in ["Inter", "Space Grotesk", "JetBrains Mono"] {
                assert!(families.iter().any(|name| name == family), "{families:?}");
            }
            assert!(!families.iter().any(|name| name == "Archivo"));
        });
    }
}
