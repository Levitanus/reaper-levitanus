use std::{
    collections::HashMap, error::Error, ffi::OsStr, fs::OpenOptions, io::Write, path::PathBuf,
    process::Command,
};

use lazy_static::lazy_static;
use log::info;
use path_absolutize::Absolutize;
use rea_rs::Timer;
use regex::Regex;

use crate::ffmpeg::options::{Encoder, EncoderType, ParsedFilter};

use super::options::{Muxer, Opt, OptionParameter};

lazy_static! {
    static ref OPT_RE: Regex =
        Regex::new(r"-?(?<name>[\w&&[^A-Z]]\w*)\s+<(?<type>\w+)>\s+[\.\w]+\s(?<description>.+)")
            .expect("can not compile opts regex");
}
lazy_static! {
    static ref OPT_RE_DEFAULT: Regex =
        Regex::new(r"\(default (?<default>.+)\)").expect("can not compile opts regex");
}
lazy_static! {
    static ref OPT_ENUM_RE_NAME: Regex =
        Regex::new(r"^(?<name>[\w&&[^A-Z]]\w*)").expect("can not compile opts enum regex");
}
lazy_static! {
    static ref OPT_ENUM_RE_DESC: Regex =
        Regex::new(r"^(?:[\w&&[^A-Z]]\w*)[\s\d]+[\.\w]\s(?<description>\w.+)")
            .expect("can not compile opts enum regex");
}

pub fn parse_all(out_dir: PathBuf) -> Result<(), Box<dyn Error>> {
    parse_muxers(muxers_path(&out_dir))?;
    parse_encoders(encoders_path(&out_dir))?;
    parse_filters(filters_path(&out_dir))?;
    Ok(())
}
pub fn check_parsed_paths(out_dir: PathBuf) -> bool {
    muxers_path(&out_dir).exists()
        && encoders_path(&out_dir).exists()
        && filters_path(&out_dir).exists()
}
fn muxers_path(out_dir: &PathBuf) -> PathBuf {
    out_dir.join("muxers.json")
}

fn encoders_path(out_dir: &PathBuf) -> PathBuf {
    out_dir.join("encoders.json")
}

fn filters_path(out_dir: &PathBuf) -> PathBuf {
    out_dir.join("filters.json")
}

fn parse_muxers(out_file: PathBuf) -> Result<(), Box<dyn Error>> {
    let string = output_with_args(["-muxers"])?;
    let lines = string.lines();
    let mux_re = Regex::new(r"\s.*E\s+(?<name>\w+)\s+(?<description>\w.*)")?;
    let ext_re = Regex::new(r"Common extensions:\s(.*)\.")?;
    let video_c_re = Regex::new(r"Default video codec:\s(\w+)\.")?;
    let audio_c_re = Regex::new(r"Default audio codec:\s(\w+)\.")?;
    let sub_c_re = Regex::new(r"Default subtitle codec:\s(\w+)\.")?;
    let info_end_re = Regex::new(r".*AVOptions:$")?;

    let mut muxers = Vec::new();
    info!("collecting muxers...");
    for line in lines {
        let Some(cap) = mux_re.captures(line) else {
            continue;
        };
        let name = cap["name"].to_string();
        let description = cap["description"].to_string();
        info!("Parsing muxer '{name}'");

        let info_string = output_with_args(["-h", &format!("muxer={name}")])?;
        let mut extensions = None;
        let mut video_codec = None;
        let mut audio_codec = None;
        let mut subtitle_codec = None;
        let mut info = Vec::new();
        let mut options: Vec<Opt> = Vec::new();

        let mut parse_flow = ParseFlow::Info;
        for mut i_line in info_string.lines() {
            i_line = i_line.trim();
            match parse_flow {
                ParseFlow::Info => {
                    if let Some(cap) = ext_re.captures(i_line) {
                        extensions = Some(
                            cap[1]
                                .to_string()
                                .split(", ")
                                .map(|s| s.to_string())
                                .collect(),
                        );
                        continue;
                    }
                    if let Some(cap) = video_c_re.captures(i_line) {
                        video_codec = Some(cap[1].to_string());
                        continue;
                    }
                    if let Some(cap) = audio_c_re.captures(i_line) {
                        audio_codec = Some(cap[1].to_string());
                        continue;
                    }
                    if let Some(cap) = sub_c_re.captures(i_line) {
                        subtitle_codec = Some(cap[1].to_string());
                        continue;
                    }
                    if info_end_re.captures(i_line).is_some() {
                        // println!("hoing parse options!");
                        parse_flow = ParseFlow::Opt;
                        continue;
                    }
                    info.push(i_line);
                }
                ParseFlow::Opt => parse_flow = parse_option(i_line, &mut options)?,
                ParseFlow::Enum => parse_flow = parse_enum(i_line, &mut options)?,
            }
        }
        let muxer = Muxer {
            name,
            info: info.join("\n"),
            extensions,
            video_codec,
            audio_codec,
            subtitle_codec,
            description,
            options,
        };
        muxers.push(muxer);
    }
    let muxers_string: String = serde_json::to_string(&muxers)?;
    info!(
        "\ndamping muxers to the file: {}\n",
        out_file.absolutize()?.display()
    );
    let mut f = OpenOptions::new().write(true).create(true).open(out_file)?;
    f.write_all(muxers_string.as_bytes())?;
    Ok(())
}

