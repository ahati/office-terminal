//! Stage-1 measurement and the permanent correctness lock for [`super`].
//!
//! The wrap phase queries the width oracle at the fit probe's stops, and the
//! split decision consumes the UAX #14-legal ones — so the lock below
//! compares the oracle against a fresh `make_line` probe-by-probe
//! measurement at EVERY atom cut of the corpus (legal and interior), and
//! against `advance(prefix)` directly at every legal cut. The naive
//! per-cluster prefix-sum oracle the first stage considered is measured on
//! the same corpus for the record: it diverges exactly where the word-level
//! reconstruction stays bit-identical.

use super::*;
use crate::text::linebreak::grapheme::grapheme_spans;
use crate::text::linebreak::{may_break, BreakItem};
use crate::text::measure::{TextMeasure, WrapWords};
use crate::text::shape;
use std::cell::Cell;

/// 18 pt in EMU — the corpus size.
const PT18_EMU: f32 = 18.0 * 12700.0;

/// The metric-compatible faces the pipeline actually resolves (see
/// `compiler::text::family_substitute` and the bundled set in `text::fonts`):
/// Carlito for Calibri metrics, Liberation Serif/Sans/Mono for
/// Times/Arial/Courier, Nunito Sans as the bundled kerned face pinned by
/// `font_metrics_tests`.
const FAMILIES: &[&str] = &[
    "Carlito",
    "Liberation Serif",
    "Liberation Sans",
    "Liberation Mono",
    "Nunito Sans",
];

/// Corpus texts: ligature-heavy, kern-heavy, plain wrap fodder, hyphen cuts,
/// CJK with a CJK/Latin seam, CJK punctuation (where the custom kernel cuts
/// inside a UAX #14 word), Arabic joining, mixed bidi, combining marks, a long
/// unbreakable word for emergency tears, Thai (dictionary script), and
/// zero-width-space cuts inside words.
const TEXTS: &[&str] = &[
    "fluffy offices waffle flow swiftly shifting",
    "Tavern Avatar Velvet LT AVATAR WALTZ",
    "The quick brown fox jumps over the lazy dog and keeps going",
    "non-managed re-entry co-operation state-of-the-art",
    "日本語PowerPoint テキスト rendering engine",
    "あい、う。「漢」です、よね",
    "السلام عليكم ورحمة الله وبركاته",
    "hello سلام وأهلا welcome mixing",
    "na\u{301}ive caf\u{e9} re\u{301}sume\u{301} pre\u{301}mise",
    "supercalifragilisticexpialidocious antidisestablishment",
    "สวัสดีครับทดสอบการแสดงผล",
    "tab\u{200b}sep\u{200b}arated\u{200b}zwsp\u{200b}cuts",
    "THE FOREST STATE wraps at the pinned budget",
];

/// A production-shaped measurer: `advance` is exactly what `Measurer::advance`
/// computes for a zero-tracking style, and `wrap_words` goes through the same
/// `shape::shape_words` the real one does.
struct FontMeasurer {
    faces: Vec<(&'static str, f32, bool, bool)>,
}

impl FontMeasurer {
    fn new() -> Self {
        let mut faces = Vec::new();
        for family in FAMILIES {
            for (bold, italic) in [(false, false), (true, false), (false, true)] {
                faces.push((*family, PT18_EMU, bold, italic));
            }
        }
        Self { faces }
    }

    fn style(&self, family: &str, bold: bool, italic: bool) -> StyleId {
        let index = self
            .faces
            .iter()
            .position(|(f, _, b, i)| *f == family && *b == bold && *i == italic)
            .unwrap_or(0);
        StyleId(index)
    }
}

impl TextMeasure for FontMeasurer {
    fn advance(&self, text: &str, style: StyleId) -> f32 {
        let face = &self.faces[style.0];
        shape::measure(text, face.0, face.1, face.2, face.3)
    }

    fn space(&self, style: StyleId) -> f32 {
        self.advance(" ", style)
    }

    fn wrap_words(&self, text: &str, style: StyleId) -> Option<WrapWords> {
        let face = &self.faces[style.0];
        let shaped = shape::shape_words(text, face.0, face.1, face.2, face.3)?;
        Some(WrapWords::from_shaped(shaped, 0.0))
    }
}

/// A measurer that delegates but never offers the oracle capability: the
/// kernel must fall back to the probe-by-`make_line` walk (the baseline).
struct Legacy<'a>(&'a FontMeasurer);

