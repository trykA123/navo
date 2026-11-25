use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::grid::{Cell, Color, Grid, Snapshot, Viewport, Width};
use crate::{Sink, Size};

const SIZE: Size = Size { cols: 80, rows: 24 };

fn grid() -> (Grid, Arc<AtomicUsize>) {
    let wakes = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&wakes);
    let grid = Grid::new(SIZE, 10_000, move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    (grid, wakes)
}

fn text(cells: &[Cell]) -> String {
    let line: String = cells
        .iter()
        .filter(|cell| cell.width != Width::Spacer)
        .map(|cell| cell.c)
        .collect();
    line.trim_end().to_owned()
}

fn bottom(grid: &Grid) -> Snapshot {
    grid.snapshot(Viewport::default())
}

#[test]
fn grid_keeps_the_newest_ten_thousand_lines_of_twenty_thousand() {
    let (grid, _) = grid();
    let lines: String = (0..20_000).map(|n| format!("line {n}\r\n")).collect();
    grid.output(lines.as_bytes());
    assert_eq!(grid.history_len(), 10_000);
    let top = grid.snapshot(Viewport { offset: usize::MAX });
    assert_eq!(top.display_offset, 10_000);
    let first = top.rows.first().map(|row| text(row));
    // 10,000 history lines + 24 screen lines, the last of them the empty cursor line.
    assert_eq!(first.as_deref(), Some("line 9977"));
    let last = bottom(&grid);
    let shown = last.rows.get(22).map(|row| text(row));
    assert_eq!(shown.as_deref(), Some("line 19999"));
}

#[test]
fn grid_gives_a_wide_cjk_char_two_cells() {
    let (grid, _) = grid();
    grid.output("中a".as_bytes());
    let snapshot = bottom(&grid);
    let row = snapshot.rows.first().cloned().unwrap_or_default();
    let widths: Vec<_> = row
        .iter()
        .take(3)
        .map(|cell| (cell.c, cell.width))
        .collect();
    assert_eq!(
        widths,
        [
            ('中', Width::Wide),
            (' ', Width::Spacer),
            ('a', Width::Single)
        ]
    );
    let cursor = snapshot.cursor.map(|cursor| (cursor.row, cursor.col));
    assert_eq!(cursor, Some((0, 3)));
}

#[test]
fn grid_reads_sgr_colours_and_flags() {
    let (grid, _) = grid();
    grid.output(b"\x1b[31mr\x1b[0;1;3;4;7;38;2;1;2;3;48;5;200mx");
    let snapshot = bottom(&grid);
    let row = snapshot.rows.first().cloned().unwrap_or_default();
    let red = row.first().copied().unwrap_or_default();
    assert_eq!((red.fg, red.bg), (Color::Ansi(1), Color::Default));
    let styled = row.get(1).copied().unwrap_or_default();
    assert_eq!(styled.fg, Color::Rgb(1, 2, 3));
    assert_eq!(styled.bg, Color::Ansi(200));
    assert!(styled.bold && styled.italic && styled.underline && styled.inverse);
}

#[test]
fn grid_reads_dim_hidden_and_strikeout_flags() {
    let (grid, _) = grid();
    grid.output(b"p\x1b[2;8;9mx");
    let snapshot = bottom(&grid);
    let row = snapshot.rows.first().cloned().unwrap_or_default();
    let flags = |cell: Cell| (cell.dim, cell.hidden, cell.strikeout);
    let plain = row.first().copied().unwrap_or_default();
    assert_eq!(flags(plain), (false, false, false));
    let styled = row.get(1).copied().unwrap_or_default();
    assert_eq!(flags(styled), (true, true, true));
}

#[test]
fn grid_marks_each_visible_row_damaged_once_after_a_one_mib_burst() {
    let (grid, wakes) = grid();
    drop(bottom(&grid));
    let line = format!("{}\r\n", "x".repeat(62));
    let burst = line.repeat(1024 * 1024 / line.len() + 1);
    grid.output(burst.as_bytes());
    grid.output(b"tail");
    assert_eq!(
        wakes.load(Ordering::SeqCst),
        1,
        "one wake until the next snapshot"
    );
    assert!(grid.is_dirty());
    let snapshot = bottom(&grid);
    let rows: Vec<u16> = (0..SIZE.rows).collect();
    assert_eq!(snapshot.damaged, rows);
    assert!(!grid.is_dirty());
    assert!(bottom(&grid).damaged.is_empty(), "damage is taken once");
    grid.output(b"!");
    assert_eq!(wakes.load(Ordering::SeqCst), 2);
    assert_eq!(bottom(&grid).damaged, [SIZE.rows - 1]);
}

