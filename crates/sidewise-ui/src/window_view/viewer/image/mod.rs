//! Picture decoding: raster and SVG decode, EXIF entries and the shared picture type.
pub mod decode;
pub mod exif;
pub mod svg;

pub use decode::{IMAGE_LIMIT, Picture, decode_image};
pub use exif::ExifEntry;
pub use svg::unzipped;
