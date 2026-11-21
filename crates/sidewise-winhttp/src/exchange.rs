//! A streaming HTTPS exchange: one request with an optional body, then the status, the raw
//! headers and the body read piece by piece. The gix `Http` adapter (RG8) builds on it.

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_INVALID_PARAMETER};
use windows_sys::Win32::Networking::WinHttp::{
    WINHTTP_QUERY_RAW_HEADERS_CRLF, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse,
    WinHttpSendRequest,
};

use super::{
    Handle, Target, WinHttpError, check, connect, last_error, open_request, open_session,
    status_code,
};

/// The HTTP method of an exchange.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    /// `GET`, sent without a body.
    Get,
    /// `POST`, sent with the body given to `send`.
    Post,
}

/// One open request under the same rules as `get` (https only, redirects, cookies and logon
/// credentials off, system proxy, TLS 1.2/1.3, the target's timeouts).
pub struct Exchange {
    // Fields drop in order: request, then connection, then session.
    request: Handle,
    _connection: Handle,
    _session: Handle,
    headers: Vec<u16>,
    status: u16,
}

impl Exchange {
    /// Opens `verb path` on `target` with `headers` (CRLF-joined lines, may be empty); sends nothing.
    /// Fails with `Network(win32 code)`.
    pub fn open(
        target: &Target<'_>,
        verb: Verb,
        path: &str,
        headers: &str,
    ) -> Result<Self, WinHttpError> {
        let session = open_session(target)?;
        let connection = connect(&session, target)?;
        let name = match verb {
            Verb::Get => "GET",
            Verb::Post => "POST",
        };
        let request = open_request(&connection, name, path)?;
        Ok(Self {
            request,
            _connection: connection,
            _session: session,
            headers: headers.encode_utf16().collect(),
            status: 0,
        })
    }

    /// Sends the headers and `body` (empty for none), waits for the response headers and keeps the
    /// status. Fails with `Network(win32 code)`; a second call fails in WinHTTP.
    #[allow(
        unsafe_code,
        reason = "WinHttpSendRequest and WinHttpReceiveResponse are FFI calls"
    )]
    pub fn send(&mut self, body: &[u8]) -> Result<(), WinHttpError> {
        let invalid = |_| WinHttpError::Network(ERROR_INVALID_PARAMETER);
        let header_len = u32::try_from(self.headers.len()).map_err(invalid)?;
        let body_len = u32::try_from(body.len()).map_err(invalid)?;
        // SAFETY: the request is open; the headers are null or `header_len` UTF-16 units and the body
        // is null or `body_len` bytes, both borrowed for the whole synchronous call.
        check(unsafe {
            WinHttpSendRequest(
                self.request.0,
                or_null(&self.headers),
                header_len,
                or_null(body).cast(),
                body_len,
                body_len,
                0,
            )
        })?;
        // SAFETY: the request is open and sent; the reserved pointer must be null.
        check(unsafe { WinHttpReceiveResponse(self.request.0, null_mut()) })?;
        let code = status_code(&self.request)?;
        self.status = u16::try_from(code).unwrap_or(u16::MAX);
        Ok(())
    }

    /// The HTTP status code of the response, 0 before a successful `send`.
    pub fn status(&self) -> u16 {
        self.status
    }

    /// The status line and response headers as received, CRLF-separated.
    /// Fails with `Network(win32 code)`, also before `send`.
    #[allow(unsafe_code, reason = "WinHttpQueryHeaders is an FFI call")]
    pub fn raw_headers(&self) -> Result<String, WinHttpError> {
        let mut size = 0_u32;
        // SAFETY: the request is open; a null buffer with size 0 asks only for the byte length.
        let sized = unsafe {
            WinHttpQueryHeaders(
                self.request.0,
                WINHTTP_QUERY_RAW_HEADERS_CRLF,
                null(),
                null_mut(),
                &raw mut size,
                null_mut(),
            )
        };
        match (sized, last_error()) {
            (0, WinHttpError::Network(ERROR_INSUFFICIENT_BUFFER)) => {}
            (0, error) => return Err(error),
            _ => return Ok(String::new()),
        }
        let mut text = vec![0_u16; (size as usize).div_ceil(2)];
        // SAFETY: `text` holds at least `size` bytes, borrowed mutably for the call only; `size` is a
        // live local that WinHTTP overwrites with the bytes written, without the final NUL.
        check(unsafe {
            WinHttpQueryHeaders(
                self.request.0,
                WINHTTP_QUERY_RAW_HEADERS_CRLF,
                null(),
                text.as_mut_ptr().cast(),
                &raw mut size,
                null_mut(),
            )
        })?;
        text.truncate((size as usize / 2).min(text.len()));
        Ok(String::from_utf16_lossy(&text))
    }

    /// Reads the next body bytes into `buffer` and returns their count, 0 at the end of the body.
    /// Blocks up to the receive timeout; fails with `Network(win32 code)`, also before `send`.
    #[allow(unsafe_code, reason = "WinHttpReadData is an FFI call")]
    pub fn read(&mut self, buffer: &mut [u8]) -> Result<usize, WinHttpError> {
        let want = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
        let mut read = 0_u32;
        // SAFETY: `buffer` has at least `want` writable bytes, borrowed mutably for the call only;
        // `read` is a live local.
        check(unsafe {
            WinHttpReadData(
                self.request.0,
                buffer.as_mut_ptr().cast(),
                want,
                &raw mut read,
            )
        })?;
        Ok(usize::try_from(read).map_or(0, |n| n.min(buffer.len())))
    }
}

/// Points at the first item of `items`, or is null when it is empty (WinHTTP's "none").
fn or_null<T>(items: &[T]) -> *const T {
    if items.is_empty() {
        null()
    } else {
        items.as_ptr()
    }
}
