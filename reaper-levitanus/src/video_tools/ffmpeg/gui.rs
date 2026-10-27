use std::{
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use anyhow::{anyhow, Result};
use log::error;
use rea_rs::{
    ActionHook, CommandId, ControlEvent, Insets, ProgressBar, ProgressBarOptions, Reaper,
    ReaperWindow, SwellId, WidgetSize, WindowHandler, WindowSpec,
};

use crate::{
    utils::extension_folder,
    video_tools::ffmpeg::{
        parser::{parse_all, ParsingProgress},
        FfmpegProjectSettings,
    },
    LevitanusError,
};

static FFMPEG_WIND_ID: &'static str = "FFMPEG_GUI";

pub fn toggle_ffmpeg_gui(hook: &mut ActionHook) -> Result<()> {
    let rpr = Reaper::get_mut();
    match hook.toggle_state().expect("should be toggle state here") {
        false => {
            rpr.register_window_handler(Box::new(FfmpegGui::new(hook.command_id())?))?;
        }
        true => {
            if rpr.is_window_registered(&FFMPEG_WIND_ID.into()) {
                rpr.unregister_window_handler(&FFMPEG_WIND_ID.into())?;
            }
        }
    }
    Ok(())
}

struct FfmpegGui {
    window: ReaperWindow,
    command: CommandId,
    timer_id: SwellId,
    state: GuiState,
}
impl FfmpegGui {
    fn new(command: CommandId) -> Result<Self> {
        let rpr = Reaper::get();
        let spec = WindowSpec::new("Levitanus: ffmpeg GUI")
            .allow_show(true)
            .min_size(300, 150)
            .size(300, 150);
        let window = rpr.create_window(&spec)?;
        let timer_id = SwellId::new_timer();
        window.start_timer(timer_id, Duration::from_millis(150))?;
        Ok(FfmpegGui {
            window,
            command,
            timer_id,
            state: GuiState::Init,
        })
    }
    fn rebuild(&mut self) -> Result<()> {
        let ui = self.window.build_ui()?.with_insets(Insets::uniform(25));
        let rpr = Reaper::get();
        match &mut self.state {
            GuiState::Init => {
                if !super::parser::check_parsed_paths(extension_folder()?) {
                    match rpr.show_message_box(
                        "Parse ffmpeg",
                        "ffmpeg is not yet parsed: do you want to parse it?",
                        rea_rs::MessageBoxType::YesNo,
                    )? {
                        rea_rs::MessageBoxValue::Yes => self.parse()?,
                        _ => {
                            self.close()?;
                        }
                    }
                }
            }
            GuiState::Parse {
                rx: _,
                progress_id,
                last_progress,
                cancel_id,
            } => {
                match last_progress {
                    ParsingProgress::Progress(p) => {
                        let pbar = ui.progress_bar(
                            *progress_id,
                            WidgetSize::new_fill_x(100, 30),
                            ProgressBarOptions::empty(),
                        )?;
                        pbar.set_position((*p * 100.0) as i32)?;
                        ui.button(*cancel_id, "Cancel", WidgetSize::fixed(100, 30))?;
                    }
                    ParsingProgress::Result(r) => match r {
                        Ok(_) => return self.update_state(),
                        Err(e) => return Err(anyhow!("{}", e)),
                    },
                };
            }
            GuiState::Normal => check_state()?,
        }
        Ok(())
    }
    fn update_state(&mut self) -> Result<()> {
        if !super::parser::check_parsed_paths(extension_folder()?) {
            return Err(
                LevitanusError::FrontInitialization("ffmpeg is not parsed".to_owned()).into(),
            );
        }
        let rpr = Reaper::get();
        let pr = rpr.current_project();
        let pr_settings = FfmpegProjectSettings::load(&pr)?;

        self.state = GuiState::Normal;
        self.rebuild()
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
            last_progress: ParsingProgress::Progress(0.0),
            cancel_id: SwellId::new_control(),
        };
        self.rebuild()
    }

    fn close(&mut self) -> Result<()> {
        Reaper::get().action_toggle_state(self.command);
        self.window.destroy()?;
        return Ok(());
    }
}
impl WindowHandler for FfmpegGui {
    fn window_id(&self) -> rea_rs::WindowId {
        FFMPEG_WIND_ID.into()
    }

    fn window(&self) -> &rea_rs::ReaperWindow {
        &self.window
    }
    fn on_open(&mut self) -> anyhow::Result<()> {
        self.rebuild()
    }
    fn on_control_event(
        &mut self,
        event: rea_rs::swell_gui::events::ControlEvent,
    ) -> anyhow::Result<()> {
        match self.state {
            GuiState::Init => {
                error!("inexpected control event: {:?}", event);
                Ok(())
            }
            GuiState::Parse {
                rx: _,
                progress_id: _,
                last_progress: _,
                cancel_id,
            } => match event {
                ControlEvent::ButtonClicked { control } => {
                    if control == cancel_id {
                        return self.close();
                    }
                    Ok(())
                }
                _ => Ok(()),
            },
            GuiState::Normal => check_state(),
        }
    }
    fn on_timer(&mut self, id: SwellId) -> anyhow::Result<()> {
        if id != self.timer_id {
            return Ok(());
        }
        match &mut self.state {
            GuiState::Init => Ok(()),
            GuiState::Parse {
                rx,
                last_progress,
                progress_id,
                cancel_id: _,
            } => {
                while let Ok(p) = rx.try_recv() {
                    *last_progress = p.clone();
                    match p {
                        ParsingProgress::Progress(p) => {
                            let handle = self.window.control(*progress_id).ok_or(
                                LevitanusError::KeyError(
                                    "window".to_owned(),
                                    format!("control_id: {:?}", progress_id),
                                ),
                            )?;
                            let control = ProgressBar::new(handle);
                            control.set_position((p * 100.0) as i32)?;
                        }
                        ParsingProgress::Result(r) => match r {
                            Ok(_) => return self.update_state(),
                            Err(e) => return Err(anyhow!(e)),
                        },
                    }
                }
                Ok(())
            }
            GuiState::Normal => check_state(),
        }
    }
}

#[deprecated]
fn check_state() -> Result<()> {
    Ok(())
}

enum GuiState {
    Init,
    Parse {
        rx: Receiver<ParsingProgress>,
        last_progress: ParsingProgress,
        progress_id: SwellId,
        cancel_id: SwellId,
    },
    Normal,
}
