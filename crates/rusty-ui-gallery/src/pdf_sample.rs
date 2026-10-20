use std::sync::Arc;

use rusty_core::{Location, ProviderId};
use rusty_pdf::protocol::{
    Bitmap, CharBox, LinkTarget, Outline, OutlineItem, PageLinks, PageText, PdfError,
    RenderRequest, TextChar,
};
use rusty_ui::prelude::*;
use rusty_ui::window_view::{OpenPdf, PdfPages, PdfStart, PdfView};

const NAME: &str = "vh4410-trm-rev-c.pdf";
const PAGES: u32 = 312;
const WIDTH: f32 = 595.;
const HEIGHT: f32 = 842.;
const MARGIN: f32 = 64.;
#[derive(Clone, Copy)]
struct Style {
    advance: f32,
    line: f32,
    is_heading: bool,
}

const BODY: Style = Style {
    advance: 5.2,
    line: 15.,
    is_heading: false,
};
const HEADING: Style = Style {
    advance: 9.,
    line: 26.,
    is_heading: true,
};
const SENTENCES: [&str; 8] = [
    "The VH4410 monitors VIN continuously and reports every fault on the PGOOD pin.",
    "Switching resumes once VIN rises above the rising threshold after the deglitch time.",
    "The hysteresis between the two thresholds keeps the device from cycling under load.",
    "Thermal shutdown turns the output off above 160 degrees and restarts below 140 degrees.",
    "The soft-start capacitor sets the ramp time of the output from zero to its set point.",
    "Table 4-3 lists the minimum, typical and maximum values over the full temperature range.",
    "Each fault sets a bit in the status register until the host reads and clears it.",
    "The current limit protects the switch during a hard short at the output.",
];
const KEYWORD: [&str; 2] = [
    "When VIN falls below the falling undervoltage threshold, the converter stops switching.",
    "A resistor divider from VIN to EN raises the undervoltage threshold for weak supplies.",
];

struct Glyph {
    text: TextChar,
    is_heading: bool,
}

fn place(glyphs: &mut Vec<Glyph>, line: &str, top: f32, style: Style) {
    for (column, ch) in line.chars().enumerate() {
        let left = MARGIN + column as f32 * style.advance;
        let bounds = CharBox {
            left,
            top,
            right: left + style.advance,
            bottom: top + style.line * 0.8,
        };
        glyphs.push(Glyph {
            text: TextChar { ch, bounds },
            is_heading: style.is_heading,
        });
    }
}

