//! Minimal PNG chunk reader for `tEXt` / `iTXt` text chunks.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{bail, Result};
use flate2::read::ZlibDecoder;

pub const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

const MAX_TEXT_CHUNK: u32 = 64 * 1024 * 1024;

pub fn looks_like_png(path: &Path) -> bool {
    let mut sig = [0u8; 8];
    File::open(path)
        .and_then(|mut f| f.read_exact(&mut sig))
        .map(|_| sig == PNG_SIGNATURE)
        .unwrap_or(false)
}

/// Return every `tEXt` / `iTXt` chunk of a PNG as `{keyword: text}`.
///
/// Other chunks (including the bulky `IDAT` data) are skipped by seeking, so
/// this stays cheap regardless of image size.
pub fn read_text_chunks(path: &Path) -> Result<BTreeMap<String, String>> {
    let mut chunks = BTreeMap::new();
    let mut f = BufReader::new(File::open(path)?);

    let mut sig = [0u8; 8];
    f.read_exact(&mut sig)?;
    if sig != PNG_SIGNATURE {
        bail!("不是 PNG 文件");
    }

    loop {
        let mut header = [0u8; 8];
        if f.read_exact(&mut header).is_err() {
            break;
        }
        let length = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        let ctype = &header[4..8];

        match ctype {
            b"tEXt" | b"iTXt" if length <= MAX_TEXT_CHUNK => {
                let mut data = vec![0u8; length as usize];
                f.read_exact(&mut data)?;
                f.seek(SeekFrom::Current(4))?; // CRC

                let (key_raw, rest) = match data.iter().position(|&b| b == 0) {
                    Some(pos) => (&data[..pos], &data[pos + 1..]),
                    None => (&data[..], &data[data.len()..]),
                };
                let key = latin1(key_raw);

                if ctype == b"tEXt" {
                    chunks.insert(key, decode_text(rest));
                } else {
                    chunks.insert(key, parse_itxt(rest));
                }
            }
            b"IEND" => break,
            _ => {
                f.seek(SeekFrom::Current(i64::from(length) + 4))?;
            }
        }
    }

    Ok(chunks)
}

/// Parse the body of an `iTXt` chunk (after the keyword + NUL).
fn parse_itxt(rest: &[u8]) -> String {
    if rest.len() < 2 {
        return String::new();
    }
    let compressed = rest[0] != 0;
    let rest = &rest[2..];

    // text = <language tag>\0<translated keyword>\0<UTF-8 text>
    let mut parts = rest.splitn(3, |&b| b == 0);
    let _language = parts.next().unwrap_or_default();
    let _translated = parts.next();
    let text = match parts.next() {
        Some(text) => text,
        None => return String::new(),
    };

    if compressed {
        let mut out = Vec::new();
        match ZlibDecoder::new(text).read_to_end(&mut out) {
            Ok(_) => decode_text(&out),
            Err(_) => String::new(),
        }
    } else {
        decode_text(text)
    }
}

/// ComfyUI stores JSON (ASCII-escaped) in latin-1 chunks; be liberal.
fn decode_text(raw: &[u8]) -> String {
    String::from_utf8(raw.to_vec()).unwrap_or_else(|_| latin1(raw))
}

fn latin1(raw: &[u8]) -> String {
    raw.iter().map(|&b| b as char).collect()
}
