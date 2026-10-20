use super::fields::{Control, F, FieldSpec, Look, PRESETS, S, spec};

pub(super) const VIEWER_FIELDS: [FieldSpec; 11] = [
    spec(
        (F::TextLineNumbers, S::Display, "text.view.line-numbers"),
        (
            "Line numbers",
            "Shows the number of each line at its start.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::TextLineEnds, S::Display, "text.view.line-ends"),
        ("Line ends", "Marks where each line ends, as LF or CRLF."),
        Control::Toggle,
    ),
    spec(
        (F::ImageDetails, S::Display, "image.view.details"),
        (
            "Details",
            "Shows a panel with the camera, date and other EXIF data.",
        ),
        Control::Toggle,
    ),
    spec(
        (
            F::MarkdownPreviewSplit,
            S::Display,
            "markdown.view.preview-split",
        ),
        (
            "Preview split",
            "Shows the rendered page beside the source while you edit a Markdown file.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::PreviewTypeBars, S::Display, "preview.view.type-bars"),
        (
            "Type bars",
            "Draws a size bar on each file type in a folder preview.",
        ),
        Control::Toggle,
    ),
    spec(
        (
            F::PreviewLargestFiles,
            S::Display,
            "preview.view.largest-files",
        ),
        (
            "Largest files",
            "Lists the ten largest files in a folder preview.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::AutomotivePreset, S::Display, "automotive.view.preset"),
        (
            "View",
            "Simple keeps the view calm, Rich turns every part on, Custom uses the toggles below.",
        ),
        Control::Choices(&PRESETS, Look::Segments),
    ),
    spec(
        (
            F::AutomotiveInspector,
            S::Display,
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
            S::Display,
            "automotive.view.memory-strip",
        ),
        (
            "Memory strip",
            "Shows a map of the address space beside the ranges list.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::AutomotiveCharts, S::Display, "automotive.view.charts"),
        (
            "Charts",
            "Shows bars, sparklines, bus load and signal curves where a view has them.",
        ),
        Control::Toggle,
    ),
    spec(
        (F::AutomotiveRowBars, S::Display, "automotive.view.row-bars"),
        (
            "Row bars",
            "Shows a size bar and counts on each row of the ranges list.",
        ),
        Control::Toggle,
    ),
];