fn wrapped(paragraph: &str, room: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in paragraph.split(' ') {
        let line = lines.last_mut().map_or(0, |line| line.len());
        if line + word.len() + 1 > room {
            lines.push(String::new());
        }
        if let Some(line) = lines.last_mut() {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
    }
    lines
}

fn glyphs(page: u32) -> Vec<Glyph> {
    let mut glyphs = Vec::new();
    let room = ((WIDTH - 2. * MARGIN) / BODY.advance) as usize;
    let heading = format!("4.{} Power stage, part {}\n", page % 9 + 1, page + 1);
    place(&mut glyphs, &heading, 72., HEADING);
    let mut top = 72. + HEADING.line * 1.6;
    for paragraph in 0..5 {
        let mut text: Vec<&str> = (0..3)
            .map(|at| SENTENCES[(page as usize + paragraph * 3 + at) % SENTENCES.len()])
            .collect();
        if page.is_multiple_of(9) && paragraph % 2 == 1 {
            text[1] = KEYWORD[paragraph / 2 % 2];
        }
        for line in wrapped(&text.join(" "), room) {
            place(&mut glyphs, &format!("{line}\n"), top, BODY);
            top += BODY.line;
        }
        top += BODY.line;
    }
    glyphs
}

struct Sheet {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

fn paint(sheet: &mut Sheet, bounds: CharBox, shade: u8) {
    let (width, height) = (sheet.width, sheet.height);
    let scale = width as f32 / WIDTH;
    let x = (bounds.left * scale) as u32..(bounds.right * scale).ceil() as u32;
    let tall = bounds.bottom - bounds.top;
    let y = ((bounds.top + tall * 0.3) * scale) as u32
        ..((bounds.bottom - tall * 0.1) * scale).ceil() as u32;
    for row in y.clone().filter(|row| *row < height) {
        for column in x.clone().filter(|column| *column < width) {
            let at = (row * width + column) as usize * 4;
            sheet.rgba[at..at + 3].fill(shade);
        }
    }
}

struct Manual;

impl PdfPages for Manual {
    fn page_count(&self) -> u32 {
        PAGES
    }

    fn render(&mut self, request: RenderRequest) -> Result<Bitmap, PdfError> {
        let width = request.width;
        let height = (width as f32 * HEIGHT / WIDTH).round() as u32;
        let rgba = vec![255; width as usize * height as usize * 4];
        let mut sheet = Sheet {
            rgba,
            width,
            height,
        };
        for glyph in glyphs(request.page) {
            if glyph.text.ch.is_whitespace() {
                continue;
            }
            let shade = if glyph.is_heading { 48 } else { 150 };
            paint(&mut sheet, glyph.text.bounds, shade);
        }
        Bitmap::new(width, height, sheet.rgba).map_err(|_| PdfError::TooLarge)
    }

    fn text(&mut self, page: u32) -> Result<PageText, PdfError> {
        let chars = glyphs(page).into_iter().map(|glyph| glyph.text).collect();
        PageText::new(WIDTH, HEIGHT, chars).map_err(|_| PdfError::Corrupt)
    }

    fn links(&mut self, _: u32) -> Result<PageLinks, PdfError> {
        PageLinks::new(WIDTH, HEIGHT, Vec::new()).map_err(|_| PdfError::Corrupt)
    }

    fn outline(&mut self) -> Result<Outline, PdfError> {
        let items = CHAPTERS
            .into_iter()
            .map(|(title, level, page)| OutlineItem {
                title: title.to_owned(),
                level,
                target: page.map_or(LinkTarget::Unsupported, |page| LinkTarget::Page {
                    page,
                    top: None,
                }),
            })
            .collect();
        Outline::new(items).map_err(|_| PdfError::Corrupt)
    }
}

const CHAPTERS: [(&str, u8, Option<u32>); 14] = [
    ("1 Overview", 0, Some(0)),
    ("1.1 Features", 1, Some(0)),
    ("1.2 Applications", 1, Some(1)),
    ("2 Pin configuration and functions", 0, Some(3)),
    ("3 Specifications", 0, Some(6)),
    ("3.1 Absolute maximum ratings", 1, Some(6)),
    (
        "3.2 Electrical characteristics over the full recommended operating temperature range",
        1,
        Some(7),
    ),
    ("3.2.1 Input supply and undervoltage lockout", 2, Some(8)),
    ("4 Detailed description", 0, Some(12)),
    ("4.1 Power stage", 1, Some(12)),
    ("4.2 Fault protection", 1, Some(31)),
    ("5 Application information", 0, Some(48)),
    ("Revision history", 0, None),
    (
        "Mechanical, packaging and orderable information",
        0,
        Some(310),
    ),
];

fn start() -> PdfStart {
    let open: OpenPdf = Arc::new(|_, _| Ok(Box::new(Manual) as Box<dyn PdfPages>));
    PdfStart {
        title: NAME.into(),
        len: 18_402_361,
        location: Location::provider_root(ProviderId::local()),
        open,
        open_uri: Arc::new(|_| {}),
        opened: Ok(Box::new(Manual)),
        is_preview: false,
    }
}

pub fn pdf_views(search: &str, window: &mut Window, cx: &mut App) -> [Entity<PdfView>; 4] {
    [
        pdf_view(None, window, cx),
        pdf_view(Some(search), window, cx),
        pdf_view(Some(search), window, cx),
        outline_view(window, cx),
    ]
}

fn outline_view(window: &mut Window, cx: &mut App) -> Entity<PdfView> {
    cx.new(|cx| {
        let mut view = PdfView::new(start(), window, cx);
        view.open_outline(window, cx);
        view
    })
}

pub fn pdf_view(search: Option<&str>, window: &mut Window, cx: &mut App) -> Entity<PdfView> {
    cx.new(|cx| {
        let mut view = PdfView::new(start(), window, cx);
        if let Some(text) = search {
            view.find_text(text, window, cx);
        }
        view
    })
}
