use anyhow::anyhow;
use rea_rs::{ActionHook, ActionKind, PluginContext, Reaper};
use rea_rs_macros::reaper_extension_plugin;
use rea_rs_test::*;
use reaper_levitanus::autofreeze::{
    make_track_rendered,
    render::{action_freeze_selected_items, freeze_selected_items},
};

#[reaper_extension_plugin]
fn test_extension(context: PluginContext) -> Result<(), anyhow::Error> {
    let test = ReaperTest::setup(context, "test_action");
    let reaper = Reaper::get_mut();
    // Add custom action for freeze selected items
    reaper.register_action(
        "FREEZE_SELECTED_ITEMS",
        "Freeze Selected Items (Test)",
        ActionKind::NotToggleable,
        |_hook: &mut ActionHook| action_freeze_selected_items(_hook),
        None,
    )?;
    reaper.register_action(
        "AUTOFREEZE_ADD_TRACK",
        "Make selected track autofreezed (Test)",
        ActionKind::NotToggleable,
        |_hook: &mut ActionHook| make_track_rendered(_hook),
        None,
    )?;

    test.push_test_step(TestStep::new("Load test project", |reaper| {
        // Load the test orchestral project
        let project_path =
            "~/gits/reaper-levitanus/test/test orchestral project/test orchestral project.RPP";
        reaper.open_project(project_path, false, true)?;
        Ok(())
    }));
    test.push_test_step(TestStep::new("Select first item", |reaper| {
        let project = reaper.current_project();
        if let Some(mut item) = project.get_item(0)? {
            item.set_selected(true)?;
            println!("Selected first item in project");
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
        Ok(())
    }));
    test.push_test_step(TestStep::new("First freeze operation", |reaper| {
        println!("Running first freeze operation...");
        // Run freeze selected items
        freeze_selected_items()?;
        println!("First freeze operation completed");
        Ok(())
    }));
    test.push_test_step(TestStep::new("Second freeze operation", |reaper| {
        println!("Running second freeze operation...");
        // Run freeze selected items again
        freeze_selected_items()?;
        Ok(())
    }));

    Ok(())
}
