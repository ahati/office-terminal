use super::geometry::RectF32;
use serde::{Deserialize, Serialize};

/// Maximum accepted hyperlink URI length in bytes.
pub const MAX_HYPERLINK_URL_LEN: usize = 2048;

/// Semantic destination of an interactive hyperlink.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HyperlinkTarget {
    /// Validated external web or communication URI (https://, http://, mailto:).
    ExternalUri(String),
    /// 0-indexed chronological slide number in the presentation.
    SlideIndex(usize),
    /// Native PowerPoint presentation control action.
    Action(SlideAction),
}

/// Standard presentation control actions from `ppaction://` verbs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SlideAction {
    NextSlide,
    PreviousSlide,
    FirstSlide,
    LastSlide,
    EndShow,
}

/// Rich hyperlink annotation anchoring an interactive target to a slide coordinate region.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SlideLink {
    pub target: HyperlinkTarget,
    pub bounds_emu: RectF32,
    pub tooltip: Option<String>,
}

/// PRD 10 §3.1 name for the same descriptor.
pub type HyperlinkDescriptor = SlideLink;

impl SlideLink {
    pub fn new(target: HyperlinkTarget, bounds_emu: RectF32, tooltip: Option<String>) -> Self {
        Self {
            target,
            bounds_emu,
            tooltip,
        }
    }
}

/// Errors produced by the URL security gate (PRD 10 §4.3).
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum SecurityError {
    #[error("hyperlink URL is empty")]
    EmptyUrl,
    #[error("hyperlink URL exceeds {MAX_HYPERLINK_URL_LEN} bytes")]
    UrlTooLong,
    #[error("hyperlink URL contains ASCII control characters")]
    IllegalControlCharacters,
    #[error("invalid hyperlink URI: {0}")]
    InvalidUri(String),
    #[error("forbidden hyperlink scheme: {0}")]
    ForbiddenScheme(&'static str),
    #[error("unsupported hyperlink scheme: {0}")]
    UnsupportedScheme(String),
}

/// Validate and normalize an untrusted hyperlink target before it is exposed to
/// OSC 8, SVG, or an OS subprocess (PRD 10 §4.3).
///
/// Only `https`, `http` and `mailto` are accepted. ASCII control characters and
/// terminal escape injection are rejected outright, as are over-long URLs.
pub fn sanitize_hyperlink_url(raw_url: &str) -> Result<String, SecurityError> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return Err(SecurityError::EmptyUrl);
    }
    if trimmed.len() > MAX_HYPERLINK_URL_LEN {
        return Err(SecurityError::UrlTooLong);
    }
    if trimmed.bytes().any(|b| b < 0x20 || b == 0x7F) {
        return Err(SecurityError::IllegalControlCharacters);
    }

    let Some(colon) = trimmed.find(':') else {
        return Err(SecurityError::InvalidUri("URI has no scheme component".to_string()));
    };
    if trimmed[..colon].chars().any(|ch| ch == '/' || ch == '?' || ch == '#') {
        return Err(SecurityError::InvalidUri("URI has no scheme component".to_string()));
    }
    let scheme = &trimmed[..colon];
    if scheme.is_empty()
        || !scheme.chars().next().is_some_and(|ch| ch.is_ascii_alphabetic())
        || !scheme
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
    {
        return Err(SecurityError::InvalidUri(format!(
            "invalid scheme component '{scheme}'"
        )));
    }

    let rest = &trimmed[colon + 1..];
    match scheme.to_ascii_lowercase().as_str() {
        "http" | "https" => {
            let authority = rest.strip_prefix("//").unwrap_or("");
            if authority.is_empty() || authority.starts_with('/') {
                return Err(SecurityError::InvalidUri("http(s) URI has no authority".to_string()));
            }
            Ok(trimmed.to_string())
        }
        "mailto" => {
            if rest.trim().is_empty() {
                return Err(SecurityError::InvalidUri("mailto URI has no address".to_string()));
            }
            Ok(trimmed.to_string())
        }
        "file" => Err(SecurityError::ForbiddenScheme("file:// access blocked")),
        "javascript" => Err(SecurityError::ForbiddenScheme("javascript:// execution blocked")),
        "data" => Err(SecurityError::ForbiddenScheme("data:// URIs blocked")),
        "shell" | "cmd" | "powershell" => Err(SecurityError::ForbiddenScheme("shell execution blocked")),
        other => Err(SecurityError::UnsupportedScheme(other.to_string())),
    }
}

/// One hyperlink together with its stable 1-based hotkey id.
#[derive(Clone, Debug, PartialEq)]
pub struct IndexedHyperlink {
    pub id: usize,
    pub link: SlideLink,
}

/// Build-once spatial index over a slide's hyperlinks (PRD 10 §3.4).
///
/// Links are kept in source order (later links win when bounds overlap, matching
/// the display list's z-order) while a secondary index sorted by the rectangle's
/// top edge allows hit tests to stop early.
#[derive(Clone, Debug, Default)]
pub struct SlideLinkIndex {
    links: Vec<IndexedHyperlink>,
    y_order: Vec<usize>,
}

