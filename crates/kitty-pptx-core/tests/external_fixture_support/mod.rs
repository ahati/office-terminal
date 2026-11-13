//! Shared manifest, cache, and image helpers for the opt-in external tier.
//!
//! The committed `fixtures.json` is the single source of truth: commit, paths,
//! slide counts, and strict/diagnostic policy all come from it, so adding a
//! fixture never requires editing this test.

pub mod metrics;
pub mod render;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

pub const MANIFEST_RELATIVE: &str = "tests/golden/external/fixtures.json";
pub const CACHE_RELATIVE: &str = "target/external";
pub const RUN_COMMAND: &str = "python3 crates/kitty-pptx-core/tests/golden/external/run.py";
pub const FAILURES_FILE: &str = "unrenderable.json";
/// Cap on names echoed into the summary; the counts are always exact.
pub const MAX_LISTED: usize = 12;

#[derive(Deserialize)]
pub struct Manifest {
    pub upstream: String,
    pub commit: String,
    pub license: String,
    pub render_width: u32,
    pub files: Vec<Fixture>,
    #[serde(default)]
    pub skipped: Vec<Skipped>,
}

#[derive(Deserialize)]
pub struct Fixture {
    pub name: String,
    pub path: String,
    pub sha256: String,
    pub slides: usize,
    pub strict: bool,
    pub note: String,
    pub thresholds: Option<Thresholds>,
}

#[derive(Deserialize)]
pub struct Skipped {
    pub name: String,
    pub path: String,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Thresholds {
    pub min_cell_pass_ratio: f64,
    pub max_global_mean_abs_diff: f64,
    pub max_ink_fraction_delta: f64,
}

impl Thresholds {
    pub const DEFAULT: Self = Self {
        min_cell_pass_ratio: 0.90,
        max_global_mean_abs_diff: 8.0,
        max_ink_fraction_delta: 0.03,
    };
}

impl Fixture {
    pub fn thresholds(&self) -> Thresholds {
        self.thresholds.unwrap_or(Thresholds::DEFAULT)
    }

    pub fn golden(&self, goldens_dir: &Path, slide: usize) -> PathBuf {
        goldens_dir
            .join(&self.name)
            .join(format!("slide_{slide:03}.png"))
    }
}

/// The subset of `stamp.json` that proves cached pixels match the corpus.
#[derive(Deserialize)]
struct Stamp {
    commit: String,
    render_width: u32,
    fixtures: Vec<StampFixture>,
}

#[derive(Deserialize)]
struct StampFixture {
    name: String,
    sha256: String,
    slides: usize,
}

/// Decks that exist in the manifest and have complete cached references.
pub struct Selection {
    pub compare: Vec<usize>,
    pub missing_goldens: Vec<String>,
    pub missing_fixtures: Vec<String>,
}

pub struct Corpus {
    pub manifest: Manifest,
    pub fixtures_dir: PathBuf,
    pub goldens_dir: PathBuf,
    /// Decks the reference viewer rejects, with the recorded reason.
    pub failures: BTreeMap<String, String>,
}

impl Corpus {
    /// Discover the corpus, returning a human-readable reason to self-skip when
    /// the cache is absent, stale, or has no rendered references yet.
    pub fn discover() -> Result<Self, String> {
        let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(MANIFEST_RELATIVE);
        let manifest = load_manifest(&manifest_path);
        let commit = manifest.commit.clone();
        let fixtures_dir = cache_dir(
            "KITTY_PPTX_EXTERNAL_FIXTURES",
            repo_root().join(CACHE_RELATIVE).join("fixtures").join(&commit),
        );
        let goldens_dir = cache_dir(
            "KITTY_PPTX_EXTERNAL_GOLDENS",
            repo_root().join(CACHE_RELATIVE).join("goldens").join(&commit),
        );

        if !fixtures_dir.is_dir() {
            return Err(unavailable(format!(
                "fixture cache is missing: {}",
                fixtures_dir.display()
            )));
        }
        if !manifest
            .files
            .iter()
            .any(|fixture| fixtures_dir.join(format!("{}.pptx", fixture.name)).is_file())
        {
            return Err(unavailable(format!(
                "fixture cache contains no manifest decks: {}",
                fixtures_dir.display()
            )));
        }
        if !goldens_dir.is_dir() {
            return Err(unavailable(format!(
                "reference cache is missing: {}",
                goldens_dir.display()
            )));
        }

        if let Some(stamp) = read_stamp(&goldens_dir)? {
            check_stamp(&stamp, &manifest, &goldens_dir)?;
        }

        Ok(Self {
            manifest,
            fixtures_dir,
            failures: read_failures(&goldens_dir),
            goldens_dir,
        })
    }

