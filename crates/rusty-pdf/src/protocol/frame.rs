use std::io::{self, Read, Write};

use super::{
    CodecError, MAX_REQUEST_FRAME, MAX_RESPONSE_FRAME, Request, Response, decode_request,
    decode_response, encode_request, encode_response,
};

pub fn write_frame(output: &mut impl Write, payload: &[u8], max: usize) -> Result<(), CodecError> {
    if payload.len() > max {
        return Err(CodecError::TooLarge {
            what: "frame",
            len: payload.len() as u64,
            max: max as u64,
        });
    }
    output.write_all(&(payload.len() as u32).to_le_bytes())?;
    output.write_all(payload)?;
    output.flush()?;
    Ok(())
}

pub fn read_frame(input: &mut impl Read, max: usize) -> Result<Option<Vec<u8>>, CodecError> {
    let mut header = [0; 4];
    let filled = fill(input, &mut header)?;
    if filled == 0 {
        return Ok(None);
    }
    if filled < header.len() {
        return Err(CodecError::Truncated);
    }
    let len = u32::from_le_bytes(header) as usize;
    if len > max {
        return Err(CodecError::TooLarge {
            what: "frame",
            len: len as u64,
            max: max as u64,
        });
    }
    let mut payload = Vec::new();
    input.take(len as u64).read_to_end(&mut payload)?;
    if payload.len() < len {
        return Err(CodecError::Truncated);
    }
    Ok(Some(payload))
}

pub fn write_request(output: &mut impl Write, request: &Request) -> Result<(), CodecError> {
    write_frame(output, &encode_request(request)?, MAX_REQUEST_FRAME)
}

pub fn read_request(input: &mut impl Read) -> Result<Option<Request>, CodecError> {
    read_frame(input, MAX_REQUEST_FRAME)?
        .map(|payload| decode_request(&payload))
        .transpose()
}

pub fn write_response(output: &mut impl Write, response: &Response) -> Result<(), CodecError> {
    write_frame(output, &encode_response(response)?, MAX_RESPONSE_FRAME)
}

pub fn read_response(input: &mut impl Read) -> Result<Option<Response>, CodecError> {
    read_frame(input, MAX_RESPONSE_FRAME)?
        .map(|payload| decode_response(&payload))
        .transpose()
}

fn fill(input: &mut impl Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match input.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(read) => filled += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(filled)
}