#[test]
fn grid_exposes_bracketed_paste_and_app_cursor_modes() {
    let (grid, _) = grid();
    assert_eq!(grid.modes(), crate::grid::Modes::default());
    grid.output(b"\x1b[?2004h\x1b[?1h");
    let modes = grid.modes();
    assert!(modes.bracketed_paste && modes.app_cursor);
    grid.output(b"\x1b[?2004l");
    assert!(!grid.modes().bracketed_paste);
}

#[test]
fn grid_records_the_exit_and_wakes() {
    let (grid, wakes) = grid();
    grid.exited(Some(7));
    assert_eq!(grid.exit(), Some(Some(7)));
    assert_eq!(wakes.load(Ordering::SeqCst), 1);
}

/// The lock handover test reads `/proc` to see the reader block, so it runs on Linux only.
#[cfg(target_os = "linux")]
mod handover {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, OnceLock};

    use super::SIZE;
    use crate::Sink;
    use crate::grid::Grid;

    /// Spins (yielding, no sleep) until `done` holds; fails after a generous bound.
    fn spin_until(done: impl Fn() -> bool) {
        let start = std::time::Instant::now();
        while !done() {
            assert!(
                start.elapsed() < crate::tests::DEADLINE,
                "spin_until timed out"
            );
            std::thread::yield_now();
        }
    }

    /// The calling thread's `/proc` stat file, so another thread can see it block.
    fn own_stat() -> PathBuf {
        let task = std::fs::read_link("/proc/thread-self").unwrap_or_default();
        Path::new("/proc").join(task).join("stat")
    }

    /// Whether the thread behind `stat` sleeps (blocked on the lock).
    fn sleeping(stat: &Path) -> bool {
        let text = std::fs::read_to_string(stat).unwrap_or_default();
        let state = text.rsplit_once(") ").map(|(_, rest)| rest);
        state.is_some_and(|rest| rest.starts_with('S'))
    }

    #[test]
    fn grid_hands_the_lock_to_a_waiting_reader_between_output_chunks() {
        const CHUNKS: usize = 32;
        let fed = Arc::new(AtomicUsize::new(0));
        let reader_stat = Arc::new(OnceLock::<PathBuf>::new());
        let done = Arc::new(AtomicBool::new(false));
        let (hook_fed, hook_stat, hook_done) = (
            Arc::clone(&fed),
            Arc::clone(&reader_stat),
            Arc::clone(&done),
        );
        let grid = Grid::new(SIZE, 10_000, || {}).with_chunk_hook(move || {
            match hook_fed.fetch_add(1, Ordering::SeqCst) {
                // Chunk 1 holds the lock until the reader is blocked waiting for it.
                0 => spin_until(|| hook_stat.get().is_some_and(|stat| sleeping(stat))),
                // Chunk 2 runs only after the reader had the lock: a plain unlock never gets here.
                1 => spin_until(|| hook_done.load(Ordering::SeqCst)),
                _ => {}
            }
        });
        let grid = Arc::new(grid);
        let line = format!("{}\r\n", "x".repeat(62));
        let burst = line.repeat(CHUNKS * 64 * 1024 / line.len());
        let reader = {
            let (grid, fed, stat) = (Arc::clone(&grid), Arc::clone(&fed), reader_stat);
            std::thread::spawn(move || {
                spin_until(|| fed.load(Ordering::SeqCst) >= 1);
                drop(stat.set(own_stat()));
                let _ = grid.history_len();
                done.store(true, Ordering::SeqCst);
            })
        };
        grid.output(burst.as_bytes());
        assert!(reader.join().is_ok());
        assert_eq!(fed.load(Ordering::SeqCst), CHUNKS);
    }
}
