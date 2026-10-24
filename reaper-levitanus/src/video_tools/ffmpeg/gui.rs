use std::{
    sync::mpsc::{self, Receiver},
    thread,
};

use anyhow::Result;
use rea_rs::{
    ActionHook, CommandId, ProgressBarOptions, Reaper, ReaperWindow, SwellId, WidgetSize,
    WindowHandler, WindowSpec,
};

use crate::{
    utils::extension_folder,
    video_tools::ffmpeg::parser::{ParsingProgress, parse_all},
};

pub fn toggle_ffmpeg_gui(hook: &mut ActionHook) -> Result<()> {
    match hook.toggle_state().expect("should be toggle state here") {
        false => {
            Reaper::get_mut()
                .register_window_handler(Box::new(FfmpegGui::new(hook.command_id())?))?;
        }
        true => todo!(),
    }
    Ok(())
}

struct FfmpegGui {
    window: ReaperWindow,
    command: CommandId,
    state: GuiState,
}
impl FfmpegGui {
    fn new(command: CommandId) -> Result<Self> {
        let rpr = Reaper::get();
        let spec = WindowSpec::new("Levitanus: ffmpeg GUI").allow_show(true);
        let window = rpr.create_window(&spec)?;
        Ok(FfmpegGui {
            window,
            command,
            state: GuiState::Init,
        })
    }
    fn rebuild(&mut self) -> Result<()> {
        let ui = self.window.build_ui()?;
        let ui = ui.panel(rea_rs::Panel::Central)?;
        match self.state {
            GuiState::Parse { rx, progress_id } => {
                match rx.try_recv()? {
                    ParsingProgress::Progress(p) => ui.progress_bar(
                        progress_id,
                        WidgetSize::new_fill_x(150, 50),
                        ProgressBarOptions::empty(),
                    ),
                };
            }
        }
        Ok(())
    }
    fn parse(&mut self) -> Result<()> {
        let (tx, rx): _ = mpsc::channel();
        let out_dir = extension_folder()?;
        let _ = thread::spawn(|| {
            if let Err(e) = parse_all(out_dir, tx) {
                eprintln!("parse error: {:?}", e)
            }
        });
        self.state = GuiState::Parse {
            rx,
            progress_id: SwellId::new_control(),
        };
        Ok(())
    }
}
impl WindowHandler for FfmpegGui {
    fn window_id(&self) -> rea_rs::WindowId {
        "FFMPEG_GUI".into()
    }

    fn window(&self) -> &rea_rs::ReaperWindow {
        &self.window
    }
    fn on_open(&mut self) -> anyhow::Result<()> {
        let rpr = Reaper::get();
        if !super::parser::check_parsed_paths(extension_folder()?) {
            match rpr.show_message_box(
                "Parse ffmpeg",
                "ffmpeg is not yet parsed: do you want to parse it?",
                rea_rs::MessageBoxType::YesNo,
            )? {
                rea_rs::MessageBoxValue::Yes => self.parse()?,
                _ => {
                    rpr.action_toggle_state(self.command);
                    self.window.destroy()?;
                    return Ok(());
                }
            }
        }
        self.rebuild()
    }
}

enum GuiState {
    Init,
    Parse {
        rx: Receiver<ParsingProgress>,
        progress_id: SwellId,
    },
}
