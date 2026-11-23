//! Reading-order assignment for one page's run candidates (issue #40 §7.10).
//!
//! Word reads a page top-down, but inside a multi-column section it reads
//! down column 1 and then down column 2, while a table is read row by row.
//! Both look identical geometrically (side-by-side text on shared baselines),
//! so the engine's logical structure tree decides: lines whose structure
//! ancestry includes a table role keep row-major `(y, x)` order. For body
//! text, horizontally disjoint bands separated by at least [`COLUMN_GAP_PT`]
//! (Word's default column gap is 0.5in = 36pt; adjacent table cells are a
//! cell margin apart) read band by band; everything else reads top-down,
//! left to right. Wide lines (a heading spanning the columns) and table lines
//! cut the page into strips, so a band never reaches across a full-width
//! block or a table.
//!
//! Runs without source attribution (list labels such as "1." / "•", field
//! results) are attached to the nearest attributed line on the same baseline
//! when they sit within [`ATTACH_GAP_FACTOR`] font sizes of it, so a list
//! number stays in front of its item instead of trailing the page.

use oxml_layout::{Point, StructureId, StructureRole};

/// Horizontal gap (pt) between text bands that reads as a column separation.
pub(crate) const COLUMN_GAP_PT: f64 = 24.0;
/// Width fraction of the page beyond which a line spans every column.
pub(crate) const WIDE_FRACTION: f64 = 0.6;
/// Unattributed runs within this many font sizes of a line join it.
pub(crate) const ATTACH_GAP_FACTOR: f64 = 2.5;
/// Baseline grouping tolerance in typographic points.
pub(crate) const BASELINE_TOLERANCE_PT: f64 = 0.5;

/// Coarse baseline band of a y position (same band = same visual line).
pub(crate) fn baseline_band(y: f64) -> i64 {
    (y / BASELINE_TOLERANCE_PT).round() as i64
}

/// One candidate run with its reading-order keys.
#[derive(Clone, Debug)]
pub(crate) struct RunCandidate {
    pub node: Option<u32>,
    pub char_start: u32,
    pub origin: Point,
    pub font_size: f64,
    pub advance: f64,
    pub text: String,
    /// Whether the run's structure ancestry includes a table role.
    pub table: bool,
}

impl RunCandidate {
    fn x_min(&self) -> f64 {
        self.origin.x
    }

    fn x_max(&self) -> f64 {
        self.origin.x + self.advance
    }
}

/// Which structure nodes sit inside a table, indexed once per document.
#[derive(Clone, Debug, Default)]
pub(crate) struct TableStructure {
    inside_table: Vec<bool>,
}

impl TableStructure {
    /// Index the tree: [`Self::contains`] is true for every node below a
    /// table role. Empty (and always `false`) without a structure tree.
    pub(crate) fn index(structure: Option<&oxml_layout::DocumentStructure>) -> Self {
        let Some(structure) = structure else {
            return Self::default();
        };
        let is_table_role = |role: &StructureRole| {
            matches!(
                role,
                StructureRole::Table
                    | StructureRole::TableRow
                    | StructureRole::TableCell
                    | StructureRole::TableHeaderCell
            )
        };
        let mut inside_table = vec![false; structure.nodes.len() + 1];
        for node in &structure.nodes {
            if !is_table_role(&node.role) {
                continue;
            }
            let mut stack = vec![node.id];
            while let Some(id) = stack.pop() {
                let index = id.get() as usize;
                if let Some(flag) = inside_table.get_mut(index) {
                    *flag = true;
                }
                if let Some(node) = structure.node(id) {
                    stack.extend(node.children.iter().copied());
                }
            }
        }
        Self { inside_table }
    }

    /// Whether `id` (or an ancestor) is a table role.
    pub(crate) fn contains(&self, id: StructureId) -> bool {
        self.inside_table.get(id.get() as usize).copied().unwrap_or(false)
    }
}

