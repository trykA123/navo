//! The terminal grid: an `alacritty_terminal::Term` fed by a session's output, read by the UI
//! as a `Snapshot` in Navo types. Output sets a dirty flag and wakes the UI once until the next
//! snapshot, so a burst never queues repaints.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{self, Column, Line};
use alacritty_terminal::term::cell::{Cell as TermCell, Flags};
use alacritty_terminal::term::{Config, Term, TermDamage, TermMode};
use alacritty_terminal::vte::ansi::{self, CursorShape as TermShape, NamedColor, Processor, Rgb};
use parking_lot::{Mutex, MutexGuard};

use crate::{ReplyFn, Sink, Size};

/// Default history kept above the visible rows, in lines.
pub const DEFAULT_SCROLLBACK: usize = 10_000;
/// Output parsed per lock; a fair unlock after each chunk lets a waiting snapshot in.
const FEED_CHUNK: usize = 64 * 1024;

/// A cell colour: the theme's default, an ANSI palette index (0–255) or a literal RGB.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Color {
    /// The theme's default foreground or background.
    #[default]
    Default,
    /// An ANSI palette index: 0–15 the named colours, 16–255 the extended palette.
    Ansi(u8),
    /// A literal colour as red, green and blue.
    Rgb(u8, u8, u8),
}

/// How many columns a cell's character spans.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Width {
    #[default]
    Single,
    /// The first of a wide character's two cells.
    Wide,
    /// The second cell of a wide character; draw nothing.
    Spacer,
}

/// One character cell with its colours and style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    /// The character drawn in the cell (a space when empty).
    pub c: char,
    /// How many columns the character spans.
    pub width: Width,
    /// The text colour.
    pub fg: Color,
    /// The background colour.
    pub bg: Color,
    /// SGR 1: bold text.
    pub bold: bool,
    /// SGR 2: faint text, painted at reduced strength.
    pub dim: bool,
    /// SGR 3: italic text.
    pub italic: bool,
    /// Any underline style (single, double, curly, dotted, dashed).
    pub underline: bool,
    /// SGR 7: foreground and background swapped.
    pub inverse: bool,
    /// SGR 8: the character is not drawn.
    pub hidden: bool,
    /// SGR 9: a line through the text.
    pub strikeout: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            c: ' ',
            width: Width::Single,
            fg: Color::Default,
            bg: Color::Default,
            bold: false,
            dim: false,
            italic: false,
            underline: false,
            inverse: false,
            hidden: false,
            strikeout: false,
        }
    }
}

/// One visible row, one entry per column.
pub type RowCells = Vec<Cell>;

/// How the cursor is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorShape {
    Block,
    HollowBlock,
    Underline,
    Bar,
}

/// The cursor's cell in viewport coordinates (row 0 = top visible row).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub row: u16,
    pub col: u16,
    pub shape: CursorShape,
}

/// A grid position: `line` 0 is the top screen row, negative lines are in the scrollback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub line: i32,
    pub col: usize,
}

/// The selected cells, `start` top-left and `end` bottom-right, inclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub start: Point,
    pub end: Point,
    /// A rectangular (block) selection rather than a run of text.
    pub block: bool,
}

/// Which part of the history to show: `offset` lines above the bottom (clamped to the history).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Viewport {
    pub offset: usize,
}

/// Terminal modes the input encoder needs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modes {
    /// Pastes are wrapped in `ESC[200~` … `ESC[201~`.
    pub bracketed_paste: bool,
    /// Arrow keys send `ESC O` sequences instead of `ESC [`.
    pub app_cursor: bool,
}

/// What the UI paints: the visible rows and which of them changed since the last snapshot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub rows: Vec<RowCells>,
    /// `None` while the shell hides the cursor or it is scrolled out of view.
    pub cursor: Option<Cursor>,
    pub selection: Option<Selection>,
    /// Lines the view is scrolled above the bottom.
    pub display_offset: usize,
    /// Visible rows changed since the previous snapshot, ascending, each once.
    pub damaged: Vec<u16>,
}

