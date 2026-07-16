use std::{collections::HashSet, path::PathBuf};

use anyhow::anyhow;
use log::debug;
use rea_rs::{
    project_info::{BoundsMode, RenderMode},
    Duration, Position, Project, SoloMode, SourceOffset,
};

pub(super) const DEFAULT_RENDER_TARGETS_BUF_SIZE: usize = 1024;
pub(super) const FALLBACK_RENDER_TARGETS_BUF_SIZE: usize = 1024 * 10;

#[derive(Debug, Clone)]
pub(super) struct RenderTargetsBuild {
    pub(super) targets: Vec<RenderTarget>,
    pub(super) required_buffer_size: usize,
}

#[derive(Debug, Clone)]
pub(super) struct RenderTarget {
    pub(super) path: PathBuf,
    pub(super) video_source: Option<PathBuf>,
    pub(super) availble_for_render: AvailbleForRender,
    pub(super) duration: Duration,
    pub(super) source_offset: SourceOffset,
}

#[derive(Debug, Clone)]
pub(super) enum AvailbleForRender {
    Ok,
    NoVideo,
    OutOfBounds(Position),
}

const TIMELINE_PRECISION: u32 = 1_000_000;

#[derive(Debug, Clone)]
struct RenderBound {
    start: Position,
    end: Position,
    rendered_tracks: Vec<(usize, String)>,
}

#[derive(Debug, Clone)]
enum TargetTrackScope {
    MasterMix,
    Stem(usize),
}

#[derive(Debug, Clone)]
struct RenderTargetPlan {
    render_target: PathBuf,
    bound: RenderBound,
    scope: TargetTrackScope,
}

pub(super) fn build_render_targets(
    pr: &mut Project,
    render_targets_buf_size: usize,
) -> anyhow::Result<RenderTargetsBuild> {
    debug!("ffmpeg_new/render_targets: build_render_targets start");
    let plans = build_render_target_plan(pr, render_targets_buf_size)?;
    debug!(
        "ffmpeg_new/render_targets: render target plans built count={}",
        plans.plans.len()
    );
    let targets = plans
        .plans
        .iter()
        .map(|plan| build_render_target_from_plan(pr, plan))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let ok = targets
        .iter()
        .filter(|t| matches!(t.availble_for_render, AvailbleForRender::Ok))
        .count();
    let no_video = targets
        .iter()
        .filter(|t| matches!(t.availble_for_render, AvailbleForRender::NoVideo))
        .count();
    let out_of_bounds = targets
        .iter()
        .filter(|t| matches!(t.availble_for_render, AvailbleForRender::OutOfBounds(_)))
        .count();
    debug!(
        "ffmpeg_new/render_targets: build done total={} ok={} no_video={} out_of_bounds={}",
        targets.len(),
        ok,
        no_video,
        out_of_bounds
    );
    Ok(RenderTargetsBuild {
        targets,
        required_buffer_size: plans.required_buffer_size,
    })
}

fn build_render_target_from_plan(
    pr: &Project,
    plan: &RenderTargetPlan,
) -> anyhow::Result<RenderTarget> {
    let bound_start = plan.bound.start.with_precision(TIMELINE_PRECISION);
    let bound_end = plan.bound.end.with_precision(TIMELINE_PRECISION);
    let duration = rea_rs::Duration::from_std((bound_end - bound_start).as_duration())
        .unwrap_or_else(|_| rea_rs::Duration::zero());

    for track_idx in tracks_for_scope(pr, &plan.scope) {
        let track = pr
            .get_track(track_idx)
            .ok_or_else(|| anyhow!("can not get track with index {track_idx}"))?;

        for item_idx in 0..track.n_items() {
            let item = track
                .get_item(item_idx)
                .ok_or_else(|| anyhow!("can not get item {item_idx} on track {track_idx}"))?;

            if item.is_muted() {
                continue;
            }

            let take = item.active_take();
            let Some(source) = take.source() else {
                continue;
            };
            if source.type_string() != "VIDEO" {
                continue;
            }

            let item_start = item.position().with_precision(TIMELINE_PRECISION);
            let item_end = item.end_position().with_precision(TIMELINE_PRECISION);

            if item_start >= bound_end || item_end <= bound_start {
                continue;
            }

            if item_start > bound_start || item_end < bound_end {
                let out_of_bounds = if item_start > bound_start {
                    bound_start
                } else {
                    item_end
                };
                debug!(
					"ffmpeg_new/render_targets: OutOfBounds target={} item_bounds=({:?},{:?}) render_bounds=({:?},{:?})",
					plan.render_target.display(),
					item_start,
					item_end,
					bound_start,
					bound_end
				);
                return Ok(RenderTarget {
                    path: plan.render_target.clone(),
                    video_source: Some(source.filename()),
                    availble_for_render: AvailbleForRender::OutOfBounds(out_of_bounds),
                    duration,
                    source_offset: take.start_offset(),
                });
            }

            let source_offset = take.start_offset() + (bound_start - item.position()).as_duration();
            return Ok(RenderTarget {
                path: plan.render_target.clone(),
                video_source: Some(source.filename()),
                availble_for_render: AvailbleForRender::Ok,
                duration,
                source_offset,
            });
        }
    }

    Ok(RenderTarget {
        path: plan.render_target.clone(),
        video_source: None,
        availble_for_render: AvailbleForRender::NoVideo,
        duration,
        source_offset: SourceOffset::from_secs_f64(0.0),
    })
}

