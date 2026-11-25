//! PDF logical page labels (`/PageLabels` in the catalog, ISO 32000 §7.4.3).
//!
//! Documents renumber their pages for display: front matter in roman
//! numerals, the body restarting at 1, optional prefixes. The fuzzy TOC
//! search and the reader status bar show these labels; navigation keeps
//! using physical page indexes.

use hayro_syntax::object::{Array, Dict, MaybeRef, Name, Number, Object, String as PdfString};
use hayro_syntax::xref::XRef;

/// Compute the display label for every page, or `None` when the catalog
/// carries no `/PageLabels` tree (callers fall back to physical numbers).
pub fn parse_page_labels(root: &Dict<'_>, xref: &XRef, page_count: usize) -> Option<Vec<String>> {
    let labels_dict = root.get::<Dict<'_>>(b"PageLabels")?;
    let nums = labels_dict.get::<Array<'_>>(b"Nums")?;

    // Collect `(range_start, range)` pairs; keys arrive in ascending order.
    let mut ranges: Vec<(usize, LabelRange)> = Vec::new();
    let mut iter = nums.raw_iter();
    while let (Some(k), Some(v)) = (iter.next(), iter.next()) {
        let start = match &k {
            MaybeRef::NotRef(Object::Number(n)) => n.as_i64().max(0) as usize,
            _ => continue,
        };
        let range_dict = match v {
            MaybeRef::Ref(r) => xref.get::<Dict<'_>>(r.into()),
            MaybeRef::NotRef(Object::Dict(d)) => Some(d),
            _ => None,
        };
        let Some(range_dict) = range_dict else {
            continue;
        };
        ranges.push((start, LabelRange::from_dict(&range_dict)));
    }
    if ranges.is_empty() {
        return None;
    }

    let mut labels = Vec::with_capacity(page_count);
    let mut current = 0usize; // index into `ranges`
    for page in 0..page_count {
        while current + 1 < ranges.len() && ranges[current + 1].0 <= page {
            current += 1;
        }
        let (start, range) = &ranges[current];
        let offset = page.saturating_sub(*start) as i64;
        let number = range.first_number.saturating_add(offset);
        let mut label = range.prefix.clone();
        if let Some(style) = &range.style {
            label.push_str(&style.format(number));
        }
        labels.push(label);
    }
    Some(labels)
}

/// One `/PageLabels` number-tree range: label rule for pages from its key
/// until the next range key (or the end of the document).
struct LabelRange {
    /// `/P` prefix text prepended to every label in the range.
    prefix: String,
    /// `/S` numbering style; `None` means prefix-only labels.
    style: Option<LabelStyle>,
    /// `/St` displayed number of the range's first page (default 1).
    first_number: i64,
}

impl LabelRange {
    fn from_dict(dict: &Dict<'_>) -> Self {
        let prefix = dict
            .get::<PdfString<'_>>(b"P")
            .map(|s| decode_label_text(s.as_bytes()))
            .unwrap_or_default();
        let style = dict
            .get::<Name<'_>>(b"S")
            .and_then(|n| LabelStyle::from_name(n.as_ref()));
        let first_number = dict.get::<Number>(b"St").map(|n| n.as_i64().max(0)).unwrap_or(1);
        Self {
            prefix,
            style,
            first_number,
        }
    }
}

enum LabelStyle {
    Decimal,
    RomanUpper,
    RomanLower,
    LettersUpper,
    LettersLower,
}

impl LabelStyle {
    fn from_name(name: &[u8]) -> Option<Self> {
        match name {
            b"D" => Some(Self::Decimal),
            b"R" => Some(Self::RomanUpper),
            b"r" => Some(Self::RomanLower),
            b"A" => Some(Self::LettersUpper),
            b"a" => Some(Self::LettersLower),
            _ => None,
        }
    }

    fn format(&self, number: i64) -> String {
        match self {
            Self::Decimal => number.to_string(),
            Self::RomanUpper => roman_upper(number),
            Self::RomanLower => roman_lower(number),
            Self::LettersUpper => letters_upper(number),
            Self::LettersLower => letters_lower(number),
        }
    }
}

/// Decode a label prefix: UTF-16BE with BOM, else Latin-1 bytes.
fn decode_label_text(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        bytes.iter().map(|&b| b as char).collect()
    }
}

/// Bijective base-26 letters: `1 -> a`, `26 -> z`, `27 -> aa`.
fn letters_lower(number: i64) -> String {
    letters(number, b'a')
}

fn letters_upper(number: i64) -> String {
    letters(number, b'A')
}

fn letters(number: i64, base: u8) -> String {
    if number <= 0 {
        return String::new();
    }
    let mut n = number;
    let mut out = Vec::new();
    while n > 0 {
        let rem = ((n - 1) % 26) as u8;
        out.push(base + rem);
        n = (n - 1) / 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

fn roman_lower(number: i64) -> String {
    roman(number).to_lowercase()
}

fn roman_upper(number: i64) -> String {
    roman(number)
}

/// Standard subtractive roman numerals; `0` and negatives yield "".
fn roman(number: i64) -> String {
    const TABLE: [(i64, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut n = number;
    let mut out = String::new();
    for &(value, symbol) in &TABLE {
        while n >= value {
            out.push_str(symbol);
            n -= value;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_are_bijective_base_26() {
        assert_eq!(letters_lower(1), "a");
        assert_eq!(letters_lower(26), "z");
        assert_eq!(letters_lower(27), "aa");
        assert_eq!(letters_lower(28), "ab");
        assert_eq!(letters_upper(52), "AZ");
    }

    #[test]
    fn roman_numerals_subtractive() {
        assert_eq!(roman_lower(1), "i");
        assert_eq!(roman_upper(4), "IV");
        assert_eq!(roman_lower(9), "ix");
        assert_eq!(roman_upper(14), "XIV");
        assert_eq!(roman_upper(1987), "MCMLXXXVII");
        assert_eq!(roman_lower(0), "");
    }
}
