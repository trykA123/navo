/// Registry hive a value is read from. Other platforms have no registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hive {
    CurrentUser,
    LocalMachine,
}

/// Reads a string value from the 64-bit registry view. Returns `None` when
/// the key or value is missing, is not a string, or on a platform without a registry.
#[cfg(windows)]
pub fn text(hive: Hive, key: &str, name: &str) -> Option<String> {
    imp::text(hive, key, name)
}

#[cfg(not(windows))]
pub fn text(_hive: Hive, _key: &str, _name: &str) -> Option<String> {
    None
}

#[cfg(any(windows, test))]
const START_UNITS: usize = 128;
#[cfg(any(windows, test))]
const MAX_UNITS: usize = 32_767;

#[cfg(any(windows, test))]
enum Fetch {
    Done { bytes: usize },
    More { bytes: usize },
    Missing,
}

#[cfg(any(windows, test))]
fn read_growing(mut fetch: impl FnMut(&mut [u16]) -> Fetch) -> Option<String> {
    let mut units = START_UNITS;
    loop {
        let mut buffer = vec![0_u16; units];
        match fetch(&mut buffer) {
            Fetch::Missing => return None,
            Fetch::Done { bytes } => {
                let used = (bytes / 2).min(units);
                let text = String::from_utf16_lossy(&buffer[..used]);
                return Some(text.trim_end_matches('\0').to_owned());
            }
            Fetch::More { bytes } => {
                let needed = bytes.div_ceil(2).max(units + 1);
                if units >= MAX_UNITS {
                    return None;
                }
                units = needed.min(MAX_UNITS);
            }
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RRF_SUBKEY_WOW6464KEY,
        RegGetValueW,
    };

    use super::{Fetch, Hive, read_growing};

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    fn root(hive: Hive) -> HKEY {
        match hive {
            Hive::CurrentUser => HKEY_CURRENT_USER,
            Hive::LocalMachine => HKEY_LOCAL_MACHINE,
        }
    }

    pub(super) fn text(hive: Hive, key: &str, name: &str) -> Option<String> {
        let (key, name) = (wide(key), wide(name));
        read_growing(|buffer| {
            let mut size = (buffer.len() * 2) as u32;
            // SAFETY: the names are NUL-terminated, and `size` is the byte length of the writable `buffer`.
            let status = unsafe {
                RegGetValueW(
                    root(hive),
                    key.as_ptr(),
                    name.as_ptr(),
                    RRF_RT_REG_SZ | RRF_SUBKEY_WOW6464KEY,
                    null_mut(),
                    buffer.as_mut_ptr().cast(),
                    &raw mut size,
                )
            };
            match status {
                ERROR_SUCCESS => Fetch::Done {
                    bytes: size as usize,
                },
                ERROR_MORE_DATA => Fetch::More {
                    bytes: size as usize,
                },
                _ => Fetch::Missing,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    fn fake(value: Vec<u16>) -> impl FnMut(&mut [u16]) -> Fetch {
        move |buffer| {
            if buffer.len() < value.len() {
                return Fetch::More {
                    bytes: value.len() * 2,
                };
            }
            buffer[..value.len()].copy_from_slice(&value);
            Fetch::Done {
                bytes: value.len() * 2,
            }
        }
    }

    #[test]
    fn short_value_is_read_without_the_terminator() {
        assert_eq!(
            read_growing(fake(encode("wt.exe"))).as_deref(),
            Some("wt.exe")
        );
    }

    #[test]
    fn long_value_grows_the_buffer() {
        let long = "a".repeat(5_000);
        assert_eq!(read_growing(fake(encode(&long))), Some(long));
    }

    #[test]
    fn value_over_the_limit_is_refused() {
        let huge = "a".repeat(MAX_UNITS + 10);
        assert_eq!(read_growing(fake(encode(&huge))), None);
    }

    #[test]
    fn missing_value_is_none() {
        assert_eq!(read_growing(|_| Fetch::Missing), None);
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_stub_returns_none() {
        assert_eq!(text(Hive::LocalMachine, "SOFTWARE", "Name"), None);
    }
}