fn build_render_target_plan(
    pr: &mut Project,
    render_targets_buf_size: usize,
) -> anyhow::Result<RenderTargetsPlan> {
    let settings = pr.get_render_settings();
    let bounds = collect_render_bounds(pr)?;
    pr.set_string_param_size(render_targets_buf_size);
    let targets = pr
        .get_render_targets()
        .map_err(|e| anyhow!("can not get render targets: {e}"))?
        .into_iter()
        .collect::<Vec<_>>();
    let required_buffer_size = render_targets_buffer_size(&targets);
    let targets = targets
        .into_iter()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .collect::<Vec<_>>();

    let master_tracks = collect_active_track_indices(pr);
    let stem_tracks = collect_stem_tracks(pr);

    if matches!(settings.mode, RenderMode::RenderMatrix) {
        let plans = build_render_matrix_plan(&bounds, &targets)?;
        return Ok(RenderTargetsPlan {
            plans,
            required_buffer_size,
        });
    }

    let scopes = match settings.mode {
        RenderMode::MasterMix => vec![TargetTrackScope::MasterMix],
        RenderMode::Stems => {
            if stem_tracks.is_empty() {
                return Err(anyhow!(
                    "render mode is Stems, but no selected tracks found"
                ));
            }
            stem_tracks
                .iter()
                .map(|(idx, _)| TargetTrackScope::Stem(*idx))
                .collect()
        }
        RenderMode::MasterAndStems => {
            if stem_tracks.is_empty() {
                return Err(anyhow!(
                    "render mode is MasterAndStems, but no selected tracks found"
                ));
            }
            let mut scopes = vec![TargetTrackScope::MasterMix];
            scopes.extend(
                stem_tracks
                    .iter()
                    .map(|(idx, _)| TargetTrackScope::Stem(*idx)),
            );
            scopes
        }
        RenderMode::SelectedItems | RenderMode::SelectedItemsViaMaster => {
            if master_tracks.is_empty() {
                return Err(anyhow!(
                    "no active tracks available for SelectedItems render"
                ));
            }
            vec![TargetTrackScope::MasterMix]
        }
        RenderMode::RenderMatrix => unreachable!("render matrix handled above"),
    };
    debug!(
        "ffmpeg_new/render_targets: plan mode={:?} bounds={} scopes={} targets={}",
        settings.mode,
        bounds.len(),
        scopes.len(),
        targets.len()
    );

    let expected_targets = bounds.len() * scopes.len();
    if targets.len() < expected_targets {
        return Err(anyhow!(
            "render target count mismatch: expected at least {expected_targets}, got {}",
            targets.len()
        ));
    }

    let mut plans = Vec::with_capacity(expected_targets);
    let mut target_idx = 0_usize;
    for bound in bounds {
        for scope in &scopes {
            plans.push(RenderTargetPlan {
                render_target: targets[target_idx].clone(),
                bound: bound.clone(),
                scope: scope.clone(),
            });
            target_idx += 1;
        }
    }

    Ok(RenderTargetsPlan {
        plans,
        required_buffer_size,
    })
}

#[derive(Debug, Clone)]
struct RenderTargetsPlan {
    plans: Vec<RenderTargetPlan>,
    required_buffer_size: usize,
}

fn render_targets_buffer_size(targets: &[String]) -> usize {
    let payload = targets.iter().map(|target| target.len()).sum::<usize>();
    let separators = targets.len().saturating_sub(1);
    payload + separators + 2
}

