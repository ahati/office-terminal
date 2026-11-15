//! Replaces `DrawMetafile` markers with playback output (PRD 13 §3.1).

use crossbeam_channel::Sender;

use crate::contracts::{FidelityClass, VectorCommand, VectorSlide, WarningEvent};
use crate::loader::AssetStore;

use super::cache::global_resolver;
use super::diagnostics::dedup;
use super::engine_metafile_rs::MetafileRsEngine;
use super::limits::MetafileLimits;
use super::mapping;
use super::warning_for;

/// Counts produced by one slide expansion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MetafileExpansionSummary {
    pub expanded: usize,
    pub placeholders: usize,
    pub warnings: usize,
}

/// Replaces every `VectorCommand::DrawMetafile` in `scene` with playback
/// output, or the standard metafile placeholder when playback fails.
///
/// Runs where both the display list and the `AssetStore` are available:
/// the render pipeline, the CLI export path, and the golden harness.
pub fn expand_metafiles(
    scene: &mut VectorSlide,
    assets: &dyn AssetStore,
    warnings: &Sender<WarningEvent>,
) -> MetafileExpansionSummary {
    let mut summary = MetafileExpansionSummary::default();
    if !scene
        .commands
        .iter()
        .any(|command| matches!(command, VectorCommand::DrawMetafile { .. }))
    {
        // Fast path: the overwhelming majority of slides carry no metafiles.
        return summary;
    }
    let engine = MetafileRsEngine;
    let limits = MetafileLimits::default();
    let resolver = global_resolver();
    let mut expanded_commands = Vec::with_capacity(scene.commands.len());

    for command in std::mem::take(&mut scene.commands) {
        let (rel_id, transform, src_rect, dest_rect, opacity) = match command {
            VectorCommand::DrawMetafile {
                rel_id,
                transform,
                src_rect,
                dest_rect,
                opacity,
            } => (rel_id, transform, src_rect, dest_rect, opacity),
            other => {
                expanded_commands.push(other);
                continue;
            }
        };

        match resolver.resolve(
            &rel_id, assets, &engine, &limits, &src_rect, &dest_rect,
        ) {
            Ok(compiled) if !compiled.commands.is_empty() => {
                for command in &compiled.commands {
                    expanded_commands
                        .push(mapping::transform_command(command, &transform));
                }
                let element_id = element_id(&rel_id);
                for diagnostic in dedup(compiled.diagnostics.clone()) {
                    let _ = warnings.send(warning_for(element_id.clone(), &diagnostic));
                    summary.warnings += 1;
                }
                if opacity < 1.0 {
                    let _ = warnings.send(WarningEvent::FidelityDegraded {
                        element_id,
                        feature: "metafile_opacity",
                        class: FidelityClass::Degraded,
                    });
                    summary.warnings += 1;
                }
                summary.expanded += 1;
            }
            Ok(compiled) => {
                // Playback succeeded but produced no ink (e.g. an empty
                // metafile); a placeholder is still required (PRD 13 §7).
                let element_id = element_id(&rel_id);
                for diagnostic in dedup(compiled.diagnostics.clone()) {
                    let _ = warnings.send(warning_for(element_id.clone(), &diagnostic));
                    summary.warnings += 1;
                }
                emit_placeholder(
                    &mut expanded_commands,
                    &mut summary,
                    warnings,
                    &rel_id,
                    dest_rect,
                    transform,
                    "metafile produced no drawable content",
                );
            }
            Err(error) => {
                emit_placeholder(
                    &mut expanded_commands,
                    &mut summary,
                    warnings,
                    &rel_id,
                    dest_rect,
                    transform,
                    &error.to_string(),
                );
            }
        }
    }

    scene.commands = expanded_commands;
    summary
}

#[allow(clippy::too_many_arguments)]
fn emit_placeholder(
    commands: &mut Vec<VectorCommand>,
    summary: &mut MetafileExpansionSummary,
    warnings: &Sender<WarningEvent>,
    rel_id: &str,
    dest_rect: crate::contracts::RectF32,
    transform: crate::contracts::AffineTransform,
    reason: &str,
) {
    commands.extend(crate::compiler::placeholders::compile_metafile_placeholder(
        dest_rect, transform, reason,
    ));
    let _ = warnings.send(WarningEvent::MediaFailed {
        rel_id: rel_id.to_string(),
        reason: reason.to_string(),
    });
    summary.placeholders += 1;
    summary.warnings += 1;
}

/// Stable warning element id for a metafile picture.
fn element_id(rel_id: &str) -> String {
    format!("picture/{rel_id}")
}
