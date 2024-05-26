use std::{path::PathBuf, time::Duration};

use fraction::Fraction;
use rea_rs::Position;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum FilterParamValue {
    File(PathBuf),
    Int(Option<i32>),
    Float(Option<f32>),
    Bool(Option<bool>),
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FilterParam {
    name: String,
    description: String,
    value: FilterParamValue,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, strum::Display)]
pub enum ScaleAspectRationOption {
    disable,
    decrease,
    increase,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, strum::Display)]
pub enum FpsRoundOption {
    zero,
    inf,
    down,
    up,
    near,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Filter {
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
    pub fn name(&self) -> &str {
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
    pub fn description(&self) -> &str {
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
    pub fn num_sinks(&self) -> (usize, usize) {
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
    pub fn get_render_string(&self) -> String {
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
    pub fn new_scale(
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

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, strum::Display)]
pub enum XFadeTransition {
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
