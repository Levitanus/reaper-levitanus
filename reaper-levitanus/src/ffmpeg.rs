use std::io::{self, Write};
use std::{error::Error, path::PathBuf, process::Command, time::Duration};

use fraction::Fraction;
use itertools::Itertools;
use rea_rs::{
    project_info::{BoundsMode, RenderMode},
    Position, Project, Reaper, SourceOffset,
};
use serde::{Deserialize, Serialize};

fn build_render_timelines(
    render_settings: &RenderSettings,
) -> Result<Vec<TimeLine>, Box<dyn Error>> {
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

#[derive(Debug)]
struct TimeLine {
    outfile: PathBuf,
    start: Position,
    end: Position,
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
            start,
            end,
            resolution: render_settings.resolution,
            pad_color: render_settings.pad_color,
            fps: render_settings.fps,
            inputs: Vec::new(),
        }
    }
    fn length(&self) -> Duration {
        (self.end - self.start).as_duration()
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
    fn get_nodes(&self) -> Result<(Vec<Node>, Vec<Node>), String> {
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
            if let Some(prev_input_node) = input_nodes.last_mut() {
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

#[derive(Debug, Clone, Deserialize, Serialize)]
enum NodeContent {
    Filter(Filter),
    Input {
        file: PathBuf,
        source_offset: Position,
        length: Duration,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Node {
    inputs: Vec<Pin>,
    outputs: Vec<Pin>,
    content: NodeContent,
}
impl Node {
    fn get_name(&self) -> String {
        match &self.content {
            NodeContent::Filter(f) => f.name().into(),
            NodeContent::Input {
                file,
                source_offset: _,
                length: _,
            } => file
                .file_name()
                .expect("no base filename")
                .to_str()
                .expect("can not convert path to string")
                .to_string(),
        }
    }
    fn connect_sink(
        &mut self,
        other: &mut Node,
        sink_index: usize,
        source_index: usize,
    ) -> Result<(), String> {
        let sink = match self.inputs.get(sink_index) {
            Some(sink) => sink,
            None => return Err(format!("can not get sink with index: {sink_index}")),
        };
        let source = match other.outputs.get(source_index) {
            Some(source) => source,
            None => return Err(format!("can not get sink with index: {sink_index}")),
        };
        let new_sink = sink.clone().with_target(Some(source.get_name()));
        let new_source = source.clone().with_target(Some(sink.get_name()));
        self.inputs[sink_index] = new_sink;
        other.outputs[source_index] = new_source;
        Ok(())
    }
    fn connect_source(
        &mut self,
        other: &mut Node,
        source_index: usize,
        sink_index: usize,
    ) -> Result<(), String> {
        let sink = match other.inputs.get(sink_index) {
            Some(sink) => sink,
            None => return Err(format!("can not get sink with index: {sink_index}")),
        };
        let source = match self.outputs.get(source_index) {
            Some(source) => source,
            None => return Err(format!("can not get sink with index: {sink_index}")),
        };
        let new_sink = sink.clone().with_target(Some(source.get_name()));
        let new_source = source.clone().with_target(Some(sink.get_name()));
        other.inputs[sink_index] = new_sink;
        self.outputs[source_index] = new_source;
        Ok(())
    }
    fn get_sink_target(&self, sink_index: usize) -> Result<Option<String>, String> {
        match self.inputs.get(sink_index) {
            Some(sink) => Ok(sink.get_target()),
            None => Err(format!("no sink with index: {sink_index}")),
        }
    }
    fn get_sink_name(&self, sink_index: usize) -> Result<String, String> {
        match self.inputs.get(sink_index) {
            Some(sink) => Ok(sink.get_name()),
            None => Err(format!("no sink with index: {sink_index}")),
        }
    }
    fn get_source_target(&self, source_index: usize) -> Result<Option<String>, String> {
        match self.outputs.get(source_index) {
            Some(source) => Ok(source.get_target()),
            None => Err(format!("no source with index: {source_index}")),
        }
    }
    fn get_source_name(&self, source_index: usize) -> Result<String, String> {
        match self.outputs.get(source_index) {
            Some(source) => Ok(source.get_name()),
            None => Err(format!("no source with index: {source_index}")),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
enum Pin {
    Video {
        name: String,
        target: Option<String>,
    },
    Audio {
        name: String,
        target: Option<String>,
    },
}
impl Pin {
    fn get_name(&self) -> String {
        match self {
            Pin::Video { name, target: _ } => name.clone(),
            Pin::Audio { name, target: _ } => name.clone(),
        }
    }
    fn get_target(&self) -> Option<String> {
        match self {
            Pin::Video { name: _, target } => target.clone(),
            Pin::Audio { name: _, target } => target.clone(),
        }
    }
    fn with_target(self, target: Option<String>) -> Self {
        match self {
            Pin::Video { name, target: _ } => Pin::Video { name, target },
            Pin::Audio { name, target: _ } => Pin::Audio { name, target },
        }
    }
    fn connect(self, other: Pin) -> Result<(Self, Self), String> {
        match self {
            Pin::Video { name, target: _ } => match other {
                Pin::Video {
                    name: other_name,
                    target: _,
                } => Ok((
                    Pin::Video {
                        name: name.clone(),
                        target: Some(other_name.clone()),
                    },
                    Pin::Video {
                        name: other_name,
                        target: Some(name),
                    },
                )),
                Pin::Audio {
                    name: other_name,
                    target: _,
                } => Err(format!(
                    "can not connect Video Pin {name} to Audio Pin {other_name}"
                )),
            },
            Pin::Audio { name, target: _ } => match other {
                Pin::Audio {
                    name: other_name,
                    target: _,
                } => Ok((
                    Pin::Audio {
                        name: name.clone(),
                        target: Some(other_name.clone()),
                    },
                    Pin::Audio {
                        name: other_name,
                        target: Some(name),
                    },
                )),
                Pin::Video {
                    name: other_name,
                    target: _,
                } => Err(format!(
                    "can not connect Audio Pin {name} to Video Pin {other_name}"
                )),
            },
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
enum FilterParamValue {
    File(PathBuf),
    Int(Option<i32>),
    Float(Option<f32>),
    Bool(Option<bool>),
}
#[derive(Debug, Clone, Deserialize, Serialize)]
struct FilterParam {
    name: String,
    description: String,
    value: FilterParamValue,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, strum::Display)]
enum ScaleAspectRationOption {
    disable,
    decrease,
    increase,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, strum::Display)]
enum FpsRoundOption {
    zero,
    inf,
    down,
    up,
    near,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
enum Filter {
    /// segments: number of segments to concatenate,
    /// video_streams: number of output video streams
    /// audio_streams: number of output audio streams
    /// unsafe_mode: if it should try to concatenate sources
    /// of different scale and framerate
    Concat {
        segments: usize,
        video_streams: usize,
        audio_streams: usize,
        unsafe_mode: bool,
    },
    /// transition is fade by default
    /// duration is fade duration
    /// offset is offset from the first stream start
    /// expression is for custom transition
    XFade {
        transition: Option<XFadeTransition>,
        duration: Duration,
        offset: Position,
        expression: Option<String>,
    },
    /// Not fully implemented
    Scale {
        width: usize,
        height: usize,
        interl: Option<bool>,
        force_original_aspect_ratio: Option<ScaleAspectRationOption>,
        force_divisible_by: Option<usize>,
    },
    Pad {
        width: Option<String>,
        height: Option<String>,
        x: Option<String>,
        y: Option<String>,
        color: Option<String>,
        aspect: Option<Fraction>,
    },
    Setsar {
        ratio: Option<String>,
        max: Option<usize>,
    },
    /// if round_eof is false ‒ if will be passed
    Fps {
        fps: Option<String>,
        start_time: Option<f64>,
        round: Option<FpsRoundOption>,
        round_eof: Option<bool>,
    },
}
impl Filter {
    fn name(&self) -> &str {
        match self {
            Self::Concat {
                segments: _,
                video_streams: _,
                audio_streams: _,
                unsafe_mode: _,
            } => "concat",
            Self::XFade {
                transition: _,
                duration: _,
                offset: _,
                expression: _,
            } => "xfade",
            Self::Scale {
                width: _,
                height: _,
                interl: _,
                force_original_aspect_ratio: _,
                force_divisible_by: _,
            } => "scale",
            Self::Pad {
                width: _,
                height: _,
                x: _,
                y: _,
                color: _,
                aspect: _,
            } => "pad",
            Self::Setsar { ratio: _, max: _ } => "setsar",
            Self::Fps {
                fps: _,
                start_time: _,
                round: _,
                round_eof: _,
            } => "fps",
        }
    }
    fn description(&self) -> &str {
        match self {
            Self::Concat {
                segments: _,
                video_streams: _,
                audio_streams: _,
                unsafe_mode: _,
            } => "Concatenate audio and video streams.",
            Self::XFade {
                transition: _,
                duration: _,
                offset: _,
                expression: _,
            } => "Cross fade one video with another video.",
            Self::Scale {
                width: _,
                height: _,
                interl: _,
                force_original_aspect_ratio: _,
                force_divisible_by: _,
            } => "Scale the input video size and/or convert the image format.",
            Self::Pad {
                width: _,
                height: _,
                x: _,
                y: _,
                color: _,
                aspect: _,
            } => "Pad the input video.",
            Self::Setsar { ratio: _, max: _ } => "Set the pixel sample aspect ratio.",
            Self::Fps {
                fps: _,
                start_time: _,
                round: _,
                round_eof: _,
            } => "Force constant framerate.",
        }
    }
    /// (video, audio)
    fn num_sinks(&self) -> (usize, usize) {
        match self {
            Self::Concat {
                segments,
                video_streams,
                audio_streams,
                unsafe_mode: _,
            } => (video_streams * segments, audio_streams * segments),
            Self::XFade {
                transition: _,
                duration: _,
                offset: _,
                expression: _,
            } => (2, 0),
            Self::Scale {
                width: _,
                height: _,
                interl: _,
                force_original_aspect_ratio: _,
                force_divisible_by: _,
            } => (1, 0),
            Self::Pad {
                width: _,
                height: _,
                x: _,
                y: _,
                color: _,
                aspect: _,
            } => (1, 0),
            Self::Setsar { ratio: _, max: _ } => (1, 0),
            Self::Fps {
                fps: _,
                start_time: _,
                round: _,
                round_eof: _,
            } => (1, 0),
        }
    }
    fn get_render_string(&self) -> String {
        match self {
            Self::Concat {
                segments,
                video_streams,
                audio_streams,
                unsafe_mode,
            } => {
                format!(
                    "concat=n={segments}:v={video_streams}:a={audio_streams}:unsafe={unsafe_mode}"
                )
            }
            Self::XFade {
                transition,
                duration,
                offset,
                expression,
            } => {
                let mut tr_out = Vec::new();
                // tr_out.push("xfade=");
                if let Some(tr) = transition {
                    tr_out.push(format!("transition={tr}"));
                }
                tr_out.push(format!("duration={}", duration.as_secs_f64()));
                tr_out.push(format!("offset={}", offset.as_duration().as_secs_f64()));
                if let Some(expr) = expression {
                    tr_out.push(format!("expr={expr}"));
                }
                String::from("xfade=") + &tr_out.join(":")
            }
            Self::Scale {
                width,
                height,
                interl,
                force_original_aspect_ratio,
                force_divisible_by,
            } => {
                let mut tr_out = Vec::new();
                tr_out.push(format!("w={width}"));
                tr_out.push(format!("h={height}"));
                if let Some(interl) = interl {
                    tr_out.push(format!("interl={interl}"));
                }
                if let Some(asr) = force_original_aspect_ratio {
                    tr_out.push(format!("force_original_aspect_ratio={asr}"));
                }
                if let Some(div) = force_divisible_by {
                    tr_out.push(format!("force_divisible_by={div}"));
                }
                String::from("scale=") + &tr_out.join(":")
            }
            Self::Pad {
                width,
                height,
                x,
                y,
                color,
                aspect,
            } => {
                let mut tr_out = Vec::new();
                if let Some(width) = width {
                    tr_out.push(format!("width={width}"));
                }
                if let Some(height) = height {
                    tr_out.push(format!("height={height}"));
                }
                if let Some(x) = x {
                    tr_out.push(format!("x={x}"));
                }
                if let Some(y) = y {
                    tr_out.push(format!("y={y}"));
                }
                if let Some(color) = color {
                    tr_out.push(format!("color={color}"));
                }
                if let Some(aspect) = aspect {
                    tr_out.push(format!("aspect={aspect}"));
                }
                String::from("pad=") + &tr_out.join(":")
            }
            Self::Setsar { ratio, max } => {
                let mut tr_out = Vec::new();
                if let Some(ratio) = ratio {
                    tr_out.push(format!("ratio={ratio}"));
                }
                if let Some(max) = max {
                    tr_out.push(format!("max={max}"));
                }
                String::from("setsar=") + &tr_out.join(":")
            }
            Self::Fps {
                fps,
                start_time,
                round,
                round_eof,
            } => {
                let mut tr_out = Vec::new();
                if let Some(fps) = fps {
                    tr_out.push(format!("fps={fps}"));
                }
                if let Some(start_time) = start_time {
                    tr_out.push(format!("start_time={start_time}"));
                }
                if let Some(round) = round {
                    tr_out.push(format!("round={round}"));
                }
                if let Some(round_eof) = round_eof {
                    tr_out.push(format!("round_eof={round_eof}"));
                }
                String::from("fps=") + &tr_out.join(":")
            }
        }
    }
    fn new_scale(
        width: usize,
        height: usize,
        interlacing: impl Into<Option<bool>>,
        force_original_aspect_ratio: impl Into<Option<ScaleAspectRationOption>>,
        force_divisible_by: impl Into<Option<usize>>,
    ) -> Self {
        Self::Scale {
            width,
            height,
            interl: interlacing.into(),
            force_original_aspect_ratio: force_original_aspect_ratio.into(),
            force_divisible_by: force_divisible_by.into(),
        }
    }
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct RenderRegion {
    start: Position,
    end: Position,
    file: PathBuf,
}

fn get_render_targets(pr: &Project, idx: usize) -> Result<PathBuf, Box<dyn Error>> {
    let string = match pr.get_render_targets()?.get(idx) {
        Some(file) => file.clone(),
        None => {
            return Err("Can not estimate region output filename."
                .to_string()
                .into())
        }
    };
    Ok(PathBuf::from(string))
}

fn get_render_regions() -> Result<Vec<RenderRegion>, Box<dyn Error>> {
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
            BoundsMode::SelectedItems => Err("No support for rendering selected items.".into()),
            BoundsMode::SelectedRegions => {
                Err("No support for render Matrix (selected regions)".into())
            }
        },
        _ => Err("currently, supports just render with MasterMix in render settings".into()),
    }
}

pub fn render_video() -> Result<(), Box<dyn Error>> {
    let render_settings = RenderSettings::default();
    let timelines = build_render_timelines(&render_settings)?;
    let render = Render { render_settings };
    render.render_timelines(timelines)?;
    Ok(())
}

#[derive(Debug)]
struct Render {
    render_settings: RenderSettings,
}
impl Render {
    fn render_timelines(&self, timelines: Vec<TimeLine>) -> Result<(), Box<dyn Error>> {
        for timeline in timelines {
            self.render_timeline(timeline)?;
        }
        Ok(())
    }
    fn render_timeline(&self, timeline: TimeLine) -> Result<(), Box<dyn Error>> {
        println!("rendering timleline:\n{:#?}", timeline);
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
        main_seq.push(format!("{}", self.render_settings.codec));
        main_seq.extend(self.render_settings.codec_options.clone());
        main_seq.push("-r".to_string());
        main_seq.push(format!("{}", self.render_settings.fps));
        main_seq.push(format!(
            "{}",
            timeline
                .outfile
                .with_extension(&self.render_settings.format)
                .display()
        ));

        let mut ffmpeg = Command::new("ffmpeg");
        ffmpeg.arg("-hide_banner");
        ffmpeg.arg("-y");
        ffmpeg.args(main_seq);
        println!("{:#?}", ffmpeg.get_args());

        let output = ffmpeg.output()?;
        // println!("{:?}", out.status);
        println!("status: {}", output.status);
        io::stdout().write_all(&output.stdout).unwrap();
        io::stderr().write_all(&output.stderr).unwrap();

        Ok(())
    }
    fn render_node(&self, node: &Node) -> Result<Vec<String>, String> {
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
                        input.get_target().ok_or("No input in sink")?
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct RenderSettings {
    format: String,
    resolution: Resolution,
    codec: String,
    codec_options: Vec<String>,
    fps: Fraction,
    pad_color: String,
}
impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            format: "mkv".to_string(),
            resolution: Resolution::default(),
            codec: "libx264".to_string(),
            // codec_options: vec!["-crf".to_string(), "15".to_string()],
            codec_options: vec!["-crf", "15", "-pix_fmt", "yuv420p"]
                .into_iter()
                .map(|s| s.to_string())
                .collect(),
            fps: Fraction::new(30000_u16, 1001_u16),
            pad_color: "DarkCyan".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Resolution {
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

trait Timestamp {
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

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, strum::Display)]
enum XFadeTransition {
    custom,
    fade,
    wipeleft,
    wiperight,
    wipeup,
    wipedown,
    slideleft,
    slideright,
    slideup,
    slidedown,
    circlecrop,
    rectcrop,
    distance,
    fadeblack,
    fadewhite,
    radial,
    smoothleft,
    smoothright,
    smoothup,
    smoothdown,
    circleopen,
    circleclose,
    vertopen,
    vertclose,
    horzopen,
    horzclose,
    dissolve,
    pixelize,
    diagtl,
    diagtr,
    diagbl,
    diagbr,
    hlslice,
    hrslice,
    vuslice,
    vdslice,
    hblur,
    fadegrays,
    wipetl,
    wipetr,
    wipebl,
    wipebr,
    squeezeh,
    squeezev,
    zoomin,
    fadefast,
    fadeslow,
    hlwind,
    hrwind,
    vuwind,
    vdwind,
    coverleft,
    coverright,
    coverup,
    coverdown,
    revealleft,
    revealright,
    revealup,
    revealdown,
}
