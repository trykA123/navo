use std::io::{Read, Seek};

use sevenz_rust2::{
    Archive, ArchiveEntry as SevenEntry, BlockDecoder, EncoderMethod, Error, Password,
};

use super::{ArchiveEntry, ENCRYPTED, Entry, Stamp, damaged, missing, read_capped};

const UNIX_EPOCH_TICKS: u64 = 116_444_736_000_000_000;
const TICKS_PER_MS: u64 = 10_000;

fn failed(error: Error) -> String {
    match error {
        Error::PasswordRequired | Error::MaybeBadPassword(_) => ENCRYPTED.to_owned(),
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
        is_folder: entry.is_directory(),
        is_encrypted: is_encrypted(archive, index),
    }
}

pub(super) fn list<R: Read + Seek>(mut reader: R) -> Result<Vec<ArchiveEntry>, String> {
    let archive = Archive::read(&mut reader, &Password::empty()).map_err(failed)?;
    Ok(archive
        .files
        .iter()
        .enumerate()
        .map(|(index, entry)| describe(&archive, index, entry))
        .collect())
}

pub(super) fn read<R: Read + Seek>(mut reader: R, at: Entry) -> Result<Vec<u8>, String> {
    let archive = Archive::read(&mut reader, &Password::empty()).map_err(failed)?;
    let entry = archive.files.get(at.index).ok_or_else(missing)?;
    at.check(&describe(&archive, at.index, entry))?;
    let block = archive.stream_map.file_block_index.get(at.index);
    let Some(block) = block.copied().flatten() else {
        return Ok(Vec::new());
    };
    let password = Password::empty();
    let decoder = BlockDecoder::new(1, block, &archive, &password, &mut reader);
    let mut found = None;
    decoder
        .for_each_entries(&mut |file, data| {
            if !std::ptr::eq(file, entry) {
                std::io::copy(data, &mut std::io::sink())?;
                return Ok(true);
            }
            found = Some(read_capped(data, at.limit));
            Ok(false)
        })
        .map_err(failed)?;
    found.ok_or_else(missing)?
}
