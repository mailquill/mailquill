//! zstd compression for message bodies and drafts before they are written to
//! the database or blob store.

use std::io::{Read, Write};

/// Compress `data` with zstd at level 3. The encoder writes to memory, so the
/// internal `expect`s cannot fail in practice.
pub fn compress_body(data: &[u8]) -> Vec<u8> {
    let mut encoder = zstd::Encoder::new(Vec::new(), 3).expect("zstd encoder");
    encoder.write_all(data).expect("zstd write");
    encoder.finish().expect("zstd finish")
}

/// Decompress a buffer produced by [`compress_body`]. Returns an I/O error when
/// `data` is not a valid zstd stream.
pub fn decompress_body(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut decoder = zstd::Decoder::new(data)?;
    let mut out = Vec::new();
    decoder.read_to_end(&mut out)?;
    Ok(out)
}
