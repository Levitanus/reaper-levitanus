use std::{
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use anyhow::{anyhow, Result};
use log::error;
use rea_rs::{
    ActionHook, CommandId, ControlEvent, ProgressBar, ProgressBarOptions, Reaper, ReaperWindow,
    SwellId, WidgetSize, WindowHandler, WindowSpec,
};

use crate::{
    utils::extension_folder,
    video_tools::ffmpeg::parser::{parse_all, ParsingProgress},
    LevitanusError,
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
    timer_id: SwellId,
    state: GuiState,
}
impl FfmpegGui {
    fn new(command: CommandId) -> Result<Self> {
        let rpr = Reaper::get();
        let spec = WindowSpec::new("Levitanus: ffmpeg GUI").allow_show(true);
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
        let ui = self.window.build_ui()?;
        let ui = ui.panel(rea_rs::Panel::Central)?;
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
                rx,
                progress_id,
                cancel_id,
            } => {
                match rx.try_recv()? {
                    ParsingProgress::Progress(p) => {
                        let pbar = ui.progress_bar(
                            *progress_id,
                            WidgetSize::new_fill_x(150, 50),
                            ProgressBarOptions::empty(),
                        )?;
                        pbar.set_position((p * 100.0) as i32)?;
                        let row =
                            ui.row(SwellId::new_control(), WidgetSize::new_fill_x(150, 30))?;
                        row.button(*cancel_id, "Cancel", WidgetSize::fixed(100, 30))?;
                    }
                    ParsingProgress::Result(r) => match r {
                        Ok(_) => return self.update_state(),
                        Err(e) => return Err(anyhow!("{}", e)),
                    },
                    ParsingProgress::Unparsed => {
                        return Err(LevitanusError::Unexpected(
                            "Unparsed state while parsing".to_owned(),
                        )
                        .into())
                    }
                };
            }
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
        pr.get_render_settings()?.mode
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
            cancel_id: SwellId::new_control(),
        };
        Ok(())
    }

    fn close(&mut self) -> Result<()> {
        Reaper::get().action_toggle_state(self.command);
        self.window.destroy()?;
        return Ok(());
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
        }
    }
    fn on_timer(&mut self, id: SwellId) -> anyhow::Result<()> {
        if id != self.timer_id {
            return Ok(());
        }
        match &self.state {
            GuiState::Init => Ok(()),
            GuiState::Parse {
                rx,
                progress_id,
                cancel_id: _,
            } => {
                if let Ok(p) = rx.try_recv() {
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
                        ParsingProgress::Unparsed => self.parse()?,
                        ParsingProgress::Result(r) => match r {
                            Ok(_) => return self.update_state(),
                            Err(e) => return Err(anyhow!(e)),
                        },
                    }
                }
                Ok(())
            }
        }
    }
}

enum GuiState {
    Init,
    Parse {
        rx: Receiver<ParsingProgress>,
        progress_id: SwellId,
        cancel_id: SwellId,
    },
}
