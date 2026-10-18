//! Minimal JPEG EXIF reader for the `UserComment` tag (A1111 writes the
//! `parameters` block there when the image is a JPEG).

use std::path::Path;

pub fn user_comment_from_file(path: &Path) -> Option<String> {
    let data = std::fs::read(path).ok()?;
    let tiff = find_exif(&data)?;
    let comment = user_comment(tiff)?;
    decode_comment(&comment)
}

/// Locate the `Exif\0\0` APP1 payload in a JPEG byte stream.
fn find_exif(data: &[u8]) -> Option<&[u8]> {
    let mut i = 2; // skip SOI
    while i + 4 <= data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        if marker == 0xFF {
            i += 1;
            continue;
        }
        if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            i += 2;
            continue;
        }
        if marker == 0xDA {
            break; // start of scan, no more headers
        }
        let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        if len < 2 {
            break;
        }
        let payload_start = i + 4;
        let payload_end = (i + 2 + len).min(data.len());
        if payload_end < payload_start {
            break;
        }
        if marker == 0xE1 {
            let payload = &data[payload_start..payload_end];
            if let Some(tiff) = payload.strip_prefix(b"Exif\0\0") {
                return Some(tiff);
            }
        }
        i = payload_end.max(i + 2);
    }
    None
}

struct Tiff<'a> {
    data: &'a [u8],
    little: bool,
}

impl<'a> Tiff<'a> {
    fn new(data: &'a [u8]) -> Option<Self> {
        if data.len() < 8 {
            return None;
        }
        let little = match &data[0..2] {
            b"II" => true,
            b"MM" => false,
            _ => return None,
        };
        let tiff = Tiff { data, little };
        if tiff.u16_at(2)? != 42 {
            return None;
        }
        Some(tiff)
    }

    fn u16_at(&self, pos: usize) -> Option<u16> {
        let b = self.data.get(pos..pos + 2)?;
        Some(if self.little {
            u16::from_le_bytes([b[0], b[1]])
        } else {
            u16::from_be_bytes([b[0], b[1]])
        })
    }

    fn u32_at(&self, pos: usize) -> Option<u32> {
        let b = self.data.get(pos..pos + 4)?;
        Some(if self.little {
            u32::from_le_bytes([b[0], b[1], b[2], b[3]])
        } else {
            u32::from_be_bytes([b[0], b[1], b[2], b[3]])
        })
    }

    /// `(count, raw 4-byte value field)` of `tag` inside the IFD at `ifd`.
    fn entry(&self, ifd: usize, tag: u16) -> Option<(u32, [u8; 4])> {
        let count = self.u16_at(ifd)? as usize;
        for i in 0..count {
            let pos = ifd + 2 + i * 12;
            if self.u16_at(pos)? != tag {
                continue;
            }
            let n = self.u32_at(pos + 4)?;
            let raw = self.data.get(pos + 8..pos + 12)?;
            return Some((n, [raw[0], raw[1], raw[2], raw[3]]));
        }
        None
    }

    fn offset_of(field: [u8; 4], little: bool) -> u32 {
        if little {
            u32::from_le_bytes(field)
        } else {
            u32::from_be_bytes(field)
        }
    }
}

fn user_comment(tiff: &[u8]) -> Option<Vec<u8>> {
    let tiff = Tiff::new(tiff)?;
    let ifd0 = tiff.u32_at(4)? as usize;
    let (_, exif_ptr) = tiff.entry(ifd0, 0x8769)?;
    let exif_ifd = Tiff::offset_of(exif_ptr, tiff.little) as usize;
    let (count, value) = tiff.entry(exif_ifd, 0x9286)?;

    let count = count as usize;
    if count <= 4 {
        return Some(value[..count].to_vec());
    }
    let start = Tiff::offset_of(value, tiff.little) as usize;
    let bytes = tiff.data.get(start..start + count)?;
    Some(bytes.to_vec())
}

fn decode_comment(bytes: &[u8]) -> Option<String> {
    if let Some(body) = bytes.strip_prefix(b"UNICODE\0") {
        let pairs: Vec<u16> = body
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return tidy(String::from_utf16_lossy(&pairs));
    }
    if let Some(body) = bytes.strip_prefix(b"ASCII\0\0\0") {
        return tidy(String::from_utf8_lossy(body).into_owned());
    }
    tidy(String::from_utf8_lossy(bytes).into_owned())
}

fn tidy(text: String) -> Option<String> {
    let trimmed = text.trim_matches('\u{0}').trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}
