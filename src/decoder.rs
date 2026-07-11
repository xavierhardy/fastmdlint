//! Automatic character-encoding detection (BOM-based, UTF-8/16/32).

fn decode_utf16(data: &[u8], big_endian: bool) -> Result<String, String> {
    if !data.len().is_multiple_of(2) {
        return Err("truncated UTF-16 data".to_string());
    }
    let units: Vec<u16> = data
        .chunks_exact(2)
        .map(|c| {
            if big_endian {
                u16::from_be_bytes([c[0], c[1]])
            } else {
                u16::from_le_bytes([c[0], c[1]])
            }
        })
        .collect();
    String::from_utf16(&units).map_err(|e| e.to_string())
}

fn decode_utf32(data: &[u8], big_endian: bool) -> Result<String, String> {
    if !data.len().is_multiple_of(4) {
        return Err("truncated UTF-32 data".to_string());
    }
    data.chunks_exact(4)
        .map(|c| {
            let code = if big_endian {
                u32::from_be_bytes([c[0], c[1], c[2], c[3]])
            } else {
                u32::from_le_bytes([c[0], c[1], c[2], c[3]])
            };
            char::from_u32(code).ok_or_else(|| format!("invalid code point {code:#x}"))
        })
        .collect()
}

/// Detect the encoding from the BOM / first bytes and decode.
pub fn auto_decode(data: &[u8]) -> Result<String, String> {
    const BOM_UTF32_BE: &[u8] = &[0x00, 0x00, 0xFE, 0xFF];
    const BOM_UTF32_LE: &[u8] = &[0xFF, 0xFE, 0x00, 0x00];
    const BOM_UTF16_BE: &[u8] = &[0xFE, 0xFF];
    const BOM_UTF16_LE: &[u8] = &[0xFF, 0xFE];
    const BOM_UTF8: &[u8] = &[0xEF, 0xBB, 0xBF];

    if data.starts_with(BOM_UTF32_BE) {
        decode_utf32(&data[4..], true)
    } else if data.starts_with(&[0x00, 0x00, 0x00]) && data.len() >= 4 {
        decode_utf32(data, true)
    } else if data.starts_with(BOM_UTF32_LE) {
        decode_utf32(&data[4..], false)
    } else if data.len() >= 4 && data[1..4] == [0x00, 0x00, 0x00] {
        decode_utf32(data, false)
    } else if data.starts_with(BOM_UTF16_BE) {
        decode_utf16(&data[2..], true)
    } else if data.starts_with(&[0x00]) && data.len() >= 2 {
        decode_utf16(data, true)
    } else if data.starts_with(BOM_UTF16_LE) {
        decode_utf16(&data[2..], false)
    } else if data.len() >= 2 && data[1] == 0x00 {
        decode_utf16(data, false)
    } else if data.starts_with(BOM_UTF8) {
        String::from_utf8(data[3..].to_vec()).map_err(|e| e.to_string())
    } else {
        String::from_utf8(data.to_vec()).map_err(|e| e.to_string())
    }
}