fn parse_encoders(out_file: PathBuf) -> Result<(), Box<dyn Error>> {
    let string = output_with_args(["-encoders"])?;
    let lines = string.lines();
    let enc_re = Regex::new(r"^(?<flags>[\w\.]{6})\s(?<name>\w+)\s+(?<description>\w.*)")?;
    let pix_f_re = Regex::new(r"Supported pixel formats: (.*)")?;
    let info_end_re = Regex::new(r".*AVOptions:$")?;

    let mut encoders = Vec::new();
    info!("collecting encoders...");
    for mut line in lines {
        line = line.trim();
        let Some(cap) = enc_re.captures(line) else {
            continue;
        };
        let name = cap["name"].to_string();
        let description = cap["description"].to_string();
        info!("Parsing encoder '{name}'");

        let info_string = output_with_args(["-h", &format!("encoder={name}")])?;
        let mut info = Vec::new();
        let flatgs_string = cap["flags"].to_string();
        let mut flags = flatgs_string.chars();
        let encoder_type = match flags.next().ok_or("can not read a char")? {
            'V' => EncoderType::Video,
            'A' => EncoderType::Audio,
            'S' => EncoderType::Subtitle,
            s => {
                return Err(
                    format!("Can not estimate encoder type {s}. The line is: {line}").into(),
                )
            }
        };
        let frame_level_multithreading = match flags.next().ok_or("can not read a char")? {
            'F' => true,
            _ => false,
        };
        let slice_level_multithreading = match flags.next().ok_or("can not read a char")? {
            'S' => true,
            _ => false,
        };
        let is_experimenal = match flags.next().ok_or("can not read a char")? {
            'X' => true,
            _ => false,
        };
        let supports_draw_horiz_band = match flags.next().ok_or("can not read a char")? {
            'B' => true,
            _ => false,
        };
        let supports_direct_rendering_method_1 = match flags.next().ok_or("can not read a char")? {
            'D' => true,
            _ => false,
        };
        let mut pixel_formats = None;

        let mut options: Vec<Opt> = Vec::new();
        let mut parse_flow = ParseFlow::Info;
        for mut i_line in info_string.lines() {
            i_line = i_line.trim();
            match parse_flow {
                ParseFlow::Info => {
                    if let Some(cap) = pix_f_re.captures(i_line) {
                        pixel_formats = Some(cap[1].split(" ").map(|s| s.to_string()).collect());
                    }
                    if info_end_re.captures(i_line).is_some() {
                        // println!("hoing parse options!");
                        parse_flow = ParseFlow::Opt;
                        continue;
                    }
                    info.push(i_line);
                }
                ParseFlow::Opt => parse_flow = parse_option(i_line, &mut options)?,
                ParseFlow::Enum => parse_flow = parse_enum(i_line, &mut options)?,
            }
        }
        let encoder = Encoder {
            name,
            description,
            info: info.join("\n"),
            pixel_formats,
            encoder_type,
            frame_level_multithreading,
            slice_level_multithreading,
            is_experimenal,
            supports_draw_horiz_band,
            supports_direct_rendering_method_1,
            options,
        };
        encoders.push(encoder);
    }
    let encoders_string: String = serde_json::to_string(&encoders)?;
    info!(
        "\ndamping encoders to the file: {}\n",
        out_file.absolutize()?.display()
    );
    let mut f = OpenOptions::new().write(true).create(true).open(out_file)?;
    f.write_all(encoders_string.as_bytes())?;
    Ok(())
}

