use log::{log, Level};
use rea_rs::{ActionKind, MessageBoxValue, PluginContext, Reaper};
use rea_rs_macros::reaper_extension_plugin;
use reaper_levitanus::{
    autofreeze::{
        action_freeze_selected_items, create_bg_instrument, delete_default_state, is_running,
        make_track_rendered, restore_default_state, toggle_action as toggle_background_renderer,
    },
    envelope_snap::register_envelope_actions,
    normalization::normalize_all_takes_on_selected_items,
    notation::test_notation,
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
    if let Err(err) = res {
        log_error("can not register normalize takes", err)
    }
    let res = rpr.register_action(
        "LEVITANUS_NORM_TAKES_CM_GN",
        "normalize_all_takes_on_selected_items (common gain)",
        ActionKind::NotToggleable,
        |_| normalize_all_takes_on_selected_items(true.into()),
        None,
    );
    if let Err(err) = res {
        log_error("can not register normalize takes", err)
    }
    match register_envelope_actions(rpr) {
        Err(err) => log_error("can not register envelope actions", err),
        Ok(_) => (),
    }

    // // FFMPEG
    // {
    //     let res = rpr.register_action(
    //         "LEVITANUS_FFMPEG_GUI",
    //         "ffmpeg GUI",
    //         ActionKind::NotToggleable,
    //         |_| ffmpeg_gui(),
    //         None,
    //     );
    //     if let Err(err) = res {
    //         log_error("can not register ffmpeg gui", err)
    //     }
    // }

    // AUTOFREEZE
    {
        if let Err(err) = restore_default_state() {
            log_error("can not restore autofreeze state", err);
            if rpr.show_message_box(
                "Corrupted Autofreeze State",
                "The state for Autofreeze is corrupted. \
            Probably, version mismatch.\n\n Apply default state?",
                rea_rs::MessageBoxType::YesNo,
            )? == MessageBoxValue::Yes
            {
                delete_default_state()?;
            }
        }

        let res = rpr.register_action(
            "LEVITANUS_AUTOFREEZE",
            "Autofreeze: toggle",
            ActionKind::Toggleable(is_running()),
            |hook| toggle_background_renderer(hook),
            None,
        );
        if let Err(err) = res {
            log_error("can not register Autofreeze: toggle", err)
        }

        let res = rpr.register_action(
            "LEVITANUS_AUTOFREEZE_CREATE_INSTRUMENT",
            "Autofreeze: create instrument",
            ActionKind::NotToggleable,
            |hook| create_bg_instrument(hook),
            None,
        );
        if let Err(err) = res {
            log_error("can not register Autofreeze: create instrument", err)
        }

        let res = rpr.register_action(
            "LEVITANUS_AUTOFREEZE_ADD_TRACK",
            "Autofreeze: make track autofreezed",
            ActionKind::NotToggleable,
            |hook| make_track_rendered(hook),
            None,
        );
        if let Err(err) = res {
            log_error("can not register Autofreeze: make track autofreezed", err)
        }

        let res = rpr.register_action(
            "LEVITANUS_AUTOFREEZE_FREEZE_ITEMS",
            "Autofreeze: freeze selected items",
            ActionKind::NotToggleable,
            |hook| action_freeze_selected_items(hook),
            None,
        );
        if let Err(err) = res {
            log_error("can not register Autofreeze: freeze selected items", err)
        }
    }

    // OTIO EXPORT
    {
        let res = rpr.register_action(
            "LEVITANUS_OTIO_EXPORT",
            "export OTIO timelines",
            ActionKind::NotToggleable,
            |_| export_otio_project(),
            None,
        );
        if let Err(err) = res {
            log_error("can not register OTIO export", err)
        }

        let res = rpr.register_action(
            "LEVITANUS_OTIO_FPS_MEDIAN",
            "set OTIO FPS to median",
            ActionKind::NotToggleable,
            |_| set_project_fps(OtioFpsPolicy::MedianVideo),
            None,
        );
        if let Err(err) = res {
            log_error("can not register OTIO FPS median", err)
        }

        let res = rpr.register_action(
            "LEVITANUS_OTIO_FPS_PROJECT",
            "set OTIO FPS to Project FPS",
            ActionKind::NotToggleable,
            |_| set_project_fps(OtioFpsPolicy::Project),
            None,
        );
        if let Err(err) = res {
            log_error("can not register OTIO FPS project", err)
        }

        let res = rpr.register_action(
            "LEVITANUS_OTIO_FPS_FIRST_VIDEO",
            "set OTIO FPS to first video in timeline",
            ActionKind::NotToggleable,
            |_| set_project_fps(OtioFpsPolicy::FirstVideo),
            None,
        );
        if let Err(err) = res {
            log_error("can not register OTIO FPS first video", err)
        }

        let res = rpr.register_action(
            "LEVITANUS_OTIO_YOUTUBE_TIMECODES",
            "export YouTube timecodes from markers",
            ActionKind::NotToggleable,
            |_| export_youtube_timecodes(),
            None,
        );
        if let Err(err) = res {
            log_error("can not register OTIO timecodes export", err)
        }
    }

    // NOTATION
    {
        let res = rpr.register_action(
            "LEVITANUS_NOTATION_TEST",
            "Levitanus Notation: test",
            ActionKind::NotToggleable,
            |_| test_notation(),
            None,
        );
        if let Err(err) = res {
            log_error("can not register Levitanus Notation: test", err)
        }
    }

    Ok(())
}

/// Show error box with OK button to user
fn log_error(_title: impl Into<String>, error: impl Into<anyhow::Error>) {
    let error = error.into();
    Reaper::get().show_console_msg(format!("Error occurred:\n{}", error.to_string()));
}