impl TextMeasure for Legacy<'_> {
    fn advance(&self, text: &str, style: StyleId) -> f32 {
        self.0.advance(text, style)
    }
    fn space(&self, style: StyleId) -> f32 {
        self.0.space(style)
    }
}

/// Atoms exactly as `build_regions` segments one authored text run.
fn atoms_of<'a>(text: &'a str, style: StyleId) -> Vec<Atom<'a>> {
    text_atoms(text, style, 0)
}

fn text_atoms<'a>(text: &'a str, style: StyleId, run: usize) -> Vec<Atom<'a>> {
    grapheme_spans(text)
        .into_iter()
        .map(|(from, to)| Atom {
            kind: AtomKind::Text(&text[from..to]),
            style,
            run,
        })
        .collect()
}

fn options<'a>(measurer: &'a dyn TextMeasure) -> BreakOptions<'a> {
    options_at(measurer, f32::INFINITY)
}

fn options_at<'a>(measurer: &'a dyn TextMeasure, max_width: f32) -> BreakOptions<'a> {
    BreakOptions {
        max_width,
        first_line_indent: 0.0,
        tab_stops: &[],
        default_tab_size: 0.0,
        east_asian_line_break: true,
        measurer,
    }
}

/// The legal cut positions per the wrap kernel's own rule (both `eaLnBrk`
/// settings; the split decision only ever consumes these).
fn legal_cuts(atoms: &[Atom<'_>]) -> Vec<usize> {
    (0..=atoms.len())
        .filter(|&i| may_break(atoms, i, true) || may_break(atoms, i, false))
        .collect()
}

fn prefix_text(atoms: &[Atom<'_>], stop: usize) -> String {
    atoms[..stop]
        .iter()
        .map(|atom| match atom.kind {
            AtomKind::Text(text) => text,
            _ => "",
        })
        .collect()
}

/// The permanent lock: the oracle prices every cut the fit probe could query
/// bit-identically to a fresh probe-by-probe `make_line`, and every legal cut
/// bit-identically to `advance(prefix)`.
#[test]
fn oracle_matches_probe_measurement_at_every_cut() {
    let measurer = FontMeasurer::new();
    let mut cuts_checked = 0usize;
    let mut legal_checked = 0usize;
    for family in FAMILIES {
        for (bold, italic) in [(false, false), (true, false), (false, true)] {
            let style = measurer.style(family, bold, italic);
            for text in TEXTS {
                let atoms = atoms_of(text, style);
                let o = options(&measurer);
                let oracle =
                    LineOracle::build(&atoms, 0, &o).unwrap_or_else(|| panic!("oracle must model {family} {text:?}"));
                for stop in 0..=atoms.len() {
                    let oracle_width = oracle.width_at(stop);
                    let probe_width = super::super::make_line(&atoms, 0, stop, 0, &o).width;
                    assert!(
                        oracle_width == probe_width,
                        "{family} b={bold} i={italic} {text:?} cut {stop}: \
                         oracle {oracle_width:.6} vs probe {probe_width:.6}"
                    );
                    cuts_checked += 1;
                }
                for stop in legal_cuts(&atoms) {
                    let advance = measurer.advance(&prefix_text(&atoms, stop), style);
                    assert!(
                        oracle.width_at(stop) == advance,
                        "{family} {text:?} legal cut {stop}: oracle != advance(prefix)"
                    );
                    legal_checked += 1;
                }
            }
        }
    }
    println!("oracle lock: {cuts_checked} cuts, {legal_checked} of them legal, all bit-identical");
}

/// A mixed-style line with an inline object: the item fold must match
/// `make_line`'s coalescing and the object's fixed advance exactly.
#[test]
fn oracle_matches_probes_on_mixed_style_and_object_lines() {
    let measurer = FontMeasurer::new();
    let regular = measurer.style("Carlito", false, false);
    let bold = measurer.style("Carlito", true, false);
    let mut atoms = text_atoms("fluffy ", regular, 0);
    atoms.extend(text_atoms("offices", bold, 1));
    atoms.push(Atom {
        kind: AtomKind::Object(36000.0),
        style: regular,
        run: 2,
    });
    atoms.extend(text_atoms(" waffle flow", regular, 3));
    let o = options(&measurer);
    let oracle = LineOracle::build(&atoms, 0, &o).expect("oracle must model mixed-style lines");
    for stop in 0..=atoms.len() {
        let oracle_width = oracle.width_at(stop);
        let probe_width = super::super::make_line(&atoms, 0, stop, 0, &o).width;
        assert!(
            oracle_width == probe_width,
            "mixed-style line cut {stop}: oracle {oracle_width:.6} vs probe {probe_width:.6}"
        );
    }
}

/// The stage-1 naive oracle: per-cluster advances from ONE full shaping,
/// prefix-summed in logical order. Returns `(cluster start bytes, sums)`.
fn naive_cluster_sums(text: &str, family: &str, bold: bool, italic: bool) -> Option<(Vec<usize>, Vec<f32>)> {
    crate::text::fonts::with_font_system(|font_system| {
        let size_px = (PT18_EMU / 12700.0).max(0.5);
        let mut buffer = cosmic_text::Buffer::new(font_system, cosmic_text::Metrics::new(size_px, size_px * 1.2));
        buffer.set_size(font_system, None, None);
        let mut attrs = cosmic_text::Attrs::new().family(cosmic_text::Family::Name(family));
        if bold {
            attrs = attrs.weight(cosmic_text::Weight::BOLD);
        }
        if italic {
            attrs = attrs.style(cosmic_text::Style::Italic);
        }
        buffer.set_text(font_system, text, attrs, cosmic_text::Shaping::Advanced);
        buffer.shape_until_scroll(font_system, false);
        let line = buffer.lines.first()?.shape_opt().as_ref()?;
        let line_rtl = line.rtl;
        // Per-glyph (cluster start byte, advance) in the wrap fold's order.
        let mut glyphs: Vec<(usize, f32)> = Vec::new();
        for span in &line.spans {
            let words = if line_rtl != span.level.is_rtl() {
                span.words.iter().rev().collect::<Vec<_>>()
            } else {
                span.words.iter().collect::<Vec<_>>()
            };
            for word in words {
                for glyph in &word.glyphs {
                    glyphs.push((glyph.start, glyph.width(size_px)));
                }
            }
        }
        glyphs.sort_by_key(|(start, _)| *start);
        let mut bytes = Vec::new();
        let mut sums = Vec::new();
        for (start, advance) in glyphs {
            if bytes.last() == Some(&start) {
                if let Some(sum) = sums.last_mut() {
                    *sum += advance;
                }
            } else {
                bytes.push(start);
                sums.push(advance);
            }
        }
        Some((bytes, sums))
    })
}

/// Stage-1 measurement, kept for the record: the naive cluster-sum oracle
/// diverges from fresh `advance(prefix)` at legal cuts (fold association plus
/// truncated-word kern/ligature loss) exactly where the landed word-level
/// oracle stays bit-identical.
#[test]
fn naive_cluster_sums_diverge_where_the_word_oracle_is_exact() {
    let measurer = FontMeasurer::new();
    let mut legal_points = 0usize;
    let mut divergent = 0usize;
    let mut worst: f32 = 0.0;
    let mut worst_at = String::new();
    for family in FAMILIES {
        for (bold, italic) in [(false, false), (true, false), (false, true)] {
            let style = measurer.style(family, bold, italic);
            for text in TEXTS {
                let atoms = atoms_of(text, style);
                let Some((bytes, sums)) = naive_cluster_sums(text, family, bold, italic) else {
                    continue;
                };
                let o = options(&measurer);
                let oracle =
                    LineOracle::build(&atoms, 0, &o).unwrap_or_else(|| panic!("oracle must model {family} {text:?}"));
                for stop in legal_cuts(&atoms) {
                    let cut: usize = atoms[..stop]
                        .iter()
                        .map(|atom| match atom.kind {
                            AtomKind::Text(text) => text.len(),
                            _ => 0,
                        })
                        .sum();
                    let mut naive = 0.0f32;
                    for (index, start) in bytes.iter().enumerate() {
                        if *start < cut {
                            naive += sums[index];
                        } else {
                            break;
                        }
                    }
                    let naive_emu = naive * 12700.0;
                    let fresh = measurer.advance(&prefix_text(&atoms, stop), style);
                    legal_points += 1;
                    if naive_emu != fresh {
                        divergent += 1;
                        let delta = (naive_emu - fresh).abs();
                        if delta > worst {
                            worst = delta;
                            worst_at = format!("{family} {text:?} legal cut {stop}");
                        }
                    }
                    // The landed oracle stays exact at the same cut.
                    assert!(oracle.width_at(stop) == fresh, "{family} {text:?}");
                }
            }
        }
    }
    println!(
        "naive cluster sums: {legal_points} legal points, {divergent} divergent, \
         worst |delta| {worst:.4} EMU at {worst_at}"
    );
}

/// The behavioural lock: the oracle-driven wrap equals the probe-by-probe
/// wrap, line for line, width for width.
#[test]
fn oracle_breaks_identically_to_probe_by_probe() {
    let measurer = FontMeasurer::new();
    let legacy = Legacy(&measurer);
    let mut cases = 0usize;
    for family in FAMILIES {
        let style = measurer.style(family, false, false);
        for text in TEXTS {
            let full = measurer.advance(text, style);
            for fraction in [0.08, 0.35, 0.5, 0.7, 0.95] {
                let items = [BreakItem::Text {
                    text: (*text).to_string(),
                    style,
                }];
                let oracle_lines =
                    crate::text::linebreak::break_paragraph(&items, &options_at(&measurer, full * fraction));
                let legacy_lines =
                    crate::text::linebreak::break_paragraph(&items, &options_at(&legacy, full * fraction));
                assert_eq!(
                    format!("{oracle_lines:?}"),
                    format!("{legacy_lines:?}"),
                    "{family} {text:?} at {fraction}"
                );
                cases += 1;
            }
        }
    }
    println!("behavioural lock: {cases} wraps identical");
}

thread_local! {
    static FRAGMENTS: Cell<u64> = const { Cell::new(0) };
}

/// Shaping work per wrap: the probe-by-probe walk against the oracle.
#[test]
fn oracle_cuts_shaping_work_per_wrap() {
    struct Counting {
        inner: FontMeasurer,
        advances: Cell<u64>,
        builds: Cell<u64>,
    }
    impl TextMeasure for Counting {
        fn advance(&self, text: &str, style: StyleId) -> f32 {
            self.advances.set(self.advances.get() + 1);
            self.inner.advance(text, style)
        }
        fn space(&self, style: StyleId) -> f32 {
            self.inner.space(style)
        }
        fn wrap_words(&self, text: &str, style: StyleId) -> Option<WrapWords> {
            self.builds.set(self.builds.get() + 1);
            let mut words = self.inner.wrap_words(text, style)?;
            for span in &mut words.spans {
                let original = std::mem::replace(&mut span.fragment, Box::new(|_, _, _, _| 0.0));
                span.fragment = Box::new(move |text, start, cut, rtl| {
                    FRAGMENTS.with(|count| count.set(count.get() + 1));
                    original(text, start, cut, rtl)
                });
            }
            Some(words)
        }
    }
    // The legacy counter delegates every call but never offers the oracle.
    struct CountingLegacy<'a>(&'a FontMeasurer, Cell<u64>);
    impl TextMeasure for CountingLegacy<'_> {
        fn advance(&self, text: &str, style: StyleId) -> f32 {
            self.1.set(self.1.get() + 1);
            self.0.advance(text, style)
        }
        fn space(&self, style: StyleId) -> f32 {
            self.0.space(style)
        }
    }

    let measurer = FontMeasurer::new();
    let counting = Counting {
        inner: FontMeasurer::new(),
        advances: Cell::new(0),
        builds: Cell::new(0),
    };
    let counting_legacy = CountingLegacy(&measurer, Cell::new(0));
    let style = measurer.style("Carlito", false, false);

    let mut wraps = 0u64;
    for text in TEXTS {
        let full = measurer.advance(text, style);
        for fraction in [0.08, 0.35, 0.5, 0.7] {
            let items = [BreakItem::Text {
                text: (*text).to_string(),
                style,
            }];
            crate::text::linebreak::break_paragraph(&items, &options_at(&counting_legacy, full * fraction));
            crate::text::linebreak::break_paragraph(&items, &options_at(&counting, full * fraction));
            wraps += 1;
        }
    }
    let legacy_full = counting_legacy.1.get();
    let oracle_full = counting.advances.get() + counting.builds.get();
    let fragments = FRAGMENTS.with(Cell::get);
    println!(
        "shaping work over {wraps} wraps: probe-by-probe {legacy_full} full line shapes \
         ({:.1}/wrap); oracle {oracle_full} shapes ({:.1}/wrap) + {fragments} \
         tiny truncated-word fragments ({:.1}/wrap)",
        legacy_full as f64 / wraps as f64,
        oracle_full as f64 / wraps as f64,
        fragments as f64 / wraps as f64,
    );
    assert!(
        oracle_full < legacy_full,
        "oracle used {oracle_full} shapes vs {legacy_full} probe shapes"
    );
}
