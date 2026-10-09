use rea_rs::{PluginContext, Reaper};
use rea_rs_macros::reaper_extension_plugin;
use rea_rs_test::*;

// mod autofreeze;
mod score;
use crate::score::register_score_tests;

#[reaper_extension_plugin]
fn test_extension(context: PluginContext) -> Result<(), anyhow::Error> {
    let test = ReaperTest::setup(context, "test_action");
    let reaper = Reaper::get_mut();
    env_logger::init();
    register_score_tests(test, reaper)?;

    // test.push_test_step(TestStep::new("test_autofreeze", test_autofreeze));

    Ok(())
}
