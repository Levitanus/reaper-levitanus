use log::{log, Level};
use rea_rs::{
    // keys::{FVirt, KeyBinding, VKeys},
    // IntEnum,
    PluginContext,
    Reaper,
};
use rea_rs_macros::reaper_extension_plugin;
use reaper_levitanus::{
    // ffmpeg::{gui::gui, render_video},
    envelope_snap::register_envelope_actions,
    ffmpeg::ffmpeg_gui,
    normalization::normalize_all_takes_on_selected_items,
};

use std::error::Error;

#[reaper_extension_plugin]
fn plugin_main(context: PluginContext) -> Result<(), Box<dyn Error>> {
    env_logger::init();
    log!(Level::Info, "reaper_levitanus extension... ");
    Reaper::init_global(context);
    // Swell::load(plugin_context);
    let rpr = Reaper::get_mut();
    let res = rpr.register_action(
        "LEVITANUS_NORM_TAKES",
        "normalize_all_takes_on_selected_items",
        |_: i32| normalize_all_takes_on_selected_items(false.into()),
        None,
    );
    match res {
        Err(err) => error_box("can not register normalize takes", err.to_string()),
        Ok(_) => (),
    }
    let res = rpr.register_action(
        "LEVITANUS_NORM_TAKES_CM_GN",
        "normalize_all_takes_on_selected_items (common gain)",
        |_: i32| normalize_all_takes_on_selected_items(true.into()),
        None,
    );
    match res {
        Err(err) => error_box("can not register normalize takes", err.to_string()),
        Ok(_) => (),
    }
    match register_envelope_actions(rpr) {
        Err(err) => error_box("can not register envelope actions", err.to_string()),
        Ok(_) => (),
    }
    // let res = rpr.register_action(
    //     "LEVITANUS_FFMPEG_RENDER_ALL",
    //     "render project video",
    //     |_: i32| render_video(),
    //     None,
    // );
    // match res {
    //     Err(err) => error_box("can not register render video", err.to_string()),
    //     Ok(_) => (),
    // }
    let res = rpr.register_action(
        "LEVITANUS_FFMPEG_GUI",
        "ffmpeg gui",
        |_: i32| ffmpeg_gui(),
        None,
    );
    match res {
        Err(err) => error_box("can not register ffmpeg gui", err.to_string()),
        Ok(_) => (),
    }

    Ok(())
}

/// Show error box with OK button to user
fn error_box(title: impl Into<String>, msg: impl Into<String>) {
    Reaper::get()
        .show_message_box(
            title,
            format!("Error occurred:\n{}", msg.into()),
            rea_rs::MessageBoxType::Ok,
        )
        .expect("Error while displaying error");
}
