use anyhow::Result;
use rea_rs::{ActionRegistrationOptions, ExtState, Project, Reaper, ReaperResult};
use serde::{Deserialize, Serialize};

use crate::video_tools::ffmpeg::options::Opt;

mod base_types;
// mod nodes;
mod gui;
mod options;
mod parser;
mod stream_ids;

static EXT_SECTION: &'static str = "LEVITANUS_FFMPEG";

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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FfmpegProjectSettings {
    muxer: String,
    audio_codec: String,
    video_codec: String,
    subtitle_codec: String,
    muxer_options: Vec<Opt>,
    ac_options: Vec<Opt>,
    vc_options: Vec<Opt>,
    sc_options: Vec<Opt>,
}
impl Default for FfmpegProjectSettings {
    fn default() -> Self {
        Self {
            muxer: "matroska".to_string(),
            audio_codec: "copy".to_owned(),
            video_codec: "copy".to_owned(),
            subtitle_codec: "copy".to_owned(),
            muxer_options: Vec::new(),
            ac_options: Vec::new(),
            vc_options: Vec::new(),
            sc_options: Vec::new(),
        }
    }
}
impl FfmpegProjectSettings {
    pub fn load(project: &Project) -> ReaperResult<Self> {
        Ok(Self::ext_state(project).get()?.unwrap_or_default())
    }
    pub fn save(&self, project: &Project) -> ReaperResult<()> {
        Self::ext_state(project).set(self.clone())
    }
    fn ext_state(project: &Project) -> ExtState<'_, Self, Project> {
        ExtState::<Self, Project>::existing(EXT_SECTION, "project_settings", true, project, None)
    }
}
