use super::TooLarge;

pub fn check_size(bytes: u64, limit: u64) -> Result<u64, TooLarge> {
    if bytes > limit {
        Err(TooLarge { bytes, limit })
    } else {
        Ok(bytes)
    }
}
