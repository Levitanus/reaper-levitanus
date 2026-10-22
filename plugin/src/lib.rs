use log::{log, Level};
use rea_rs::{PluginContext, Reaper};
use rea_rs_macros::reaper_extension_plugin;
use reaper_levitanus::{
    // autofreeze::register_autofreeze_actions, envelope_snap::register_envelope_actions,
    // ffmpeg_new::register_ffmpeg_acions, normalization::register_normalization_actions,
    // otio_export::register_otio_export_actions,
    score::register_score_actions,
};

#[reaper_extension_plugin]
fn plugin_main(context: PluginContext) -> Result<(), anyhow::Error> {
    env_logger::init();
    log!(Level::Info, "reaper_levitanus extension... ");
    Reaper::init_global(context);
    // Swell::load(plugin_context);
    let rpr = Reaper::get_mut();
    // match register_normalization_actions(rpr) {
    //     Err(err) => log_error("can not register normalization actions", err),
    //     Ok(_) => (),
    // }
    // match register_envelope_actions(rpr) {
    //     Err(err) => log_error("can not register envelope actions", err),
    //     Ok(_) => (),
    // }

    // // FFMPEG
    // {
    //     match register_ffmpeg_acions(rpr) {
    //         Err(err) => log_error("can not register ffmpeg actions", err),
    //         Ok(_) => (),
    //     }
    // }

    // // AUTOFREEZE
    // {
    //     match register_autofreeze_actions(rpr) {
    //         Err(err) => log_error("can not register autofreeze actions", err),
    //         Ok(_) => (),
    //     }
    // }

    // // OTIO EXPORT
    // {
    //     match register_otio_export_actions(rpr) {
    //         Err(err) => log_error("can not register otio export actions", err),
    //         Ok(_) => (),
    //     }
    // }

    // SCORE
    {
        match register_score_actions(rpr) {
            Err(err) => log_error("can not register score actions", err),
            Ok(_) => (),
        }
    }

    Ok(())
}

/// Show error box with OK button to user
fn log_error(_title: impl Into<String>, error: impl Into<anyhow::Error>) {
    let error = error.into();
    let _ = Reaper::get().show_console_msg(format!("Error occurred:\n{}", error.to_string()));
}
