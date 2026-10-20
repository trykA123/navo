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
        font!("Archivo-Regular"),
        font!("Archivo-Medium"),
        font!("Archivo-SemiBold"),
        font!("Archivo-Bold"),
        font!("JetBrainsMono-Regular"),
        font!("JetBrainsMono-Medium"),
        font!("JetBrainsMono-SemiBold"),
        font!("JetBrainsMono-Bold"),
    ];
    cx.text_system()
        .add_fonts(fonts)
        .map_err(|error| UiError::Fonts(format!("{error:#}")))
}
