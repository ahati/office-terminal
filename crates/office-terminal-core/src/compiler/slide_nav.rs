//! Slide-part (relationship Target) resolution for internal navigation.
//!
//! `ppaction://hlinksldjump` names a slide through its relationship Target —
//! authored relative to the slide's own part (`../slides/slide3.xml`), rooted
//! at the package (`/ppt/slides/slide3.xml`) or already canonical when inherited
//! from a master/layout (`ppt/slides/slide3.xml`). Resolution is a normalized
//! part-name lookup against the deck slice, so the index is the slide's
//! `sldIdLst` position, never its `Slide::index` field.
//!
//! ref `packages/pptx/src/slide-nav.ts:24-70` (`buildSlidePartIndex`,
//! `resolveSlidePartTarget`) and `packages/core/src/nav/internal-target.ts:28-46`
//! (`resolveOpcPartName`).

use pptx_model::Slide;

/// Resolve an internal slide part reference to its 0-based presentation index.
///
/// `source_part` is the referencing slide's part name; its directory is the base
/// for relative targets. A root-absolute (`/…`) or canonical (`ppt/…`) target
/// ignores that base. Duplicate part names resolve to the earliest slide
/// (`sldIdLst` scan order). ref slide-nav.ts:32-70
pub fn resolve_internal_slide(raw_target: &str, source_part: &str, deck: &[Slide]) -> Option<usize> {
    if deck.is_empty() {
        return None;
    }
    let target = raw_target.trim();
    if target.is_empty() || target.contains("://") || target.starts_with("mailto:") {
        return None;
    }

    if let Some(normalized) = normalize_part_name(source_part, target) {
        if let Some(index) = deck.iter().position(|slide| {
            slide
                .part_name
                .as_deref()
                .is_some_and(|part| part.trim_start_matches('/').eq_ignore_ascii_case(&normalized))
        }) {
            return Some(index);
        }
    }

    // Tolerate producers that stamp only the basename ("slide5.xml") or a
    // path relative to the package root without the "ppt/" prefix.
    let basename = target
        .split('#')
        .next()
        .unwrap_or(target)
        .rsplit('/')
        .next()
        .unwrap_or("");
    if basename.is_empty() {
        return None;
    }
    let mut matches = deck.iter().filter(|slide| {
        slide
            .part_name
            .as_deref()
            .and_then(|part| part.rsplit('/').next())
            .is_some_and(|name| name.eq_ignore_ascii_case(basename))
    });
    let first = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    deck.iter().position(|slide| slide.index == first.index)
}

/// Whether a target looks like an XML part name (used to drop an unresolvable
/// slide jump silently rather than reporting it as an invalid URL).
pub(crate) fn looks_like_slide_part(target: &str) -> bool {
    let path = target.split('#').next().unwrap_or(target);
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".xml") && (lower.contains("slide") || lower.contains("slides/") || lower.contains("/ppt/"))
}

/// Normalize an OPC relationship target to a root-relative part name.
/// ref internal-target.ts:28-46 (`resolveOpcPartName`)
fn normalize_part_name(source_part: &str, target: &str) -> Option<String> {
    let target = target.split('#').next().unwrap_or(target);
    if target.is_empty() {
        return None;
    }
    let source = source_part.trim_start_matches('/');
    let mut parts: Vec<&str> = if target.starts_with('/') || target.starts_with("ppt/") {
        Vec::new()
    } else {
        source
            .rsplit_once('/')
            .map(|(directory, _)| directory)
            .unwrap_or("")
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect()
    };
    for segment in target.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slide(index: usize, part: &str) -> Slide {
        serde_json::from_value(serde_json::json!({
            "index": index,
            "slideNumber": index + 1,
            "partName": part,
            "elements": [],
            "elementSources": [],
            "comments": []
        }))
        .expect("slide JSON")
    }

    #[test]
    fn relative_root_absolute_and_canonical_targets_normalize_identically() {
        let deck = vec![slide(0, "ppt/slides/slide1.xml"), slide(1, "ppt/slides/slide2.xml")];
        for target in [
            "../slides/slide2.xml",
            "/ppt/slides/slide2.xml",
            "ppt/slides/slide2.xml",
            "slide2.xml",
        ] {
            assert_eq!(
                resolve_internal_slide(target, "ppt/slides/slide1.xml", &deck),
                Some(1),
                "{target}"
            );
        }
        assert_eq!(
            resolve_internal_slide("../slides/slide9.xml", "ppt/slides/slide1.xml", &deck),
            None
        );
    }
}
