use std::io;

pub(super) fn require_private(
    keeps_acls: io::Result<bool>,
    protect: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    if keeps_acls? {
        protect()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acl_query_failure_refuses_the_vault() {
        let result = require_private(
            Err(io::Error::from(io::ErrorKind::PermissionDenied)),
            || panic!("a failed ACL query must stop before protection"),
        );
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn acl_protection_failure_refuses_the_vault() {
        let result = require_private(Ok(true), || {
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        });
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn acl_volume_requires_successful_protection() {
        let mut protected = false;
        require_private(Ok(true), || {
            protected = true;
            Ok(())
        })
        .unwrap();
        assert!(protected);
    }

    #[test]
    fn volume_without_acls_skips_protection() {
        require_private(Ok(false), || panic!("this volume cannot store ACLs")).unwrap();
    }
}
