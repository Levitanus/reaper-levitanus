use std::{path::PathBuf, sync::mpsc::Receiver};

use super::{Front, FrontMessage};
use crate::{ffmpeg::parser::ParsingProgress, LevitanusError};
use egui::{
    text::LayoutJob, Area, Color32, ComboBox, Context, FontId, Frame, Id, InnerResponse, Layout,
    Modal, ProgressBar, Response, RichText, Stroke, TextFormat, Ui, Window,
};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct RenderJob {
    filename: PathBuf,
    progress: RenderProgress,
    reciever: Option<Receiver<RenderProgress>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RenderProgress {
    Progress(f32),
    Result(Result<(), String>),
}

impl Front {
    pub(crate) fn widget_render(&mut self, ctx: &Context, ui: &mut Ui) {
        Self::frame(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("render").clicked() {
                    self.emit(FrontMessage::Render);
                }
                ui.checkbox(&mut self.state.parallel_render, "render files parallel");
            });
            if self.render_jobs.len() > 0 {
                Modal::new(Id::new("render")).show(ctx, |ui| {
                    ui.with_layout(
                        Layout::left_to_right(egui::Align::Min).with_main_wrap(true),
                        |ui| {
                            for job in self.render_jobs.iter() {
                                let (progress, status, error) = match &job.progress {
                                    RenderProgress::Progress(p) => (*p, "rendering", None),
                                    RenderProgress::Result(r) => match r {
                                        Ok(()) => (1.0, "rendered", None),
                                        Err(e) => (-1.0, "error", Some(e)),
                                    },
                                };
                                ui.label(job.filename.to_string_lossy());
                                ui.label(status);
                                match error {
                                    None => {
                                        ui.add(ProgressBar::new(progress));
                                    }
                                    Some(e) => {
                                        if ui.button("show error").clicked() {
                                            Window::new("render error").show(ctx, |ui| ui.label(e));
                                        }
                                    }
                                }
                            }
                        },
                    );
                });
            }
        });
    }
    pub(crate) fn widget_parser(&self, ctx: &egui::Context, ui: &mut egui::Ui) {
        Self::frame(ui, |ui| {
            match &self.parsing_progress {
                ParsingProgress::Unparsed => {
                    Modal::new(Id::new("parse yes no")).show(ctx, |ui| {
                        ui.heading("Parse FFMPEG");
                        ui.label(
                            "FFMPEG muxers, codecs and filters are not yet parsed.\n\
                        Do you wish to parse them now?\n\
                        It will take up to 30 seconds.",
                        );
                        ui.horizontal_centered(|ui| {
                            if ui.button("Yes").clicked() {
                                self.emit(FrontMessage::Parse);
                                // ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                            if ui.button("No").clicked() {
                                self.emit(FrontMessage::Exit);
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        });
                    });
                }
                ParsingProgress::Progress(p) => {
                    ui.add(ProgressBar::new(*p));
                }
                ParsingProgress::Result(r) => {
                    // ui.horizontal(|ui| {
                    match r {
                        Ok(_) => (),
                        Err(e) => {
                            ui.label(
                                RichText::new(format!("failed to parse FFMPEG: {}", e))
                                    .color(Color32::RED),
                            );
                        }
                    }
                    if ui.button("reparse ffmpeg").clicked() {
                        self.emit(FrontMessage::Parse);
                    }
                    // });
                }
            }
        });
    }

    pub(crate) fn widget_error_box(&self, ctx: &Context, error: impl AsRef<str>) {
        Modal::new(Id::new("error")).show(ctx, |ui| {
            ui.with_layout(Layout::top_down_justified(egui::Align::Center), |ui| {
                ui.heading("Error!");
                ui.label("Application will be closed because of the error:");
                ui.label(RichText::new(error.as_ref()).color(Color32::RED));
                if ui.button("Ok").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            })
        });
    }

    pub(crate) fn frame<F>(
        ui: &mut Ui,
        add_contents: impl FnOnce(&mut Ui) -> F,
    ) -> InnerResponse<F> {
        Frame::new()
            .stroke(Stroke::new(1.5, Color32::from_white_alpha(0x20)))
            .corner_radius(10.0)
            .inner_margin(7.0)
            .show(ui, add_contents)
    }

    pub(crate) fn encoder_flag<'a>(
        ui: &mut Ui,
        name: &str,
        status: bool,
        invert_color: bool,
    ) -> Response {
        let color = |mut cond: bool| {
            if invert_color {
                cond = !cond;
            }
            if cond {
                Color32::GREEN
            } else {
                Color32::RED
            }
        };
        let text = |cond| if cond { "yes" } else { "no" };
        let mut job = LayoutJob::default();
        job.append(&format!("{}: ", name), 0.0, TextFormat::default());
        job.append(
            text(status),
            0.0,
            TextFormat::simple(FontId::default(), color(status)),
        );
        ui.label(job)
    }

    pub(crate) fn alternative_value(
        &self,
        ctx: &Context,
        art: impl AsRef<str>,
        value: impl AsRef<str>,
        list: impl Iterator<Item = impl AsRef<str>>,
    ) -> Option<String> {
        Modal::new(Id::new("alternative value"))
            .show(ctx, |ui| {
                ui.heading("Key Error");
                ui.label(format!(
                    "Can not find {} with name {}",
                    art.as_ref(),
                    value.as_ref()
                ));
                ui.label("please, choose an alternative:");
                ComboBox::from_id_salt("alternative")
                    .selected_text(&self.alternative_value)
                    .show_ui(ui, |ui| {
                        for name in list {
                            let name = name.as_ref();
                            if ui
                                .selectable_label(name == &self.alternative_value, name)
                                .clicked()
                            {
                                self.emit(FrontMessage::AlternativeValue(name.to_string()));
                            }
                        }
                    });
                if ui.button("Ok").clicked() {
                    return Some(self.alternative_value.clone());
                }
                None
            })
            .inner
    }
}
