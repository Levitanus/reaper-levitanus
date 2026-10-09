use std::path::PathBuf;

use anyhow::{anyhow, Result};
use log::debug;
use rea_rs::{Item, Reaper};
use rea_rs_test::{ReaperTest, TestStep};
use reaper_levitanus::score::{
    get_notations_of_item, register_score_actions, register_score_test_actions,
};

pub fn register_score_tests(test: &mut ReaperTest, reaper: &mut Reaper) -> Result<()> {
    register_score_actions(reaper)?;
    register_score_test_actions(reaper)?;
    test.push_test_step(TestStep::new("test score projcet", score_project));
    Ok(())
}

pub fn score_project(reaper: &'static mut Reaper) -> Result<()> {
    let project_path =
        PathBuf::from(reaper.get_binary_directory()?).join("../../../../test/test notation.RPP");
    // "~/gits/reaper-levitanus/test/test orchestral project/test orchestral project.RPP";
    let project = reaper.open_project(project_path, false, true)?;
    debug!("open project result: {:?}", project);
    let item: Item = {
        let mut item = None;
        for track in project.iter_tracks() {
            if track.name()? == "test" {
                item = track.get_item(0)?;
            }
        }
        match item {
            Some(item) => item,
            None => return Err(anyhow!("no item on the track test")),
        }
    };
    let item_notations = get_notations_of_item(item)?;
    // debug!("notes: {:#?}", notes);
    debug!("track_notations: {:#?}", item_notations);
    Ok(())
}
