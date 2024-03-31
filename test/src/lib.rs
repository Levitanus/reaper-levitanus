use rea_rs::PluginContext;
use rea_rs_macros::reaper_extension_plugin;
use rea_rs_test::*;
use std::error::Error;

#[reaper_extension_plugin]
fn test_extension(context: PluginContext) -> Result<(), Box<dyn Error>> {
    let _ = ReaperTest::setup(context, "test_action");
    Ok(())
}

// fn clear_project(reaper: &mut Reaper) -> Project {
//     let mut pr = reaper.current_project();
//     for idx in pr.n_tracks()..0 {
//         pr.get_track_mut(idx).unwrap().delete();
//     }
//     pr
// }
