use std::io::{Read, Write};

use bytes::Bytes;
use flate2::bufread::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;

use crate::git_error::GitResult;

pub fn read(bytes: Bytes) -> GitResult<(usize, Bytes)> {
    let mut decoder = ZlibDecoder::new(bytes.as_ref());
    let mut content = Vec::new();
    decoder.read_to_end(&mut content)?;
    Ok((decoder.total_in() as usize, Bytes::from(content)))
}

pub fn write(data: &[u8]) -> GitResult<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data)?;
    Ok(encoder.finish()?)
}