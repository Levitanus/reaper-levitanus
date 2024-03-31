use std::{process::Command, str::FromStr, string::ParseError};

use lazy_static::lazy_static;
use regex::Regex;

#[test]
fn test_filters_list() {
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-hide_banner").arg("-filters");
    let out = String::from_utf8(cmd.output().expect("can't get output").stdout)
        .expect("can not get utf8");
    let mut filters = Vec::new();
    for line in out.lines() {
        let filter_header: FilterHeader = match line.parse() {
            Ok(h) => h,
            Err(_) => {
                continue;
            }
        };
        filters.push(filter_header);
    }
    let video_filters = filters.iter().filter(|x| {
        match x.input_type {
            SocketType::Video => (),
            _ => return false,
        };
        match x.output_type {
            SocketType::Video => (),
            _ => return false,
        };
        if x.input_amount != 1 {
            return false;
        }
        if x.output_amount != 1 {
            return false;
        }
        true
    });
    println!("{:#?}", video_filters.collect::<Vec<_>>());
}

lazy_static! {
    static ref FILTER_REGEXP: Regex = Regex::new(r".{4}(\w+)\b\s*(\S+)->(\S+)\s*(\w.*)").unwrap();
}
#[derive(Debug)]
struct FilterHeader {
    name: String,
    description: String,
    input_type: SocketType,
    input_amount: u8,
    output_type: SocketType,
    output_amount: u8,
}
impl FromStr for FilterHeader {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some(caps) = FILTER_REGEXP.captures(s) else {
            return Err("Not parsed".into());
        };
        let name = caps[1].to_string();
        let description = caps[4].to_string();
        let input_amount = caps[2].len() as u8;
        let output_amount = caps[3].len() as u8;
        let input_type: SocketType = caps[2].parse()?;
        let output_type: SocketType = caps[3].parse()?;
        Ok(Self {
            name,
            description,
            input_type,
            input_amount,
            output_type,
            output_amount,
        })
    }
}

#[derive(Debug)]
enum SocketType {
    Video,
    Audio,
    Null,
    Multiple,
}
impl FromStr for SocketType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.chars().nth(0).expect("No char in SocketType") {
            'V' => Ok(Self::Video),
            'A' => Ok(Self::Audio),
            '|' => Ok(Self::Null),
            'N' => Ok(Self::Multiple),
            _ => Err("Can not parse filter socket input type".into()),
        }
    }
}
