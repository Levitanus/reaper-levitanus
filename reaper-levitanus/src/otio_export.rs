use std::{
	collections::{HashMap, HashSet},
	error::Error,
	path::{Path, PathBuf},
	process::Command,
};

use anyhow::anyhow;
use log::{info, warn};
use rea_rs::{
	ExtState,
	project_info::{BoundsMode, RenderMode},
	CommandId, MessageBoxType, MessageBoxValue, Position, Project, Reaper, SoloMode, Take,
};
use thiserror::Error;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const TIMELINE_PRECISION: u32 = 1_000_000;
const DEFAULT_OTIO_RATE: f64 = 25.0;
const PLAY_RATE_EFFECT_EPSILON: f64 = 1e-3;
const OTIO_FRAME_VALUE_PRECISION: f64 = 1.0;
const MIN_SERIALIZED_GAP_FRAMES: f64 = 0.5;
const KDENLIVE_COMPAT_DISABLE_TIMEWARP: bool = false;
const RENDER_PROJECT_USING_LAST_SETTINGS_ACTION: u32 = 41824;
const OTIO_EXT_SECTION: &str = "levitanus_otio_export";
const OTIO_FPS_POLICY_KEY: &str = "fps_policy";

#[derive(Debug, Clone)]
struct RenderBound {
	start: Position,
	end: Position,
	rendered_tracks: Vec<(usize, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OtioFpsPolicy {
	FirstVideo,
	MedianVideo,
	Project,
}
impl Default for OtioFpsPolicy {
	fn default() -> Self {
		Self::MedianVideo
	}
}


#[derive(Debug, Clone, Copy, Serialize)]
pub struct RationalTime {
    #[serde(rename = "OTIO_SCHEMA")]
    schema: &'static str,
    pub value: f64,
    pub rate: f64,
}

impl RationalTime {
    pub fn new(value: f64, rate: f64) -> Self {
        Self {
            schema: "RationalTime.1",
            value,
            rate,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct TimeRange {
    #[serde(rename = "OTIO_SCHEMA")]
    schema: &'static str,
    pub start_time: RationalTime,
    pub duration: RationalTime,
}

impl TimeRange {
    pub fn new(start_time: RationalTime, duration: RationalTime) -> Result<Self, OtioError> {
        if duration.value <= 0.0 {
            return Err(OtioError::InvalidDuration);
        }

        Ok(Self {
            schema: "TimeRange.1",
            start_time,
            duration,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ExternalReference {
    #[serde(rename = "OTIO_SCHEMA")]
    schema: &'static str,
    pub target_url: String,
    pub available_range: Option<TimeRange>,
    pub available_image_bounds: Option<serde_json::Value>,
    pub metadata: serde_json::Value,
}

impl ExternalReference {
    pub fn new(target_url: impl Into<String>) -> Self {
        Self {
            schema: "ExternalReference.1",
            target_url: target_url.into(),
            available_range: None,
            available_image_bounds: None,
            metadata: serde_json::json!({}),
        }
    }

    pub fn with_available_range(mut self, available_range: TimeRange) -> Self {
        self.available_range = Some(available_range);
        self
    }
}
#[derive(Debug, Error)]
pub enum OtioError {
    #[error("clip duration must be greater than zero")]
    InvalidDuration,
    #[error("time scalar must be greater than zero")]
    InvalidTimeScalar,
}
#[derive(Debug, Clone, Serialize)]
pub struct LinearTimeWarp {
    #[serde(rename = "OTIO_SCHEMA")]
    schema: &'static str,
    pub name: String,
    pub time_scalar: f64,
    pub metadata: serde_json::Value,
}

impl LinearTimeWarp {
    pub fn new(time_scalar: f64) -> Result<Self, OtioError> {
        if time_scalar <= 0.0 {
            return Err(OtioError::InvalidTimeScalar);
        }

        Ok(Self {
            schema: "LinearTimeWarp.1",
            name: "LinearTimeWarp".to_string(),
            time_scalar,
            metadata: serde_json::json!({}),
        })
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Clip {
    #[serde(rename = "OTIO_SCHEMA")]
    schema: &'static str,
    pub name: String,
    pub source_range: TimeRange,
    pub media_reference: ExternalReference,
    pub metadata: serde_json::Value,
    pub effects: Vec<LinearTimeWarp>,
    pub markers: Vec<serde_json::Value>,
    pub enabled: bool,
}

impl Clip {
    pub fn new(
        name: impl Into<String>,
        media_reference: ExternalReference,
        source_range: TimeRange,
    ) -> Self {
        Self {
            schema: "Clip.2",
            name: name.into(),
            source_range,
            media_reference,
            metadata: serde_json::json!({}),
            effects: vec![],
            markers: vec![],
            enabled: true,
        }
    }

    pub fn with_time_stretch(mut self, time_scalar: f64) -> Result<Self, OtioError> {
        self.effects.push(LinearTimeWarp::new(time_scalar)?);
        Ok(self)
    }
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

#[derive(Debug, Clone)]
struct VideoSlice {
	file: PathBuf,
	track_name: String,
	timeline_start: f64,
	timeline_end: f64,
	source_start: f64,
	source_end: f64,
	enabled: bool,
	source_fps: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
struct StretchPoint {
	item_pos: f64,
	source_pos: f64,
}

#[derive(Debug, Clone, Serialize)]
struct OtioTrack {
	#[serde(rename = "OTIO_SCHEMA")]
	schema: &'static str,
	metadata: serde_json::Value,
	name: String,
	source_range: Option<TimeRange>,
	effects: Vec<serde_json::Value>,
	markers: Vec<serde_json::Value>,
	enabled: bool,
	color: Option<serde_json::Value>,
	children: Vec<serde_json::Value>,
	kind: String,
}

impl OtioTrack {
	fn new(name: impl Into<String>, kind: impl Into<String>) -> Self {
		Self {
			schema: "Track.1",
			metadata: json!({}),
			name: name.into(),
			source_range: None,
			effects: Vec::new(),
			markers: Vec::new(),
			enabled: true,
			color: None,
			children: Vec::new(),
			kind: kind.into(),
		}
	}
}

#[derive(Debug, Clone, Serialize)]
struct OtioStack {
	#[serde(rename = "OTIO_SCHEMA")]
	schema: &'static str,
	metadata: serde_json::Value,
	name: String,
	source_range: Option<TimeRange>,
	effects: Vec<serde_json::Value>,
	markers: Vec<serde_json::Value>,
	enabled: bool,
	color: Option<serde_json::Value>,
	children: Vec<OtioTrack>,
}

impl OtioStack {
	fn new(children: Vec<OtioTrack>) -> Self {
		Self {
			schema: "Stack.1",
			metadata: json!({}),
			name: "tracks".to_string(),
			source_range: None,
			effects: Vec::new(),
			markers: Vec::new(),
			enabled: true,
			color: None,
			children,
		}
	}
}

#[derive(Debug, Clone, Serialize)]
struct OtioTimeline {
	#[serde(rename = "OTIO_SCHEMA")]
	schema: &'static str,
	metadata: serde_json::Value,
	name: String,
	global_start_time: Option<RationalTime>,
	tracks: OtioStack,
}

pub fn export_otio_project() -> Result<(), Box<dyn Error>> {
	let rpr = Reaper::get();
	let pr = rpr.current_project();
	let fps_policy = get_project_fps_policy(&pr);
	let plans = build_render_target_plan(&pr)?;
	let audio_exists = ensure_render_audio_exists(&pr, &plans)?;

	for plan in plans {
		let mut video_slices = collect_video_slices(&pr, &plan)?;
		let otio_rate = detect_otio_rate(&pr, &video_slices, fps_policy)
			.unwrap_or(DEFAULT_OTIO_RATE);
		apply_crossfade_trim(&mut video_slices);

		let mut track_map: HashMap<String, Vec<VideoSlice>> = HashMap::new();
		let mut track_order: Vec<String> = Vec::new();

		for slice in video_slices {
			let entry = track_map.entry(slice.track_name.clone()).or_insert_with(|| {
				track_order.push(slice.track_name.clone());
				Vec::new()
			});
			entry.push(slice);
		}

		let mut tracks = Vec::new();
		if audio_exists {
			if let Some(audio_clip) = make_audio_clip(&plan, otio_rate)? {
				let mut audio_track = OtioTrack::new("audio", "Audio");
				audio_track.children.push(serde_json::to_value(audio_clip)?);
				tracks.push(audio_track);
			}
		}

		for (video_track_idx, track_name) in track_order.into_iter().enumerate() {
			let slices = track_map.remove(&track_name).unwrap_or_default();
			let children = build_video_track_children(slices, otio_rate)?;
			let exported_name = normalize_video_track_name(video_track_idx, &track_name);
			let mut track = OtioTrack::new(exported_name, "Video");
			track.children = children;
			tracks.push(track);
		}

		let timeline_name = format!(
			"{} [{}]",
			pr.name(),
			plan.render_target
				.file_name()
				.and_then(|f| f.to_str())
				.unwrap_or("render_target")
		);
		let mut stack = OtioStack::new(tracks);
		stack.markers = collect_otio_markers(&pr, &plan.bound, otio_rate);

		let timeline = OtioTimeline {
			schema: "Timeline.1",
			name: timeline_name,
			metadata: json!({}),
			global_start_time: Some(RationalTime::new(0.0, otio_rate)),
			tracks: stack,
		};

		let out_file = plan.render_target.with_extension("otio");
		if !confirm_overwrite(&out_file)? {
			info!("OTIO export skipped: {}", out_file.display());
			continue;
		}
		let mut value = serde_json::to_value(&timeline)?;
		patch_otio_for_kdenlive(&mut value);
		let json = serde_json::to_string_pretty(&value)?;
		std::fs::write(&out_file, json)?;
		info!("OTIO exported: {}", out_file.display());
	}

	Ok(())
}

fn confirm_overwrite(path: &Path) -> anyhow::Result<bool> {
	if !path.exists() {
		return Ok(true);
	}

	let response = Reaper::get().show_message_box(
		"OTIO export",
		format!(
			"OTIO file already exists:\n{}\n\nOverwrite it?",
			path.display()
		),
		MessageBoxType::YesNo,
	)?;
	Ok(response == MessageBoxValue::Yes)
}

pub fn export_youtube_timecodes() -> Result<(), Box<dyn Error>> {
	let pr = Reaper::get().current_project();
	let plans = build_render_target_plan(&pr)?;
	for plan in plans {
		let start = plan.bound.start.as_duration().as_secs_f64();
		let end = plan.bound.end.as_duration().as_secs_f64();

		let mut lines = Vec::new();
		for marker in pr.iter_markers_and_regions().filter(|m| !m.is_region) {
			let pos = marker.position.as_duration().as_secs_f64();
			if pos < start || pos > end {
				continue;
			}
			let rel = (pos - start).max(0.0);
			let name = if marker.name.trim().is_empty() {
				"Marker".to_string()
			} else {
				marker.name
			};
			lines.push(format!("{} - {}", format_youtube_timecode(rel), name));
		}

		let output_path = timecodes_output_path_for_target(&plan.render_target);
		std::fs::write(&output_path, lines.join("\n"))?;
		info!("YouTube timecodes exported: {}", output_path.display());
	}
	Ok(())
}

pub fn set_project_fps(policy: OtioFpsPolicy) -> Result<(), Box<dyn Error>> {
	let pr = Reaper::get().current_project();
	let mut state = ExtState::new(
		OTIO_EXT_SECTION,
		OTIO_FPS_POLICY_KEY,
		Some(policy),
		true,
		&pr,
		None,
	);
	state.set(policy);
	Ok(())
}

fn get_project_fps_policy(pr: &Project) -> OtioFpsPolicy {
	let state: ExtState<OtioFpsPolicy, Project> = ExtState::new(
		OTIO_EXT_SECTION,
		OTIO_FPS_POLICY_KEY,
		Some(OtioFpsPolicy::default()),
		true,
		pr,
		None,
	);
	state.get().ok().flatten().unwrap_or_default()
}

fn build_render_target_plan(pr: &Project) -> anyhow::Result<Vec<RenderTargetPlan>> {
	let settings = pr.get_render_settings();
	let bounds = collect_render_bounds(pr)?;
	let targets = pr
		.get_render_targets()
		.map_err(|e| anyhow!("can not get render targets: {e}"))?
		.into_iter()
		.filter(|s| !s.trim().is_empty())
		.map(PathBuf::from)
		.collect::<Vec<_>>();

	let master_tracks = collect_active_track_indices(pr);
	let stem_tracks = collect_stem_tracks(pr);

	let scopes = match settings.mode {
		RenderMode::MasterMix => vec![TargetTrackScope::MasterMix],
		RenderMode::Stems => {
			if stem_tracks.is_empty() {
				return Err(anyhow!("render mode is Stems, but no selected tracks found"));
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
			scopes.extend(stem_tracks.iter().map(|(idx, _)| TargetTrackScope::Stem(*idx)));
			scopes
		}
		RenderMode::SelectedItems | RenderMode::SelectedItemsViaMaster => {
			if master_tracks.is_empty() {
				return Err(anyhow!("no active tracks available for SelectedItems render"));
			}
			vec![TargetTrackScope::MasterMix]
		}
		RenderMode::RenderMatrix => return build_render_matrix_plan(&bounds, &targets),
	};

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
	Ok(plans)
}

fn build_render_matrix_plan(bounds: &[RenderBound], targets: &[PathBuf]) -> anyhow::Result<Vec<RenderTargetPlan>> {
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
	let mode = pr.get_render_bounds_mode();
	match mode {
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
		.map(|region| {
			let rendered_tracks = region
				.iter_rendered_tracks(pr)
				.map(|tr| (tr.index(), tr.name()))
				.collect::<Vec<_>>();
			RenderBound {
				start: region.position,
				end: region.rgn_end,
				rendered_tracks,
			}
		})
		.collect()
}

fn collect_active_track_indices(pr: &Project) -> Vec<usize> {
	let any_solo = pr.any_track_solo();
	pr.iter_tracks()
		.rev()
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
		.rev()
		.filter(|tr| selected.contains(&tr.index()))
		.filter(|tr| !tr.muted())
		.filter(|tr| !any_solo || tr.solo() != SoloMode::NotSoloed)
		.map(|tr| (tr.index(), tr.name()))
		.collect()
}

fn tracks_for_scope(pr: &Project, scope: &TargetTrackScope) -> Vec<usize> {
	match scope {
		TargetTrackScope::MasterMix => collect_active_track_indices(pr),
		TargetTrackScope::Stem(track_idx) => vec![*track_idx],
	}
}

fn ensure_render_audio_exists(pr: &Project, plans: &[RenderTargetPlan]) -> anyhow::Result<bool> {
	let missing_any = plans
		.iter()
		.map(|p| p.render_target.as_path())
		.any(|path| !path.exists());
	if !missing_any {
		return Ok(true);
	}

	let response = Reaper::get().show_message_box(
		"OTIO export",
		"Audio render target file is missing. Render project audio now?",
		MessageBoxType::YesNo,
	)?;
	if response != MessageBoxValue::Yes {
		return Ok(false);
	}

	Reaper::get().perform_action(
		CommandId::new(RENDER_PROJECT_USING_LAST_SETTINGS_ACTION),
		0,
		Some(pr),
	);

	let still_missing = plans
		.iter()
		.map(|p| p.render_target.as_path())
		.any(|path| !path.exists());
	if still_missing {
		warn!("render target audio file is still missing after render action");
		Ok(false)
	} else {
		Ok(true)
	}
}

fn collect_video_slices(pr: &Project, plan: &RenderTargetPlan) -> anyhow::Result<Vec<VideoSlice>> {
	let tracks = tracks_for_scope(pr, &plan.scope);
	let bound_start = plan.bound.start.with_precision(TIMELINE_PRECISION);
	let bound_end = plan.bound.end.with_precision(TIMELINE_PRECISION);

	let mut slices = Vec::new();
	let mut fps_cache: HashMap<PathBuf, Option<f64>> = HashMap::new();
	for track_idx in tracks {
		let track = pr
			.get_track(track_idx)
			.ok_or_else(|| anyhow!("can not get track with index {track_idx}"))?;
		let track_name = format!(
			"{:02} {}",
			track_idx + 1,
			if track.name().is_empty() {
				"Track".to_string()
			} else {
				track.name()
			}
		);

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

			let clipped_start = if item_start > bound_start { item_start } else { bound_start };
			let clipped_end = if item_end < bound_end { item_end } else { bound_end };
			let item_len = item.length().as_secs_f64();

			let local_start = (clipped_start - item.position()).as_duration().as_secs_f64();
			let local_end = (clipped_end - item.position()).as_duration().as_secs_f64();
			if local_end <= local_start {
				continue;
			}

			let stretch_points = build_stretch_points(&take, item_len);
			let file = source.filename();
			let source_fps = match fps_cache.get(&file) {
				Some(v) => *v,
				None => {
					let v = probe_video_fps(&file);
					fps_cache.insert(file.clone(), v);
					v
				}
			};

			for segment in segment_item_by_stretch(
				&stretch_points,
				local_start,
				local_end,
				item.position().as_duration().as_secs_f64(),
				bound_start.as_duration().as_secs_f64(),
			) {
				if segment.timeline_end <= segment.timeline_start {
					continue;
				}
				slices.push(VideoSlice {
					file: file.clone(),
					track_name: track_name.clone(),
					timeline_start: segment.timeline_start,
					timeline_end: segment.timeline_end,
					source_start: segment.source_start,
					source_end: segment.source_end,
					enabled: true,
					source_fps,
				});
			}
		}
	}

	Ok(slices)
}

fn normalize_video_track_name(export_index: usize, original_name: &str) -> String {
	let suffix = original_name
		.trim_start_matches(|c: char| c.is_ascii_digit() || c.is_whitespace())
		.trim();
	if suffix.is_empty() {
		export_index.to_string()
	} else {
		format!("{} {}", export_index, suffix)
	}
}

#[derive(Debug, Clone)]
struct Segment {
	timeline_start: f64,
	timeline_end: f64,
	source_start: f64,
	source_end: f64,
}

fn build_stretch_points(take: &Take<rea_rs::Immutable>, item_len: f64) -> Vec<StretchPoint> {
	let play_rate: f64 = take.play_rate().into();
	let mut points = Vec::new();
	points.push(StretchPoint {
		item_pos: 0.0,
		source_pos: take.start_offset().as_secs_f64(),
	});

	let mut markers = get_take_stretch_markers(take)
		.into_iter()
		.filter(|(pos, _)| *pos > 0.0 && *pos < item_len)
		.collect::<Vec<_>>();
	markers.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
	for (item_pos, source_pos) in markers {
		points.push(StretchPoint {
			item_pos,
			source_pos,
		});
	}

	let last = points.last().copied().unwrap_or(StretchPoint {
		item_pos: 0.0,
		source_pos: take.start_offset().as_secs_f64(),
	});
	points.push(StretchPoint {
		item_pos: item_len,
		source_pos: last.source_pos + (item_len - last.item_pos) * play_rate,
	});
	points
}

fn get_take_stretch_markers(take: &Take<rea_rs::Immutable>) -> Vec<(f64, f64)> {
	take
		.iter_stretch_markers()
		.map(|marker| {
			(
				marker.position.as_duration().as_secs_f64(),
				marker.source_position.as_secs_f64(),
			)
		})
		.collect()
}

fn segment_item_by_stretch(
	points: &[StretchPoint],
	local_start: f64,
	local_end: f64,
	item_project_start: f64,
	bound_start: f64,
) -> Vec<Segment> {
	let mut segments = Vec::new();
	if local_end <= local_start || points.len() < 2 {
		return segments;
	}

	for pair in points.windows(2) {
		let left = pair[0];
		let right = pair[1];
		if right.item_pos <= local_start || left.item_pos >= local_end {
			continue;
		}

		let seg_local_start = left.item_pos.max(local_start);
		let seg_local_end = right.item_pos.min(local_end);
		if seg_local_end <= seg_local_start {
			continue;
		}

		let item_delta = right.item_pos - left.item_pos;
		if item_delta <= 0.0 {
			continue;
		}
		let src_delta = right.source_pos - left.source_pos;
		let rate = src_delta / item_delta;

		let src_start = left.source_pos + (seg_local_start - left.item_pos) * rate;
		let src_end = left.source_pos + (seg_local_end - left.item_pos) * rate;
		let project_start = item_project_start + seg_local_start;
		let project_end = item_project_start + seg_local_end;

		segments.push(Segment {
			timeline_start: project_start - bound_start,
			timeline_end: project_end - bound_start,
			source_start: src_start,
			source_end: src_end,
		});
	}

	segments
}

fn apply_crossfade_trim(slices: &mut Vec<VideoSlice>) {
	let mut by_track: HashMap<String, Vec<usize>> = HashMap::new();
	for (idx, slice) in slices.iter().enumerate() {
		by_track.entry(slice.track_name.clone()).or_default().push(idx);
	}

	for indices in by_track.values_mut() {
		indices.sort_by(|a, b| {
			slices[*a]
				.timeline_start
				.partial_cmp(&slices[*b].timeline_start)
				.unwrap_or(std::cmp::Ordering::Equal)
		});

		for pair in indices.windows(2) {
			let left_idx = pair[0];
			let right_idx = pair[1];
			let left_end = slices[left_idx].timeline_end;
			if slices[right_idx].timeline_start < left_end {
				trim_slice_start(&mut slices[right_idx], left_end);
			}
		}

		for pair in indices.windows(2) {
			let left_idx = pair[0];
			let right_idx = pair[1];
			let right_start = slices[right_idx].timeline_start;
			if slices[left_idx].timeline_end > right_start {
				trim_slice_end(&mut slices[left_idx], right_start);
			}
		}
	}

	slices.retain(|s| {
		s.timeline_end > s.timeline_start && s.source_end > s.source_start
	});
}

fn trim_slice_start(slice: &mut VideoSlice, new_timeline_start: f64) {
	let old_timeline_len = slice.timeline_end - slice.timeline_start;
	let old_source_len = slice.source_end - slice.source_start;
	let trim_amount = new_timeline_start - slice.timeline_start;
	if trim_amount <= 0.0 || old_timeline_len <= 0.0 || old_source_len <= 0.0 {
		return;
	}
	let ratio = old_source_len / old_timeline_len;
	slice.timeline_start = new_timeline_start.min(slice.timeline_end);
	slice.source_start = (slice.source_start + trim_amount * ratio).min(slice.source_end);
}

fn trim_slice_end(slice: &mut VideoSlice, new_timeline_end: f64) {
	let old_timeline_len = slice.timeline_end - slice.timeline_start;
	let old_source_len = slice.source_end - slice.source_start;
	let new_timeline_len = new_timeline_end - slice.timeline_start;
	if new_timeline_len <= 0.0 || old_timeline_len <= 0.0 || old_source_len <= 0.0 {
		return;
	}
	let ratio = new_timeline_len / old_timeline_len;
	slice.timeline_end = new_timeline_end.max(slice.timeline_start);
	slice.source_end = slice.source_start + old_source_len * ratio;
}

fn slice_to_otio_clip(slice: &VideoSlice, otio_rate: f64) -> anyhow::Result<Clip> {
	let source_duration = slice.source_end - slice.source_start;
	let timeline_duration = slice.timeline_end - slice.timeline_start;
	if source_duration <= 0.0 || timeline_duration <= 0.0 {
		return Err(anyhow!("clip duration must be positive"));
	}
	let time_scalar = source_duration / timeline_duration;
	let use_timewarp = (time_scalar - 1.0).abs() > PLAY_RATE_EFFECT_EPSILON
		&& !KDENLIVE_COMPAT_DISABLE_TIMEWARP;
	let serialized_duration = if use_timewarp {
		source_duration
	} else {
		timeline_duration
	};
	let min_clip_duration = if otio_rate > 0.0 { 1.0 / otio_rate } else { 0.0 };
	let serialized_duration = serialized_duration.max(min_clip_duration);

	let source_range = TimeRange::new(
		otio_time_from_seconds(slice.source_start, otio_rate),
		otio_time_from_seconds(serialized_duration, otio_rate),
	)?;
	let media = ExternalReference::new(path_to_target_url(&slice.file));
	let base_name = clip_name(&slice.file);
	let unique_name = format!(
		"{} - t{:.3}-s{:.3}",
		base_name,
		slice.timeline_start,
		slice.source_start
	);
	let mut clip = Clip::new(unique_name, media, source_range);
	if use_timewarp {
		clip = clip.with_time_stretch(time_scalar.abs())?;
	}
	clip.enabled = slice.enabled;
	Ok(clip)
}

fn make_audio_clip(plan: &RenderTargetPlan, otio_rate: f64) -> anyhow::Result<Option<Clip>> {
	if !plan.render_target.exists() {
		return Ok(None);
	}
	let duration = (plan.bound.end - plan.bound.start).as_duration().as_secs_f64();
	if duration <= 0.0 {
		return Ok(None);
	}
	let source_range = TimeRange::new(
		otio_time_from_seconds(0.0, otio_rate),
		otio_time_from_seconds(duration, otio_rate),
	)?;
	let media = ExternalReference::new(path_to_target_url(&plan.render_target));
	Ok(Some(Clip::new(clip_name(&plan.render_target), media, source_range)))
}

fn build_video_track_children(
	mut slices: Vec<VideoSlice>,
	otio_rate: f64,
) -> anyhow::Result<Vec<serde_json::Value>> {
	slices.sort_by(|a, b| {
		a.timeline_start
			.partial_cmp(&b.timeline_start)
			.unwrap_or(std::cmp::Ordering::Equal)
	});

	let mut children = Vec::new();
	let mut cursor = 0.0_f64;
	for mut slice in slices {
		if slice.timeline_start < cursor {
			let overlap = cursor - slice.timeline_start;
			let timeline_duration = slice.timeline_end - slice.timeline_start;
			let source_duration = slice.source_end - slice.source_start;
			if timeline_duration > 0.0 && source_duration > 0.0 {
				let ratio = source_duration / timeline_duration;
				slice.timeline_start = cursor;
				slice.source_start += overlap * ratio;
			}
		}

		if slice.timeline_end <= slice.timeline_start || slice.source_end <= slice.source_start {
			continue;
		}

		let gap_len = slice.timeline_start - cursor;
		if gap_len > 0.0
			&& quantize_otio_frame_value(gap_len * otio_rate) < MIN_SERIALIZED_GAP_FRAMES
		{
			slice.timeline_start = cursor;
		}

		let gap_len = slice.timeline_start - cursor;
		if gap_len > 1e-6 {
			children.push(make_gap(gap_len, otio_rate)?);
		}

		let emitted_duration = serialized_timeline_duration_seconds(&slice, otio_rate);
		let clip = slice_to_otio_clip(&slice, otio_rate)?;
		children.push(serde_json::to_value(clip)?);
		cursor = slice.timeline_start + emitted_duration;
	}

	Ok(children)
}

fn serialized_timeline_duration_seconds(slice: &VideoSlice, otio_rate: f64) -> f64 {
	let timeline_duration = (slice.timeline_end - slice.timeline_start).max(0.0);
	let min_timeline_duration = if otio_rate > 0.0 {
		1.0 / otio_rate
	} else {
		0.0
	};
	snap_seconds_to_timeline_frames(timeline_duration, otio_rate).max(min_timeline_duration)
}
fn make_gap(duration_secs: f64, rate: f64) -> anyhow::Result<serde_json::Value> {
	let source_range = TimeRange::new(
		otio_time_from_seconds(0.0, rate),
		otio_time_from_seconds(duration_secs, rate),
	)?;
	Ok(json!({
		"OTIO_SCHEMA": "Gap.1",
		"name": "",
		"source_range": source_range,
		"effects": [],
		"markers": [],
		"enabled": true,
		"metadata": {}
	}))
}

fn otio_time_from_seconds(seconds: f64, rate: f64) -> RationalTime {
	let value = quantize_otio_frame_value(seconds * rate);
	RationalTime::new(value, rate)
}

fn quantize_otio_frame_value(value: f64) -> f64 {
	let rounded = (value * OTIO_FRAME_VALUE_PRECISION).round() / OTIO_FRAME_VALUE_PRECISION;
	if rounded == -0.0 { 0.0 } else { rounded }
}

fn snap_seconds_to_timeline_frames(seconds: f64, rate: f64) -> f64 {
	if rate <= 0.0 {
		return seconds;
	}
	(seconds * rate).round() / rate
}

fn detect_otio_rate(
	pr: &Project,
	slices: &[VideoSlice],
	policy: OtioFpsPolicy,
) -> Option<f64> {
	let files = slices
		.iter()
		.map(|s| s.file.clone())
		.collect::<HashSet<_>>()
		.into_iter()
		.collect::<Vec<_>>();

	match policy {
		OtioFpsPolicy::FirstVideo => slices
			.iter()
			.filter_map(|s| s.source_fps)
			.find(|v| v.is_finite() && *v > 0.0)
			.or_else(|| files.into_iter().find_map(|f| probe_video_fps(&f))),
		OtioFpsPolicy::MedianVideo => {
			let mut fps = files
				.into_iter()
				.filter_map(|f| probe_video_fps(&f))
				.filter(|v| v.is_finite() && *v > 0.0)
				.collect::<Vec<_>>();
			if fps.is_empty() {
				fps = slices
					.iter()
					.filter_map(|s| s.source_fps)
					.filter(|v| v.is_finite() && *v > 0.0)
					.collect::<Vec<_>>();
			}
			if fps.is_empty() {
				None
			} else {
				fps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
				Some(fps[fps.len() / 2])
			}
		}
		OtioFpsPolicy::Project => detect_project_rate(pr),
	}
}

fn detect_project_rate(pr: &Project) -> Option<f64> {
	let render_target = pr
		.get_render_targets()
		.ok()?
		.into_iter()
		.find(|p| !p.trim().is_empty())?;
	let target = PathBuf::from(render_target);
	if !target.exists() {
		warn!(
			"OTIO FPS policy Project selected, but render target file does not exist; using default"
		);
		return None;
	}
	probe_video_fps(&target)
}

fn probe_video_fps(file: &Path) -> Option<f64> {
	let path = file.to_str()?;
	let output = Command::new("ffprobe")
		.args([
			"-v",
			"error",
			"-select_streams",
			"v:0",
			"-show_entries",
			"stream=avg_frame_rate,r_frame_rate",
			"-of",
			"default=noprint_wrappers=1:nokey=1",
			path,
		])
		.output()
		.ok()?;
	if !output.status.success() {
		return None;
	}
	let out = std::str::from_utf8(&output.stdout).ok()?;
	for line in out.lines() {
		if let Some(rate) = parse_fps_fraction(line.trim()) {
			if rate > 0.0 {
				return Some(rate);
			}
		}
	}
	None
}

fn parse_fps_fraction(value: &str) -> Option<f64> {
	if value.is_empty() || value == "0/0" {
		return None;
	}
	if let Some((num, den)) = value.split_once('/') {
		let n: f64 = num.parse().ok()?;
		let d: f64 = den.parse().ok()?;
		if d == 0.0 {
			return None;
		}
		return Some(n / d);
	}
	value.parse().ok()
}

fn patch_otio_for_kdenlive(value: &mut Value) {
	let mut duration_cache: HashMap<String, Option<f64>> = HashMap::new();
	patch_otio_for_kdenlive_inner(value, &mut duration_cache);
	reorder_otio_keys_like_library_export(value);
}

fn patch_otio_for_kdenlive_inner(
	value: &mut Value,
	duration_cache: &mut HashMap<String, Option<f64>>,
) {
	match value {
		Value::Array(arr) => {
			for v in arr {
				patch_otio_for_kdenlive_inner(v, duration_cache);
			}
		}
		Value::Object(map) => {
			let schema = map
				.get("OTIO_SCHEMA")
				.and_then(|v| v.as_str())
				.unwrap_or_default()
				.to_string();
			let is_clip2 = map
				.get("OTIO_SCHEMA")
				.and_then(|v| v.as_str())
				.map(|s| s == "Clip.2")
				.unwrap_or(false);
			if is_clip2 {
				map.entry("color".to_string()).or_insert(Value::Null);
				if let Some(media_ref) = map.remove("media_reference") {
					let mut refs = serde_json::Map::new();
					refs.insert("DEFAULT_MEDIA".to_string(), media_ref);
					map.insert("media_references".to_string(), Value::Object(refs));
					map.insert(
						"active_media_reference_key".to_string(),
						Value::String("DEFAULT_MEDIA".to_string()),
					);
				}
			}

			let is_linear_time_warp = map
				.get("OTIO_SCHEMA")
				.and_then(|v| v.as_str())
				.map(|s| s == "LinearTimeWarp.1")
				.unwrap_or(false);
			if is_linear_time_warp {
				if let Some(name) = map.remove("name") {
					map.insert("effect_name".to_string(), name);
				}
			}

			if is_clip2 {
				let clip_rate = map
					.get("source_range")
					.and_then(Value::as_object)
					.and_then(|r| r.get("start_time").or_else(|| r.get("duration")))
					.and_then(Value::as_object)
					.and_then(|t| t.get("rate"))
					.and_then(Value::as_f64)
					.filter(|v| v.is_finite() && *v > 0.0)
					.unwrap_or(DEFAULT_OTIO_RATE);

				if let Some(media_refs) = map.get_mut("media_references").and_then(Value::as_object_mut)
				{
					if let Some(default_media) = media_refs
						.get_mut("DEFAULT_MEDIA")
						.and_then(Value::as_object_mut)
					{
						let is_external_ref = default_media
							.get("OTIO_SCHEMA")
							.and_then(Value::as_str)
							.map(|s| s == "ExternalReference.1")
							.unwrap_or(false);
						let has_available_range = default_media
							.get("available_range")
							.map(|v| !v.is_null())
							.unwrap_or(false);

						if is_external_ref && !has_available_range {
							if let Some(target_url) = default_media
								.get("target_url")
								.and_then(Value::as_str)
							{
								if let Some(duration_secs) =
									probe_media_duration_cached(target_url, duration_cache)
								{
									default_media.insert(
										"available_range".to_string(),
										json!({
											"OTIO_SCHEMA": "TimeRange.1",
											"start_time": {
												"OTIO_SCHEMA": "RationalTime.1",
												"value": 0.0,
												"rate": clip_rate,
											},
											"duration": {
												"OTIO_SCHEMA": "RationalTime.1",
												"value": quantize_otio_frame_value(duration_secs * clip_rate),
												"rate": clip_rate,
											},
										}),
									);
								}
							}
						}
					}
				}
			}

			if schema == "Gap.1" {
				map.entry("color".to_string()).or_insert(Value::Null);
			}

			if schema == "ExternalReference.1" {
				map.entry("name".to_string())
					.or_insert_with(|| Value::String(String::new()));
			}

			for v in map.values_mut() {
				patch_otio_for_kdenlive_inner(v, duration_cache);
			}
		}
		_ => {}
	}
}

fn reorder_otio_keys_like_library_export(value: &mut Value) {
	match value {
		Value::Array(arr) => {
			for item in arr {
				reorder_otio_keys_like_library_export(item);
			}
		}
		Value::Object(map) => {
			for child in map.values_mut() {
				reorder_otio_keys_like_library_export(child);
			}

			let schema = map
				.get("OTIO_SCHEMA")
				.and_then(Value::as_str)
				.unwrap_or_default();
			let Some(order) = library_key_order_for_schema(schema) else {
				return;
			};

			let mut old = std::mem::take(map);
			let mut reordered = serde_json::Map::new();
			for key in order {
				if let Some(value) = old.remove(*key) {
					reordered.insert((*key).to_string(), value);
				}
			}
			for (key, value) in old {
				reordered.insert(key, value);
			}
			*map = reordered;
		}
		_ => {}
	}
}

fn library_key_order_for_schema(schema: &str) -> Option<&'static [&'static str]> {
	match schema {
		"Timeline.1" => Some(&[
			"OTIO_SCHEMA",
			"metadata",
			"name",
			"global_start_time",
			"tracks",
		]),
		"Stack.1" => Some(&[
			"OTIO_SCHEMA",
			"metadata",
			"name",
			"source_range",
			"effects",
			"markers",
			"enabled",
			"color",
			"children",
		]),
		"Track.1" => Some(&[
			"OTIO_SCHEMA",
			"metadata",
			"name",
			"source_range",
			"effects",
			"markers",
			"enabled",
			"color",
			"children",
			"kind",
		]),
		"Clip.2" => Some(&[
			"OTIO_SCHEMA",
			"metadata",
			"name",
			"source_range",
			"effects",
			"markers",
			"enabled",
			"color",
			"media_references",
			"active_media_reference_key",
		]),
		"Gap.1" => Some(&[
			"OTIO_SCHEMA",
			"metadata",
			"name",
			"source_range",
			"effects",
			"markers",
			"enabled",
			"color",
		]),
		"ExternalReference.1" => Some(&[
			"OTIO_SCHEMA",
			"metadata",
			"name",
			"available_range",
			"available_image_bounds",
			"target_url",
		]),
		_ => None,
	}
}

fn probe_media_duration_cached(
	target_url: &str,
	duration_cache: &mut HashMap<String, Option<f64>>,
) -> Option<f64> {
	if let Some(cached) = duration_cache.get(target_url) {
		return *cached;
	}

	let path = media_path_from_target_url(target_url);
	let duration = probe_media_duration(&path);
	duration_cache.insert(target_url.to_string(), duration);
	duration
}

fn media_path_from_target_url(target_url: &str) -> PathBuf {
	if target_url.starts_with("file://") {
		if let Ok(url) = url::Url::parse(target_url) {
			if let Ok(path) = url.to_file_path() {
				return path;
			}
		}
	}
	PathBuf::from(target_url)
}

fn probe_media_duration(path: &Path) -> Option<f64> {
	let src = path.to_str()?;
	let output = Command::new("ffprobe")
		.args([
			"-v",
			"error",
			"-show_entries",
			"format=duration",
			"-of",
			"default=noprint_wrappers=1:nokey=1",
			src,
		])
		.output()
		.ok()?;
	if !output.status.success() {
		return None;
	}
	let out = std::str::from_utf8(&output.stdout).ok()?.trim();
	let duration: f64 = out.parse().ok()?;
	if duration.is_finite() && duration > 0.0 {
		Some(duration)
	} else {
		None
	}
}

fn collect_otio_markers(pr: &Project, bound: &RenderBound, rate: f64) -> Vec<serde_json::Value> {
	let start = bound.start.with_precision(TIMELINE_PRECISION);
	let end = bound.end.with_precision(TIMELINE_PRECISION);
	let frame = if rate > 0.0 { 1.0 } else { 0.0 };

	pr.iter_markers_and_regions()
		.filter(|m| !m.is_region)
		.filter(|m| {
			let pos = m.position.with_precision(TIMELINE_PRECISION);
			pos >= start && pos <= end
		})
		.map(|marker| {
			let rel = quantize_otio_frame_value(
				(marker.position - start).as_duration().as_secs_f64() * rate,
			);
			let (r, g, b) = marker.color.get();
			json!({
				"OTIO_SCHEMA": "Marker.2",
				"name": marker.name,
				"color": otio_marker_color(r, g, b),
				"marked_range": {
					"OTIO_SCHEMA": "TimeRange.1",
					"start_time": {
						"OTIO_SCHEMA": "RationalTime.1",
						"value": rel,
						"rate": rate,
					},
					"duration": {
						"OTIO_SCHEMA": "RationalTime.1",
						"value": quantize_otio_frame_value(frame),
						"rate": rate,
					}
				},
				"metadata": {
					"reaper": {
						"color_rgb": [r, g, b],
						"color_hex": format!("#{r:02X}{g:02X}{b:02X}")
					}
				}
			})
		})
		.collect()
}

fn otio_marker_color(r: u8, g: u8, b: u8) -> &'static str {
	let max = r.max(g).max(b);
	let min = r.min(g).min(b);
	if max < 20 {
		return "BLACK";
	}
	if min > 230 {
		return "WHITE";
	}
	if r >= g && r >= b {
		if g > 180 {
			"YELLOW"
		} else if b > 140 {
			"MAGENTA"
		} else {
			"RED"
		}
	} else if g >= r && g >= b {
		if b > 150 {
			"CYAN"
		} else {
			"GREEN"
		}
	} else if r > 160 {
		"MAGENTA"
	} else {
		"BLUE"
	}
}

fn timecodes_output_path_for_target(render_target: &Path) -> PathBuf {
	render_target.with_extension("txt")
}

fn format_youtube_timecode(seconds: f64) -> String {
	let total = seconds.floor().max(0.0) as u64;
	let h = total / 3600;
	let m = (total % 3600) / 60;
	let s = total % 60;
	if h > 0 {
		format!("{h:02}:{m:02}:{s:02}")
	} else {
		format!("{m:02}:{s:02}")
	}
}

fn path_to_target_url(path: &Path) -> String {
	let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
	resolved.to_string_lossy().replace('\\', "/")
}

fn clip_name(path: &Path) -> String {
	path.file_name()
		.and_then(|v| v.to_str())
		.map(String::from)
		.unwrap_or_else(|| "clip".to_string())
}
