use std::{path::PathBuf, process::Command, time::Duration};

use crate::LevitanusError;

use super::base_types::{RenderSettings, Resolution};

use fraction::Fraction;
use log::debug;
use rea_rs::{
    project_info::{BoundsMode, RenderMode},
    Position, Project, Reaper, SourceOffset,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderSequence {
    input: Vec<String>,
    filter: Vec<String>,
    output: Vec<String>,
}

#[derive(Debug)]
pub struct Render {
    pub render_settings: RenderSettings,
}
impl Render {
    pub fn get_render_job(&self, timeline: TimeLine) -> Result<Command, LevitanusError> {
        // let (input_nodes, filter_nodes) = timeline.get_nodes()?;
        // println!(
        //     "inputs are:\n{:#?}\n filters are:\n{:#?}",
        //     input_nodes, filter_nodes
        // );
        let mut main_seq: Vec<String> = Vec::new();
        // for node in input_nodes.iter() {
        //     main_seq.extend(self.render_node(node)?);
        // }
        // main_seq.push("-filter_complex".to_string());
        // let mut filter_seq = Vec::new();
        // for node in filter_nodes.iter() {
        //     filter_seq.push(self.render_node(node)?);
        // }
        // main_seq.push(filter_seq.into_iter().map(|vec| vec.join("")).join(";"));

        // main_seq.push(format!("-c:v {}", self.render_settings.codec));
        // main_seq.extend([
        //     "-map".to_string(),
        //     format!("[{}]", filter_nodes.last().unwrap().outputs[0].get_name()),
        // ]);
        main_seq.push("-c:v".to_string());
        main_seq.push(format!("{}", self.render_settings.video_encoder));
        main_seq.extend(
            self.render_settings
                .video_encoder_options
                .iter()
                .filter_map(|opt| {
                    if let Some(par) = opt.parameter.ffmpeg_representation() {
                        Some([format!("-{}", opt.name), par])
                    } else {
                        None
                    }
                })
                .flatten(),
        );
        if let Some(audio_encoder) = &self.render_settings.audio_encoder {
            main_seq.push("-c:a".to_string());
            main_seq.push(format!("{}", audio_encoder));
        }
        main_seq.push("-r".to_string());
        main_seq.push(format!("{}", self.render_settings.fps));
        main_seq.extend(["-progress".to_string(), "pipe:1".to_string()]);
        main_seq.push(format!(
            "{}",
            timeline
                .outfile
                .with_extension(&self.render_settings.extension)
                .display()
        ));

        let mut ffmpeg = Command::new("ffmpeg");
        ffmpeg.arg("-hide_banner");
        ffmpeg.arg("-y");
        ffmpeg.args(main_seq);
        debug!("{:#?}", ffmpeg.get_args());
        Ok(ffmpeg)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeLineContent {
    content_type: TimeLineContentType,
    timeline_position: Position,
    timeline_end_position: Position,
    z_index: usize,
}
impl TimeLineContent {
    fn new(duration: Duration) -> Self {
        let z_index = Reaper::get().current_project().n_tracks();
        Self {
            content_type: TimeLineContentType::Background,
            timeline_position: Position::default(),
            timeline_end_position: Position::from(duration),
            z_index,
        }
    }
    fn push_video(&mut self, video: VideoInput) {
        assert!(
            video.track_index <= self.z_index,
            "pushing underlying video"
        );
        if video.fade_in.is_none()
            && video.fade_out.is_none()
            && video.timeline_position == self.timeline_position
            && video.timeline_end_position == self.timeline_end_position
        {
            self.z_index = video.track_index;
            self.content_type = Video::new(video).content_type;
            return;
        }
        let solid_start = match video.fade_in {
            None => video.timeline_position,
            Some(d) => video.timeline_position + d.into(),
        };
        let solid_end = match video.fade_out {
            None => video.timeline_end_position,
            Some(d) => video.timeline_end_position - d.into(),
        };
        let self_left = match solid_start == self.timeline_position {
            true => None,
            false => {
                let (left, _right) = self.split(solid_start);
                Some(left)
            }
        };
        let self_right = match solid_end == self.timeline_end_position {
            true => None,
            false => {
                let (_left, right) = self.split(solid_end);
                Some(right)
            }
        };
        // debug!("self_left: {:?},\nself_right: {:?}", self_right, self_right);
        let fade_out = video.fade_out.clone();
        // debug!("fade_out: {:?}", fade_out);
        let left = match self_left {
            None => Video::new(video),
            Some(left) => match video.fade_in {
                None => Concat::new(left, Video::new(video)),
                Some(d) => FadeX::new(left, Video::new(video), d),
            },
        };
        // debug!("left: {:?}", left);
        match self_right {
            None => {
                self.z_index = left.z_index;
                self.content_type = left.content_type;
            }
            Some(right) => match fade_out {
                None => {
                    let content = Concat::new(left, right);
                    self.z_index = content.z_index;
                    self.content_type = content.content_type;
                }
                Some(d) => {
                    let content = FadeX::new(left, right, d);
                    self.z_index = content.z_index;
                    self.content_type = content.content_type;
                }
            },
        };
    }
    fn split(&self, position: Position) -> (TimeLineContent, TimeLineContent) {
        match self.content_type.clone() {
            TimeLineContentType::Background => {
                let left = TimeLineContent {
                    content_type: TimeLineContentType::Background,
                    timeline_position: self.timeline_position,
                    timeline_end_position: position,
                    z_index: self.z_index,
                };
                let right = TimeLineContent {
                    content_type: TimeLineContentType::Background,
                    timeline_position: position,
                    timeline_end_position: self.timeline_end_position,
                    z_index: self.z_index,
                };
                (left, right)
            }
            TimeLineContentType::Video(v) => {
                let mut left = v.clone();
                let mut right = v.clone();
                left.fade_out = None;
                right.fade_in = None;
                right.source_offset = SourceOffset::from_secs_f64(
                    right.source_offset.as_secs_f64() + position.as_duration().as_secs_f64(),
                );
                let left = TimeLineContent {
                    content_type: TimeLineContentType::Video(left),
                    timeline_position: self.timeline_position,
                    timeline_end_position: position,
                    z_index: self.z_index,
                };
                let right = TimeLineContent {
                    content_type: TimeLineContentType::Video(right),
                    timeline_position: position,
                    timeline_end_position: self.timeline_end_position,
                    z_index: self.z_index,
                };
                (left, right)
            }
            TimeLineContentType::Concat(concat) => {
                if position == concat.left.timeline_end_position {
                    (*concat.left, *concat.right)
                } else {
                    let (left, center, right) = if position < concat.left.timeline_end_position {
                        let (left, center) = concat.left.split(position);
                        (left, center, *concat.right)
                    } else {
                        let (center, right) = concat.right.split(position);
                        (*concat.left, center, right)
                    };
                    if center.timeline_position == position {
                        (left, Concat::new(center, right))
                    } else {
                        (Concat::new(left, center), right)
                    }
                }
            }
            TimeLineContentType::FadeX(fadex) => {
                if position.as_duration()
                    <= fadex.left.timeline_end_position.as_duration() - fadex.fade_duration
                {
                    let (left, right) = fadex.left.split(position);
                    (left, FadeX::new(right, *fadex.right, fadex.fade_duration))
                } else if fadex.right.timeline_position.as_duration() + fadex.fade_duration
                    <= position.as_duration()
                {
                    let (left, right) = fadex.right.split(position);
                    (FadeX::new(left, *fadex.left, fadex.fade_duration), right)
                } else {
                    let (l_left, l_right) = fadex.left.split(position);
                    let (r_left, r_right) = fadex.right.split(position);
                    let l_d =
                        (r_left.timeline_end_position - r_left.timeline_end_position).as_duration();
                    let r_d = (l_right.timeline_end_position - l_right.timeline_end_position)
                        .as_duration();
                    let left = FadeX::new(l_left, r_left, l_d);
                    let right = FadeX::new(l_right, r_right, r_d);
                    (left, right)
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TimeLineContentType {
    Background,
    Concat(Concat),
    FadeX(FadeX),
    Video(Video),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Concat {
    left: Box<TimeLineContent>,
    right: Box<TimeLineContent>,
}
impl Concat {
    fn new(left: TimeLineContent, right: TimeLineContent) -> TimeLineContent {
        assert_eq!(
            left.timeline_end_position, right.timeline_position,
            "wrong connection"
        );
        let timeline_position = left.timeline_position;
        let timeline_end_position = right.timeline_end_position;
        let z_index = left.z_index.min(right.z_index);
        TimeLineContent {
            content_type: TimeLineContentType::Concat(Concat {
                left: Box::new(left),
                right: Box::new(right),
            }),
            timeline_position,
            timeline_end_position,
            z_index,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FadeX {
    left: Box<TimeLineContent>,
    right: Box<TimeLineContent>,
    fade_duration: Duration,
}
impl FadeX {
    fn new(left: TimeLineContent, right: TimeLineContent, duration: Duration) -> TimeLineContent {
        assert_eq!(
            left.timeline_end_position - duration.into(),
            right.timeline_position,
            "wrong duration length. left: {:#?}, right: {:#?}, duration: {:?}",
            left,
            right,
            duration
        );
        let z_index = left.z_index.min(right.z_index);
        let timeline_position = left.timeline_position;
        let timeline_end_position = right.timeline_end_position;
        TimeLineContent {
            content_type: TimeLineContentType::FadeX(FadeX {
                left: Box::new(left),
                right: Box::new(right),
                fade_duration: duration,
            }),
            timeline_position,
            timeline_end_position,
            z_index,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Video {
    file: PathBuf,
    fade_in: Option<Duration>,
    fade_out: Option<Duration>,
    source_offset: SourceOffset,
}
impl Video {
    fn new(video: VideoInput) -> TimeLineContent {
        TimeLineContent {
            content_type: TimeLineContentType::Video(Video {
                file: video.file,
                fade_in: video.fade_in,
                fade_out: video.fade_out,
                source_offset: video.source_offset,
            }),
            timeline_position: video.timeline_position,
            timeline_end_position: video.timeline_end_position,
            z_index: video.track_index,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VideoInput {
    file: PathBuf,
    timeline_position: Position,
    timeline_end_position: Position,
    source_offset: SourceOffset,
    fade_in: Option<Duration>,
    fade_out: Option<Duration>,
    fade_out_is_x_fade: bool,
    track_index: usize,
}
impl VideoInput {
    fn split_at(self, timeline_position: Position) -> (Self, Self) {
        let mut head = self.clone();
        let mut tail = self;
        head.timeline_end_position = timeline_position;
        tail.source_offset =
            tail.source_offset + (timeline_position - tail.timeline_position).as_duration();
        tail.timeline_position = timeline_position;
        (head, tail)
    }
    fn get_length(&self) -> Duration {
        (self.timeline_end_position - self.timeline_position).as_duration()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeLine {
    pub outfile: PathBuf,
    start: Position,
    end: Position,
    resolution: Resolution,
    pad_color: String,
    fps: Fraction,
    content: TimeLineContent,
}
impl TimeLine {
    fn new(
        outfile: PathBuf,
        start: Position,
        end: Position,
        render_settings: RenderSettings,
    ) -> Self {
        let content_duration = (end - start).as_duration();
        Self {
            outfile,
            start,
            end,
            resolution: render_settings.resolution,
            pad_color: render_settings.pad_color.ffmpeg_representation(),
            fps: render_settings.fps,
            content: TimeLineContent::new(content_duration),
        }
    }
    pub fn duration(&self) -> Duration {
        (self.end - self.start).as_duration()
    }
    fn push(&mut self, input: VideoInput) {
        self.content.push_video(input)
    }
}

pub fn build_render_timelines(render_settings: &RenderSettings) -> anyhow::Result<Vec<TimeLine>> {
    let render_regions = get_render_regions()?;
    let timelines = render_regions
        .into_iter()
        .map(|reg| build_timeline(reg, render_settings.clone()));
    Ok(timelines.collect())
}

fn build_timeline(render_region: RenderRegion, render_settings: RenderSettings) -> TimeLine {
    let rpr = Reaper::get();
    let pr = rpr.current_project();
    let (start, end) = (render_region.start, render_region.end);
    let mut timeline = TimeLine::new(render_region.file, start, end, render_settings);
    for track in pr.iter_tracks().rev() {
        for idx in 0..track.n_items() {
            let item = track
                .get_item(idx)
                .expect(&format!("can not get item with index {idx}"));
            if item.position() >= end {
                continue;
            }
            if item.end_position() <= start {
                continue;
            }
            if item.is_muted() {
                continue;
            }
            let take = item.active_take();
            let source = take.source().expect("can not get take source");
            if source.type_string() != "VIDEO" {
                continue;
            }
            let file = source.filename();
            debug!("file: {:?}", file);
            let timeline_position = if start < item.position() {
                item.position() - start
            } else {
                Position::from(0.0)
            };
            debug!("timeline_position: {:?}", timeline_position);
            let timeline_end_position = if end > item.end_position() {
                item.end_position() - start
            } else {
                end - start
            };
            debug!("timeline_end_position: {:?}", timeline_end_position);
            let source_offset = if start > item.position() {
                item.active_take().start_offset() + (start - item.position()).as_duration()
            } else {
                item.active_take().start_offset()
            };
            debug!("source_offset: {:?}", source_offset);
            let fade_in = item.fade_in().length;
            let fade_out = item.fade_out().length;
            debug!("fade_in: {:?}, fade_out: {:?}", fade_in, fade_out);

            timeline.push(VideoInput {
                file,
                timeline_position,
                timeline_end_position,
                source_offset,
                fade_in: if fade_in.is_zero() {
                    None
                } else {
                    Some(fade_in)
                },
                fade_out: if fade_out.is_zero() {
                    None
                } else {
                    Some(fade_out)
                },
                fade_out_is_x_fade: false,
                track_index: track.index(),
            })
        }
    }
    debug!("{:#?}", timeline);
    timeline
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderRegion {
    start: Position,
    end: Position,
    file: PathBuf,
}

fn get_render_targets(pr: &Project, idx: usize) -> anyhow::Result<PathBuf> {
    debug!("idx:{}", idx);
    let string = pr
        .get_render_targets()
        .map_err(|e| LevitanusError::Reaper(e.to_string()))?
        .get(idx)
        .ok_or(LevitanusError::Render(
            "Can not estimate region output filename.".to_string(),
        ))?
        .clone();
    debug!("string:{}", string);
    Ok(PathBuf::from(string))
}

fn get_render_regions() -> anyhow::Result<Vec<RenderRegion>> {
    let rpr = Reaper::get();
    let pr = rpr.current_project();
    let settings = pr.get_render_settings();
    match settings.mode {
        RenderMode::MasterMix => match pr.get_render_bounds_mode() {
            BoundsMode::EntireProject => Ok(vec![RenderRegion {
                start: Position::from(0.0),
                end: pr.length().into(),
                file: get_render_targets(&pr, 0)?,
            }]),
            BoundsMode::Custom => {
                let (start, end) = pr.get_render_bounds();
                Ok(vec![RenderRegion {
                    start,
                    end,
                    file: get_render_targets(&pr, 0)?,
                }])
            }
            BoundsMode::TimeSelection => {
                let ts = pr.get_time_selection();
                Ok(vec![RenderRegion {
                    start: ts.get_start(),
                    end: ts.get_end(),
                    file: get_render_targets(&pr, 0)?,
                }])
            }
            BoundsMode::AllRegions => {
                let mut bounds = Vec::new();
                for (idx, region) in pr.iter_markers_and_regions().enumerate() {
                    if !region.is_region {
                        continue;
                    }
                    let file = get_render_targets(&pr, idx)?;

                    bounds.push(RenderRegion {
                        start: region.position,
                        end: region.rgn_end,
                        file,
                    });
                }
                Ok(bounds)
            }
            BoundsMode::SelectedItems => Err(LevitanusError::Render(
                "No support for rendering selected items.".to_string(),
            )
            .into()),
            BoundsMode::SelectedRegions => Err(LevitanusError::Render(
                "No support for render Matrix (selected regions)".to_string(),
            )
            .into()),
        },
        _ => Err(LevitanusError::Render(
            "currently, supports just render with MasterMix in render settings".to_string(),
        )
        .into()),
    }
}
