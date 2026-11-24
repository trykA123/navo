use super::fields::{Control, F, FieldSpec, Look, PRESETS, S, SYNTAX_THEMES, spec};

pub(super) const VIEWER_FIELDS: [FieldSpec; 15] = [
    spec(
        (F::TextLineNumbers, S::Viewers, "text.view.line-numbers"),
        (
            "Line numbers",
            "Shows the number of each line at its start.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::TextLineEnds, S::Viewers, "text.view.line-ends"),
        ("Line ends", "Marks where each line ends, as LF or CRLF."),
        Control::Toggle,
    ),
    spec(
        (F::TextSyntaxTheme, S::Viewers, "text.view.syntax-theme"),
        ("settings-syntax-theme", "settings-syntax-theme-help"),
        Control::Choices(&SYNTAX_THEMES, Look::Segments),
    ),
    spec(
        (F::ImageDetails, S::Viewers, "image.view.details"),
        (
            "Details",
            "Shows a panel with the camera, date and other EXIF data.",
        ),
        Control::Toggle,
    ),
    spec(
        (
            F::MarkdownPreviewSplit,
            S::Viewers,
            "markdown.view.preview-split",
        ),
        (
            "Preview split",
            "Shows the rendered page beside the source while you edit a Markdown file.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::PreviewTypeBars, S::Viewers, "preview.view.type-bars"),
        (
            "Type bars",
            "Draws a size bar on each file type in a folder preview.",
        ),
        Control::Toggle,
    ),
    spec(
        (
            F::PreviewLargestFiles,
            S::Viewers,
            "preview.view.largest-files",
        ),
        (
            "Largest files",
            "Lists the ten largest files in a folder preview.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::AutomotivePreset, S::Viewers, "automotive.view.preset"),
        (
            "View",
            "Simple keeps the view calm, Rich turns every part on, Custom uses the toggles below.",
        ),
        Control::Choices(&PRESETS, Look::Segments),
    ),
    spec(
        (
            F::AutomotiveInspector,
            S::Viewers,
            "automotive.view.inspector",
        ),
        (
            "Open the inspector",
            "Opens the details drawer by itself when you pick a range or section.",
        ),
        Control::Toggle,
    ),
    spec(
        (
            F::AutomotiveMemoryStrip,
            S::Viewers,
            "automotive.view.memory-strip",
        ),
        (
            "Memory strip",
            "Shows a map of the address space beside the ranges list.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::AutomotiveCharts, S::Viewers, "automotive.view.charts"),
        (
            "Charts",
            "Shows bars, sparklines, bus load and signal curves where a view has them.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::AutomotiveRowBars, S::Viewers, "automotive.view.row-bars"),
        (
            "Row bars",
            "Shows a size bar and counts on each row of the ranges list.",
        ),
        Control::Toggle,
    ),
    spec(
        (
            F::AutomotiveUsageBars,
            S::Viewers,
            "automotive.view.usage-bars",
        ),
        ("settings-usage-bars", "settings-usage-bars-help"),
        Control::Toggle,
    ),
    spec(
        (F::AutomotiveWhoUses, S::Viewers, "automotive.view.who-uses"),
        ("settings-who-uses", "settings-who-uses-help"),
        Control::Toggle,
    ),
    spec(
        (F::AutomotiveModules, S::Viewers, "automotive.modules"),
        ("settings-module-prefixes", "settings-module-prefixes-help"),
        Control::Text,
    ),
];
