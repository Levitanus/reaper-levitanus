use super::{Front, FrontMessage};
use crate::ffmpeg::parser::ParsingProgress;
use egui::{Color32, Id, Layout, Modal, ProgressBar, RichText};

impl Front {
    pub(crate) fn widget_parser(&self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.with_layout(Layout::top_down(egui::Align::Center), |ui| {
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
}
