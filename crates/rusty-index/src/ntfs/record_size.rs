use super::{NtfsError, field};

pub(super) fn record_size(output: &mut [u8], reference: u64) -> Result<Option<u64>, NtfsError> {
    let found = u64::from_le_bytes(field(output, 0)?);
    let length = u32::from_le_bytes(field(output, 8)?) as usize;
    let bytes = output
        .get_mut(12..)
        .and_then(|bytes| bytes.get_mut(..length))
        .ok_or(NtfsError::InvalidResponse("truncated file record"))?;
    if found & 0x0000_ffff_ffff_ffff != reference & 0x0000_ffff_ffff_ffff {
        return Err(NtfsError::FileGone);
    }
    if bytes.get(..4) != Some(b"FILE") {
        return Err(NtfsError::InvalidResponse("invalid file record signature"));
    }
    let sequence = u16::from_le_bytes(field(bytes, 16)?);
    if u64::from(sequence) != reference >> 48 {
        return Err(NtfsError::FileGone);
    }
    fixup(bytes)?;
    let used = u32::from_le_bytes(field(bytes, 24)?) as usize;
    let bytes = bytes
        .get(..used)
        .ok_or(NtfsError::InvalidResponse("invalid record length"))?;
    let mut at = usize::from(u16::from_le_bytes(field(bytes, 20)?));
    if at < 42 || !at.is_multiple_of(8) {
        return Err(NtfsError::InvalidResponse("invalid attribute offset"));
    }
    let mut attribute_list = false;
    loop {
        let kind = u32::from_le_bytes(field(bytes, at)?);
        if kind == u32::MAX {
            return Ok(if attribute_list { None } else { Some(0) });
        }
        let length = u32::from_le_bytes(field(bytes, at + 4)?) as usize;
        if length < 24 || !length.is_multiple_of(8) {
            return Err(NtfsError::InvalidResponse("invalid attribute length"));
        }
        let attribute = bytes
            .get(at..)
            .and_then(|tail| tail.get(..length))
            .ok_or(NtfsError::InvalidResponse("truncated attribute"))?;
        attribute_list |= kind == 0x20;
        if kind == 0x80 && attribute[9] == 0 {
            return data_size(attribute);
        }
        at += length;
    }
}

fn data_size(attribute: &[u8]) -> Result<Option<u64>, NtfsError> {
    match attribute[8] {
        0 => {
            let size = u32::from_le_bytes(field(attribute, 16)?);
            let offset = usize::from(u16::from_le_bytes(field(attribute, 20)?));
            if offset < 24 || offset + size as usize > attribute.len() {
                return Err(NtfsError::InvalidResponse("invalid resident data"));
            }
            Ok(Some(u64::from(size)))
        }
        1 if u64::from_le_bytes(field(attribute, 16)?) == 0 => {
            let size = i64::from_le_bytes(field(attribute, 48)?);
            u64::try_from(size)
                .map(Some)
                .map_err(|_| NtfsError::InvalidResponse("negative data size"))
        }
        1 => Ok(None),
        _ => Err(NtfsError::InvalidResponse("invalid attribute form")),
    }
}

