use anyhow::Result;
use rea_rs::Reaper;

mod ffmpeg;

pub fn register_video_tools_actions(reaper: &mut Reaper) -> Result<()> {
    // reaper.register_preferences_page(id, display_name, builder)
    ffmpeg::register_ffmpeg_actions(reaper)
}
