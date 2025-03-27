use std::error::Error;
use std::fmt::Display;
use std::io::{self, Write};
use std::{path::PathBuf, process::Command, time::Duration};

use crate::LevitanusError;

use super::filters::{Filter, ScaleAspectRationOption};
use super::nodes::{Node, NodeContent, Pin};
use super::options::{FfmpegColor, Opt};

use fraction::Fraction;
use itertools::Itertools;
use lazy_static::lazy_static;
use log::debug;
use rea_rs::{
    project_info::{BoundsMode, RenderMode},
    Position, Project, Reaper, SourceOffset,
};
use regex::Regex;
use serde::{Deserialize, Serialize};

lazy_static! {
    static ref RES_RE: Regex =
        Regex::new(r"(?<width>\d+)x(?<height>\d+)").expect("can not compile opts regex");
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderSettings {
    pub muxer: String,
    pub muxer_options: Vec<Opt>,
    pub extension: String,
    pub video_encoder: String,
    pub video_encoder_options: Vec<Opt>,
    pub audio_encoder: Option<String>,
    pub audio_encoder_options: Vec<Opt>,
    pub subtitle_encoder: Option<String>,
    pub subtitle_encoder_options: Vec<Opt>,
    pub fps: Fraction,
    pub pixel_format: String,
    pub resolution: Resolution,
    pub pad_color: FfmpegColor,
}
impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            muxer: "matroska".to_string(),
            muxer_options: Vec::new(),
            extension: "mkv".to_string(),
            video_encoder: "libx264".to_string(),
            video_encoder_options: Vec::new(),
            audio_encoder: Some("aac".to_string()),
            audio_encoder_options: Vec::new(),
            subtitle_encoder: Some("ass".to_string()),
            subtitle_encoder_options: Vec::new(),
            fps: Fraction::new(3000_u64, 1001_u64),
            pixel_format: "yuv420p".to_string(),
            resolution: Resolution::default(),
            pad_color: FfmpegColor::new(0, 0xff),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resolution {
    pub width: usize,
    pub height: usize,
}
impl Default for Resolution {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
        }
    }
}
impl Display for Resolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}
impl Resolution {
    pub fn from_file(file: PathBuf) -> Result<Self, anyhow::Error> {
        // ffprobe -v error -select_streams v -show_entries stream=width,height -of csv=p=0:s=x input.m4v
        let mut ffprobe = Command::new("ffprobe");
        ffprobe.args([
            "-v",
            "error",
            "-select_streams",
            "v",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0:s=x",
            match file.to_str() {
                Some(s) => s,
                None => {
                    return Err(LevitanusError::Unexpected(
                        "Can not convert pathbuf to str".to_string(),
                    )
                    .into())
                }
            },
        ]);
        let output = ffprobe.output()?;
        let out = std::str::from_utf8(&output.stdout)?;
        debug!("filename: {:?}, ffprobe output: {}", file, out);
        if let Some(cap) = RES_RE.captures(out) {
            Ok(Self {
                width: cap["width"].parse()?,
                height: cap["height"].parse()?,
            })
        } else {
            Err(
                LevitanusError::Unexpected("Can not parse resolution from output".to_string())
                    .into(),
            )
        }
    }
}

