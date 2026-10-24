use anyhow::Result;
use rea_rs::{ActionRegistrationOptions, Reaper};

mod base_types;
// mod nodes;
mod gui;
mod options;
mod parser;
mod stream_ids;

pub fn register_ffmpeg_actions(reaper: &mut Reaper) -> Result<()> {
    reaper.register_action(
        "LEVITANUS_FFMPEG_GUI",
        "Levitanus: ffmpeg GUI",
        rea_rs::ActionKind::Toggleable(false),
        gui::toggle_ffmpeg_gui,
        ActionRegistrationOptions::default(),
    )?;
    Ok(())
}
