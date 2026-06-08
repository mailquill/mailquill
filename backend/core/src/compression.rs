use std::io::{Read, Write};

pub fn compress_body(data: &[u8]) -> Vec<u8> {
    let mut encoder = zstd::Encoder::new(Vec::new(), 3).expect("zstd encoder");
    encoder.write_all(data).expect("zstd write");
    encoder.finish().expect("zstd finish")
}

pub fn decompress_body(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut decoder = zstd::Decoder::new(data)?;
    let mut out = Vec::new();
    decoder.read_to_end(&mut out)?;
    Ok(out)
}
