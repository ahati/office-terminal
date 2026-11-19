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
//!
//! The module also owns the crate's ONE shared font runtime: a per-thread
//! [`cosmic_text::FontSystem`] and a per-thread raw `fontdb::Database`, both
//! built from the same system scan plus the bundled faces above. Shaping
//! (`text::shape`), the rasteriser and the WordArt warp compiler all resolve
//! fonts through them (see the thread-local block below).

use std::cell::RefCell;

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

// ---------------------------------------------------------------------------
// Shared thread-local font runtime
// ---------------------------------------------------------------------------
//
// `FontSystem::new()` and `fontdb::Database::load_system_fonts()` each scan
// every face the host has installed, and each store then copies the fourteen
// bundled faces above. Every consumer needs exactly the same store, so before
// this block existed each one paid the scan plus the copies in its own
// thread-local static: the layout measurer, the rasteriser and the warp
// compiler each held a full `FontSystem`, and the measurer and the rasteriser
// each held a full raw database — up to five font scans per thread. The two
// statics below are the ONE shared runtime per thread.

thread_local! {
    /// The one per-thread `cosmic-text` font system: system scan plus every
    /// bundled face, exactly as every former per-site static built it.
    static FONT_SYSTEM: RefCell<cosmic_text::FontSystem> = RefCell::new({
        let mut fs = cosmic_text::FontSystem::new();
        install_into_font_system(&mut fs);
        fs
    });
    /// The one per-thread raw `fontdb` database: system scan plus every
    /// bundled face, for consumers that query face records or raw face bytes
    /// directly instead of going through `cosmic-text`.
    pub(crate) static FACE_DB: RefCell<fontdb::Database> = RefCell::new({
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        install_into_db(&mut db);
        db
    });
}

/// Run `f` with this thread's shared [`cosmic_text::FontSystem`].
///
/// The system is constructed once per thread (system fonts plus the bundled
/// faces above) and reused by every shaping consumer: the layout measurer
/// (`text::shape`), the run rasteriser and the WordArt warp compiler. The
/// closure receives the system mutably because `cosmic_text::Buffer` shaping
/// takes `&mut FontSystem`; no consumer nests this call, so the `RefCell`
/// cannot alias.
pub fn with_font_system<R>(f: impl FnOnce(&mut cosmic_text::FontSystem) -> R) -> R {
    FONT_SYSTEM.with(|cell| f(&mut cell.borrow_mut()))
}

/// Run `f` with this thread's shared raw font database.
///
/// Same construction as the font system above (system scan plus the bundled
/// faces) for consumers that need `fontdb` queries or raw face bytes: the
/// ascent probe and `with_face_data` seam (`text::shape`), the affine/warp
/// outline painters, the glyph-feature probes and the character map. Every
/// current use is read-only, so the database is handed out immutably; the
/// static itself stays `pub(crate)` because the rasteriser's painters borrow
/// the cell directly through their `super::` path.
pub fn with_face_db<R>(f: impl FnOnce(&fontdb::Database) -> R) -> R {
    FACE_DB.with(|cell| f(&cell.borrow()))
}
