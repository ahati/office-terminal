//! Metafile AST/PNG golden tests and coverage ledger checker (PRD 13a §4, §5, §9, §10).

#![cfg(feature = "metafile")]

mod metafile_support;
#[path = "parity_support/pixelmatch.rs"]
mod pixelmatch;

use std::fs;
use std::path::PathBuf;

use metafile_support as support;
use office_terminal_core::contracts::{
    FillRule, PathSegment, RenderTargetSpec, VectorCommand, VectorPath, VectorSlide, WarningEvent,
};
use office_terminal_core::metafile;
use office_terminal_core::{SlideRasterizer, TinySkiaRasterizer};
use pixelmatch::{compare_rgba, DiffOptions};
use serde::Deserialize;
use serde_json::{json, Value};
use support::ast::{command_to_ast, compare_ast_commands, scene_to_ast};

const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/metafile");

#[derive(Debug, Deserialize)]
struct Manifest {
    fixtures: Vec<FixtureEntry>,
}

#[derive(Debug, Deserialize)]
struct FixtureEntry {
    id: String,
    path: String,
    #[allow(dead_code)]
    format: String,
    expect: Expectation,
}

#[derive(Debug, Deserialize)]
struct Expectation {
    class: String,
    #[serde(default)]
    diagnostics: Vec<String>,
    ast: String,
    png: Vec<PngTarget>,
}

#[derive(Debug, Deserialize)]
struct PngTarget {
    target: String,
    #[serde(default = "default_pixelmatch_max")]
    pixelmatch_max: f64,
}

fn default_pixelmatch_max() -> f64 {
    0.005
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(FIXTURE_DIR)
}

fn mime_for_path(path: &str) -> &'static str {
    if path.ends_with(".wmf") || path.ends_with(".wmz") {
        "image/x-wmf"
    } else {
        "image/x-emf"
    }
}

fn expand_fixture(
    rel_id: &str,
    mime: &str,
    bytes: Vec<u8>,
) -> (VectorSlide, metafile::MetafileExpansionSummary, Vec<WarningEvent>) {
    metafile::clear_global_cache();
    let (compile_tx, _compile_rx) = crossbeam_channel::unbounded();
    let mut scene = support::compile_slide(&support::picture_slide(rel_id, mime), &compile_tx);
    let store = support::FixtureStore::new(rel_id, bytes);
    let (tx, rx) = crossbeam_channel::unbounded();
    let summary = metafile::expand_metafiles(&mut scene, &store, &tx);
    (scene, summary, rx.try_iter().collect())
}

fn assert_clip_balance(scene: &VectorSlide, fixture_id: &str) {
    let mut clip_depth = 0;
    for cmd in &scene.commands {
        match cmd {
            VectorCommand::PushClip(_) => clip_depth += 1,
            VectorCommand::PopClip => {
                assert!(
                    clip_depth > 0,
                    "PopClip without matching PushClip in fixture '{fixture_id}'"
                );
                clip_depth -= 1;
            }
            _ => {}
        }
    }
    assert_eq!(
        clip_depth, 0,
        "Unclosed PushClip at end of scene in fixture '{fixture_id}'"
    );
}

fn assert_finite_transforms(scene: &VectorSlide, fixture_id: &str) {
    for cmd in &scene.commands {
        let t = match cmd {
            VectorCommand::DrawPath { transform, .. }
            | VectorCommand::DrawLineEnd { transform, .. }
            | VectorCommand::DrawText { transform, .. }
            | VectorCommand::DrawImage { transform, .. }
            | VectorCommand::DrawMetafile { transform, .. } => transform,
            VectorCommand::DrawBitmap { placement, .. } => placement,
            _ => continue,
        };
        assert!(
            t.sx.is_finite()
                && t.ky.is_finite()
                && t.kx.is_finite()
                && t.sy.is_finite()
                && t.tx.is_finite()
                && t.ty.is_finite(),
            "Non-finite transform {t:?} in fixture '{fixture_id}'"
        );
    }
}

fn assert_no_surviving_draw_metafile(scene: &VectorSlide, fixture_id: &str) {
    let surviving = scene
        .commands
        .iter()
        .any(|c| matches!(c, VectorCommand::DrawMetafile { .. }));
    assert!(
        !surviving,
        "DrawMetafile marker survived expansion in fixture '{fixture_id}'"
    );
}