/// A run and where it belongs inside its line.
#[derive(Debug)]
struct Slot {
    run: RunCandidate,
    /// `None` for attributed runs (ordered by `char_start`); `Some(is_left)`
    /// for a label attached to the line, left or right of its text.
    label: Option<bool>,
}

/// One visual line: the runs of one source node on one baseline, with any
/// labels attached to it.
#[derive(Debug)]
struct Line {
    slots: Vec<Slot>,
    table: bool,
}

impl Line {
    fn from_runs(runs: Vec<RunCandidate>, label: Option<bool>) -> Self {
        let table = runs.iter().any(|run| run.table);
        let slots = runs
            .into_iter()
            .map(|run| Slot { run, label })
            .collect();
        Self { slots, table }
    }

    fn y_min(&self) -> f64 {
        self.slots
            .iter()
            .map(|slot| slot.run.origin.y)
            .fold(f64::INFINITY, f64::min)
    }

    fn x_min(&self) -> f64 {
        self.slots
            .iter()
            .map(|slot| slot.run.x_min())
            .fold(f64::INFINITY, f64::min)
    }

    fn x_max(&self) -> f64 {
        self.slots
            .iter()
            .map(|slot| slot.run.x_max())
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// Horizontal extent of the attributed runs on one baseline band.
    fn attributed_extent_on(&self, band: i64) -> Option<(f64, f64)> {
        let mut extent: Option<(f64, f64)> = None;
        for slot in &self.slots {
            if slot.label.is_some() || baseline_band(slot.run.origin.y) != band {
                continue;
            }
            extent = Some(match extent {
                None => (slot.run.x_min(), slot.run.x_max()),
                Some((lo, hi)) => (lo.min(slot.run.x_min()), hi.max(slot.run.x_max())),
            });
        }
        extent
    }
}

/// Order one page's runs into reading order and return them grouped per
/// visual line, each line's runs already in reading order.
pub(crate) fn reading_order(
    candidates: Vec<RunCandidate>,
    page_width: f64,
) -> Vec<Vec<RunCandidate>> {
    // 1. Attributed runs group by source node; unattributed runs wait for
    //    attachment to a line.
    let mut nodes: Vec<(u32, Vec<RunCandidate>)> = Vec::new();
    let mut labels: Vec<RunCandidate> = Vec::new();
    for run in candidates {
        match run.node {
            Some(node) => match nodes.iter_mut().find(|(id, _)| *id == node) {
                Some((_, runs)) => runs.push(run),
                None => nodes.push((node, vec![run])),
            },
            None => labels.push(run),
        }
    }

    // 2. A node wraps over several baselines: one line per baseline band,
    //    its attributed runs kept in char order.
    let mut lines: Vec<Line> = Vec::new();
    for (_, mut runs) in nodes {
        runs.sort_by_key(|run| (baseline_band(run.origin.y), run.char_start));
        let mut start = 0;
        while start < runs.len() {
            let band = baseline_band(runs[start].origin.y);
            let end = runs[start..]
                .iter()
                .position(|run| baseline_band(run.origin.y) != band)
                .map_or(runs.len(), |offset| start + offset);
            lines.push(Line::from_runs(runs[start..end].to_vec(), None));
            start = end;
        }
    }

    // 3. Attach labels to the nearest attributed line on the same baseline.
    let mut standalone: Vec<RunCandidate> = Vec::new();
    for label in labels {
        let band = baseline_band(label.origin.y);
        let gap_limit = ATTACH_GAP_FACTOR * label.font_size.max(1.0);
        let mut best: Option<(usize, f64, bool)> = None;
        for (index, line) in lines.iter().enumerate() {
            let Some((lo, hi)) = line.attributed_extent_on(band) else {
                continue;
            };
            let (gap, left) = if label.x_max() <= lo {
                (lo - label.x_max(), true)
            } else if label.x_min() >= hi {
                (label.x_min() - hi, false)
            } else {
                (0.0, label.x_min() < (lo + hi) / 2.0)
            };
            if gap <= gap_limit && best.is_none_or(|(_, best_gap, _)| gap < best_gap) {
                best = Some((index, gap, left));
            }
        }
        match best {
            Some((index, _, left)) => {
                lines[index].slots.push(Slot { run: label, label: Some(left) });
            }
            None => standalone.push(label),
        }
    }
    // Unattached runs on one baseline read left to right as a single line.
    let mut groups: Vec<(i64, Vec<RunCandidate>)> = Vec::new();
    for run in standalone {
        let band = baseline_band(run.origin.y);
        match groups.iter_mut().find(|(b, _)| *b == band) {
            Some((_, runs)) => runs.push(run),
            None => groups.push((band, vec![run])),
        }
    }
    for (_, runs) in groups {
        lines.push(Line::from_runs(runs, Some(false)));
    }

    // 4. Table lines and wide lines split the page into strips; body lines
    //    inside a strip read by column band when the strip has several.
    lines.sort_by(|a, b| line_key(a).partial_cmp(&line_key(b)).unwrap_or(std::cmp::Ordering::Equal));
    let wide_limit = WIDE_FRACTION * page_width;
    let mut ordered: Vec<Line> = Vec::with_capacity(lines.len());
    let mut strip: Vec<Line> = Vec::new();
    for line in lines {
        if line.table || line.x_max() - line.x_min() >= wide_limit {
            flush_strip(&mut strip, &mut ordered);
            ordered.push(line);
        } else {
            strip.push(line);
        }
    }
    flush_strip(&mut strip, &mut ordered);

    // 5. Within a line: labels on the left (by x), attributed runs in char
    //    order, labels on the right (by x).
    ordered
        .into_iter()
        .map(|line| {
            let mut slots = line.slots;
            slots.sort_by(|a, b| slot_key(a).partial_cmp(&slot_key(b)).unwrap_or(std::cmp::Ordering::Equal));
            slots.into_iter().map(|slot| slot.run).collect()
        })
        .collect()
}

/// Top-down, then left-to-right sort key of a line.
fn line_key(line: &Line) -> (f64, f64) {
    (line.y_min(), line.x_min())
}

fn slot_key(slot: &Slot) -> (i64, u8, f64) {
    let rank = match slot.label {
        Some(true) => 0,
        None => 1,
        Some(false) => 2,
    };
    let position = match slot.label {
        None => f64::from(slot.run.char_start),
        _ => slot.run.origin.x,
    };
    (baseline_band(slot.run.origin.y), rank, position)
}

/// Append a strip of body lines in reading order: by column band when its
/// text forms two or more horizontally disjoint bands, else top-down.
fn flush_strip(strip: &mut Vec<Line>, ordered: &mut Vec<Line>) {
    let strip_lines = std::mem::take(strip);
    if strip_lines.is_empty() {
        return;
    }
    let bands = cluster_bands(&strip_lines);
    let multi_band = bands.len() >= 2;
    let mut indexed: Vec<(usize, Line)> = strip_lines
        .into_iter()
        .map(|line| {
            let band = bands
                .iter()
                .position(|(lo, hi)| line.x_min() >= *lo && line.x_min() <= *hi)
                .unwrap_or(0);
            (band, line)
        })
        .collect();
    indexed.sort_by(|a, b| {
        let ka = (if multi_band { a.0 } else { 0 }, line_key(&a.1));
        let kb = (if multi_band { b.0 } else { 0 }, line_key(&b.1));
        ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
    });
    ordered.extend(indexed.into_iter().map(|(_, line)| line));
}

/// Cluster line x-extents into bands separated by at least [`COLUMN_GAP_PT`].
fn cluster_bands(lines: &[Line]) -> Vec<(f64, f64)> {
    let mut extents: Vec<(f64, f64)> = lines.iter().map(|l| (l.x_min(), l.x_max())).collect();
    extents.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut bands: Vec<(f64, f64)> = Vec::new();
    for (lo, hi) in extents {
        match bands.last_mut() {
            Some((_, band_hi)) if lo - *band_hi < COLUMN_GAP_PT => *band_hi = band_hi.max(hi),
            _ => bands.push((lo, hi)),
        }
    }
    bands
}
