use navo_clipboard::{SealError, Sealer, SecretBytes};

/// Seals secrets with the Windows data-protection API. Elsewhere every call is refused.
pub(super) struct ShellSealer;

fn fault(error: navo_shell::SealError) -> SealError {
    match error {
        navo_shell::SealError::Unsupported => SealError::Unsupported,
        _ => SealError::Failed,
    }
}

impl Sealer for ShellSealer {
    fn seal(&self, plain: &[u8]) -> Result<Vec<u8>, SealError> {
        navo_shell::seal(plain).map_err(fault)
    }

    fn unseal(&self, sealed: &[u8]) -> Result<SecretBytes, SealError> {
        let plain = navo_shell::unseal(sealed).map_err(fault)?;
        Ok(SecretBytes::new(plain.expose().to_vec()))
    }
}
