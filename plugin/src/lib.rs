use log::{log, Level};
use rea_rs::{
    ActionKind,
    // keys::{FVirt, KeyBinding, VKeys},
    // IntEnum,
    PluginContext,
    Reaper,
};
use rea_rs_macros::reaper_extension_plugin;
use reaper_levitanus::{
    background_render::{
        create_bg_instrument, is_running, make_track_rendered, restore_default_state,
        toggle_action as toggle_background_renderer,
    },
    envelope_snap::register_envelope_actions,
    ffmpeg_new::ffmpeg_gui,
    normalization::normalize_all_takes_on_selected_items,
    otio_export::{export_otio_project, export_youtube_timecodes, set_project_fps, OtioFpsPolicy},
};

#[reaper_extension_plugin]
fn plugin_main(context: PluginContext) -> Result<(), anyhow::Error> {
    env_logger::init();
    log!(Level::Info, "reaper_levitanus extension... ");
    Reaper::init_global(context);
    // Swell::load(plugin_context);
    let rpr = Reaper::get_mut();
    let res = rpr.register_action(
        "LEVITANUS_NORM_TAKES",
        "normalize_all_takes_on_selected_items",
        ActionKind::NotToggleable,
        |_| normalize_all_takes_on_selected_items(false.into()),
        None,
    );
    match res {
        Err(err) => error_box("can not register normalize takes", err),
        Ok(_) => (),
    }
    let res = rpr.register_action(
        "LEVITANUS_NORM_TAKES_CM_GN",
        "normalize_all_takes_on_selected_items (common gain)",
        ActionKind::NotToggleable,
        |_| normalize_all_takes_on_selected_items(true.into()),
        None,
    );
    match res {
        Err(err) => error_box("can not register normalize takes", err),
        Ok(_) => (),
    }
    match register_envelope_actions(rpr) {
        Err(err) => error_box("can not register envelope actions", err),
        Ok(_) => (),
    }
    let res = rpr.register_action(
        "LEVITANUS_FFMPEG_GUI",
        "ffmpeg GUI",
        ActionKind::NotToggleable,
        |_| ffmpeg_gui(),
        None,
    );
    match res {
        Err(err) => error_box("can not register ffmpeg gui", err),
        Ok(_) => (),
    }

    if let Err(err) = restore_default_state() {
        error_box("can not restore background renderer state", err);
    }

    let res = rpr.register_action(
        "LEVITANUS_BG_RENDER",
        "toggle BackgroudRenderer",
        ActionKind::Toggleable(is_running()),
        |hook| toggle_background_renderer(hook),
        None,
    );
    match res {
        Err(err) => error_box("can not register background renderer toggle", err),
        Ok(_) => (),
    }

    let res = rpr.register_action(
        "LEVITANUS_BG_RENDER_CREATE_INSTRUMENT",
        "create BackgroundRenderer instrument",
        ActionKind::NotToggleable,
        |hook| create_bg_instrument(hook),
        None,
    );
    match res {
        Err(err) => error_box("can not register create BackgroundRenderer instrument", err),
        Ok(_) => (),
    }

    let res = rpr.register_action(
        "LEVITANUS_BG_RENDER_ADD_TRACK",
        "add track to BackgroundRenderer",
        ActionKind::NotToggleable,
        |hook| make_track_rendered(hook),
        None,
    );
    match res {
        Err(err) => error_box("can not register add track to BackgroundRenderer", err),
        Ok(_) => (),
    }

    let res = rpr.register_action(
        "LEVITANUS_OTIO_EXPORT",
        "export OTIO timelines",
        ActionKind::NotToggleable,
        |_| export_otio_project(),
        None,
    );
    match res {
        Err(err) => error_box("can not register OTIO export", err),
        Ok(_) => (),
    }

    let res = rpr.register_action(
        "LEVITANUS_OTIO_FPS_MEDIAN",
        "set OTIO FPS to median",
        ActionKind::NotToggleable,
        |_| set_project_fps(OtioFpsPolicy::MedianVideo),
        None,
    );
    match res {
        Err(err) => error_box("can not register OTIO FPS median", err),
        Ok(_) => (),
    }

    let res = rpr.register_action(
        "LEVITANUS_OTIO_FPS_PROJECT",
        "set OTIO FPS to Project FPS",
        ActionKind::NotToggleable,
        |_| set_project_fps(OtioFpsPolicy::Project),
        None,
    );
    match res {
        Err(err) => error_box("can not register OTIO FPS project", err),
        Ok(_) => (),
    }

    let res = rpr.register_action(
        "LEVITANUS_OTIO_FPS_FIRST_VIDEO",
        "set OTIO FPS to first video in timeline",
        ActionKind::NotToggleable,
        |_| set_project_fps(OtioFpsPolicy::FirstVideo),
        None,
    );
    match res {
        Err(err) => error_box("can not register OTIO FPS first video", err),
        Ok(_) => (),
    }

    let res = rpr.register_action(
        "LEVITANUS_OTIO_YOUTUBE_TIMECODES",
        "export YouTube timecodes from markers",
        ActionKind::NotToggleable,
        |_| export_youtube_timecodes(),
        None,
    );
    match res {
        Err(err) => error_box("can not register OTIO timecodes export", err),
        Ok(_) => (),
    }

    Ok(())
}

/// Show error box with OK button to user
fn error_box(_title: impl Into<String>, error: impl Into<anyhow::Error>) {
    let error = error.into();
    Reaper::get().show_console_msg(format!("Error occurred:\n{}", error.to_string()));
}