fn parse_filters(out_file: PathBuf) -> Result<(), Box<dyn Error>> {
    let string = output_with_args(["-filters"])?;
    let lines = string.lines();
    let filter_re = Regex::new(
        r"^(?<flags>[\w\.]{3})\s(?<name>\w+)\s+(?<inputs>V+)->(?<outputs>V+)\s+(?<description>\w.*)",
    )?;
    let info_end_re = Regex::new(r".*AVOptions:$")?;

    let mut filters = Vec::new();
    info!("collecting filters...");
    for mut line in lines {
        line = line.trim();
        let Some(cap) = filter_re.captures(line) else {
            continue;
        };
        let name = cap["name"].to_string();
        let description = cap["description"].to_string();
        if ["frei0r", "ass"]
            .into_iter()
            .find(|n| {
                if name.contains(*n) {
                    return true;
                }
                false
            })
            .is_some()
        {
            info!("skipping '{name}'");
            continue;
        }
        info!("Parsing filter '{name}'");

        let info_string = output_with_args(["-h", &format!("filter={name}")])?;
        let mut info = Vec::new();
        let flatgs_string = cap["flags"].to_string();
        let mut flags = flatgs_string.chars();
        let timeline_support = match flags.next().ok_or("can not read a char")? {
            'T' => true,
            _ => false,
        };
        let slice_level_multithreading = match flags.next().ok_or("can not read a char")? {
            'S' => true,
            _ => false,
        };
        let command_support = match flags.next().ok_or("can not read a char")? {
            'C' => true,
            _ => false,
        };
        let n_sockets = (cap["inputs"].len(), cap["outputs"].len());

        let mut options: Vec<Opt> = Vec::new();
        let mut parse_flow = ParseFlow::Info;
        for mut i_line in info_string.lines() {
            i_line = i_line.trim();
            match parse_flow {
                ParseFlow::Info => {
                    if info_end_re.captures(i_line).is_some() {
                        // println!("hoing parse options!");
                        parse_flow = ParseFlow::Opt;
                        continue;
                    }
                    info.push(i_line);
                }
                ParseFlow::Opt => {
                    parse_flow = {
                        if info_end_re.captures(i_line).is_some() {
                            break;
                        }
                        parse_option(i_line, &mut options)?
                    }
                }
                ParseFlow::Enum => {
                    parse_flow = {
                        if info_end_re.captures(i_line).is_some() {
                            break;
                        }
                        parse_enum(i_line, &mut options)?
                    }
                }
            }
        }
        let filter = ParsedFilter {
            name,
            description,
            info: info.join("\n"),
            n_sockets,
            timeline_support,
            slice_level_multithreading,
            command_support,
            options,
        };
        filters.push(filter);
    }
    let filters_string: String = serde_json::to_string(&filters)?;
    info!(
        "\ndamping filters_string to the file: {}\n",
        out_file.absolutize()?.display()
    );
    let mut f = OpenOptions::new().write(true).create(true).open(out_file)?;
    f.write_all(filters_string.as_bytes())?;
    Ok(())
}

fn parse_option(line: &str, mut options: &mut Vec<Opt>) -> Result<ParseFlow, Box<dyn Error>> {
    let Some(cap) = OPT_RE.captures(line) else {
        return parse_enum(line, &mut options);
    };
    let parameter = match &cap["type"] {
        "int" => OptionParameter::Int,
        "int64" => OptionParameter::Int,
        "string" => OptionParameter::String,
        "float" => OptionParameter::Float,
        "double" => OptionParameter::Float,
        "boolean" => OptionParameter::Bool,
        "binary" => OptionParameter::Binary,
        "rational" => OptionParameter::Rational,
        "duration" => OptionParameter::Duration,
        "dictionary" => OptionParameter::Dictionary,
        "color" => OptionParameter::Color,
        "image_size" => OptionParameter::ImageSize,
        "video_rate" => OptionParameter::FrameRate,
        "flags" => OptionParameter::Flags(HashMap::new()),
        t => return Err(format!("unknown type: {t}. The line was: {line}").into()),
    };
    let default = if let Some(cap) = OPT_RE_DEFAULT.find(line) {
        Some(cap.as_str().to_string())
    } else {
        None
    };
    options.push(Opt {
        name: cap["name"].to_string(),
        description: cap["description"].to_string(),
        parameter,
        default,
    });
    Ok(ParseFlow::Opt)
}

fn parse_enum(line: &str, options: &mut Vec<Opt>) -> Result<ParseFlow, Box<dyn Error>> {
    let Some(cap) = OPT_ENUM_RE_NAME.captures(line) else {
        return Ok(ParseFlow::Opt);
    };
    if OPT_RE.captures(line).is_some() {
        return Ok(ParseFlow::Opt);
    }
    let description = match OPT_ENUM_RE_DESC.find(line) {
        Some(d) => d.as_str().to_string(),
        None => "".to_string(),
    };
    let last = options
        .last_mut()
        .ok_or(format!("options are empty, line is {line}"))?;
    let new_par = match &mut last.parameter {
        OptionParameter::Flags(hm) => {
            hm.insert(cap["name"].to_string(), description);
            None
        }
        OptionParameter::Enum(hm) => {
            hm.insert(cap["name"].to_string(), description);
            None
        }
        OptionParameter::Int => {
            let map = HashMap::from_iter([(cap["name"].to_string(), description)]);
            Some(OptionParameter::Enum(map))
        }
        OptionParameter::String => {
            let map = HashMap::from_iter([(cap["name"].to_string(), description)]);
            Some(OptionParameter::Enum(map))
        }
        p => {
            return Err(format!(
                "Can not convert option parameter to enum: {:?}. The line was: {line}",
                p
            )
            .into())
        }
    };
    if let Some(new_par) = new_par {
        last.parameter = new_par;
    }
    Ok(ParseFlow::Enum)
}

enum ParseFlow {
    Info,
    Opt,
    Enum,
}

fn output_with_args(
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> Result<String, Box<dyn Error>> {
    let mut ffmpeg = Command::new("ffmpeg");
    ffmpeg.arg("-hide_banner");
    ffmpeg.args(args);
    let output = ffmpeg.output()?;
    let string = String::from_utf8(output.stdout)?;
    Ok(string)
}

#[test]
fn test_parsing() -> Result<(), Box<dyn Error>> {
    std::env::set_var("RUST_LOG", "debug");
    env_logger::try_init()?;
    parse_all(PathBuf::from("./"))?;
    Ok(())
}
