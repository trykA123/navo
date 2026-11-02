use std::sync::atomic::AtomicBool;

use super::Checkpoint;
use super::stream::Stream;
use crate::{Body, Entry, ReadAt, Source, Trace, Watch};

#[derive(Debug, Clone)]
struct Step {
    checkpoint: Checkpoint,
    wanted: Vec<u64>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct BinaryPlan {
    steps: Vec<Step>,
    end: u64,
}

impl BinaryPlan {
    pub fn new(trace: &Trace, rows: &[u64]) -> Self {
        let mut rows: Vec<_> = rows
            .iter()
            .copied()
            .filter(|row| *row < trace.rows())
            .collect();
        rows.sort_unstable();
        rows.dedup();
        let mut steps: Vec<Step> = Vec::new();
        for row in rows {
            let at = trace
                .binary_blocks
                .partition_point(|block| block.row <= row);
            let Some(checkpoint) = at.checked_sub(1).and_then(|at| trace.binary_blocks.get(at))
            else {
                continue;
            };
            match steps.last_mut() {
                Some(step) if step.checkpoint == *checkpoint => step.wanted.push(row),
                _ => steps.push(Step {
                    checkpoint: *checkpoint,
                    wanted: vec![row],
                }),
            }
        }
        Self {
            steps,
            end: trace.end,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn decode(&self, read: &mut ReadAt<'_>) -> Result<Vec<Entry>, String> {
        let mut stream = Stream::new(Source::new(read, self.end), self.end, 0);
        self.decode_stream(&mut stream)
            .map_err(|error| format!("Cannot read the BLF trace: {error}"))
    }

    fn decode_stream<R: std::io::Read + std::io::Seek>(
        &self,
        stream: &mut Stream<R>,
    ) -> super::Result<Vec<Entry>> {
        let mut out = Vec::new();
        let mut watch = Watch {
            cancel: &AtomicBool::new(false),
            progress: &mut |_| {},
        };
        for step in &self.steps {
            out.extend(decode_step(step, stream, &mut watch)?);
        }
        Ok(out)
    }

    #[cfg(test)]
    pub(super) fn decode_counted(&self, bytes: &[u8]) -> (Vec<Entry>, usize) {
        let mut stream = Stream::new(std::io::Cursor::new(bytes), self.end, 0);
        let entries = self.decode_stream(&mut stream).unwrap();
        (entries, stream.cache.inflations)
    }
}

fn decode_step<R: std::io::Read + std::io::Seek>(
    step: &Step,
    stream: &mut Stream<R>,
    watch: &mut Watch<'_>,
) -> super::Result<Vec<Entry>> {
    stream.seek(step.checkpoint.position);
    let (mut row, mut ordinal) = (step.checkpoint.row, step.checkpoint.object);
    let mut wanted = step.wanted.iter().peekable();
    let mut out = Vec::new();
    while let Some(next) = wanted.peek().copied() {
        let object = stream.next(watch)?.ok_or(super::Error::Invalid(
            "BLF window ended before the indexed row",
        ))?;
        let line = ordinal;
        ordinal += 1;
        let Some((time, frame)) = object.frame else {
            continue;
        };
        if row == *next {
            wanted.next();
            out.push(Entry {
                row,
                line,
                time_ns: Some(time),
                body: Body::Frame(frame),
            });
        }
        row += 1;
    }
    Ok(out)
}