pub trait Timestamp {
    fn timestump(&self) -> String;
}
impl Timestamp for Duration {
    fn timestump(&self) -> String {
        let hours = self.as_secs() / 60 / 60;
        let mins = self.as_secs() / 60;
        let secs = self.as_secs();
        let millis = self.subsec_millis();
        format!("{hours}:{mins}:{secs}.{millis}")
    }
}

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
    pub fn render_timelines(&self, timelines: Vec<TimeLine>) -> Result<(), Box<dyn Error>> {
        for timeline in timelines {
            self.render_timeline(timeline)?;
        }
        Ok(())
    }
    pub fn get_render_job(&self, timeline: TimeLine) -> Result<Command, LevitanusError> {
        let (input_nodes, filter_nodes) = timeline.get_nodes()?;
        println!(
            "inputs are:\n{:#?}\n filters are:\n{:#?}",
            input_nodes, filter_nodes
        );
        let mut main_seq: Vec<String> = Vec::new();
        for node in input_nodes.iter() {
            main_seq.extend(self.render_node(node)?);
        }
        main_seq.push("-filter_complex".to_string());
        let mut filter_seq = Vec::new();
        for node in filter_nodes.iter() {
            filter_seq.push(self.render_node(node)?);
        }
        main_seq.push(filter_seq.into_iter().map(|vec| vec.join("")).join(";"));

        // main_seq.push(format!("-c:v {}", self.render_settings.codec));
        main_seq.extend([
            "-map".to_string(),
            format!("[{}]", filter_nodes.last().unwrap().outputs[0].get_name()),
        ]);
        main_seq.push("-c:v".to_string());
        // main_seq.push(format!("{}", self.render_settings.encoder));
        // main_seq.extend(self.render_settings.encoder_options.clone());
        main_seq.push("-r".to_string());
        main_seq.push(format!("{}", self.render_settings.fps));
        main_seq.push(format!(
            "{}",
            timeline
                .outfile
                // .with_extension(&self.render_settings.muxer)
                .display()
        ));

        let mut ffmpeg = Command::new("ffmpeg");
        ffmpeg.arg("-hide_banner");
        ffmpeg.arg("-y");
        ffmpeg.args(main_seq);
        println!("{:#?}", ffmpeg.get_args());
        Ok(ffmpeg)
    }
    fn render_timeline(&self, timeline: TimeLine) -> anyhow::Result<()> {
        println!("rendering timleline:\n{:#?}", timeline);
        let mut ffmpeg = self.get_render_job(timeline)?;

        let output = ffmpeg.output()?;
        // println!("{:?}", out.status);
        println!("status: {}", output.status);
        io::stdout().write_all(&output.stdout).unwrap();
        io::stderr().write_all(&output.stderr).unwrap();

        Ok(())
    }
    fn render_node(&self, node: &Node) -> Result<Vec<String>, LevitanusError> {
        let mut node_seq = Vec::new();
        match &node.content {
            NodeContent::Input {
                file,
                source_offset,
                length,
            } => {
                node_seq.push("-ss".to_string());
                node_seq.push(source_offset.as_duration().as_secs_f64().to_string());
                node_seq.push("-t".to_string());
                node_seq.push(length.as_secs_f64().to_string());
                node_seq.push("-i".to_string());
                node_seq.push(
                    file.to_str()
                        .expect("Can not convert filename to string")
                        .to_string(),
                );
            }
            NodeContent::Filter(filter) => {
                for input in &node.inputs {
                    node_seq.push(format!(
                        "[{}]",
                        input
                            .get_target()
                            .ok_or(LevitanusError::Render("No input in sink".to_string()))?
                    ))
                }
                node_seq.push(filter.get_render_string());
                for out in &node.outputs {
                    node_seq.push(format!("[{}]", out.get_name()))
                }
            }
        }
        Ok(node_seq)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeLine {
    outfile: PathBuf,
    _start: Position,
    _end: Position,
    resolution: Resolution,
    pad_color: String,
    fps: Fraction,
    inputs: Vec<VideoInput>,
}
impl TimeLine {
    fn new(
        outfile: PathBuf,
        start: Position,
        end: Position,
        render_settings: RenderSettings,
    ) -> Self {
        Self {
            outfile,
            _start: start,
            _end: end,
            resolution: render_settings.resolution,
            pad_color: render_settings.pad_color.ffmpeg_representation(),
            fps: render_settings.fps,
            inputs: Vec::new(),
        }
    }
    fn _length(&self) -> Duration {
        (self._end - self._start).as_duration()
    }
    fn push(&mut self, mut input: VideoInput) {
        for (idx, item) in self.inputs.iter_mut().enumerate() {
            //    item
            // input
            if input.timeline_position < item.timeline_position {
                //       item
                // input
                if input.timeline_end_position <= item.timeline_position {
                    self.inputs.insert(idx, input);
                    return;
                }
                //     item
                // input
                if input.timeline_end_position <= item.timeline_end_position {
                    input.check_xfade_at_end(item);
                    self.inputs.insert(idx, input);
                    return;
                }
                //    item
                //  input--|
                if input.timeline_end_position > item.timeline_end_position {
                    match item.fade_out {
                        None => {
                            let (mut head, tail) = input.split_at(item.timeline_end_position);
                            head.check_xfade_at_end(item);
                            self.inputs.insert(idx, head);
                            return self.push(tail);
                        }
                        Some(fade) => {
                            let (mut head, tail) =
                                input.split_at(item.timeline_end_position - fade.into());
                            head.check_xfade_at_end(item);
                            self.inputs.insert(idx, head);
                            return self.push(tail);
                        }
                    }
                }
            }
            // item
            // input
            if input.timeline_position >= item.timeline_position {
                //   item--|
                //   input
                if item.timeline_end_position > input.timeline_end_position {
                    return;
                }
                // item
                //      input
                if input.timeline_position >= item.timeline_end_position {
                    continue;
                }
                // item
                // input--|
                if input.timeline_end_position > item.timeline_end_position {
                    match item.fade_out {
                        None => {
                            let (_, tail) = input.split_at(item.timeline_end_position);
                            return self.push(tail);
                        }
                        Some(fade) => {
                            if item.fade_out_is_x_fade {
                                let (_, tail) = input.split_at(item.timeline_end_position);
                                return self.push(tail);
                            }
                            let (_, tail) =
                                input.split_at(item.timeline_end_position - fade.into());
                            input = tail;
                            item.check_xfade_at_end(&mut input);
                            continue;
                        }
                    }
                }
            }
        }
        self.inputs.push(input);
    }
    fn get_nodes(&self) -> Result<(Vec<Node>, Vec<Node>), LevitanusError> {
        let mut nodes = Vec::new();
        let mut input_nodes = Vec::new();
        let mut filter_nodes: Vec<Vec<Node>> = Vec::new();
        for (idx, input) in self.inputs.iter().enumerate() {
            let mut input_node = Node {
                inputs: Vec::new(),
                outputs: vec![Pin::Video {
                    name: format!("{idx}:v"),
                    target: None,
                }],
                content: NodeContent::Input {
                    file: input.file.clone(),
                    source_offset: input.source_offset.into(),
                    length: input.get_length(),
                },
            };
            let mut filters_connected = Vec::new();
            let mut fps = Node {
                inputs: vec![Pin::Video {
                    name: format!("sc{idx}"),
                    target: None,
                }],
                outputs: vec![Pin::Video {
                    name: format!("scout{idx}"),
                    target: None,
                }],
                content: NodeContent::Filter(Filter::Fps {
                    fps: Some(self.fps.to_string()),
                    start_time: None,
                    round: None,
                    round_eof: None,
                }),
            };
            fps.connect_sink(&mut input_node, 0, 0)?;
            let mut scale = Node {
                inputs: vec![Pin::Video {
                    name: format!("sc{idx}"),
                    target: None,
                }],
                outputs: vec![Pin::Video {
                    name: format!("scout{idx}"),
                    target: None,
                }],
                content: NodeContent::Filter(Filter::new_scale(
                    self.resolution.width,
                    self.resolution.height,
                    None,
                    ScaleAspectRationOption::decrease,
                    2,
                )),
            };
            fps.connect_source(&mut scale, 0, 0)?;
            let mut pad = Node {
                inputs: vec![Pin::Video {
                    name: format!("sc{idx}"),
                    target: None,
                }],
                outputs: vec![Pin::Video {
                    name: format!("scout{idx}"),
                    target: None,
                }],
                content: NodeContent::Filter(Filter::Pad {
                    width: Some(self.resolution.width.to_string()),
                    height: Some(self.resolution.height.to_string()),
                    x: Some(format!("{}/2-iw/2", self.resolution.width)),
                    y: Some(format!("{}/2-ih/2", self.resolution.height)),
                    color: Some(self.pad_color.clone()),
                    aspect: None,
                }),
            };
            scale.connect_source(&mut pad, 0, 0)?;
            let mut setsar = Node {
                inputs: vec![Pin::Video {
                    name: format!("sc{idx}"),
                    target: None,
                }],
                outputs: vec![Pin::Video {
                    name: format!("scout{idx}"),
                    target: None,
                }],
                content: NodeContent::Filter(Filter::Setsar {
                    ratio: "1:1".to_string().into(),
                    max: None,
                }),
            };
            pad.connect_source(&mut setsar, 0, 0)?;
            filters_connected.push(fps);
            filters_connected.push(scale);
            filters_connected.push(pad);
            filters_connected.push(setsar);
            for (idx, filter) in input.item_filters.iter().enumerate() {
                let mut filter = filter.clone();
                if idx == 1 {
                    filters_connected
                        .last_mut()
                        .expect("no last filter in chain")
                        .connect_source(&mut filter, 0, 0)?;
                }
                filters_connected.push(filter);
            }
            for (idx, filter) in input.track_filters.iter().enumerate() {
                let mut filter = filter.clone();
                if idx == 0 {
                    filters_connected
                        .last_mut()
                        .expect("no last filter in chain")
                        .connect_source(&mut filter, 0, 0)?;
                }
                filters_connected.push(filter);
            }
            if let Some(_) = input_nodes.last_mut() {
                let prev_input = self
                    .inputs
                    .get(idx - 1)
                    .expect("there is no previous input");
                if prev_input.fade_out_is_x_fade {
                    let mut fade_out = Node {
                        inputs: vec![
                            Pin::Video {
                                name: format!("xf{idx}_1st"),
                                target: None,
                            },
                            Pin::Video {
                                name: format!("xf{idx}_2nd"),
                                target: None,
                            },
                        ],
                        outputs: vec![Pin::Video {
                            name: format!("xf{idx}out"),
                            target: None,
                        }],
                        content: NodeContent::Filter(Filter::XFade {
                            transition: None,
                            duration: prev_input.fade_out.expect("no fade_in on prev input"),
                            offset: (prev_input.get_length()
                                - prev_input.fade_out.expect("no fade_out in prev input"))
                            .into(),
                            expression: None,
                        }),
                    };
                    fade_out.connect_sink(
                        filter_nodes
                            .last_mut()
                            .expect("no last filter chain")
                            .last_mut()
                            .expect("no last filter in chain"),
                        0,
                        0,
                    )?;
                    fade_out.connect_sink(
                        filters_connected
                            .last_mut()
                            .expect("no last in filter chain"),
                        1,
                        0,
                    )?;

                    filters_connected.push(fade_out);
                    filter_nodes
                        .last_mut()
                        .expect("no last filter_nodes chain")
                        .append(&mut filters_connected);
                } else {
                    filter_nodes.push(filters_connected);
                }
            } else {
                filter_nodes.push(filters_connected);
            }
            input_nodes.push(input_node);
        }
        if filter_nodes.len() > 1 {
            let n_inputs = filter_nodes.len();
            let mut concat_inputs = Vec::new();
            for i in 0..n_inputs {
                concat_inputs.push(Pin::Video {
                    name: format!("cncti{i}"),
                    target: None,
                });
            }
            let mut concat = Node {
                inputs: concat_inputs,
                outputs: vec![Pin::Video {
                    name: "cncto".to_string(),
                    target: None,
                }],
                content: NodeContent::Filter(Filter::Concat {
                    segments: n_inputs,
                    video_streams: 1,
                    audio_streams: 0,
                    unsafe_mode: false,
                }),
            };
            for (i, filters) in filter_nodes.iter_mut().enumerate() {
                concat.connect_sink(filters.last_mut().expect("no last filter in chain"), i, 0)?;
            }
            filter_nodes
                .last_mut()
                .expect("no last chain in filter nodes")
                .push(concat);
        }
        nodes.extend(filter_nodes.into_iter().flatten());
        Ok((input_nodes, nodes))
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
    item_filters: Vec<Node>,
    track_filters: Vec<Node>,
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
    /// check inputs timeline boundaries, fade_in and fade_out.
    /// Shorten self at need and setting fades to the same value,
    /// if x_fade should be applied. Also sets fade_out_is_x_fade to true, if need.
    fn check_xfade_at_end(&mut self, other: &mut VideoInput) {
        let fade_duration = match self.fade_out {
            None => match other.fade_in {
                None => None,
                Some(fade) => Some(fade),
            },
            Some(fade) => match other.fade_out {
                None => Some(fade),
                Some(o_fade) => {
                    if fade > o_fade {
                        Some(fade)
                    } else {
                        Some(o_fade)
                    }
                }
            },
        };
        match fade_duration {
            None => self.timeline_end_position = other.timeline_position,
            Some(fade) => self.resolve_overlaps(other, fade),
        }
    }

    /// find the length of x-fade and set inputs boundaries.
    fn resolve_overlaps(&mut self, other: &mut VideoInput, fade: Duration) {
        let overlap = (self.timeline_end_position - other.timeline_position).as_duration();
        if fade > overlap {
            self.fade_out = Some(overlap);
            other.fade_in = Some(overlap);
        } else {
            self.timeline_end_position = other.timeline_position + fade.into();
            self.fade_out = Some(fade);
        }
        self.fade_out_is_x_fade = true;
    }
    fn get_length(&self) -> Duration {
        (self.timeline_end_position - self.timeline_position).as_duration()
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
    for track in pr.iter_tracks() {
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
            // println!("file: {:?}", file);
            let timeline_position = if start < item.position() {
                item.position() - start
            } else {
                Position::from(0.0)
            };
            // println!("timeline_position: {:?}", timeline_position);
            let timeline_end_position = if end > item.end_position() {
                item.end_position() - start
            } else {
                end - start
            };
            // println!("timeline_end_position: {:?}", timeline_end_position);
            let source_offset = if start > item.position() {
                item.active_take().start_offset() + (item.position() - start).as_duration()
            } else {
                item.active_take().start_offset()
            };
            // println!("source_offset: {:?}", source_offset);
            let fade_in = item.fade_in().length;
            let fade_out = item.fade_out().length;
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
                item_filters: Vec::new(),
                track_filters: Vec::new(),
            })
        }
    }
    timeline
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderRegion {
    start: Position,
    end: Position,
    file: PathBuf,
}

fn get_render_targets(pr: &Project, idx: usize) -> anyhow::Result<PathBuf> {
    let string = pr
        .get_render_targets()
        .map_err(|e| LevitanusError::Reaper(e.to_string()))?
        .get(idx)
        .ok_or(LevitanusError::Render(
            "Can not estimate region output filename.".to_string(),
        ))?
        .clone();

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
