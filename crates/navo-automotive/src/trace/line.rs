use std::io::{self, BufRead};

pub(crate) const MAX_LINE: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Taken {
    pub consumed: u64,
    pub too_long: bool,
    pub terminated: bool,
}

pub(crate) fn next_line(
    source: &mut impl BufRead,
    line: &mut Vec<u8>,
) -> io::Result<Option<Taken>> {
    line.clear();
    let mut taken = Taken {
        consumed: 0,
        too_long: false,
        terminated: false,
    };
    while !taken.terminated {
        let available = match source.fill_buf() {
            Ok(available) => available,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        if available.is_empty() {
            break;
        }
        let end = available.iter().position(|byte| *byte == b'\n');
        let chunk = available
            .get(..end.map_or(available.len(), |at| at + 1))
            .unwrap_or_default();
        if !taken.too_long {
            if line.len() + chunk.len() > MAX_LINE + 2 {
                taken.too_long = true;
                line.clear();
            } else {
                line.extend_from_slice(chunk);
            }
        }
        let count = chunk.len();
        source.consume(count);
        taken.consumed += count as u64;
        taken.terminated = end.is_some();
    }
    Ok((taken.consumed > 0).then_some(taken))
}
