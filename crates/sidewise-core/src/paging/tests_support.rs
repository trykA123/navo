use super::ByteSource;

pub(super) struct Short {
    bytes: Vec<u8>,
    max: usize,
}

impl Short {
    pub(super) fn new(bytes: &[u8], max: usize) -> Self {
        Self {
            bytes: bytes.to_vec(),
            max,
        }
    }
}

impl ByteSource for Short {
    fn len(&self) -> u64 {
        self.bytes.as_slice().len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        let take = buf.len().min(self.max);
        self.bytes.as_slice().read_at(offset, &mut buf[..take])
    }
}
