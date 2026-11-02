use pulldown_cmark::{CodeBlockKind, Event, LinkType, Options, Parser, Tag, TagEnd};

use super::block::{Open, Piece};
use super::table::TableState;
use super::{BlockKind, Document, Link, MAX_DEPTH, Marker, NO_LINK, Refusal, Style};

fn clamp(depth: usize) -> u8 {
    u8::try_from(depth).unwrap_or(u8::MAX).min(MAX_DEPTH)
}

const STYLES: [Style; 3] = [Style::BOLD, Style::ITALIC, Style::STRIKE];

#[derive(Default)]
struct Inline {
    open: [u32; 3],
    link: Option<u32>,
    images: Vec<u32>,
}

impl Inline {
    fn style(&self) -> (Style, u32) {
        if let Some(image) = self.images.last() {
            return (Style::IMAGE, *image);
        }
        let style = STYLES
            .iter()
            .zip(self.open)
            .filter(|(_, count)| *count > 0)
            .fold(Style::PLAIN, |style, (flag, _)| style.with(*flag));
        (style, self.link.unwrap_or(NO_LINK))
    }

    fn count(&mut self, flag: Style, is_start: bool) {
        let Some(at) = STYLES.iter().position(|style| *style == flag) else {
            return;
        };
        self.open[at] = if is_start {
            self.open[at].saturating_add(1)
        } else {
            self.open[at].saturating_sub(1)
        };
    }
}

fn flag_of(tag: TagEnd) -> Option<Style> {
    match tag {
        TagEnd::Strong => Some(Style::BOLD),
        TagEnd::Emphasis => Some(Style::ITALIC),
        TagEnd::Strikethrough => Some(Style::STRIKE),
        _ => None,
    }
}

#[derive(Default)]
pub(super) struct Builder {
    pub(super) doc: Document,
    pub(super) open: Option<Open>,
    quote: usize,
    lists: Vec<Option<u64>>,
    pending: Option<Marker>,
    inline: Inline,
    line: u32,
    pub(super) table: Option<TableState>,
}

impl Builder {
    pub(super) fn run(source: &str, options: Options) -> Result<Document, Refusal> {
        let mut builder = Self::default();
        let mut counted = 0;
        for (event, at) in Parser::new_ext(source, options).into_offset_iter() {
            if at.start > counted {
                let skipped = source.as_bytes().get(counted..at.start).unwrap_or_default();
                let breaks = skipped.iter().filter(|byte| **byte == b'\n').count();
                builder.line = builder
                    .line
                    .saturating_add(u32::try_from(breaks).unwrap_or(u32::MAX));
                counted = at.start;
            }
            builder.event(event)?;
        }
        builder.finish()?;
        Ok(builder.doc)
    }

