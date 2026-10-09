use reaper_levitanus::{
    autofreeze::{
        make_track_rendered,
        render::{action_freeze_selected_items, freeze_selected_items},
    },
    score::{register_score_actions, register_score_test_actions},
};

fn test_autofreeze(reaper: &mut Reaper) -> anyhow::Result<()> {
    // Load the test orchestral project
    let project_path = PathBuf::from(reaper.get_binary_directory())
        .join("../../../../test/test orchestral project/test orchestral project.RPP");
    // "~/gits/reaper-levitanus/test/test orchestral project/test orchestral project.RPP";
    let project = reaper.open_project(project_path, false, true)?;
    debug!("open project result: {:?}", project);
    if let Some(mut item) = project.get_item(0)? {
        item.set_selected(true)?;
        info!("Selected first item in project");
        item.track()?.make_only_selected_track()?;
        reaper.perform_action(
            reaper
                .get_action_id("AUTOFREEZE_ADD_TRACK")?
                .ok_or(anyhow!("no add trac k action"))?,
            0,
            Some(&project),
        );
    } else {
        return Err(anyhow!("No items found in project"));
    }
    info!("Running first freeze operation...");
    // Run freeze selected items
    freeze_selected_items()?;
    info!("First freeze operation completed");
    info!("Running second freeze operation...");
    // Run freeze selected items again
    freeze_selected_items()?;
    Ok(())
}