fn fixup(bytes: &mut [u8]) -> Result<(), NtfsError> {
    let at = usize::from(u16::from_le_bytes(field(bytes, 4)?));
    let count = usize::from(u16::from_le_bytes(field(bytes, 6)?));
    if count < 2 || !bytes.len().is_multiple_of(count - 1) {
        return Err(NtfsError::InvalidResponse("invalid record fixup"));
    }
    let sector = bytes.len() / (count - 1);
    if sector < 2 {
        return Err(NtfsError::InvalidResponse("invalid sector size"));
    }
    let marker: [u8; 2] = field(bytes, at)?;
    for index in 1..count {
        let replacement: [u8; 2] = field(bytes, at + index * 2)?;
        let end = index * sector;
        let trailer = &mut bytes[end - 2..end];
        if trailer == marker {
            trailer.copy_from_slice(&replacement);
        } else if trailer != replacement {
            return Err(NtfsError::InvalidResponse("torn file record"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn record(attribute: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0; 12 + 1024];
        bytes[..8].copy_from_slice(&42u64.to_le_bytes());
        bytes[8..12].copy_from_slice(&1024u32.to_le_bytes());
        let record = &mut bytes[12..];
        record[..4].copy_from_slice(b"FILE");
        record[4..6].copy_from_slice(&48u16.to_le_bytes());
        record[6..8].copy_from_slice(&3u16.to_le_bytes());
        record[16..18].copy_from_slice(&7u16.to_le_bytes());
        record[20..22].copy_from_slice(&56u16.to_le_bytes());
        record[24..28].copy_from_slice(&(60u32 + attribute.len() as u32).to_le_bytes());
        record[56..56 + attribute.len()].copy_from_slice(attribute);
        record[56 + attribute.len()..60 + attribute.len()].fill(0xff);
        bytes
    }

    #[test]
    fn reads_resident_and_nonresident_unnamed_data_and_checks_identity() {
        let mut resident = vec![0; 32];
        resident[..4].copy_from_slice(&0x80u32.to_le_bytes());
        resident[4..8].copy_from_slice(&32u32.to_le_bytes());
        resident[16..20].copy_from_slice(&5u32.to_le_bytes());
        resident[20..22].copy_from_slice(&24u16.to_le_bytes());
        let reference = (7 << 48) | 42;
        let mut output = record(&resident);
        assert_eq!(record_size(&mut output, reference).unwrap(), Some(5));
        assert!(matches!(
            record_size(&mut output, reference + 1),
            Err(NtfsError::FileGone)
        ));
        assert!(matches!(
            record_size(&mut output, (8 << 48) | 42),
            Err(NtfsError::FileGone)
        ));
        let mut nonresident = vec![0; 64];
        nonresident[..8].copy_from_slice(&resident[..8]);
        nonresident[4..8].copy_from_slice(&64u32.to_le_bytes());
        nonresident[8] = 1;
        nonresident[48..56].copy_from_slice(&(1u64 << 40).to_le_bytes());
        assert_eq!(
            record_size(&mut record(&nonresident), reference).unwrap(),
            Some(1u64 << 40)
        );
        nonresident[16..24].copy_from_slice(&1u64.to_le_bytes());
        assert_eq!(
            record_size(&mut record(&nonresident), reference).unwrap(),
            None
        );
    }

    #[test]
    fn attribute_lists_request_fallback_and_malformed_records_fail() {
        let mut list = vec![0; 24];
        list[..4].copy_from_slice(&0x20u32.to_le_bytes());
        list[4..8].copy_from_slice(&24u32.to_le_bytes());
        let reference = (7 << 48) | 42;
        let mut output = record(&list);
        assert_eq!(record_size(&mut output, reference).unwrap(), None);
        for length in 0..output.len() {
            assert!(record_size(&mut output[..length], reference).is_err());
        }
        output[12 + 510] = 1;
        assert!(record_size(&mut output, reference).is_err());
    }
}

#[cfg(test)]
mod buffer_tests {
    use super::*;

    #[test]
    fn restores_sector_trailers_in_the_callers_buffer() {
        let mut output = tests::record(&[]);
        output[12 + 48..12 + 50].copy_from_slice(&[0xaa, 0xbb]);
        output[12 + 50..12 + 52].copy_from_slice(&[0x12, 0x34]);
        output[12 + 52..12 + 54].copy_from_slice(&[0x56, 0x78]);
        output[12 + 510..12 + 512].copy_from_slice(&[0xaa, 0xbb]);
        output[12 + 1022..12 + 1024].copy_from_slice(&[0xaa, 0xbb]);
        assert_eq!(record_size(&mut output, (7 << 48) | 42).unwrap(), Some(0));
        assert_eq!(&output[12 + 510..12 + 512], &[0x12, 0x34]);
        assert_eq!(&output[12 + 1022..12 + 1024], &[0x56, 0x78]);
    }

    #[test]
    #[ignore = "benchmark: parse two million synthetic MFT records"]
    fn parses_two_million_synthetic_records() {
        use std::io::Write as _;
        let mut attribute = [0; 64];
        attribute[..4].copy_from_slice(&0x80u32.to_le_bytes());
        attribute[4..8].copy_from_slice(&64u32.to_le_bytes());
        attribute[8] = 1;
        attribute[48..56].copy_from_slice(&12345u64.to_le_bytes());
        let template = tests::record(&attribute);
        let mut output = template.clone();
        let started = std::time::Instant::now();
        for _ in 0..2_000_000 {
            output.copy_from_slice(&template);
            assert_eq!(
                record_size(std::hint::black_box(&mut output), (7 << 48) | 42).unwrap(),
                Some(12345)
            );
        }
        writeln!(
            std::io::stdout(),
            "2,000,000 MFT records parsed in {:?}",
            started.elapsed()
        )
        .unwrap();
    }
}
