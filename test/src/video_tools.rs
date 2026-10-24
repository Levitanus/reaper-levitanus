use anyhow::Result;
use rea_rs::Reaper;
use rea_rs_test::ReaperTest;
use reaper_levitanus::video_tools::register_video_tools_actions;

pub fn register_video_tests(test: &mut ReaperTest, reaper: &mut Reaper) -> Result<()> {
    register_video_tools_actions(reaper)
}
