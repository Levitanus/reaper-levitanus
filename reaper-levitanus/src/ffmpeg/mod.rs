use gui::{Backend, BACKEND_ID_STRING};
use rea_rs::Reaper;

use crate::gui::{launch_frontend, stop_backend};

pub use self::base_types::RenderSettings;
pub use self::gui::{front, SOCKET_PORT};
use std::{cell::RefCell, error::Error, sync::Arc};

mod base;
mod base_types;
mod filters;
mod gui;
mod nodes;
mod options;
mod parser;
mod stream_ids;

pub fn ffmpeg_gui() -> Result<(), Box<dyn Error>> {
    let rpr = Reaper::get_mut();
    if !rpr.has_control_surface(&BACKEND_ID_STRING.to_string()) {
        let backend = Backend::new()?;
        rpr.register_control_surface(Arc::new(RefCell::new(backend)));
    }
    launch_frontend(crate::gui::ComponentType::FfmpegGui)?;
    Ok(())
}

pub fn stop_ffmpeg_gui() -> Result<(), Box<dyn Error>> {
    stop_backend(BACKEND_ID_STRING)
}