fn build_render_matrix_plan(
    bounds: &[RenderBound],
    targets: &[PathBuf],
) -> anyhow::Result<Vec<RenderTargetPlan>> {
    if bounds.is_empty() {
        return Err(anyhow!("render mode is RenderMatrix, but no regions found"));
    }

    let expected_targets = bounds
        .iter()
        .map(|b| b.rendered_tracks.len())
        .sum::<usize>();
    if expected_targets == 0 {
        return Err(anyhow!(
            "render mode is RenderMatrix, but no rendered tracks are configured in regions"
        ));
    }
    if targets.len() < expected_targets {
        return Err(anyhow!(
			"render target count mismatch in RenderMatrix: expected at least {expected_targets}, got {}",
			targets.len()
		));
    }

    let mut plans = Vec::with_capacity(expected_targets);
    let mut target_idx = 0_usize;
    for bound in bounds {
        for (track_idx, _) in &bound.rendered_tracks {
            if target_idx >= targets.len() {
                break;
            }
            plans.push(RenderTargetPlan {
                render_target: targets[target_idx].clone(),
                bound: bound.clone(),
                scope: TargetTrackScope::Stem(*track_idx),
            });
            target_idx += 1;
        }
    }

    Ok(plans)
}

fn collect_render_bounds(pr: &Project) -> anyhow::Result<Vec<RenderBound>> {
    match pr.get_render_bounds_mode() {
        BoundsMode::EntireProject => Ok(vec![RenderBound {
            start: Position::from(0.0),
            end: pr.length().into(),
            rendered_tracks: Vec::new(),
        }]),
        BoundsMode::Custom => {
            let (start, end) = pr.get_render_bounds();
            Ok(vec![RenderBound {
                start,
                end,
                rendered_tracks: Vec::new(),
            }])
        }
        BoundsMode::TimeSelection => {
            let ts = pr.get_time_selection();
            Ok(vec![RenderBound {
                start: ts.get_start(),
                end: ts.get_end(),
                rendered_tracks: Vec::new(),
            }])
        }
        BoundsMode::AllRegions => Ok(collect_region_bounds(pr, false)),
        BoundsMode::SelectedItems => {
            let mut bounds = pr
                .iter_selected_items()
                .map(|item| RenderBound {
                    start: item.position(),
                    end: item.end_position(),
                    rendered_tracks: Vec::new(),
                })
                .collect::<Vec<_>>();
            bounds.sort_by(|a, b| {
                a.start
                    .with_precision(TIMELINE_PRECISION)
                    .partial_cmp(&b.start.with_precision(TIMELINE_PRECISION))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            Ok(bounds)
        }
        BoundsMode::SelectedRegions => Ok(collect_region_bounds(pr, true)),
    }
}

fn collect_region_bounds(pr: &Project, selected_only: bool) -> Vec<RenderBound> {
    pr.iter_markers_and_regions()
        .filter(|r| r.is_region)
        .filter(|r| !selected_only || r.is_selected(pr))
        .map(|region| RenderBound {
            start: region.position,
            end: region.rgn_end,
            rendered_tracks: region
                .iter_rendered_tracks(pr)
                .map(|tr| (tr.index(), tr.name()))
                .collect(),
        })
        .collect()
}

fn collect_active_track_indices(pr: &Project) -> Vec<usize> {
    let any_solo = pr.any_track_solo();
    pr.iter_tracks()
        .filter(|tr| !tr.muted())
        .filter(|tr| !any_solo || tr.solo() != SoloMode::NotSoloed)
        .map(|tr| tr.index())
        .collect()
}

fn collect_stem_tracks(pr: &Project) -> Vec<(usize, String)> {
    let selected = pr
        .iter_selected_tracks()
        .map(|tr| tr.index())
        .collect::<HashSet<_>>();
    if selected.is_empty() {
        return Vec::new();
    }

    let any_solo = pr.any_track_solo();
    pr.iter_tracks()
        .filter(|tr| selected.contains(&tr.index()))
        .filter(|tr| !tr.muted())
        .filter(|tr| !any_solo || tr.solo() != SoloMode::NotSoloed)
        .map(|tr| (tr.index(), tr.name()))
        .collect()
}

fn tracks_for_scope(pr: &Project, scope: &TargetTrackScope) -> Vec<usize> {
    match scope {
        TargetTrackScope::MasterMix => collect_active_track_indices(pr),
        TargetTrackScope::Stem(track_idx) => {
            let any_solo = pr.any_track_solo();
            let Some(track) = pr.get_track(*track_idx) else {
                return Vec::new();
            };
            if track.muted() || (any_solo && track.solo() == SoloMode::NotSoloed) {
                Vec::new()
            } else {
                vec![*track_idx]
            }
        }
    }
}
