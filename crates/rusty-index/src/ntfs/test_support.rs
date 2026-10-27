#![allow(
    clippy::unwrap_used,
    reason = "test-support fixtures fail immediately on invalid data or broken coordination"
)]

use std::collections::VecDeque;

use super::volume_io::{VolumeIo, enumerate_volume};
use super::{Enumeration, JournalPosition, NtfsError};
use crate::pipe::CancelToken;

pub struct MftFixture {
    buffers: VecDeque<Vec<u8>>,
    journal_id: u64,
}

impl MftFixture {
    pub fn new(entries: u32, journal_id: u64) -> Self {
        let mut buffers = VecDeque::new();
        for start in (1..entries).step_by(512) {
            let end = (start + 512).min(entries);
            let mut bytes = u64::from(end).to_le_bytes().to_vec();
            for number in start..end {
                append_record(&mut bytes, number, entries);
            }
            assert!(bytes.len() <= 64 * 1024);
            buffers.push_back(bytes);
        }
        Self {
            buffers,
            journal_id,
        }
    }

    pub fn read(mut self) -> Enumeration {
        enumerate_volume(&mut self, 'C', usize::MAX, &CancelToken::new()).unwrap()
    }
}

impl VolumeIo for MftFixture {
    fn query_journal(&mut self) -> Result<Vec<u8>, NtfsError> {
        let mut bytes = vec![0; 56];
        bytes[..8].copy_from_slice(&self.journal_id.to_le_bytes());
        bytes[16..24].copy_from_slice(&100i64.to_le_bytes());
        Ok(bytes)
    }

    fn enumerate(&mut self, _: u64) -> Result<Option<Vec<u8>>, NtfsError> {
        Ok(self.buffers.pop_front())
    }

    fn read_journal(&mut self, _: JournalPosition) -> Result<Vec<u8>, NtfsError> {
        Ok(100i64.to_le_bytes().to_vec())
    }

    fn file_size(&mut self, _: u128) -> Result<u64, NtfsError> {
        Ok(1)
    }
}

fn append_record(bytes: &mut Vec<u8>, number: u32, entries: u32) {
    let name = if number == entries - 1 {
        "needle.txt".into()
    } else {
        format!("entry-{number:07}.txt")
    };
    let name: Vec<_> = name.encode_utf16().collect();
    let length = (60 + name.len() * 2).next_multiple_of(8);
    let mut record = vec![0; length];
    record[..4].copy_from_slice(&(length as u32).to_le_bytes());
    record[4..6].copy_from_slice(&2u16.to_le_bytes());
    record[8..16].copy_from_slice(&(u64::from(number) + 100).to_le_bytes());
    record[16..24].copy_from_slice(&5u64.to_le_bytes());
    record[24..32].copy_from_slice(&90i64.to_le_bytes());
    record[56..58].copy_from_slice(&((name.len() * 2) as u16).to_le_bytes());
    record[58..60].copy_from_slice(&60u16.to_le_bytes());
    let (pairs, _) = record[60..].as_chunks_mut::<2>();
    for (pair, unit) in pairs.iter_mut().zip(name) {
        pair.copy_from_slice(&unit.to_le_bytes());
    }
    bytes.extend(record);
}
