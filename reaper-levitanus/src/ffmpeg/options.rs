use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Muxer {
    pub name: String,
    pub description: String,
    pub info: String,
    pub extensions: Option<Vec<String>>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub subtitle_codec: Option<String>,
    pub options: Vec<Opt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Encoder {
    pub name: String,
    pub description: String,
    pub info: String,
    pub pixel_formats: Option<Vec<String>>,
    pub encoder_type: EncoderType,
    pub frame_level_multithreading: bool,
    pub slice_level_multithreading: bool,
    pub is_experimenal: bool,
    pub supports_draw_horiz_band: bool,
    pub supports_direct_rendering_method_1: bool,
    pub options: Vec<Opt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedFilter {
    pub name: String,
    pub description: String,
    pub info: String,
    pub n_sockets: (usize, usize),
    pub timeline_support: bool,
    pub slice_level_multithreading: bool,
    pub command_support: bool,
    pub options: Vec<Opt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EncoderType {
    Video,
    Audio,
    Subtitle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Opt {
    pub name: String,
    pub description: String,
    pub parameter: OptionParameter,
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OptionParameter {
    Int,
    String,
    Float,
    Bool,
    Binary,
    Rational,
    Duration,
    Dictionary,
    Color,
    ImageSize,
    FrameRate,
    Enum(HashMap<String, String>),
    Flags(HashMap<String, String>),
}