impl SlideLinkIndex {
    pub fn build(links: impl IntoIterator<Item = SlideLink>) -> Self {
        let links: Vec<IndexedHyperlink> = links
            .into_iter()
            .enumerate()
            .map(|(index, link)| IndexedHyperlink { id: index + 1, link })
            .collect();
        let mut y_order: Vec<usize> = (0..links.len()).collect();
        y_order.sort_by(|&a, &b| {
            links[a]
                .link
                .bounds_emu
                .y
                .partial_cmp(&links[b].link.bounds_emu.y)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Self { links, y_order }
    }

    pub fn iter(&self) -> impl Iterator<Item = &IndexedHyperlink> {
        self.links.iter()
    }

    pub fn len(&self) -> usize {
        self.links.len()
    }

    pub fn is_empty(&self) -> bool {
        self.links.is_empty()
    }

    /// Return the topmost hyperlink containing the given slide EMU point.
    pub fn hit_test(&self, x_emu: f32, y_emu: f32) -> Option<&SlideLink> {
        self.hit_test_indexed(x_emu, y_emu).map(|link| &link.link)
    }

    /// Return the topmost indexed hyperlink containing the given slide EMU point.
    pub fn hit_test_indexed(&self, x_emu: f32, y_emu: f32) -> Option<&IndexedHyperlink> {
        let mut best: Option<&IndexedHyperlink> = None;
        for &index in &self.y_order {
            let candidate = &self.links[index];
            let bounds = candidate.link.bounds_emu;
            if y_emu < bounds.y {
                break;
            }
            if bounds.contains(x_emu, y_emu) && best.is_none_or(|current| candidate.id > current.id) {
                best = Some(candidate);
            }
        }
        best
    }

    /// All hyperlinks whose bounds intersect `rect`.
    pub fn links_in_rect(&self, rect: RectF32) -> Vec<&SlideLink> {
        let mut found: Vec<&SlideLink> = self
            .links
            .iter()
            .filter(|indexed| indexed.link.bounds_emu.intersects(&rect))
            .map(|indexed| &indexed.link)
            .collect();
        found.sort_by(|a, b| {
            a.bounds_emu
                .y
                .partial_cmp(&b.bounds_emu.y)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        found
    }

    pub fn indexed_links(&self) -> &[IndexedHyperlink] {
        &self.links
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> RectF32 {
        RectF32::new(x, y, w, h)
    }

    #[test]
    fn url_gate_allows_web_and_mail() {
        assert_eq!(
            sanitize_hyperlink_url(" https://example.com/path ").as_deref(),
            Ok("https://example.com/path")
        );
        assert!(sanitize_hyperlink_url("http://example.com").is_ok());
        assert!(sanitize_hyperlink_url("mailto:someone@example.com").is_ok());
    }

    #[test]
    fn url_gate_rejects_dangerous_schemes_and_injection() {
        assert!(matches!(
            sanitize_hyperlink_url("javascript:alert(1)"),
            Err(SecurityError::ForbiddenScheme(_))
        ));
        assert!(matches!(
            sanitize_hyperlink_url("file:///etc/passwd"),
            Err(SecurityError::ForbiddenScheme(_))
        ));
        assert!(matches!(
            sanitize_hyperlink_url("data:text/html,<script>"),
            Err(SecurityError::ForbiddenScheme(_))
        ));
        assert!(matches!(
            sanitize_hyperlink_url("https://example.com/\u{1b}]8;;evil"),
            Err(SecurityError::IllegalControlCharacters)
        ));
        assert!(matches!(
            sanitize_hyperlink_url(&format!("https://example.com/{}", "a".repeat(MAX_HYPERLINK_URL_LEN))),
            Err(SecurityError::UrlTooLong)
        ));
        assert!(matches!(
            sanitize_hyperlink_url("not-a-uri"),
            Err(SecurityError::InvalidUri(_))
        ));
    }

    #[test]
    fn spatial_index_hit_testing_prefers_latest_overlapping_link() {
        let index = SlideLinkIndex::build(vec![
            SlideLink::new(
                HyperlinkTarget::ExternalUri("https://a.example".into()),
                rect(0.0, 0.0, 100.0, 100.0),
                None,
            ),
            SlideLink::new(
                HyperlinkTarget::ExternalUri("https://b.example".into()),
                rect(40.0, 40.0, 20.0, 20.0),
                None,
            ),
        ]);

        assert_eq!(index.len(), 2);
        assert!(index.hit_test(10.0, 10.0).is_some());
        let hit = index.hit_test(50.0, 50.0).expect("overlap hit");
        assert_eq!(hit.target, HyperlinkTarget::ExternalUri("https://b.example".into()));
        assert!(index.hit_test(200.0, 200.0).is_none());
        assert_eq!(index.links_in_rect(rect(30.0, 30.0, 20.0, 20.0)).len(), 2);
        assert_eq!(index.indexed_links()[0].id, 1);
    }
}
