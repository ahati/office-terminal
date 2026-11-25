//! PDF text string decoding according to ISO 32000-1 §7.9.2.2.
//!
//! A PDF text string may be encoded in:
//! - UTF-16BE: marked by leading BOM `0xFE, 0xFF`
//! - UTF-16LE: marked by leading BOM `0xFF, 0xFE`
//! - UTF-8: marked by leading BOM `0xEF, 0xBB, 0xBF`, or raw valid UTF-8
//! - PDFDocEncoding: standard 8-bit encoding mapping bytes 0x00..=0xFF

/// Decodes a raw PDF text string into a clean, sanitized Rust String.
pub fn decode_pdf_text_string(bytes: &[u8]) -> String {
    let raw = if bytes.starts_with(&[0xFE, 0xFF]) {
        decode_utf16_be(&bytes[2..])
    } else if bytes.starts_with(&[0xFF, 0xFE]) {
        decode_utf16_le(&bytes[2..])
    } else if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(&bytes[3..]).into_owned()
    } else if let Ok(s) = std::str::from_utf8(bytes) {
        s.to_string()
    } else {
        decode_pdf_doc_encoding(bytes)
    };

    sanitize_text(&raw)
}

fn decode_utf16_be(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

fn decode_utf16_le(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

fn decode_pdf_doc_encoding(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| pdf_doc_char(b)).collect()
}

fn pdf_doc_char(b: u8) -> char {
    match b {
        0x09 => '\t',
        0x0A => '\n',
        0x0D => '\r',
        0x18 => '\u{02D8}', // breve
        0x19 => '\u{02C7}', // caron
        0x1A => '\u{02C6}', // circumflex
        0x1B => '\u{02D9}', // dotaccent
        0x1C => '\u{02DD}', // hungarumlaut
        0x1D => '\u{02DB}', // ogonek
        0x1E => '\u{02DA}', // ring
        0x1F => '\u{02DC}', // tilde
        0x20..=0x7E => b as char,
        0x80 => '\u{2022}',       // bullet
        0x81 => '\u{2020}',       // dagger
        0x82 => '\u{2021}',       // daggerdbl
        0x83 => '\u{2026}',       // ellipsis
        0x84 => '\u{2014}',       // emdash
        0x85 => '\u{2013}',       // endash
        0x86 => '\u{0192}',       // florin
        0x87 => '\u{2044}',       // fraction
        0x88 => '\u{2039}',       // guilsinglleft
        0x89 => '\u{203A}',       // guilsinglright
        0x8A => '\u{2212}',       // minus
        0x8B => '\u{2030}',       // perthousand
        0x8C => '\u{201E}',       // quotedblbase
        0x8D => '\u{201C}',       // quotedblleft
        0x8E => '\u{201D}',       // quotedblright
        0x8F => '\u{2018}',       // quoteleft
        0x90 => '\u{2019}',       // quoteright
        0x91 => '\u{201A}',       // quotesinglbase
        0x92 => '\u{2122}',       // trademark
        0x93 => '\u{FB01}',       // fi
        0x94 => '\u{FB02}',       // fl
        0x95 => '\u{0141}',       // Lslash
        0x96 => '\u{0152}',       // OE
        0x97 => '\u{0160}',       // Scaron
        0x98 => '\u{0178}',       // Ydieresis
        0x99 => '\u{017D}',       // Zcaron
        0x9A => '\u{0131}',       // dotlessi
        0x9B => '\u{0142}',       // lslash
        0x9C => '\u{0153}',       // oe
        0x9D => '\u{0161}',       // scaron
        0x9E => '\u{017E}',       // zcaron
        0x9F => '\u{20AC}',       // Euro
        0xA0 => '\u{00A0}',       // non-breaking space
        0xA1..=0xFF => b as char, // matches ISO-8859-1
        _ => ' ',
    }
}

/// Strip zero-width markers, NUL bytes, control characters, and collapse whitespace.
pub fn sanitize_text(s: &str) -> String {
    s.chars()
        .filter(|&c| c != '\0' && c != '\u{FEFF}' && (!c.is_control() || c == ' ' || c == '\t'))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
