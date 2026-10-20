use std::io::{Read, Seek, SeekFrom};

use super::stream::Stream;
use super::{BlfHeader, Checkpoint, Error, check};
use crate::trace::TraceProblem;
use crate::{IndexError, Trace, Watch};

pub fn index_blf(mut source: impl Read + Seek, mut watch: Watch<'_>) -> Result<Trace, IndexError> {
    let mut trace = Trace::new_blf();
    check(&mut watch, 0).map_err(index_error)?;
    let end = source
        .seek(SeekFrom::End(0))
        .map_err(|error| IndexError::Read(error.to_string()))?;
    trace.end = end;
    let header = match BlfHeader::read(&mut source, end) {
        Ok(header) => header,
        Err(error) => return finish_problem(trace, error, Default::default()),
    };
    let start = u64::from(header.size);
    if let Some(info) = &mut trace.blf {
        info.header = Some(header);
    }
    let mut stream = Stream::new(source, end, start);
    scan(&mut trace, &mut stream, &mut watch)?;
    Ok(trace)
}

fn scan<R: Read + Seek>(
    trace: &mut Trace,
    stream: &mut Stream<R>,
    watch: &mut Watch<'_>,
) -> Result<(), IndexError> {
    loop {
        if trace.lines.is_multiple_of(4096) {
            check(watch, stream.scanned()).map_err(index_error)?;
        }
        match stream.next(watch) {
            Ok(Some(object)) => take_object(trace, object),
            Ok(None) => break,
            Err(error) => {
                if let Some(info) = &mut trace.blf {
                    info.skipped = std::mem::take(&mut stream.skipped);
                }
                record_problem(trace, error, stream.position)?;
                return Ok(());
            }
        }
    }
    if let Some(info) = &mut trace.blf {
        info.skipped = std::mem::take(&mut stream.skipped);
    }
    check(watch, trace.end).map_err(index_error)?;
    Ok(())
}

fn take_object(trace: &mut Trace, object: super::stream::Object) {
    trace.lines += 1;
    let Some((time, frame)) = object.frame else {
        return;
    };
    if trace.rows().is_multiple_of(super::super::index::BLOCK) {
        trace.binary_blocks.push(Checkpoint {
            row: trace.rows(),
            object: trace.lines,
            position: object.position,
        });
    }
    trace.binary_row(time, &frame);
}

fn finish_problem(
    mut trace: Trace,
    error: Error,
    position: super::BlfPosition,
) -> Result<Trace, IndexError> {
    record_problem(&mut trace, error, position)?;
    Ok(trace)
}

fn record_problem(
    trace: &mut Trace,
    error: Error,
    position: super::BlfPosition,
) -> Result<(), IndexError> {
    let Error::Invalid(reason) = error else {
        return Err(index_error(error));
    };
    trace.malformed += 1;
    trace.problems.push(TraceProblem {
        line: trace.lines + 1,
        reason,
    });
    if let Some(info) = &mut trace.blf {
        info.problem_position = Some(position);
    }
    Ok(())
}

fn index_error(error: Error) -> IndexError {
    match error {
        Error::Cancelled => IndexError::Cancelled,
        other => IndexError::Read(other.to_string()),
    }
}
