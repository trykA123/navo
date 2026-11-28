//! The per-session 128-bit credential that authenticates OSC 133;A readiness marks.
//! Drawn from the OS CSPRNG, encoded as lowercase hex and compared in constant time.

use std::fmt;
use std::io;

use crate::TermError;

/// Random bytes in one session nonce (128 bits -> 32 lowercase hex characters).
const NONCE_BYTES: usize = 16;

/// 128-bit random hex nonce issued per session to authenticate OSC 133;A readiness marks.
#[derive(Clone, PartialEq, Eq)]
pub struct Nonce(String);

impl Nonce {
    /// Draws 128 random bits from the OS CSPRNG and encodes them as lowercase hex.
    pub fn generate() -> Result<Self, TermError> {
        let mut bytes = [0u8; NONCE_BYTES];
        getrandom::fill(&mut bytes).map_err(|error| TermError::Io(io::Error::other(error)))?;
        Ok(Self(encode_hex(&bytes)))
    }

    /// Constructs a nonce from a hex string for deterministic tests.
    #[cfg(any(test, feature = "test-support"))]
    pub fn from_hex(hex: impl Into<String>) -> Self {
        Self(hex.into())
    }

    /// The lowercase hex encoding of the nonce.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Constant-time comparison against an untrusted candidate from PTY output.
    pub(super) fn matches(&self, candidate: &str) -> bool {
        let expected = self.0.as_bytes();
        let actual = candidate.as_bytes();
        if expected.is_empty() || expected.len() != actual.len() {
            return false;
        }
        let mut diff = 0u8;
        for (&left, &right) in expected.iter().zip(actual.iter()) {
            diff |= left ^ right;
        }
        diff == 0
    }
}

impl fmt::Debug for Nonce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Nonce([redacted])")
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len().saturating_mul(2));
    for &byte in bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}
