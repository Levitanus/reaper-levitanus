use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EncoderType {
    Video,
    Audio,
    Subtitle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Opt {
    pub name: String,
    pub description: String,
    pub parameter: OptionParameter,
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OptionParameter {
    Int(Option<i32>),
    String(Option<String>),
    Float(Option<f64>),
    Bool(Option<bool>),
    Binary(Option<String>),
    Rational(Option<String>),
    Duration(Option<String>),
    Dictionary(Option<String>),
    Color(Option<String>),
    ImageSize(Option<String>),
    FrameRate(Option<String>),
    Enum(HashMap<String, String>),
    Flags(HashMap<String, String>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PixelFormat {
    pub name: String,
    pub input_support: bool,
    pub output_support: bool,
    pub hardware_accelerated: bool,
    pub paletted: bool,
    pub bitstream: bool,
    pub nb_components: u8,
    pub bits_per_pixel: u8,
    pub bit_depth: String,
}

impl Default for Muxer {
    fn default() -> Self {
        let json = json!( {
          "name": "matroska",
          "description": "Matroska",
          "info": "Muxer matroska [Matroska]:\nMime type: video/x-matroska.",
          "extensions": [
            "mkv"
          ],
          "video_codec": "h264",
          "audio_codec": "vorbis",
          "subtitle_codec": "ass",
          "options": [
            {
              "name": "reserve_index_space",
              "description": "reserve a given amount of space (in bytes) at the beginning of the file for the index (cues) (from 0 to INT_MAX) (default 0)",
              "parameter": {
                "Int": null
              },
              "default": "(default 0)"
            },
            {
              "name": "cues_to_front",
              "description": "move Cues (the index) to the front by shifting data if necessary (default false)",
              "parameter": {
                "Bool": null
              },
              "default": "(default false)"
            },
            {
              "name": "cluster_size_limit",
              "description": "store at most the provided amount of bytes in a cluster (from -1 to INT_MAX) (default -1)",
              "parameter": {
                "Int": null
              },
              "default": "(default -1)"
            },
            {
              "name": "cluster_time_limit",
              "description": "store at most the provided number of milliseconds in a cluster (from -1 to I64_MAX) (default -1)",
              "parameter": {
                "Int": null
              },
              "default": "(default -1)"
            },
            {
              "name": "dash",
              "description": "create a WebM file conforming to WebM DASH specification (default false)",
              "parameter": {
                "Bool": null
              },
              "default": "(default false)"
            },
            {
              "name": "dash_track_number",
              "description": "track number for the DASH stream (from 1 to INT_MAX) (default 1)",
              "parameter": {
                "Int": null
              },
              "default": "(default 1)"
            },
            {
              "name": "live",
              "description": "write files assuming it is a live stream (default false)",
              "parameter": {
                "Bool": null
              },
              "default": "(default false)"
            },
            {
              "name": "allow_raw_vfw",
              "description": "allow raw VFW mode (default false)",
              "parameter": {
                "Bool": null
              },
              "default": "(default false)"
            },
            {
              "name": "flipped_raw_rgb",
              "description": "store raw RGB bitmaps in VFW mode in bottom-up mode (default false)",
              "parameter": {
                "Bool": null
              },
              "default": "(default false)"
            },
            {
              "name": "write_crc32",
              "description": "write a CRC32 element inside every Level 1 element (default true)",
              "parameter": {
                "Bool": null
              },
              "default": "(default true)"
            },
            {
              "name": "default_mode",
              "description": "control how a track's FlagDefault is inferred (from 0 to 2) (default passthrough)",
              "parameter": {
                "Enum": {
                  "passthrough": "",
                  "infer_no_subs": "",
                  "infer": ""
                }
              },
              "default": "(default passthrough)"
            }
          ]
        });
        serde_json::from_value(json).expect("can not deserealize MKV muxer in default")
    }
}

impl Default for Encoder {
    fn default() -> Self {
        let json = r#"{
            "name": "libx264",
            "description": "libx264 H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10 (codec h264)",
            "info": "Encoder libx264 [libx264 H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10]:\nGeneral capabilities: dr1 delay threads\nThreading capabilities: other\nSupported pixel formats: yuv420p yuvj420p yuv422p yuvj422p yuv444p yuvj444p nv12 nv16 nv21 yuv420p10le yuv422p10le yuv444p10le nv20le gray gray10le",
            "pixel_formats": [
                "yuv420p",
                "yuvj420p",
                "yuv422p",
                "yuvj422p",
                "yuv444p",
                "yuvj444p",
                "nv12",
                "nv16",
                "nv21",
                "yuv420p10le",
                "yuv422p10le",
                "yuv444p10le",
                "nv20le",
                "gray",
                "gray10le"
            ],
            "encoder_type": "Video",
            "frame_level_multithreading": false,
            "slice_level_multithreading": false,
            "is_experimenal": false,
            "supports_draw_horiz_band": false,
            "supports_direct_rendering_method_1": true,
            "options": []
        }"#;
        serde_json::from_str(json).expect("Can not desereilize default libx264 encoder")
    }
}

impl Default for PixelFormat {
    fn default() -> Self {
        let json = r#"{
            "name": "yuv420p",
            "input_support": true,
            "output_support": true,
            "hardware_accelerated": false,
            "paletted": false,
            "bitstream": false,
            "nb_components": 3,
            "bits_per_pixel": 12,
            "bit_depth": "8-8-8"
        }"#;
        serde_json::from_str(json).expect("can not deserealize default yuv420p pixel format")
    }
}
