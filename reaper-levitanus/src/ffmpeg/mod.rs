use rea_rs::{Reaper, Timer};
use reaper_imgui::{Context, ImGui};

pub use self::base::RenderSettings;
use self::{
    base::{build_render_timelines, Render},
    parser::parse_all,
};
use std::{error::Error, path::PathBuf};

mod base;
mod filters;
mod nodes;
mod options;
mod parser;
mod stream_ids;

pub fn render_video() -> Result<(), Box<dyn Error>> {
    let render_settings = RenderSettings::default();
    let timelines = build_render_timelines(&render_settings)?;
    let render = Render { render_settings };
    render.render_timelines(timelines)?;
    Ok(())
}

pub fn ffmpeg_gui() -> Result<(), Box<dyn Error>> {
    let rpr = Reaper::get();
    let path = PathBuf::from(rpr.get_resource_path()?)
        .join("Data")
        .join("reaper-levitanus");

    // let imgui = ImGui::load(rpr.plugin_context());
    // parse_all(json_path)?;
    Ok(())
}

struct Gui {
    _imgui: ImGui,
}
impl Gui {
    fn new(imgui: ImGui) -> Self {
        Self { _imgui: imgui }
    }
}

impl Timer for Gui {
    fn run(&mut self) -> Result<(), Box<dyn Error>> {
        todo!()
    }

    fn id_string(&self) -> String {
        "ffmpeg_gui".to_string()
    }
}
