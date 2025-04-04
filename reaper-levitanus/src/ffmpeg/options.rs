use std::{num::ParseIntError, time::Duration};

use egui::Color32;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::LevitanusError;

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
    pub supported_pixel_formats: Option<Vec<String>>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd)]
pub enum OptionParameter {
    Int(Option<i32>),
    String(Option<String>),
    Float(Option<f64>),
    Bool(Option<bool>),
    Binary(Option<String>),
    Rational(Option<String>),
    Duration(Option<DurationUnit>),
    Dictionary(Option<String>),
    Color(Option<FfmpegColor>),
    ImageSize(Option<String>),
    FrameRate(Option<String>),
    Enum {
        items: Vec<String>,
        selected_idx: Option<usize>,
    },
    Flags {
        items: Vec<String>,
        selected: Option<Vec<bool>>,
    },
}
impl OptionParameter {
    pub(crate) fn with_none(&mut self) -> Self {
        match self {
            Self::Int(_) => Self::Int(None),
            Self::String(_) => Self::String(None),
            Self::Float(_) => Self::Float(None),
            Self::Bool(_) => Self::Bool(None),
            Self::Binary(_) => Self::Binary(None),
            Self::Rational(_) => Self::Rational(None),
            Self::Duration(_) => Self::Duration(None),
            Self::Dictionary(_) => Self::Dictionary(None),
            Self::Color(_) => Self::Color(None),
            Self::ImageSize(_) => Self::ImageSize(None),
            Self::FrameRate(_) => Self::FrameRate(None),
            Self::Enum {
                items,
                selected_idx: _,
            } => Self::Enum {
                items: items.clone(),
                selected_idx: None,
            },
            Self::Flags { items, selected: _ } => Self::Flags {
                items: items.clone(),
                selected: None,
            },
        }
    }
    pub(crate) fn with_new_string_value(&mut self, val: String) -> Result<Self, LevitanusError> {
        match self {
            Self::String(_) => Ok(Self::String(Some(val))),
            Self::Binary(_) => Ok(Self::Binary(Some(val))),
            Self::Rational(_) => Ok(Self::Rational(Some(val))),
            Self::Dictionary(_) => Ok(Self::Dictionary(Some(val))),
            Self::ImageSize(_) => Ok(Self::ImageSize(Some(val))),
            Self::FrameRate(_) => Ok(Self::FrameRate(Some(val))),
            _ => Err(LevitanusError::Enum(val)),
        }
    }
    pub fn is_assigned(&self) -> bool {
        match self {
            Self::Int(a) => a.is_some(),
            Self::String(a) => a.is_some(),
            Self::Float(a) => a.is_some(),
            Self::Bool(a) => a.is_some(),
            Self::Binary(a) => a.is_some(),
            Self::Rational(a) => a.is_some(),
            Self::Duration(a) => a.is_some(),
            Self::Dictionary(a) => a.is_some(),
            Self::Color(a) => a.is_some(),
            Self::ImageSize(a) => a.is_some(),
            Self::FrameRate(a) => a.is_some(),
            Self::Enum {
                items: _,
                selected_idx,
            } => selected_idx.is_some(),
            Self::Flags { items: _, selected } => selected.is_some(),
        }
    }
    pub fn ffmpeg_representation(&self) -> Option<String> {
        match self {
            Self::Int(n) => match n {
                Some(n) => Some(format!("{}", n)),
                None => None,
            },
            Self::String(n) => match n {
                Some(n) => Some(format!("{}", n)),
                None => None,
            },
            Self::Float(n) => match n {
                Some(n) => Some(format!("{}", n)),
                None => None,
            },
            Self::Bool(n) => match n {
                Some(n) => Some(format!("{}", n)),
                None => None,
            },
            Self::Binary(n) => match n {
                Some(n) => Some(format!("{}", n)),
                None => None,
            },
            Self::Rational(n) => match n {
                Some(n) => Some(format!("{}", n)),
                None => None,
            },
            Self::Duration(n) => match n {
                Some(n) => match n {
                    DurationUnit::Seconds(s) => Some(format!("{}", s)),
                    DurationUnit::Milliseconds(ms) => Some(format!("{}ms", ms)),
                    DurationUnit::Microseconds(us) => Some(format!("{}us", us)),
                    DurationUnit::Timestamp {
                        hours,
                        minutes,
                        seconds,
                    } => Some(format!("{}:{}:{}", hours, minutes, seconds)),
                },
                None => None,
            },
            Self::Dictionary(n) => match n {
                Some(n) => Some(format!("{}", n)),
                None => None,
            },
            Self::Color(n) => match n {
                Some(n) => Some(format!("{}", n.ffmpeg_representation())),
                None => None,
            },
            Self::ImageSize(n) => match n {
                Some(n) => Some(format!("{}", n)),
                None => None,
            },
            Self::FrameRate(n) => match n {
                Some(n) => Some(format!("{}", n)),
                None => None,
            },
            Self::Enum {
                items,
                selected_idx,
            } => match selected_idx {
                Some(n) => Some(format!("{}", items[*n])),
                None => None,
            },
            Self::Flags { items, selected } => match selected {
                Some(v) => Some(
                    items
                        .iter()
                        .zip(v)
                        .map(|(it, sel)| match sel {
                            true => format!("+{}", it),
                            false => format!("-{}", it),
                        })
                        .join(""),
                ),
                None => None,
            },
        }
    }
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
        let json = json!({
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
                "name": "d",
                "description": "set minimum duration in seconds (default 2)",
                "parameter": {
                  "Duration": null
                },
                "default": "(default 2)"
            },
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

#[derive(Debug, Serialize, Deserialize, PartialEq, PartialOrd, Clone)]
pub enum DurationUnit {
    Seconds(f64),
    Milliseconds(i32),
    Microseconds(i32),
    Timestamp {
        hours: u32,
        minutes: u32,
        seconds: f64,
    },
}
impl DurationUnit {
    pub fn as_duration(&self) -> Duration {
        match self {
            Self::Seconds(s) => Duration::from_secs_f64(*s),
            Self::Milliseconds(ms) => Duration::from_millis((*ms) as u64),
            Self::Microseconds(us) => Duration::from_micros(*us as u64),
            Self::Timestamp {
                hours,
                minutes,
                seconds,
            } => {
                Duration::from_secs((*hours as u64) * 3600 + (*minutes as u64) * 60)
                    + Duration::from_secs_f64(*seconds)
            }
        }
    }
    pub fn as_seconds(&self) -> Self {
        Self::Seconds(self.as_duration().as_secs_f64())
    }
    pub fn as_milliseconds(&self) -> Self {
        Self::Milliseconds(self.as_duration().as_millis() as i32)
    }
    pub fn as_microseconds(&self) -> Self {
        Self::Microseconds(self.as_duration().as_micros() as i32)
    }
    pub fn as_timestamp(&self) -> Self {
        let dur = self.as_duration();
        let secs = dur.as_secs();
        let hours = secs / 3600;
        let minutes = secs % 3600 / 60;
        let seconds = (secs % 60) as f64 + ((dur.subsec_micros() as f64) / 1000000.0);
        Self::Timestamp {
            hours: hours as u32,
            minutes: minutes as u32,
            seconds,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, PartialOrd, Clone)]
pub struct FfmpegColor {
    pub color: u32,
    pub alpha: u8,
}
impl Default for FfmpegColor {
    fn default() -> Self {
        Self {
            color: 0x0,
            alpha: 0xff,
        }
    }
}
impl From<Color32> for FfmpegColor {
    fn from(value: Color32) -> Self {
        let alpha = value.a();
        let color: u32 =
            ((value.r() as u32) << 16) + ((value.g() as u32) << 8) + (value.b() as u32);
        Self { color, alpha }
    }
}
impl Into<Color32> for FfmpegColor {
    fn into(self) -> Color32 {
        Color32::from_rgba_premultiplied(
            (self.color >> 16) as u8,
            (self.color >> 8) as u8,
            (self.color % 0xffff00) as u8,
            self.alpha,
        )
    }
}
impl FfmpegColor {
    pub fn new(color: u32, alpha: u8) -> Self {
        Self { color, alpha }
    }
    pub fn from_hex(hex: impl AsRef<str>) -> Result<Self, ParseIntError> {
        let hex = hex.as_ref();
        let color: u32 = hex.parse()?;
        Ok(Self::new(color, 0xff))
    }
    pub fn hex(&self) -> String {
        let val: u64 = ((self.color as u64) << 8) + (self.alpha as u64);
        format!("{:#10x}", val)
    }
    pub(crate) fn ffmpeg_representation(&self) -> String {
        format!("{:#08x}@{:#04x}", self.color, self.alpha)
    }
    pub(crate) fn built_in_colors() -> impl Iterator<Item = (&'static str, u32)> {
        let names = vec![
            "AliceBlue",
            "AntiqueWhite",
            "Aqua",
            "Aquamarine",
            "Azure",
            "Beige",
            "Bisque",
            "Black",
            "BlanchedAlmond",
            "Blue",
            "BlueViolet",
            "Brown",
            "BurlyWood",
            "CadetBlue",
            "Chartreuse",
            "Chocolate",
            "Coral",
            "CornflowerBlue",
            "Cornsilk",
            "Crimson",
            "Cyan",
            "DarkBlue",
            "DarkCyan",
            "DarkGoldenRod",
            "DarkGray",
            "DarkGreen",
            "DarkKhaki",
            "DarkMagenta",
            "DarkOliveGreen",
            "Darkorange",
            "DarkOrchid",
            "DarkRed",
            "DarkSalmon",
            "DarkSeaGreen",
            "DarkSlateBlue",
            "DarkSlateGray",
            "DarkTurquoise",
            "DarkViolet",
            "DeepPink",
            "DeepSkyBlue",
            "DimGray",
            "DodgerBlue",
            "FireBrick",
            "FloralWhite",
            "ForestGreen",
            "Fuchsia",
            "Gainsboro",
            "GhostWhite",
            "Gold",
            "GoldenRod",
            "Gray",
            "Green",
            "GreenYellow",
            "HoneyDew",
            "HotPink",
            "IndianRed",
            "Indigo",
            "Ivory",
            "Khaki",
            "Lavender",
            "LavenderBlush",
            "LawnGreen",
            "LemonChiffon",
            "LightBlue",
            "LightCoral",
            "LightCyan",
            "LightGoldenRodYellow",
            "LightGreen",
            "LightGrey",
            "LightPink",
            "LightSalmon",
            "LightSeaGreen",
            "LightSkyBlue",
            "LightSlateGray",
            "LightSteelBlue",
            "LightYellow",
            "Lime",
            "LimeGreen",
            "Linen",
            "Magenta",
            "Maroon",
            "MediumAquaMarine",
            "MediumBlue",
            "MediumOrchid",
            "MediumPurple",
            "MediumSeaGreen",
            "MediumSlateBlue",
            "MediumSpringGreen",
            "MediumTurquoise",
            "MediumVioletRed",
            "MidnightBlue",
            "MintCream",
            "MistyRose",
            "Moccasin",
            "NavajoWhite",
            "Navy",
            "OldLace",
            "Olive",
            "OliveDrab",
            "Orange",
            "OrangeRed",
            "Orchid",
            "PaleGoldenRod",
            "PaleGreen",
            "PaleTurquoise",
            "PaleVioletRed",
            "PapayaWhip",
            "PeachPuff",
            "Peru",
            "Pink",
            "Plum",
            "PowderBlue",
            "Purple",
            "Red",
            "RosyBrown",
            "RoyalBlue",
            "SaddleBrown",
            "Salmon",
            "SandyBrown",
            "SeaGreen",
            "SeaShell",
            "Sienna",
            "Silver",
            "SkyBlue",
            "SlateBlue",
            "SlateGray",
            "Snow",
            "SpringGreen",
            "SteelBlue",
            "Tan",
            "Teal",
            "Thistle",
            "Tomato",
            "Turquoise",
            "Violet",
            "Wheat",
            "White",
            "WhiteSmoke",
            "Yellow",
            "YellowGreen",
        ];
        let values = vec![
            0xF0F8FF_u32,
            0xFAEBD7_u32,
            0x00FFFF_u32,
            0x7FFFD4_u32,
            0xF0FFFF_u32,
            0xF5F5DC_u32,
            0xFFE4C4_u32,
            0x000000_u32,
            0xFFEBCD_u32,
            0x0000FF_u32,
            0x8A2BE2_u32,
            0xA52A2A_u32,
            0xDEB887_u32,
            0x5F9EA0_u32,
            0x7FFF00_u32,
            0xD2691E_u32,
            0xFF7F50_u32,
            0x6495ED_u32,
            0xFFF8DC_u32,
            0xDC143C_u32,
            0x00FFFF_u32,
            0x00008B_u32,
            0x008B8B_u32,
            0xB8860B_u32,
            0xA9A9A9_u32,
            0x006400_u32,
            0xBDB76B_u32,
            0x8B008B_u32,
            0x556B2F_u32,
            0xFF8C00_u32,
            0x9932CC_u32,
            0x8B0000_u32,
            0xE9967A_u32,
            0x8FBC8F_u32,
            0x483D8B_u32,
            0x2F4F4F_u32,
            0x00CED1_u32,
            0x9400D3_u32,
            0xFF1493_u32,
            0x00BFFF_u32,
            0x696969_u32,
            0x1E90FF_u32,
            0xB22222_u32,
            0xFFFAF0_u32,
            0x228B22_u32,
            0xFF00FF_u32,
            0xDCDCDC_u32,
            0xF8F8FF_u32,
            0xFFD700_u32,
            0xDAA520_u32,
            0x808080_u32,
            0x008000_u32,
            0xADFF2F_u32,
            0xF0FFF0_u32,
            0xFF69B4_u32,
            0xCD5C5C_u32,
            0x4B0082_u32,
            0xFFFFF0_u32,
            0xF0E68C_u32,
            0xE6E6FA_u32,
            0xFFF0F5_u32,
            0x7CFC00_u32,
            0xFFFACD_u32,
            0xADD8E6_u32,
            0xF08080_u32,
            0xE0FFFF_u32,
            0xFAFAD2_u32,
            0x90EE90_u32,
            0xD3D3D3_u32,
            0xFFB6C1_u32,
            0xFFA07A_u32,
            0x20B2AA_u32,
            0x87CEFA_u32,
            0x778899_u32,
            0xB0C4DE_u32,
            0xFFFFE0_u32,
            0x00FF00_u32,
            0x32CD32_u32,
            0xFAF0E6_u32,
            0xFF00FF_u32,
            0x800000_u32,
            0x66CDAA_u32,
            0x0000CD_u32,
            0xBA55D3_u32,
            0x9370D8_u32,
            0x3CB371_u32,
            0x7B68EE_u32,
            0x00FA9A_u32,
            0x48D1CC_u32,
            0xC71585_u32,
            0x191970_u32,
            0xF5FFFA_u32,
            0xFFE4E1_u32,
            0xFFE4B5_u32,
            0xFFDEAD_u32,
            0x000080_u32,
            0xFDF5E6_u32,
            0x808000_u32,
            0x6B8E23_u32,
            0xFFA500_u32,
            0xFF4500_u32,
            0xDA70D6_u32,
            0xEEE8AA_u32,
            0x98FB98_u32,
            0xAFEEEE_u32,
            0xD87093_u32,
            0xFFEFD5_u32,
            0xFFDAB9_u32,
            0xCD853F_u32,
            0xFFC0CB_u32,
            0xDDA0DD_u32,
            0xB0E0E6_u32,
            0x800080_u32,
            0xFF0000_u32,
            0xBC8F8F_u32,
            0x4169E1_u32,
            0x8B4513_u32,
            0xFA8072_u32,
            0xF4A460_u32,
            0x2E8B57_u32,
            0xFFF5EE_u32,
            0xA0522D_u32,
            0xC0C0C0_u32,
            0x87CEEB_u32,
            0x6A5ACD_u32,
            0x708090_u32,
            0xFFFAFA_u32,
            0x00FF7F_u32,
            0x4682B4_u32,
            0xD2B48C_u32,
            0x008080_u32,
            0xD8BFD8_u32,
            0xFF6347_u32,
            0x40E0D0_u32,
            0xEE82EE_u32,
            0xF5DEB3_u32,
            0xFFFFFF_u32,
            0xF5F5F5_u32,
            0xFFFF00_u32,
            0x9ACD32_u32,
        ];
        names.into_iter().zip(values)
    }
}

#[test]
fn test_ffmpeg_color() {
    let color = FfmpegColor::new(
        FfmpegColor::built_in_colors()
            .find(|(key, _)| *key == "Wheat")
            .unwrap()
            .1,
        0xff,
    );
    assert_eq!(color.color, 0xF5DEB3_u32, "color does not match");
    assert_eq!(color.alpha, 0xff, "alpha is wrong");
    assert_eq!(
        color.ffmpeg_representation().to_uppercase(),
        "0XF5DEB3@0XFF",
        "representation is wrong"
    );
    assert_eq!(color.hex().to_uppercase(), "0XF5DEB3FF", "hex is wrong");
}
