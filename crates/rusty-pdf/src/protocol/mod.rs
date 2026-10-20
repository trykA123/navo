mod codec;
mod frame;
mod links;
mod types;

pub use codec::{decode_request, decode_response, encode_request, encode_response};
pub use frame::{
    read_frame, read_request, read_response, write_frame, write_request, write_response,
};
pub use links::{LinkTarget, PageLink, PageLinks};
pub use types::{
    Bitmap, CharBox, CodecError, PageText, PdfError, RenderRequest, Request, Response, Rotation,
    TextChar, check_render_size,
};

pub const MAX_REQUEST_FRAME: usize = 64 * 1024;
pub const MAX_PATH_BYTES: usize = 32 * 1024;
pub const MAX_PASSWORD_BYTES: usize = 1024;
pub const MAX_MESSAGE_BYTES: usize = 4096;
pub const MAX_SIDE: u32 = 16_384;
pub const MAX_PIXELS: u64 = 40_000_000;
pub const MAX_TEXT_CHARS: usize = 1_000_000;
pub const MAX_PAGE_LINKS: usize = 4096;
pub const MAX_URI_BYTES: usize = 4096;
pub const MAX_RESPONSE_FRAME: usize = MAX_PIXELS as usize * 4 + 64;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_props;