    /// Split manifest entries into comparable decks and the reasons the rest
    /// cannot be compared. Never prints: the caller owns the summary.
    pub fn select(&self) -> Selection {
        let mut compare = Vec::new();
        let mut missing_goldens = Vec::new();
        let mut missing_fixtures = Vec::new();

        for (index, fixture) in self.manifest.files.iter().enumerate() {
            let deck = self.fixtures_dir.join(format!("{}.pptx", fixture.name));
            if !deck.is_file() {
                missing_fixtures.push(fixture.name.clone());
                continue;
            }
            let complete = (1..=fixture.slides).all(|slide| {
                self.golden_is_present(fixture, slide)
            });
            if complete {
                compare.push(index);
            } else {
                missing_goldens.push(fixture.name.clone());
            }
        }

        Selection {
            compare,
            missing_goldens,
            missing_fixtures,
        }
    }

    fn golden_is_present(&self, fixture: &Fixture, slide: usize) -> bool {
        fixture
            .golden(&self.goldens_dir, slide)
            .metadata()
            .map(|metadata| metadata.len() > 0)
            .unwrap_or(false)
    }

    /// Names present in the manifest but with no complete references.
    pub fn unrenderable<'a>(&self, missing: &'a [String]) -> BTreeSet<&'a str> {
        missing
            .iter()
            .map(String::as_str)
            .filter(|name| self.failures.contains_key(*name))
            .collect()
    }
}

pub fn load_manifest(path: &Path) -> Manifest {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

fn read_stamp(goldens_dir: &Path) -> Result<Option<Stamp>, String> {
    let path = goldens_dir.join("stamp.json");
    if !path.is_file() {
        // A partial cache has no stamp; per-deck completeness still governs.
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path).map_err(|error| {
        unavailable(format!("failed to read {}: {error}", path.display()))
    })?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|error| unavailable(format!("{} is not a render stamp: {error}", path.display())))
}

/// A stamp only exists after a full render, so any identity mismatch means the
/// pixels are stale and the comparison must not trust them.
fn check_stamp(stamp: &Stamp, manifest: &Manifest, goldens_dir: &Path) -> Result<(), String> {
    if stamp.commit != manifest.commit || stamp.render_width != manifest.render_width {
        return Err(unavailable(format!(
            "references are {} at {}px, fixtures.json pins {} at {}px",
            stamp.commit, stamp.render_width, manifest.commit, manifest.render_width
        )));
    }
    let wanted: BTreeMap<&str, (&str, usize)> = manifest
        .files
        .iter()
        .map(|fixture| (fixture.name.as_str(), (fixture.sha256.as_str(), fixture.slides)))
        .collect();
    let matches = stamp.fixtures.len() == wanted.len()
        && stamp.fixtures.iter().all(|entry| {
            wanted.get(entry.name.as_str())
                == Some(&(entry.sha256.as_str(), entry.slides))
        });
    if !matches {
        return Err(unavailable(format!(
            "references in {} do not match fixtures.json",
            goldens_dir.display()
        )));
    }
    Ok(())
}

fn read_failures(goldens_dir: &Path) -> BTreeMap<String, String> {
    let path = goldens_dir.join(FAILURES_FILE);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return BTreeMap::new();
    };
    let Ok(data) = serde_json::from_str::<BTreeMap<String, String>>(&text) else {
        return BTreeMap::new();
    };
    data
}

fn unavailable(reason: String) -> String {
    format!("{reason}; run `{RUN_COMMAND}`")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate must live under <repo>/crates")
        .to_path_buf()
}

fn cache_dir(variable: &str, fallback: PathBuf) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or(fallback)
}
