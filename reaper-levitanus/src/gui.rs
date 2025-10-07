use egui::{Color32, Context, Frame, Id, InnerResponse, Layout, Modal, RichText, Stroke, Ui};
use rea_rs::Reaper;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, process::Command, str::FromStr};

use crate::LevitanusError;

static SOCKET_ADDRESS: &str = "127.0.0.1";
static APP_NAME: &str = "reaper_levitanus";

pub fn stop_backend(backend_string: impl Into<String>) -> Result<(), Box<dyn std::error::Error>> {
    let backend_string = backend_string.into();
    let rpr = Reaper::get_mut();
    if rpr.has_control_surface(&backend_string) {
        rpr.unregister_control_surface(backend_string)?;
    }
    Ok(())
}

pub fn launch_frontend(component: ComponentType) -> Result<(), Box<dyn std::error::Error>> {
    let mut front_path = PathBuf::from(Reaper::get().get_resource_path()?)
        .join(PathBuf::from("UserPlugins"))
        .join(PathBuf::from("levitanus_frontend"));
    if cfg!(target_os = "windows") {
        front_path = front_path.with_extension("exe");
    }
    if cfg!(target_os = "macos") {
        front_path = front_path.with_file_name("levitanus_frontend_osx");
    }
    Command::new(front_path)
        .arg(component.to_string())
        .spawn()?;
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    socket_address: String,
    ffmpeg_port: u16,
    sample_editor_port: u16,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            socket_address: SOCKET_ADDRESS.to_string(),
            ffmpeg_port: crate::ffmpeg::SOCKET_PORT,
            sample_editor_port: crate::sample_editor::SOCKET_PORT,
        }
    }
}

pub fn get_front_socket_address(component: ComponentType) -> String {
    let cfg: Config = confy::load(APP_NAME, None).unwrap_or_default();
    let address = cfg.socket_address;
    let port = match component {
        ComponentType::SampleEditor => cfg.sample_editor_port,
        ComponentType::FfmpegGui => cfg.ffmpeg_port,
    };
    format!("{address}:{port}")
}

pub fn set_front_socket_address(address: String) -> Result<(), Box<dyn std::error::Error>> {
    let mut cfg: Config = confy::load(APP_NAME, None).unwrap_or_default();
    cfg.socket_address = address;
    confy::store(APP_NAME, None, cfg)?;
    Ok(())
}

#[derive(Debug)]
pub enum ExitCode {
    Shutdown,
    Error(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ComponentType {
    SampleEditor,
    FfmpegGui,
}
impl ToString for ComponentType {
    fn to_string(&self) -> String {
        match self {
            ComponentType::SampleEditor => "sample_editor".to_string(),
            ComponentType::FfmpegGui => "ffmpeg_gui".to_string(),
        }
    }
}
impl FromStr for ComponentType {
    type Err = LevitanusError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sample_editor" => Ok(ComponentType::SampleEditor),
            "ffmpeg_gui" => Ok(ComponentType::FfmpegGui),
            _ => Err(LevitanusError::Unexpected(format!(
                "can not convert '{s}' to FrontEndComponent"
            ))),
        }
    }
}

pub fn widget_error_box(ctx: &Context, error: impl AsRef<str>) {
    Modal::new(Id::new("error")).show(ctx, |ui| {
        ui.with_layout(Layout::top_down_justified(egui::Align::Center), |ui| {
            ui.heading("Error!");
            ui.label("Application will be closed because of the error:");
            ui.label(RichText::new(error.as_ref()).color(Color32::RED));
            if ui.button("Ok").clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        })
    });
}

pub fn frame<F>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> F) -> InnerResponse<F> {
    Frame::new()
        .stroke(Stroke::new(1.5, Color32::from_white_alpha(0x20)))
        .corner_radius(10.0)
        .inner_margin(7.0)
        .show(ui, add_contents)
}
