//! Bundled metric faces.
//!
//! Pixel and wrap parity depend on the *same* faces being installed everywhere:
//! the layout measurer, the rasteriser and the SVG glyph outlines. The reference
//! ships `@fontsource/nunito-sans` for its demo deck and relies on the host for
//! Office faces; we bundle the metric-compatible families so a minimal container
//! behaves like a desktop install:
//!
//! | authored | bundled | license |
//! |---|---|---|
//! | Calibri | Carlito (metric-compatible) | OFL 1.1 |
//! | Cambria | Caladea (metric-compatible) | OFL 1.1 |
//! | Nunito Sans | Nunito Sans (demo deck) | OFL 1.1 |
//!
//! `assets/fonts/NOTICE.md` records the upstream sources. Faces are loaded into
//! every `fontdb::Database` at construction so system fonts and bundled fonts
//! resolve through one path.

/// Font payloads compiled into the library so headless/container runs do not
/// depend on `fc-list` containing Office faces.
pub const BUNDLED_FONTS: &[&[u8]] = &[
    include_bytes!("../../assets/fonts/Carlito-Regular.ttf"),
    include_bytes!("../../assets/fonts/Carlito-Bold.ttf"),
    include_bytes!("../../assets/fonts/Carlito-Italic.ttf"),
    include_bytes!("../../assets/fonts/Carlito-BoldItalic.ttf"),
    include_bytes!("../../assets/fonts/Caladea-Regular.ttf"),
    include_bytes!("../../assets/fonts/Caladea-Bold.ttf"),
    include_bytes!("../../assets/fonts/Caladea-Italic.ttf"),
    include_bytes!("../../assets/fonts/Caladea-BoldItalic.ttf"),
    include_bytes!("../../assets/fonts/NunitoSans-300-normal.ttf"),
    include_bytes!("../../assets/fonts/NunitoSans-400-normal.ttf"),
    include_bytes!("../../assets/fonts/NunitoSans-400-italic.ttf"),
    include_bytes!("../../assets/fonts/NunitoSans-600-normal.ttf"),
    include_bytes!("../../assets/fonts/NunitoSans-700-normal.ttf"),
    include_bytes!("../../assets/fonts/NunitoSans-700-italic.ttf"),
];

/// Install every bundled face into a `fontdb` database.
pub fn install_into_db(db: &mut fontdb::Database) {
    for data in BUNDLED_FONTS {
        db.load_font_data(data.to_vec());
    }
}

/// Install every bundled face into a `cosmic-text` font system.
pub fn install_into_font_system(font_system: &mut cosmic_text::FontSystem) {
    for data in BUNDLED_FONTS {
        font_system.db_mut().load_font_data(data.to_vec());
    }
}
