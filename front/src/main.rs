use std::{env, str::FromStr};

use reaper_levitanus::{ffmpeg, gui::ComponentType, sample_editor};

fn main() {
    env_logger::try_init().ok();
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        match ComponentType::from_str(args[1].as_str()).unwrap() {
            ComponentType::FfmpegGui => ffmpeg::front().unwrap(),
            ComponentType::SampleEditor => sample_editor::front().unwrap(),
        };
    } else {
        panic!("No argument provided: lauch with 'sample_editor' or 'ffmpeg_gui'");
    }
}
