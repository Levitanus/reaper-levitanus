use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::json;
use vizia::prelude::Data;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Data)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Data)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Data)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Data)]
pub enum EncoderType {
    Video,
    Audio,
    Subtitle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Data)]
pub struct Opt {
    pub name: String,
    pub description: String,
    pub parameter: OptionParameter,
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Data)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Data)]
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
        let json = json!({
        "name":"matroska",
        "description":"Matroska",
        "info":"Muxer matroska [Matroska]:\nMime type: video/x-matroska.",
        "extensions":["mkv"],
        "video_codec":"h264",
        "audio_codec":"vorbis",
        "subtitle_codec":"ass",
        "options":[
            {
                "name":"reserve_index_space",
                "description":"reserve a given amount of space (in bytes) at the beginning of the file for the index (cues) (from 0 to INT_MAX) (default 0)",
                "parameter":"Int",
                "default":"(default 0)"
            },
            {
                "name":"cues_to_front",
                "description":"move Cues (the index) to the front by shifting data if necessary (default false)",
                "parameter":"Bool","default":"(default false)"
            },
            {
                "name":"cluster_size_limit",
                "description":"store at most the provided amount of bytes in a cluster (from -1 to INT_MAX) (default -1)",
                "parameter":"Int","default":"(default -1)"
            },
            {
                "name":"cluster_time_limit",
                "description":"store at most the provided number of milliseconds in a cluster (from -1 to I64_MAX) (default -1)",
                "parameter":"Int","default":"(default -1)"
            },
            {
                "name":"dash",
                "description":"create a WebM file conforming to WebM DASH specification (default false)",
                "parameter":"Bool",
                "default":"(default false)"
            },
            {
                "name":"dash_track_number",
                "description":"track number for the DASH stream (from 1 to INT_MAX) (default 1)",
                "parameter":"Int",
                "default":"(default 1)"
            },
            {
                "name":"live",
                "description":"write files assuming it is a live stream (default false)",
                "parameter":"Bool",
                "default":"(default false)"
            },
            {
                "name":"allow_raw_vfw",
                "description":"allow raw VFW mode (default false)",
                "parameter":"Bool",
                "default":"(default false)"
            },
            {
                "name":"flipped_raw_rgb",
                "description":"store raw RGB bitmaps in VFW mode in bottom-up mode (default false)",
                "parameter":"Bool",
                "default":"(default false)"
            },
            {
                "name":"write_crc32",
                "description":"write a CRC32 element inside every Level 1 element (default true)",
                "parameter":"Bool",
                "default":"(default true)"
            },
            {
                "name":"default_mode",
                "description":"control how a track's FlagDefault is inferred (from 0 to 2) (default passthrough)",
                "parameter":{
                    "Enum":{"infer_no_subs":"","passthrough":"","infer":""}
                },
                "default":"(default passthrough)"
            }]
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
            "options": [
                {
                    "name": "preset",
                    "description": "Set the encoding preset (cf. x264 --fullhelp) (default \"medium\")",
                    "parameter": "String",
                    "default": "(default \"medium\")"
                },
                {
                    "name": "tune",
                    "description": "Tune the encoding params (cf. x264 --fullhelp)",
                    "parameter": "String",
                    "default": null
                },
                {
                    "name": "profile",
                    "description": "Set profile restrictions (cf. x264 --fullhelp)",
                    "parameter": "String",
                    "default": null
                },
                {
                    "name": "fastfirstpass",
                    "description": "Use fast settings when encoding first pass (default true)",
                    "parameter": "Bool",
                    "default": "(default true)"
                },
                {
                    "name": "level",
                    "description": "Specify level (as defined by Annex A)",
                    "parameter": "String",
                    "default": null
                },
                {
                    "name": "passlogfile",
                    "description": "Filename for 2 pass stats",
                    "parameter": "String",
                    "default": null
                },
                {
                    "name": "wpredp",
                    "description": "Weighted prediction for P-frames",
                    "parameter": "String",
                    "default": null
                },
                {
                    "name": "a53cc",
                    "description": "Use A53 Closed Captions (if available) (default true)",
                    "parameter": "Bool",
                    "default": "(default true)"
                },
                {
                    "name": "x264opts",
                    "description": "x264 options",
                    "parameter": "String",
                    "default": null
                },
                {
                    "name": "crf",
                    "description": "Select the quality for constant quality mode (from -1 to FLT_MAX) (default -1)",
                    "parameter": "Float",
                    "default": "(default -1)"
                },
                {
                    "name": "crf_max",
                    "description": "In CRF mode, prevents VBV from lowering quality beyond this point. (from -1 to FLT_MAX) (default -1)",
                    "parameter": "Float",
                    "default": "(default -1)"
                },
                {
                    "name": "qp",
                    "description": "Constant quantization parameter rate control method (from -1 to INT_MAX) (default -1)",
                    "parameter": "Int",
                    "default": "(default -1)"
                },
                {
                    "name": "mode",
                    "description": "AQ method (from -1 to INT_MAX) (default -1)",
                    "parameter": {
                        "Enum": {
                            "none": "",
                            "autovariance": "",
                            "variance": ""
                        }
                    },
                    "default": "(default -1)"
                },
                {
                    "name": "psy",
                    "description": "Use psychovisual optimizations. (default auto)",
                    "parameter": "Bool",
                    "default": "(default auto)"
                },
                {
                    "name": "rd",
                    "description": "Strength of psychovisual optimization, in <psy-rd>:<psy-trellis> format.",
                    "parameter": "String",
                    "default": null
                },
                {
                    "name": "lookahead",
                    "description": "Number of frames to look ahead for frametype and ratecontrol (from -1 to INT_MAX) (default -1)",
                    "parameter": "Int",
                    "default": "(default -1)"
                },
                {
                    "name": "weightb",
                    "description": "Weighted prediction for B-frames. (default auto)",
                    "parameter": "Bool",
                    "default": "(default auto)"
                },
                {
                    "name": "weightp",
                    "description": "Weighted prediction analysis method. (from -1 to INT_MAX) (default -1)",
                    "parameter": {
                        "Enum": {
                            "simple": "",
                            "none": "",
                            "smart": ""
                        }
                    },
                    "default": "(default -1)"
                },
                {
                    "name": "refresh",
                    "description": "Use Periodic Intra Refresh instead of IDR frames. (default auto)",
                    "parameter": "Bool",
                    "default": "(default auto)"
                },
                {
                    "name": "compat",
                    "description": "Bluray compatibility workarounds. (default auto)",
                    "parameter": "Bool",
                    "default": "(default auto)"
                },
                {
                    "name": "bias",
                    "description": "Influences how often B-frames are used (from INT_MIN to INT_MAX) (default INT_MIN)",
                    "parameter": "Int",
                    "default": "(default INT_MIN)"
                },
                {
                    "name": "pyramid",
                    "description": "Keep some B-frames as references. (from -1 to INT_MAX) (default -1)",
                    "parameter": {
                        "Enum": {
                            "normal": "",
                            "none": "",
                            "strict": ""
                        }
                    },
                    "default": "(default -1)"
                },
                {
                    "name": "8x8dct",
                    "description": "High profile 8x8 transform. (default auto)",
                    "parameter": "Bool",
                    "default": "(default auto)"
                },
                {
                    "name": "pskip",
                    "description": "(default auto)",
                    "parameter": "Bool",
                    "default": "(default auto)"
                },
                {
                    "name": "aud",
                    "description": "Use access unit delimiters. (default auto)",
                    "parameter": "Bool",
                    "default": "(default auto)"
                },
                {
                    "name": "mbtree",
                    "description": "Use macroblock tree ratecontrol. (default auto)",
                    "parameter": "Bool",
                    "default": "(default auto)"
                },
                {
                    "name": "deblock",
                    "description": "Loop filter parameters, in <alpha:beta> form.",
                    "parameter": "String",
                    "default": null
                },
                {
                    "name": "cplxblur",
                    "description": "Reduce fluctuations in QP (before curve compression) (from -1 to FLT_MAX) (default -1)",
                    "parameter": "Float",
                    "default": "(default -1)"
                },
                {
                    "name": "partitions",
                    "description": "A comma-separated list of partitions to consider. Possible values: p8x8, p4x4, b8x8, i8x8, i4x4, none, all",
                    "parameter": "String",
                    "default": null
                },
                {
                    "name": "pred",
                    "description": "Direct MV prediction mode (from -1 to INT_MAX) (default -1)",
                    "parameter": {
                        "Enum": {
                            "spatial": "",
                            "none": "",
                            "temporal": "",
                            "auto": ""
                        }
                    },
                    "default": "(default -1)"
                },
                {
                    "name": "stats",
                    "description": "Filename for 2 pass stats",
                    "parameter": "String",
                    "default": null
                },
                {
                    "name": "hrd",
                    "description": "Signal HRD information (requires vbv-bufsize; cbr not allowed in .mp4) (from -1 to INT_MAX) (default -1)",
                    "parameter": {
                        "Enum": {
                            "none": "",
                            "vbr": "",
                            "cbr": ""
                        }
                    },
                    "default": "(default -1)"
                },
                {
                    "name": "me_method",
                    "description": "Set motion estimation method (from -1 to 4) (default -1)",
                    "parameter": {
                        "Enum": {
                            "dia": "",
                            "hex": "",
                            "umh": "",
                            "tesa": "",
                            "esa": ""
                        }
                    },
                    "default": "(default -1)"
                },
                {
                    "name": "coder",
                    "description": "Coder type (from -1 to 1) (default default)",
                    "parameter": {
                        "Enum": {
                            "cavlc": "",
                            "default": "",
                            "ac": "",
                            "vlc": "",
                            "cabac": ""
                        }
                    },
                    "default": "(default default)"
                },
                {
                    "name": "chromaoffset",
                    "description": "QP difference between chroma and luma (from INT_MIN to INT_MAX) (default 0)",
                    "parameter": "Int",
                    "default": "(default 0)"
                },
                {
                    "name": "sc_threshold",
                    "description": "Scene change threshold (from INT_MIN to INT_MAX) (default -1)",
                    "parameter": "Int",
                    "default": "(default -1)"
                },
                {
                    "name": "noise_reduction",
                    "description": "Noise reduction (from INT_MIN to INT_MAX) (default -1)",
                    "parameter": "Int",
                    "default": "(default -1)"
                },
                {
                    "name": "udu_sei",
                    "description": "Use user data unregistered SEI if available (default false)",
                    "parameter": "Bool",
                    "default": "(default false)"
                },
                {
                    "name": "params",
                    "description": "Override the x264 configuration using a :-separated list of key=value parameters",
                    "parameter": "Dictionary",
                    "default": null
                },
                {
                    "name": "mb_info",
                    "description": "Set mb_info data through AVSideData, only useful when used from the API (default false)",
                    "parameter": "Bool",
                    "default": "(default false)"
                }
            ]
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
