use std::{fmt::Display, path::PathBuf, process::Command, time::Duration};

use fraction::Fraction;
use lazy_static::lazy_static;
use log::debug;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::LevitanusError;

use super::options::{FfmpegColor, Opt};

lazy_static! {
    static ref RES_RE: Regex =
        Regex::new(r"(?<width>\d+)x(?<height>\d+)").expect("can not compile opts regex");
}
lazy_static! {
    static ref FPS_RE: Regex =
        Regex::new(r"(?<num>\d+)/(?<denom>\d+)").expect("can not compile opts regex");
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
            fps: Fraction::new(30000_u64, 1001_u64),
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

pub fn framerate_from_video(file: PathBuf) -> Result<Fraction, anyhow::Error> {
    let mut ffprobe = Command::new("ffprobe");
    ffprobe.args([
        "-v",
        "error",
        "-select_streams",
        "v",
        "-show_entries",
        "stream=r_frame_rate",
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
    if let Some(cap) = FPS_RE.captures(out) {
        let num: u64 = cap["num"].parse()?;
        let denom: u64 = cap["denom"].parse()?;
        Ok(Fraction::new(num, denom))
    } else {
        Err(LevitanusError::Unexpected("Can not parse resolution from output".to_string()).into())
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
impl Timestamp for rea_rs::SourceOffset {
    fn timestump(&self) -> String {
        let delta = self.get();
        let hours = delta.num_hours() / 60 / 60;
        let mins = delta.num_minutes() / 60;
        let secs = delta.num_seconds();
        let millis = delta.num_milliseconds();
        format!("{hours}:{mins}:{secs}.{millis}")
    }
}
