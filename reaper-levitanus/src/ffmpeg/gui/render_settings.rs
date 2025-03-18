use egui::{
    CollapsingHeader, ComboBox, Context, DragValue, Grid, Layout, RichText, ScrollArea, Ui,
};
use itertools::Itertools;

use crate::ffmpeg::options::{Opt, OptionParameter};

use super::{Front, FrontMessage};

impl Front {
    pub(crate) fn widget_render_settings(&mut self, ctx: &Context, ui: &mut Ui) {
        CollapsingHeader::new("render settings")
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let current_muxer = &mut self.state.render_settings.muxer;
                    ui.vertical(|ui| {
                        ui.label(RichText::new("muxer:").strong());
                        ComboBox::from_id_salt("muxer")
                            .selected_text(current_muxer.name.clone())
                            .show_ui(ui, |ui| {
                                for mux in self.muxers.iter() {
                                    if ui
                                        .selectable_label(
                                            mux.name == current_muxer.name,
                                            mux.name.clone(),
                                        )
                                        .on_hover_ui(|ui| {
                                            ui.label(mux.description.clone());
                                        })
                                        .clicked()
                                    {
                                        *current_muxer = mux.clone();
                                    }
                                }
                            });
                        ui.label(RichText::new(current_muxer.description.clone()));
                        if let Some(ext) = &current_muxer.extensions {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Extension:").strong());
                                ui.label(RichText::new(
                                    ext.first().unwrap_or(&"unknown".to_string()),
                                ));
                            });
                        }
                    });
                    Self::widget_options(ui, "muxer".to_string(), &mut current_muxer.options);
                });
            });
    }
    fn widget_options(ui: &mut Ui, id: String, options: &mut Vec<Opt>) {
        ScrollArea::vertical()
            .max_height(200.0)
            .min_scrolled_height(100.0)
            .show(ui, |ui| {
                // ui.set_max_width(ui.available_width() - 30.0);
                Grid::new(id + "options")
                    .min_col_width(50.0)
                    .max_col_width(400.0)
                    .num_columns(3)
                    .striped(true)
                    .spacing((10.0, 10.0))
                    .show(ui, |ui| {
                        for opt in options {
                            ui.vertical(|ui| {
                                ui.label(RichText::new(opt.name.clone()).strong());
                                ui.label(RichText::new(opt.description.clone()).weak());
                            });
                            ui.vertical(|ui| {
                                ui.label("default");
                                ui.label(
                                    RichText::new(
                                        opt.default.clone().unwrap_or("unknown".to_string()),
                                    )
                                    .weak(),
                                );
                            });
                            match &mut opt.parameter {
                                OptionParameter::Int(v) => match v {
                                    Some(mut val) => {
                                        ui.vertical(|ui| {
                                            if ui.add(DragValue::new(&mut val)).changed() {
                                                opt.parameter = OptionParameter::Int(Some(val));
                                            };
                                            if ui.button("clear parameter").clicked() {
                                                opt.parameter = OptionParameter::Int(None);
                                            }
                                        });
                                    }
                                    None => {
                                        if ui.button("use parameter").clicked() {
                                            opt.parameter =
                                                OptionParameter::Int(Some(i32::default()));
                                        }
                                    }
                                },
                                OptionParameter::Bool(v) => match v {
                                    Some(mut val) => {
                                        ui.vertical(|ui| {
                                            if ui.checkbox(&mut val, "").clicked() {
                                                opt.parameter = OptionParameter::Bool(Some(val));
                                            };
                                            if ui.button("clear parameter").clicked() {
                                                opt.parameter = OptionParameter::Bool(None);
                                            }
                                        });
                                    }
                                    None => {
                                        if ui.button("use parameter").clicked() {
                                            opt.parameter =
                                                OptionParameter::Bool(Some(bool::default()));
                                        }
                                    }
                                },
                                _ => {
                                    ui.label("TODO");
                                }
                            };
                            ui.end_row();
                        }
                    });
                ui.set_height(ui.available_height());
            });
    }
}
