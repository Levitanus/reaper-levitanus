use std::{cell::RefCell, error::Error, sync::Arc};

use rea_rs::Reaper;
use serde::{Deserialize, Serialize};

use crate::{
    gui::{launch_frontend, stop_backend},
    sample_editor::gui::{Backend, BACKEND_ID_STRING},
};

mod gui;
pub use gui::{front, SOCKET_PORT};

pub fn sample_editor_gui() -> Result<(), Box<dyn Error>> {
    let rpr = Reaper::get_mut();
    if !rpr.has_control_surface(&BACKEND_ID_STRING.to_string()) {
        let backend = Backend::new()?;
        rpr.register_control_surface(Arc::new(RefCell::new(backend)));
    }
    launch_frontend(crate::gui::ComponentType::SampleEditor)?;
    Ok(())
}

pub fn stop_sample_editor_gui() -> Result<(), Box<dyn Error>> {
    stop_backend(BACKEND_ID_STRING)
}

fn name_regions_in_time_selection() {
    let rpr = Reaper::get();
    let pr = rpr.current_project();
    let ts = pr.get_time_selection();

    pr.iter_markers_and_regions()
        .filter(|r| r.is_region && ts.contains(r.position) && ts.contains(r.rgn_end));
}