/// Terminal theme RGB defaults used to answer OSC 10/11/12 colour queries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeRgb {
    pub fg: (u8, u8, u8),
    pub bg: (u8, u8, u8),
    pub cursor: (u8, u8, u8),
}

impl Default for ThemeRgb {
    fn default() -> Self {
        Self {
            fg: (0x1e, 0x24, 0x2b),
            bg: (0xf6, 0xf8, 0xfa),
            cursor: (0x0f, 0x76, 0x6e),
        }
    }
}

/// Forwards terminal query replies (`PtyWrite`, OSC 10/11/12 `ColorRequest`) back to the PTY.
#[derive(Clone, Default)]
struct GridEvents {
    reply: Arc<Mutex<Option<ReplyFn>>>,
    theme: Arc<Mutex<ThemeRgb>>,
}

impl GridEvents {
    fn send_reply(&self, bytes: &[u8]) {
        let sender = self.reply.lock().clone();
        if let Some(reply) = sender {
            reply(bytes);
        }
    }
}

impl EventListener for GridEvents {
    fn send_event(&self, event: Event) {
        match event {
            Event::PtyWrite(text) => self.send_reply(text.as_bytes()),
            Event::ColorRequest(index, formatter) => {
                let theme = *self.theme.lock();
                let rgb = if index == NamedColor::Foreground as usize {
                    Some(theme.fg)
                } else if index == NamedColor::Background as usize {
                    Some(theme.bg)
                } else if index == NamedColor::Cursor as usize {
                    Some(theme.cursor)
                } else {
                    None
                };
                if let Some((r, g, b)) = rgb {
                    let reply = formatter(Rgb { r, g, b });
                    self.send_reply(reply.as_bytes());
                }
            }
            _ => {}
        }
    }
}

/// `Size` in the shape `Term` wants.
struct Dims(Size);

impl Dimensions for Dims {
    fn total_lines(&self) -> usize {
        self.screen_lines()
    }

    fn screen_lines(&self) -> usize {
        usize::from(self.0.rows)
    }

    fn columns(&self) -> usize {
        usize::from(self.0.cols)
    }
}

struct State {
    term: Term<GridEvents>,
    parser: Processor,
    exit: Option<Option<u32>>,
}

/// A terminal grid shared by the session's sink thread (writer) and the UI (reader).
pub struct Grid {
    state: Mutex<State>,
    events: GridEvents,
    dirty: AtomicBool,
    wake: Box<dyn Fn() + Send + Sync>,
    /// Runs under the lock after each output chunk, so tests can see the handover.
    #[cfg(test)]
    chunk_hook: Option<Box<dyn Fn() + Send + Sync>>,
}

impl Grid {
    /// A blank grid of `size` keeping `scrollback` history lines. `wake` runs on the output
    /// thread when the grid turns dirty; it must only schedule a repaint, never block.
    pub fn new(size: Size, scrollback: usize, wake: impl Fn() + Send + Sync + 'static) -> Self {
        let config = Config {
            scrolling_history: scrollback,
            // Input encoder emits standard xterm sequences; never advertise Kitty CSI-u mode.
            kitty_keyboard: false,
            ..Config::default()
        };
        let events = GridEvents::default();
        let state = State {
            term: Term::new(config, &Dims(size), events.clone()),
            parser: Processor::new(),
            exit: None,
        };
        Self {
            state: Mutex::new(state),
            events,
            dirty: AtomicBool::new(false),
            wake: Box::new(wake),
            #[cfg(test)]
            chunk_hook: None,
        }
    }

    /// Updates the RGB colours reported to shells querying OSC 10/11/12.
    pub fn set_theme_rgb(&self, theme: ThemeRgb) {
        *self.events.theme.lock() = theme;
    }

