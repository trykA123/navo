use std::io::{Read, Seek};

use sevenz_rust2::{
    Archive, ArchiveEntry as SevenEntry, BlockDecoder, EncoderMethod, Error, Password,
};

use super::source::{Fail, Meter, Metered};
use super::stream::{StreamRequest, drain, pump};
use super::{
    ArchiveEntry, ArchiveKind, ENCRYPTED, EntryAt, Limits, Listed, Stamp, UNPACK_BUDGET,
    UNSUPPORTED_METHOD, damaged, missing, read_capped,
};

const UNIX_EPOCH_TICKS: u64 = 116_444_736_000_000_000;
const TICKS_PER_MS: u64 = 10_000;
const LZMA2_LARGEST: u8 = 40;
pub(super) const TOO_MUCH_MEMORY: &str = "Needs over 2 GB of memory to unpack: not shown.";

pub(super) fn dictionary_size(method: &[u8], properties: &[u8]) -> u64 {
    if method == EncoderMethod::ID_LZMA {
        properties.get(1..5).map_or(0, |bytes| {
            u64::from(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        })
    } else if method == EncoderMethod::ID_LZMA2 {
        match properties.first().copied() {
            Some(LZMA2_LARGEST) => u64::from(u32::MAX),
            Some(bits) if bits < LZMA2_LARGEST => (2 | u64::from(bits & 1)) << (bits / 2 + 11),
            Some(_) => u64::MAX,
            None => 0,
        }
    } else {
        0
    }
}

fn needs_too_much_memory(archive: &Archive, block: usize) -> bool {
    archive.blocks.get(block).is_some_and(|block| {
        block.coders.iter().any(|coder| {
            dictionary_size(coder.encoder_method_id(), coder.properties()) > UNPACK_BUDGET
        })
    })
}

pub(super) fn failed(error: Error) -> String {
    match error {
        Error::PasswordRequired | Error::MaybeBadPassword(_) => ENCRYPTED.to_owned(),
        Error::UnsupportedCompressionMethod(_) => UNSUPPORTED_METHOD.to_owned(),
        other => damaged(other),
    }
}

fn stamp(entry: &SevenEntry) -> Option<Stamp> {
    if !entry.has_last_modified_date {
        return None;
    }
    let ticks = u64::from(entry.last_modified_date()).checked_sub(UNIX_EPOCH_TICKS)?;
    i64::try_from(ticks / TICKS_PER_MS).ok().map(Stamp::UnixMs)
}

fn is_encrypted(archive: &Archive, index: usize) -> bool {
    archive
        .stream_map
        .file_block_index
        .get(index)
        .copied()
        .flatten()
        .and_then(|block| archive.blocks.get(block))
        .is_some_and(|block| {
            block
                .coders
                .iter()
                .any(|coder| coder.encoder_method_id() == EncoderMethod::ID_AES256_SHA256)
        })
}

fn describe(archive: &Archive, index: usize, entry: &SevenEntry) -> ArchiveEntry {
    ArchiveEntry {
        name: entry.name().to_owned(),
        size: entry.size(),
        packed: (entry.compressed_size > 0).then_some(entry.compressed_size),
        modified: stamp(entry),
        kind: if entry.is_directory() {
            ArchiveKind::Folder
        } else {
            ArchiveKind::File
        },
        is_encrypted: is_encrypted(archive, index),
        header_offset: None,
    }
}

pub(super) fn list<R: Read + Seek>(
    mut reader: R,
    limits: Limits<'_>,
) -> Result<Listed<'_>, String> {
    let archive = Archive::read(&mut reader, &Password::empty()).map_err(failed)?;
    let mut listed = Listed::new(limits);
    for (index, entry) in archive.files.iter().enumerate() {
        if !listed.push(describe(&archive, index, entry)) {
            break;
        }
    }
    Ok(listed)
}

pub(super) fn read<R: Read + Seek>(
    mut reader: R,
    at: EntryAt,
    limits: Limits<'_>,
) -> Result<Vec<u8>, String> {
    let archive = Archive::read(&mut reader, &Password::empty()).map_err(failed)?;
    let entry = archive.files.get(at.index).ok_or_else(missing)?;
    at.check(&describe(&archive, at.index, entry))?;
    let block = archive.stream_map.file_block_index.get(at.index);
    let Some(block) = block.copied().flatten() else {
        return Ok(Vec::new());
    };
    if needs_too_much_memory(&archive, block) {
        return Err(TOO_MUCH_MEMORY.to_owned());
    }
    let password = Password::empty();
    let decoder = BlockDecoder::new(1, block, &archive, &password, &mut reader);
    let mut meter = Meter::new(limits.budget, limits.cancel);
    let mut found = None;
    let decoded = decoder.for_each_entries(&mut |file, data| {
        if !std::ptr::eq(file, entry) {
            let mut skipped = Metered {
                inner: data,
                meter: &mut meter,
            };
            std::io::copy(&mut skipped, &mut std::io::sink())?;
            return Ok(true);
        }
        found = Some(read_capped(data, at.limit));
        Ok(false)
    });
    if let Err(error) = decoded {
        return Err(match meter.stopped() {
            Some(stop) => Fail::Stop(stop).into_read_error(),
            None => failed(error),
        });
    }
    found.ok_or_else(missing)?
}

pub(super) fn stream<R: Read + Seek>(
    mut reader: R,
    request: StreamRequest<'_>,
) -> Result<(), String> {
    let archive = Archive::read(&mut reader, &Password::empty()).map_err(failed)?;
    let entry = archive.files.get(request.at.index).ok_or_else(missing)?;
    request
        .at
        .check(&describe(&archive, request.at.index, entry))?;
    let block = archive
        .stream_map
        .file_block_index
        .get(request.at.index)
        .copied()
        .flatten();
    let Some(block) = block else {
        return Ok(());
    };
    if needs_too_much_memory(&archive, block) {
        return Err(TOO_MUCH_MEMORY.to_owned());
    }
    decode_block(&archive, block, &mut reader, request)
}

fn decode_block<R: Read + Seek>(
    archive: &Archive,
    block: usize,
    reader: &mut R,
    request: StreamRequest<'_>,
) -> Result<(), String> {
    let entry = archive.files.get(request.at.index).ok_or_else(missing)?;
    let password = Password::empty();
    let decoder = BlockDecoder::new(1, block, archive, &password, reader);
    let mut failure = None;
    let mut found = false;
    let decoded = decoder.for_each_entries(&mut |file, data| {
        if !std::ptr::eq(file, entry) {
            if let Err(message) = drain(data, request.cancel, request.gone) {
                failure = Some(message);
                return Err(Error::Other("archive stream stopped".into()));
            }
            return Ok(true);
        }
        found = true;
        match pump(data, request.sender, request.cancel, request.gone) {
            Ok(_) => Ok(false),
            Err(message) => {
                failure = Some(message);
                Err(Error::Other("archive stream stopped".into()))
            }
        }
    });
    if let Some(message) = failure {
        return Err(message);
    }
    match decoded {
        Ok(_) if found => Ok(()),
        Ok(_) => Err(missing()),
        Err(error) => Err(failed(error)),
    }
}
