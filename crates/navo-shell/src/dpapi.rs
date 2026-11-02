//! Seals bytes to the current Windows user with DPAPI.

use std::fmt;

/// Why sealing or unsealing failed; never carries the data or OS message text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SealError {
    #[error("sealing needs Windows")]
    Unsupported,
    #[error("there is nothing to seal")]
    Empty,
    #[error("the data is too large to seal")]
    TooLarge,
    #[error("the system refused to seal or unseal (code {code})")]
    Os { code: u32 },
}

/// Unsealed bytes; the buffer is overwritten with zeros when dropped.
pub struct SecretBytes(Box<[u8]>);

impl SecretBytes {
    #[cfg_attr(
        not(any(windows, test)),
        expect(dead_code, reason = "only the Windows unseal builds secrets")
    )]
    fn copy_of(bytes: &[u8]) -> Self {
        Self(Box::from(bytes))
    }

    pub fn expose(&self) -> &[u8] {
        &self.0
    }

    fn wipe(&mut self) {
        wipe(&mut self.0);
    }
}

impl Drop for SecretBytes {
    fn drop(&mut self) {
        self.wipe();
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecretBytes")
            .field("len", &self.0.len())
            .finish()
    }
}

fn wipe(bytes: &mut [u8]) {
    for byte in bytes.iter_mut() {
        // SAFETY: `byte` is a valid, aligned, exclusive reference.
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

/// Encrypts `data` so only the current Windows user can unseal it.
pub fn seal(data: &[u8]) -> Result<Vec<u8>, SealError> {
    if data.is_empty() {
        return Err(SealError::Empty);
    }
    os::seal(data)
}

/// Decrypts bytes made by [`seal`] for the current user.
pub fn unseal(sealed: &[u8]) -> Result<SecretBytes, SealError> {
    if sealed.is_empty() {
        return Err(SealError::Empty);
    }
    os::unseal(sealed)
}

#[cfg(windows)]
mod os {
    use std::ptr;

    use windows_sys::Win32::Foundation::{GetLastError, LocalFree};
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    };

    use super::{SealError, SecretBytes, wipe};

    // Changing this makes every sealed secret unreadable.
    const ENTROPY: &[u8] = b"Navo DPAPI seal v1";

    struct LocalBlob(CRYPT_INTEGER_BLOB);

    impl LocalBlob {
        fn new() -> Self {
            Self(CRYPT_INTEGER_BLOB {
                cbData: 0,
                pbData: ptr::null_mut(),
            })
        }

        fn bytes(&self) -> &[u8] {
            let Ok(len) = usize::try_from(self.0.cbData) else {
                return &[];
            };
            if self.0.pbData.is_null() || len == 0 {
                return &[];
            }
            // SAFETY: DPAPI returned `len` initialised bytes at `pbData`, owned by this guard.
            unsafe { std::slice::from_raw_parts(self.0.pbData, len) }
        }
    }

    impl Drop for LocalBlob {
        fn drop(&mut self) {
            if self.0.pbData.is_null() {
                return;
            }
            if let Ok(len) = usize::try_from(self.0.cbData) {
                // SAFETY: the buffer is DPAPI's LocalAlloc output, exclusively owned here.
                let bytes = unsafe { std::slice::from_raw_parts_mut(self.0.pbData, len) };
                wipe(bytes);
            }
            // SAFETY: DPAPI allocated `pbData` with LocalAlloc, and it is freed once.
            unsafe { LocalFree(self.0.pbData.cast()) };
        }
    }

    fn input(data: &[u8]) -> Result<CRYPT_INTEGER_BLOB, SealError> {
        Ok(CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(data.len()).map_err(|_| SealError::TooLarge)?,
            pbData: data.as_ptr().cast_mut(),
        })
    }

    fn last_error() -> SealError {
        // SAFETY: reads this thread's last-error value.
        let code = unsafe { GetLastError() };
        SealError::Os { code }
    }

    pub(super) fn seal(data: &[u8]) -> Result<Vec<u8>, SealError> {
        let data_in = input(data)?;
        let entropy = input(ENTROPY)?;
        let mut out = LocalBlob::new();
        // SAFETY: both input blobs borrow live slices DPAPI only reads; `out` receives a LocalAlloc buffer.
        let ok = unsafe {
            CryptProtectData(
                &data_in,
                ptr::null(),
                &entropy,
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out.0,
            )
        };
        if ok == 0 {
            return Err(last_error());
        }
        Ok(out.bytes().to_vec())
    }

    pub(super) fn unseal(sealed: &[u8]) -> Result<SecretBytes, SealError> {
        let data_in = input(sealed)?;
        let entropy = input(ENTROPY)?;
        let mut out = LocalBlob::new();
        // SAFETY: as in `seal`; a null description pointer asks for no description.
        let ok = unsafe {
            CryptUnprotectData(
                &data_in,
                ptr::null_mut(),
                &entropy,
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out.0,
            )
        };
        if ok == 0 {
            return Err(last_error());
        }
        Ok(SecretBytes::copy_of(out.bytes()))
    }
}

#[cfg(not(windows))]
mod os {
    use super::{SealError, SecretBytes};

    pub(super) fn seal(_data: &[u8]) -> Result<Vec<u8>, SealError> {
        Err(SealError::Unsupported)
    }

    pub(super) fn unseal(_sealed: &[u8]) -> Result<SecretBytes, SealError> {
        Err(SealError::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpapi_wipe_zeroes_every_byte() {
        let mut bytes = *b"hunter2-secret";
        wipe(&mut bytes);
        assert_eq!(bytes, [0; 14]);
    }

    #[test]
    fn dpapi_secret_wipe_used_by_drop_clears_the_buffer() {
        let mut secret = SecretBytes::copy_of(b"hunter2-secret");
        secret.wipe();
        assert!(secret.expose().iter().all(|&byte| byte == 0));
        assert_eq!(secret.expose().len(), 14);
    }

    #[test]
    fn dpapi_debug_shows_only_the_length() {
        let secret = SecretBytes::copy_of(b"hunter2-secret");
        let shown = format!("{secret:?}");
        assert!(!shown.contains("hunter2"), "debug must not leak content");
        assert!(shown.contains("14"));
    }

    #[test]
    fn dpapi_empty_input_is_refused() {
        assert_eq!(seal(b"").err(), Some(SealError::Empty));
        assert_eq!(unseal(b"").err(), Some(SealError::Empty));
    }

    #[cfg(not(windows))]
    #[test]
    fn dpapi_stub_reports_unsupported() {
        assert_eq!(seal(b"secret").err(), Some(SealError::Unsupported));
        assert_eq!(unseal(b"sealed").err(), Some(SealError::Unsupported));
    }
}
