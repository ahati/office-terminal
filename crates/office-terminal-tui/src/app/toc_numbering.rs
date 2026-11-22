//! Hierarchical section numbers for document outlines (PDF bookmarks).
//!
//! Outline entries carry a raw `level` — the 0-based depth in the PDF outline
//! tree. [`tree_numbers`] turns that level column into stable, human-readable
//! tree numbers (`1`, `1.1`, `1.2`, `2`, `2.1`, …) for the TOC modal, together
//! with a normalized depth so indentation and numbering always agree.

/// Pre-order tree numbers for an outline level column.
///
/// Both returned vectors are index-aligned with `levels`: the dotted number and
/// the normalized depth (0 for top-level entries).
///
/// The parent of an entry is the closest preceding entry with a strictly
/// smaller raw level — the same relation the outline tree itself uses. That
/// keeps numbering sane when an outline skips a level (`0 -> 2` numbers as
/// `1.1`, not `1.0.1`) and when it starts below level 0. Numbers depend only on
/// the outline order, so they stay stable across filtering and collapsing.
pub fn tree_numbers(levels: &[usize]) -> (Vec<String>, Vec<usize>) {
    // Open ancestor chain: (raw level, normalized depth, number, children seen).
    let mut stack: Vec<(usize, usize, String, usize)> = Vec::new();
    let mut roots = 0usize;
    let mut numbers = Vec::with_capacity(levels.len());
    let mut depths = Vec::with_capacity(levels.len());

    for &raw in levels {
        while stack.last().is_some_and(|node| node.0 >= raw) {
            stack.pop();
        }
        let (depth, number) = match stack.last_mut() {
            None => {
                roots += 1;
                (0, roots.to_string())
            }
            Some(parent) => {
                parent.3 += 1;
                (parent.1 + 1, format!("{}.{}", parent.2, parent.3))
            }
        };
        stack.push((raw, depth, number.clone(), 0));
        depths.push(depth);
        numbers.push(number);
    }

    (numbers, depths)
}
