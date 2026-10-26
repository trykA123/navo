mod count;
mod hex;
mod index;
mod layout;
mod rows;
mod search;

pub use count::{Counted, count, ordinal};
pub use hex::{HEX_WIDTH, HexRow, hex_row, hex_row_start, hex_rows};
pub use index::{LineIndex, MARK_BYTES, MARK_EVERY};
pub use layout::{Laid, Newlines, TextLayout};
pub use rows::{LOOK_BACK, MAX_ROW, Row, previous_row, row_at, row_start_at, rows_down, rows_up};
pub use search::{Found, Needle, SEARCH_CHUNK, Search, find, parse_hex};

pub trait ByteSource {
    fn len(&self) -> u64;
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String>;

    fn probe_end(&self) -> Result<(), String> {
        self.read_at(self.len(), &mut [0]).map(|_| ())
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn take_changed(&self) -> bool {
        false
    }

    fn take_rewritten(&self) -> bool {
        false
    }

    fn layout(&self) -> TextLayout {
        TextLayout::default()
    }
}

pub fn read_full<S: ByteSource + ?Sized>(
    src: &S,
    offset: u64,
    buf: &mut [u8],
) -> Result<usize, String> {
    let mut filled = 0;
    while filled < buf.len() {
        let count = src.read_at(offset + filled as u64, &mut buf[filled..])?;
        if count == 0 {
            break;
        }
        filled += count;
    }
    Ok(filled)
}

fn span(from: u64, to: u64) -> usize {
    usize::try_from(to.saturating_sub(from)).unwrap_or(usize::MAX)
}

impl ByteSource for [u8] {
    fn len(&self) -> u64 {
        u64::try_from(<[u8]>::len(self)).unwrap_or(u64::MAX)
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        let Ok(start) = usize::try_from(offset) else {
            return Ok(0);
        };
        let Some(rest) = self.get(start..) else {
            return Ok(0);
        };
        let count = rest.len().min(buf.len());
        buf[..count].copy_from_slice(&rest[..count]);
        Ok(count)
    }
}

#[cfg(test)]
mod tests_count;
#[cfg(test)]
mod tests_hex;
#[cfg(test)]
mod tests_index;
#[cfg(test)]
mod tests_layout;
#[cfg(test)]
mod tests_review;
#[cfg(test)]
mod tests_rows;
#[cfg(test)]
mod tests_search;
#[cfg(test)]
mod tests_support;