    /// Runs `hook` under the lock after each output chunk (the Linux-only handover test).
    #[cfg(all(test, target_os = "linux"))]
    pub(crate) fn with_chunk_hook(mut self, hook: impl Fn() + Send + Sync + 'static) -> Self {
        self.chunk_hook = Some(Box::new(hook));
        self
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock()
    }

    /// Sets the dirty flag; only the change from clean wakes the UI.
    fn mark_dirty(&self) {
        if !self.dirty.swap(true, Ordering::AcqRel) {
            (self.wake)();
        }
    }

    /// Whether output arrived since the last snapshot.
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Acquire)
    }

    /// Resizes the grid; the caller resizes the PTY to the same size.
    pub fn resize(&self, size: Size) {
        self.lock().term.resize(Dims(size));
        self.mark_dirty();
    }

    /// History lines held above the screen.
    pub fn history_len(&self) -> usize {
        self.lock().term.grid().history_size()
    }

    /// Monotonic grid line of the cursor (`history_size + cursor_row`).
    pub fn cursor_line(&self) -> usize {
        let state = self.lock();
        let grid = state.term.grid();
        let row = usize::try_from(grid.cursor.point.line.0).unwrap_or(0);
        grid.history_size().saturating_add(row)
    }

    /// The modes the shell turned on, for encoding keys and pastes.
    pub fn modes(&self) -> Modes {
        let mode = *self.lock().term.mode();
        Modes {
            bracketed_paste: mode.contains(TermMode::BRACKETED_PASTE),
            app_cursor: mode.contains(TermMode::APP_CURSOR),
        }
    }

    /// The shell's exit once it ended: `Some(None)` when its status could not be read.
    pub fn exit(&self) -> Option<Option<u32>> {
        self.lock().exit
    }

    /// Clears a recorded exit status before restarting a shell in the same grid.
    pub fn clear_exit(&self) {
        self.lock().exit = None;
        self.mark_dirty();
    }

    /// Clears the dirty flag and reads the visible rows at `viewport`. Damage is taken: the
    /// next snapshot reports only rows changed after this one.
    pub fn snapshot(&self, viewport: Viewport) -> Snapshot {
        let dirty = self.dirty.swap(false, Ordering::AcqRel);
        let mut state = self.lock();
        let term = &mut state.term;
        let moved = scroll_to(term, viewport.offset);
        let damaged = if dirty || moved {
            take_damage(term)
        } else {
            Vec::new()
        };
        Snapshot {
            rows: rows(term),
            cursor: cursor(term),
            selection: selection(term),
            display_offset: term.grid().display_offset(),
            damaged,
        }
    }
}

impl Sink for Grid {
    fn output(&self, bytes: &[u8]) {
        for chunk in bytes.chunks(FEED_CHUNK) {
            let mut state = self.lock();
            let State { term, parser, .. } = &mut *state;
            parser.advance(term, chunk);
            #[cfg(test)]
            if let Some(hook) = &self.chunk_hook {
                hook();
            }
            // A plain unlock lets this thread retake the lock before a parked reader wakes.
            MutexGuard::unlock_fair(state);
        }
        if !bytes.is_empty() {
            self.mark_dirty();
        }
    }

    fn exited(&self, code: Option<u32>) {
        self.lock().exit = Some(code);
        self.mark_dirty();
    }

    fn cursor_line(&self) -> Option<usize> {
        Some(self.cursor_line())
    }

    fn attach_reply(&self, reply: ReplyFn) {
        *self.events.reply.lock() = Some(reply);
    }
}

/// Moves the view `offset` lines above the bottom; returns whether it moved.
fn scroll_to(term: &mut Term<GridEvents>, offset: usize) -> bool {
    let current = term.grid().display_offset();
    let target = offset.min(term.grid().history_size());
    if target == current {
        return false;
    }
    let distance = i32::try_from(target.abs_diff(current)).unwrap_or(i32::MAX);
    let delta = if target > current {
        distance
    } else {
        -distance
    };
    term.scroll_display(Scroll::Delta(delta));
    true
}