    fn event(&mut self, event: Event<'_>) -> Result<(), Refusal> {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.inline_text(&text, Style::PLAIN),
            Event::Code(text) => self.inline_text(&text, Style::CODE),
            Event::Html(text) | Event::InlineHtml(text) => self.inline_text(&text, Style::HTML),
            Event::InlineMath(text) | Event::DisplayMath(text) => {
                self.inline_text(&text, Style::PLAIN)
            }
            Event::FootnoteReference(label) => {
                self.inline_text(&format!("[{label}]"), Style::FOOTNOTE)
            }
            Event::SoftBreak => self.inline_text(" ", Style::PLAIN),
            Event::HardBreak => self.inline_text("\n", Style::PLAIN),
            Event::Rule => {
                self.begin(BlockKind::Rule)?;
                self.finish()
            }
            Event::TaskListMarker(is_done) => {
                self.mark_task(is_done);
                Ok(())
            }
        }
    }

    fn start(&mut self, tag: Tag<'_>) -> Result<(), Refusal> {
        if let Some(flag) = flag_of(tag.to_end()) {
            self.inline.count(flag, true);
            return Ok(());
        }
        match tag {
            Tag::Paragraph => self.begin(BlockKind::Paragraph),
            Tag::Heading { level, .. } => self.begin(BlockKind::Heading(level as u8)),
            Tag::CodeBlock(kind) => self.start_code(&kind),
            Tag::HtmlBlock => self.begin(BlockKind::Html),
            Tag::BlockQuote(_) => {
                self.quote += 1;
                self.finish()
            }
            Tag::List(first) => {
                self.flush_pending()?;
                self.lists.push(first);
                Ok(())
            }
            Tag::Item => self.start_item(),
            Tag::FootnoteDefinition(label) => {
                self.flush_pending()?;
                self.pending = self.label(Some(&label)).map(Marker::Footnote);
                Ok(())
            }
            Tag::Table(_) | Tag::TableHead | Tag::TableRow | Tag::TableCell => {
                self.start_table_tag(&tag)
            }
            Tag::Link { .. } | Tag::Image { .. } => self.start_inline(tag),
            _ => Ok(()),
        }
    }

    fn start_table_tag(&mut self, tag: &Tag<'_>) -> Result<(), Refusal> {
        match tag {
            Tag::Table(alignments) => self.start_table(alignments),
            Tag::TableHead => self.start_row(true),
            Tag::TableRow => self.start_row(false),
            _ => {
                self.start_cell();
                Ok(())
            }
        }
    }

    fn start_inline(&mut self, tag: Tag<'_>) -> Result<(), Refusal> {
        match tag {
            Tag::Link {
                link_type,
                dest_url,
                ..
            } => {
                self.start_link(link_type, dest_url.into_string());
                Ok(())
            }
            Tag::Image { dest_url, .. } => {
                let link = self.add_link(dest_url.into_string());
                self.inline.images.push(link);
                self.inline_text("Image: ", Style::PLAIN)
            }
            _ => Ok(()),
        }
    }

    fn flush_pending(&mut self) -> Result<(), Refusal> {
        if self.pending.is_some() {
            self.begin(BlockKind::Paragraph)?;
        }
        self.finish()
    }

    fn start_code(&mut self, kind: &CodeBlockKind<'_>) -> Result<(), Refusal> {
        let language = match kind {
            CodeBlockKind::Fenced(info) => self.label(info.split_whitespace().next()),
            CodeBlockKind::Indented => None,
        };
        self.begin(BlockKind::Code { language })
    }

    fn start_link(&mut self, link_type: LinkType, target: String) {
        let target = if link_type == LinkType::Email {
            format!("mailto:{target}")
        } else {
            target
        };
        self.inline.link = Some(self.add_link(target));
    }

    fn end(&mut self, tag: TagEnd) -> Result<(), Refusal> {
        if let Some(flag) = flag_of(tag) {
            self.inline.count(flag, false);
            return Ok(());
        }
        match tag {
            TagEnd::BlockQuote(_) => {
                self.finish()?;
                self.quote = self.quote.saturating_sub(1);
                Ok(())
            }
            TagEnd::List(_) => {
                self.finish()?;
                self.lists.pop();
                Ok(())
            }
            TagEnd::Item | TagEnd::FootnoteDefinition => {
                if self.pending.is_some() {
                    self.begin(BlockKind::Paragraph)?;
                }
                self.finish()
            }
            TagEnd::Table => {
                self.table = None;
                self.finish()
            }
            TagEnd::TableCell => {
                self.end_cell();
                Ok(())
            }
            TagEnd::Link => {
                self.inline.link = None;
                Ok(())
            }
            TagEnd::Image => {
                self.inline.images.pop();
                Ok(())
            }
            TagEnd::Paragraph
            | TagEnd::Heading(_)
            | TagEnd::CodeBlock
            | TagEnd::HtmlBlock
            | TagEnd::TableHead
            | TagEnd::TableRow => self.finish(),
            _ => Ok(()),
        }
    }

    fn label(&mut self, label: Option<&str>) -> Option<u32> {
        let label = label.filter(|label| !label.is_empty())?;
        let id = u32::try_from(self.doc.labels.len()).ok()?;
        self.doc.labels.push(label.into());
        Some(id)
    }

    fn add_link(&mut self, target: String) -> u32 {
        let id = u32::try_from(self.doc.links.len()).unwrap_or(NO_LINK);
        let block = u32::try_from(self.doc.blocks.len()).unwrap_or(u32::MAX);
        self.doc.links.push(Link {
            target: target.into_boxed_str(),
            block,
        });
        id
    }

    fn start_item(&mut self) -> Result<(), Refusal> {
        self.flush_pending()?;
        self.pending = Some(match self.lists.last_mut() {
            Some(Some(next)) => {
                let number = *next;
                *next = next.saturating_add(1);
                Marker::Number(u32::try_from(number).unwrap_or(u32::MAX))
            }
            _ => Marker::Bullet,
        });
        Ok(())
    }

    fn mark_task(&mut self, is_done: bool) {
        if self.pending.is_some() {
            self.pending = Some(Marker::Task(is_done));
        } else if let Some(open) = self.open.as_mut() {
            open.marker = Some(Marker::Task(is_done));
        }
    }

    fn inline_text(&mut self, text: &str, extra: Style) -> Result<(), Refusal> {
        if self.open.is_none() {
            self.begin(BlockKind::Paragraph)?;
        }
        let Some(cell) = self.cell() else {
            return self.mark_cut_row();
        };
        let (style, link) = self.inline.style();
        let piece = Piece {
            style: style.with(extra),
            link,
            cell,
        };
        self.push_text(text, &piece)
    }

    pub(super) fn begin(&mut self, kind: BlockKind) -> Result<(), Refusal> {
        self.finish()?;
        self.open = Some(Open::new(
            kind,
            self.pending.take(),
            (clamp(self.quote), clamp(self.lists.len())),
            (self.doc.text.len(), self.doc.spans.len(), self.line),
        ));
        Ok(())
    }
}