fn extract_diagnostics(warnings: &[WarningEvent]) -> Vec<String> {
    let mut tags = Vec::new();
    for w in warnings {
        match w {
            WarningEvent::FidelityDegraded { feature, .. } => tags.push(feature.to_string()),
            WarningEvent::FontSubstituted { .. } => tags.push("FontSubstituted".to_string()),
            WarningEvent::ResourceLimitHit { .. } => tags.push("ResourceLimitHit".to_string()),
            WarningEvent::MediaFailed { .. } => tags.push("MediaFailed".to_string()),
            WarningEvent::UnsupportedPreset { name } => tags.push(name.clone()),
        }
    }
    tags.sort();
    tags.dedup();
    tags
}

fn parse_target(target: &str) -> (u32, u32) {
    let parts: Vec<&str> = target.split('x').collect();
    let w: u32 = parts[0].parse().expect("target width");
    let h: u32 = parts[1].parse().expect("target height");
    (w, h)
}

fn rasterize_scene(scene: &VectorSlide, w: u32, h: u32, store: &support::FixtureStore) -> image::RgbaImage {
    let target = RenderTargetSpec {
        width_px: w,
        height_px: h,
    };
    let bitmap = TinySkiaRasterizer::new()
        .rasterize(scene, &target, store)
        .expect("rasterize");
    image::RgbaImage::from_raw(w, h, bitmap.pixels.to_vec()).expect("rgba image")
}

#[test]
fn test_deliberate_divergence_fails_with_first_command_and_fixture_name() {
    let fixture_id = "test-divergence-fixture";
    let cmd1 = command_to_ast(&VectorCommand::PopClip);
    let cmd2 = command_to_ast(&VectorCommand::PushClip(VectorPath::new(
        vec![PathSegment::MoveTo { x: 0.0, y: 0.0 }],
        FillRule::NonZero,
    )));

    let ast1 = json!({ "widthEmu": 100, "heightEmu": 100, "commands": [cmd1.clone(), cmd2] });
    let ast2 = json!({ "widthEmu": 100, "heightEmu": 100, "commands": [cmd1, cmd1] });

    let err = compare_ast_commands(&ast1, &ast2, fixture_id).unwrap_err();
    assert!(
        err.contains("test-divergence-fixture"),
        "error must cite fixture name: {err}"
    );
    assert!(
        err.contains("command #1"),
        "error must cite first divergent command index: {err}"
    );
}

#[test]
fn test_metafile_ast_and_png_goldens() {
    let dir = fixture_dir();
    support::generate::ensure_synthetic_fixtures(&dir).expect("ensure synthetic fixtures");

    let manifest_bytes = fs::read(dir.join("manifest.json")).expect("read manifest.json");
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes).expect("parse manifest.json");

    let update = std::env::var("OT_UPDATE_METAFILE_GOLDENS").is_ok()
        || std::env::var("KITTY_PPTX_UPDATE_METAFILE_GOLDENS").is_ok();

    for f in &manifest.fixtures {
        let fixture_path = dir.join(&f.path);
        let bytes = fs::read(&fixture_path).unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", f.path));
        let mime = mime_for_path(&f.path);

        let (scene, summary, warnings) = expand_fixture(&f.path, mime, bytes.clone());

        assert_clip_balance(&scene, &f.id);
        assert_finite_transforms(&scene, &f.id);
        assert_no_surviving_draw_metafile(&scene, &f.id);

        let actual_class = if summary.placeholders > 0 {
            "placeholder"
        } else if !warnings.is_empty() {
            "degraded"
        } else {
            "full"
        };
        assert_eq!(actual_class, f.expect.class, "Class mismatch for fixture '{}'", f.id);

        let mut actual_tags = extract_diagnostics(&warnings);
        let mut expected_tags = f.expect.diagnostics.clone();
        actual_tags.sort();
        expected_tags.sort();
        assert_eq!(
            actual_tags, expected_tags,
            "Diagnostics mismatch for fixture '{}': expected {:?}, got {:?}",
            f.id, expected_tags, actual_tags
        );

        let actual_ast = scene_to_ast(&scene);
        let ast_file = dir.join(&f.expect.ast);

        if update {
            if let Some(parent) = ast_file.parent() {
                fs::create_dir_all(parent).expect("create ast dir");
            }
            let formatted = serde_json::to_string_pretty(&actual_ast).expect("pretty print ast");
            fs::write(&ast_file, formatted).expect("write ast golden");
        } else {
            let expected_bytes =
                fs::read(&ast_file).unwrap_or_else(|e| panic!("missing AST golden {}: {e}", ast_file.display()));
            let expected_ast: Value = serde_json::from_slice(&expected_bytes).expect("parse expected AST");
            if let Err(diff) = compare_ast_commands(&actual_ast, &expected_ast, &f.id) {
                panic!("{diff}");
            }
        }

        let store = support::FixtureStore::new(&f.path, bytes);
        for png_target in &f.expect.png {
            let (w, h) = parse_target(&png_target.target);
            let actual_img = rasterize_scene(&scene, w, h, &store);
            let png_file = dir.join(format!("png/{}-{}.png", f.id, png_target.target));

            if update {
                if let Some(parent) = png_file.parent() {
                    fs::create_dir_all(parent).expect("create png dir");
                }
                actual_img.save(&png_file).expect("save golden png");
            } else {
                let golden_img = image::open(&png_file)
                    .unwrap_or_else(|e| panic!("missing PNG golden {}: {e}", png_file.display()))
                    .to_rgba8();
                assert_eq!(
                    actual_img.dimensions(),
                    golden_img.dimensions(),
                    "Dimension mismatch for fixture '{}' target '{}'",
                    f.id,
                    png_target.target
                );

                let diff = compare_rgba(
                    &actual_img.into_raw(),
                    &golden_img.into_raw(),
                    w,
                    h,
                    &DiffOptions::default(),
                );
                let max_pct = png_target.pixelmatch_max * 100.0;
                assert!(
                    diff.diff_pct <= max_pct,
                    "PNG golden mismatch for '{}' at {}: diff {:.4}% exceeds max {:.4}%",
                    f.id,
                    png_target.target,
                    diff.diff_pct,
                    max_pct
                );
            }
        }
    }
}