fn take_damage(term: &mut Term<GridEvents>) -> Vec<u16> {
    let rows = term.screen_lines();
    let mut damaged: Vec<u16> = match term.damage() {
        TermDamage::Full => (0..rows)
            .filter_map(|row| u16::try_from(row).ok())
            .collect(),
        TermDamage::Partial(lines) => lines
            .map(|bounds| bounds.line)
            .filter(|&line| line < rows)
            .filter_map(|line| u16::try_from(line).ok())
            .collect(),
    };
    term.reset_damage();
    damaged.sort_unstable();
    damaged.dedup();
    damaged
}

/// Grid line of viewport row `row`.
fn line_of(row: usize, display_offset: usize) -> Line {
    let row = i32::try_from(row).unwrap_or(i32::MAX);
    Line(row.saturating_sub(i32::try_from(display_offset).unwrap_or(i32::MAX)))
}

fn rows(term: &Term<GridEvents>) -> Vec<RowCells> {
    let grid = term.grid();
    let offset = grid.display_offset();
    (0..grid.screen_lines())
        .map(|row| {
            let line = &grid[line_of(row, offset)];
            (0..grid.columns())
                .map(|col| cell(&line[Column(col)]))
                .collect()
        })
        .collect()
}

fn cell(cell: &TermCell) -> Cell {
    let flags = cell.flags;
    let width = if flags.contains(Flags::WIDE_CHAR) {
        Width::Wide
    } else if flags.contains(Flags::WIDE_CHAR_SPACER) {
        Width::Spacer
    } else {
        Width::Single
    };
    Cell {
        c: cell.c,
        width,
        fg: color(cell.fg),
        bg: color(cell.bg),
        bold: flags.contains(Flags::BOLD),
        dim: flags.contains(Flags::DIM),
        italic: flags.contains(Flags::ITALIC),
        underline: flags.intersects(Flags::ALL_UNDERLINES),
        inverse: flags.contains(Flags::INVERSE),
        hidden: flags.contains(Flags::HIDDEN),
        strikeout: flags.contains(Flags::STRIKEOUT),
    }
}

fn color(color: ansi::Color) -> Color {
    match color {
        ansi::Color::Spec(rgb) => Color::Rgb(rgb.r, rgb.g, rgb.b),
        ansi::Color::Indexed(index) => Color::Ansi(index),
        ansi::Color::Named(named) => named_color(named),
    }
}

/// Palette names map to their index, dim colours to their base; the rest is the default.
fn named_color(named: NamedColor) -> Color {
    let index = named as usize;
    let first_dim = NamedColor::DimBlack as usize;
    let palette = if index < 16 {
        Some(index)
    } else if (first_dim..=NamedColor::DimWhite as usize).contains(&index) {
        Some(index - first_dim)
    } else {
        None
    };
    palette
        .and_then(|index| u8::try_from(index).ok())
        .map_or(Color::Default, Color::Ansi)
}

fn cursor(term: &Term<GridEvents>) -> Option<Cursor> {
    let content = term.renderable_content();
    let shape = match content.cursor.shape {
        TermShape::Block => CursorShape::Block,
        TermShape::HollowBlock => CursorShape::HollowBlock,
        TermShape::Underline => CursorShape::Underline,
        TermShape::Beam => CursorShape::Bar,
        TermShape::Hidden => return None,
    };
    let point = content.cursor.point;
    let row = point
        .line
        .0
        .checked_add(i32::try_from(content.display_offset).ok()?)?;
    let row = u16::try_from(row).ok()?;
    (usize::from(row) < term.screen_lines()).then_some(Cursor {
        row,
        col: u16::try_from(point.column.0).ok()?,
        shape,
    })
}

fn selection(term: &Term<GridEvents>) -> Option<Selection> {
    let range = term.renderable_content().selection?;
    let point = |point: index::Point| Point {
        line: point.line.0,
        col: point.column.0,
    };
    Some(Selection {
        start: point(range.start),
        end: point(range.end),
        block: range.is_block,
    })
}
