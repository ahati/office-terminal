//! Replaces `DrawMetafile` markers with playback output (PRD 13 §3.1).

use crossbeam_channel::Sender;

use crate::contracts::{FidelityClass, VectorCommand, VectorSlide, WarningEvent};
use crate::loader::AssetStore;

use super::cache::global_resolver;
use super::diagnostics::dedup;
use super::limits::MetafileLimits;
use super::mapping;
use super::warning_for;
use super::{default_engine, playback_mode, MetafileMode};

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
    let engine = default_engine();
    let force_placeholder = playback_mode() == MetafileMode::Placeholder;
    let limits = MetafileLimits::default();
    let resolver = global_resolver();
    let mut expanded_commands = Vec::with_capacity(scene.commands.len());

    for command in std::mem::take(&mut scene.commands) {
        let (rel_id, transform, src_rect, dest_rect, opacity, effects) = match command {
            VectorCommand::DrawMetafile {
                rel_id,
                transform,
                src_rect,
                dest_rect,
                opacity,
                effects,
            } => (rel_id, transform, src_rect, dest_rect, opacity, effects),
            other => {
                expanded_commands.push(other);
                continue;
            }
        };

        let Some(engine) = engine.as_deref().filter(|_| !force_placeholder) else {
            // Engine disabled at compile time, or the debug switch forced the
            // pre-playback behaviour: placeholder without a diagnostic.
            emit_placeholder(
                &mut expanded_commands,
                &mut summary,
                warnings,
                dest_rect,
                transform,
                "metafile playback disabled",
                None,
            );
            continue;
        };

        match resolver.resolve(&rel_id, assets, engine, &limits, &src_rect, &dest_rect) {
            Ok(compiled) if !compiled.commands.is_empty() => {
                for command in &compiled.commands {
                    let mut transformed = mapping::transform_command(command, &transform);
                    super::effects::apply_metafile_effects(&mut transformed, &effects, opacity);
                    expanded_commands.push(transformed);
                }
                let element_id = element_id(&rel_id);
                for diagnostic in dedup(compiled.diagnostics.clone()) {
                    let _ = warnings.send(warning_for(element_id.clone(), &diagnostic));
                    summary.warnings += 1;
                }
                if opacity < 1.0 && super::effects::is_opacity_unrepresentable(&compiled.commands) {
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
                    dest_rect,
                    transform,
                    "metafile produced no drawable content",
                    Some(WarningEvent::MediaFailed {
                        rel_id: rel_id.clone(),
                        reason: "metafile produced no drawable content".into(),
                    }),
                );
            }
            Err(error) => {
                let reason = error.to_string();
                let event = failure_event(&rel_id, &error, &reason);
                emit_placeholder(
                    &mut expanded_commands,
                    &mut summary,
                    warnings,
                    dest_rect,
                    transform,
                    &reason,
                    Some(event),
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
    dest_rect: crate::contracts::RectF32,
    transform: crate::contracts::AffineTransform,
    reason: &str,
    event: Option<WarningEvent>,
) {
    commands.extend(crate::compiler::placeholders::compile_metafile_placeholder(
        dest_rect, transform, reason,
    ));
    if let Some(event) = event {
        let _ = warnings.send(event);
        summary.warnings += 1;
    }
    summary.placeholders += 1;
}

/// Maps a playback failure onto the same warning taxonomy as engine
/// diagnostics (PRD 13 §11 / PRD 13a §3).
fn failure_event(rel_id: &str, error: &super::MetafileError, reason: &str) -> WarningEvent {
    match error {
        super::MetafileError::Limit(_) => WarningEvent::ResourceLimitHit {
            limit: "metafile input",
        },
        super::MetafileError::Unsupported(_) => WarningEvent::FidelityDegraded {
            element_id: element_id(rel_id),
            feature: "metafile_parse",
            class: FidelityClass::Placeholder,
        },
        _ => WarningEvent::MediaFailed {
            rel_id: rel_id.to_string(),
            reason: reason.to_string(),
        },
    }
}

/// Stable warning element id for a metafile picture.
fn element_id(rel_id: &str) -> String {
    format!("picture/{rel_id}")
}
