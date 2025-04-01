use gui::{Backend, BACKEND_ID_STRING};
use rea_rs::Reaper;

pub use self::base_types::RenderSettings;
pub use self::gui::front;
use std::{cell::RefCell, error::Error, sync::Arc};

mod base;
mod base_types;
mod filters;
mod gui;
mod nodes;
mod options;
mod parser;
mod stream_ids;

// pub fn render_video() -> Result<(), Box<dyn Error>> {
//     let render_settings = RenderSettings::default();
//     let timelines = build_render_timelines(&render_settings)?;
//     let render = Render { render_settings };
//     render.render_timelines(timelines)?;
//     Ok(())
// }

pub fn ffmpeg_gui() -> Result<(), Box<dyn Error>> {
    let rpr = Reaper::get_mut();
    let id_string = BACKEND_ID_STRING.to_string();
    if rpr.has_control_surface(&id_string) {
        rpr.unregister_control_surface(id_string)?;
        return Ok(());
    }

    let backend = Backend::new()?;
    rpr.register_control_surface(Arc::new(RefCell::new(backend)));
    Ok(())
}