#[test]
fn test_coverage_table_consistency() {
    let coverage_path = fixture_dir().join("coverage.md");
    let content = fs::read_to_string(&coverage_path).expect("read coverage.md");

    let manifest_bytes = fs::read(fixture_dir().join("manifest.json")).expect("manifest");
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes).expect("manifest");
    let manifest_ids: Vec<&str> = manifest.fixtures.iter().map(|f| f.id.as_str()).collect();

    let mut in_table = false;
    let mut rows_checked = 0;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("| Family | Fixture |") {
            in_table = true;
            continue;
        }
        if in_table {
            if trimmed.is_empty() || trimmed.starts_with('#') {
                in_table = false;
                continue;
            }
            if trimmed.starts_with("| :---") {
                continue;
            }
            let cols: Vec<&str> = trimmed.split('|').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
            if cols.len() >= 6 {
                let family = cols[0];
                let fixture = cols[1].trim_matches('`');
                let ast = cols[3].trim_matches('`');
                let png = cols[4].trim_matches('`');
                let status = cols[5];

                assert_eq!(status, "✅", "Row '{family}' must be promoted to ✅, but is '{status}'");
                assert!(fixture != "—" && !fixture.is_empty(), "Row '{family}' has no fixture");
                assert!(ast != "—" && !ast.is_empty(), "Row '{family}' has no AST");
                assert!(png != "—" && !png.is_empty(), "Row '{family}' has no PNG");

                let ast_path = fixture_dir().join(ast);
                assert!(
                    ast_path.exists(),
                    "Row '{family}': AST file '{}' does not exist",
                    ast_path.display()
                );

                let png_path = fixture_dir().join(png);
                assert!(
                    png_path.exists(),
                    "Row '{family}': PNG file '{}' does not exist",
                    png_path.display()
                );

                let base_name = fixture
                    .strip_suffix(".emf")
                    .or_else(|| fixture.strip_suffix(".wmf"))
                    .or_else(|| fixture.strip_suffix(".emz"))
                    .or_else(|| fixture.strip_suffix(".wmz"))
                    .unwrap_or(fixture);

                assert!(
                    manifest_ids.contains(&base_name) || manifest_ids.contains(&fixture),
                    "Row '{family}': fixture '{fixture}' not found in manifest"
                );

                rows_checked += 1;
            }
        }
    }

    assert!(
        rows_checked >= 40,
        "Expected at least 40 matrix rows checked in coverage.md, found {rows_checked}"
    );
}
