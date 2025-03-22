use egui::{CollapsingHeader, ComboBox, Context, DragValue, Grid, RichText, ScrollArea, Ui};
use itertools::Itertools;

use super::{Front, FrontMessage};
use crate::{
    ffmpeg::{
        options::{DurationUnit, Encoder, EncoderType, FfmpegColor, Opt, OptionParameter},
        parser::ParsingProgress,
        RenderSettings,
    },
    LevitanusError,
};

impl Front {
    pub(crate) fn widget_render_settings(&mut self, ctx: &Context, ui: &mut Ui) {
        match self.parsing_progress {
            ParsingProgress::Unparsed
            | ParsingProgress::Progress(_)
            | ParsingProgress::Result(Err(_)) => return,
            ParsingProgress::Result(Ok(_)) => (),
        }
        CollapsingHeader::new("render settings")
            .default_open(true)
            .show(ui, |ui| {
                if ui.button("reset to defaults").clicked() {
                    self.state.render_settings = RenderSettings::default();
                    return;
                }
                // Muxer
                ui.horizontal(|ui| {
                    let current_muxer = match self
                        .muxers
                        .iter()
                        .find(|mux| mux.name == self.state.render_settings.muxer)
                    {
                        Some(m) => m.clone(),
                        None => {
                            self.emit(FrontMessage::Error(format!(
                                "didn't found muxer {}",
                                self.state.render_settings.muxer
                            )));
                            return;
                        }
                    };
                    self.widget_muxer(ui, current_muxer);
                });
                ui.horizontal(|ui| {
                    let current_encoder = if let Some(enc) = self
                        .encoders
                        .iter()
                        .find(|enc| enc.name == self.state.render_settings.video_encoder)
                    {
                        enc.clone()
                    } else {
                        return self.emit(FrontMessage::Error(
                            LevitanusError::KeyError(
                                "encoder".to_string(),
                                self.state.render_settings.video_encoder.clone(),
                            )
                            .to_string(),
                        ));
                    };
                    self.widget_encoder(ui, current_encoder);
                });
            });
    }

    fn widget_encoder(&mut self, ui: &mut Ui, current_encoder: Encoder) {
        ui.vertical(|ui| {
            ui.label(RichText::new("video encoder").strong());
            ComboBox::from_id_salt("encoder")
                .selected_text(&self.state.render_settings.video_encoder)
                .show_ui(ui, |ui| {
                    for enc in self
                        .encoders
                        .iter()
                        .filter(|e| e.encoder_type == EncoderType::Video)
                    {
                        if ui
                            .selectable_label(enc.name == current_encoder.name, &enc.name)
                            .clicked()
                        {
                            self.state.render_settings.video_encoder = enc.name.clone();
                            self.state.render_settings.video_encoder_options = Vec::new();
                        }
                    }
                });
            ui.label(RichText::new("pixel format").strong());
            ComboBox::from_id_salt("pixel_format")
                .selected_text(&self.state.render_settings.pixel_format)
                .show_ui(ui, |ui| {
                    if let Some(px_fmts) = &current_encoder.supported_pixel_formats {
                        for px_fmt in px_fmts {
                            if ui
                                .selectable_label(
                                    px_fmt == &self.state.render_settings.pixel_format,
                                    px_fmt,
                                )
                                .clicked()
                            {
                                self.state.render_settings.pixel_format = px_fmt.clone();
                            }
                        }
                    }
                });
        });
        Self::options_wrapper(
            ui,
            "encoder",
            &mut self.state.render_settings.video_encoder_options,
            current_encoder.options,
        );
    }

    fn widget_muxer(&mut self, ui: &mut Ui, current_muxer: crate::ffmpeg::options::Muxer) {
        ui.vertical(|ui| {
            ui.set_max_width(120.0);
            ui.label(RichText::new("muxer:").strong());
            ComboBox::from_id_salt("muxer")
                .selected_text(current_muxer.name.clone())
                .show_ui(ui, |ui| {
                    for mux in self.muxers.iter() {
                        let extensions = mux
                            .extensions
                            .as_ref()
                            .unwrap_or(&Vec::new())
                            .iter()
                            .join(",");
                        if ui
                            .selectable_label(mux.name == current_muxer.name, &mux.name)
                            .on_hover_text(format!(
                                "{}\nextensions: {:?}",
                                &mux.description, extensions
                            ))
                            .clicked()
                        {
                            self.state.render_settings.muxer = mux.name.clone();
                            if let Some(c) = &mux.video_codec {
                                self.state.render_settings.video_encoder = c.clone()
                            }
                            if let Some(c) = &mux.audio_codec {
                                self.state.render_settings.audio_encoder = c.clone()
                            }
                            if let Some(c) = &mux.subtitle_codec {
                                self.state.render_settings.subtitle_encoder = c.clone()
                            }
                            if let Some(ext) = mux.extensions.as_ref() {
                                self.state.render_settings.extension = ext[0].clone();
                            }
                        }
                    }
                });
            ui.label(RichText::new(current_muxer.description.clone()));
            if let Some(extensions) = &current_muxer.extensions {
                ui.label(RichText::new("extension:").strong());
                ComboBox::from_id_salt("extension")
                    .selected_text(&self.state.render_settings.extension)
                    .show_ui(ui, |ui| {
                        for ext in extensions {
                            ui.selectable_value(
                                &mut self.state.render_settings.extension,
                                ext.clone(),
                                ext,
                            );
                        }
                    });
            }
        });
        Self::options_wrapper(
            ui,
            "muxer",
            &mut self.state.render_settings.muxer_options,
            current_muxer.options,
        );
    }
    fn options_wrapper(
        ui: &mut Ui,
        id: &str,
        assigned_options: &mut Vec<Opt>,
        full_options: Vec<Opt>,
    ) {
        let mut options: Vec<Opt> = full_options
            .into_iter()
            .map(|opt| {
                for c_opt in assigned_options.iter_mut() {
                    if c_opt.name == opt.name {
                        return c_opt.clone();
                    }
                }
                opt
            })
            .collect();
        Self::widget_options(ui, id, &mut options);
        *assigned_options = options
            .into_iter()
            .filter(|opt| opt.parameter.is_assigned())
            .collect();
    }
    fn widget_options(ui: &mut Ui, id: &str, options: &mut Vec<Opt>) {
        ui.push_id(&id, |ui|{
            ScrollArea::vertical()
            .max_height(200.0)
            .min_scrolled_height(100.0)
            .show(ui, |ui| {
                Grid::new("options")
                    .min_col_width(50.0)
                    .max_col_width(ui.available_width()/2.0)
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
                                OptionParameter::String(v)
                                | OptionParameter::Binary(v)
                                | OptionParameter::Dictionary(v)
                                | OptionParameter::ImageSize(v)
                                | OptionParameter::Rational(v) => match v.clone() {
                                    Some(mut val) => {
                                        ui.vertical(|ui| {
                                            if ui.text_edit_singleline(&mut val).changed() {
                                                opt.parameter = opt
                                                    .parameter
                                                    .with_new_string_value(val)
                                                    .expect("can not set string as value");
                                            };
                                            if ui.button("clear parameter").clicked() {
                                                opt.parameter = opt.parameter.with_none();
                                            }
                                        });
                                    }
                                    None => {
                                        if ui.button("use parameter").clicked() {
                                            opt.parameter = opt
                                                .parameter
                                                .with_new_string_value("".to_string())
                                                .expect("can not set string as value");
                                        }
                                    }
                                },
                                OptionParameter::Float(v) => match v {
                                    Some(mut val) => {
                                        ui.vertical(|ui| {
                                            if ui.add(DragValue::new(&mut val)).changed() {
                                                opt.parameter = OptionParameter::Float(Some(val));
                                            };
                                            if ui.button("clear parameter").clicked() {
                                                opt.parameter = OptionParameter::Float(None);
                                            }
                                        });
                                    }
                                    None => {
                                        if ui.button("use parameter").clicked() {
                                            opt.parameter =
                                                OptionParameter::Float(Some(f64::default()));
                                        }
                                    }
                                },
                                OptionParameter::Color(v) => match v.clone() {
                                    Some(val) => {
                                        ui.vertical(|ui| {
                                            let mut color = val.into();
                                            if ui.color_edit_button_srgba(&mut color).changed() {
                                                opt.parameter = OptionParameter::Color(Some(
                                                    FfmpegColor::from(color),
                                                ))
                                            }
                                            ui.menu_button("built-in", |ui| {
                                                ScrollArea::vertical().show(ui, |ui| {
                                                    for (name, value) in
                                                        FfmpegColor::built_in_colors()
                                                    {
                                                        if ui.button(name).clicked() {
                                                            opt.parameter = OptionParameter::Color(
                                                                Some(FfmpegColor::new(value, 0xff)),
                                                            );
                                                            ui.close_menu();
                                                        }
                                                    }
                                                });
                                            });
                                            if ui.button("clear parameter").clicked() {
                                                opt.parameter = OptionParameter::Color(None);
                                            }
                                        });
                                    }
                                    None => {
                                        if ui.button("use parameter").clicked() {
                                            opt.parameter = OptionParameter::Color(Some(
                                                FfmpegColor::default(),
                                            ));
                                        }
                                    }
                                },
                                OptionParameter::FrameRate(v) => match v.clone() {
                                    Some(mut val) => {
                                        ui.vertical(|ui| {
                                            if ui.text_edit_singleline(&mut val).changed() {
                                                opt.parameter = opt
                                                    .parameter
                                                    .with_new_string_value(val)
                                                    .expect("can not set string as value");
                                            };
                                            ui.menu_button("built-in", |ui| {
                                                ScrollArea::vertical().show(ui, |ui| {
                                                    for (name, hint) in [
                                                        ("ntsc", "30000 / 1001"),
                                                        ("pal", "25"),
                                                        ("qntsc", "30000 / 1001"),
                                                        ("qpal", "25"),
                                                        ("sntsc", "30000 / 1001"),
                                                        ("spal", "25"),
                                                        ("film", "24"),
                                                        ("ntsc-film", "24000 / 1001"),
                                                    ] {
                                                        if ui
                                                            .button(name)
                                                            .on_hover_ui(|ui| {
                                                                ui.label(hint);
                                                            })
                                                            .clicked()
                                                        {
                                                            opt.parameter =
                                                                OptionParameter::FrameRate(Some(
                                                                    name.to_string(),
                                                                ));
                                                            ui.close_menu();
                                                        }
                                                    }
                                                });
                                            });
                                            if ui.button("clear parameter").clicked() {
                                                opt.parameter = opt.parameter.with_none();
                                            }
                                        });
                                    }
                                    None => {
                                        if ui.button("use parameter").clicked() {
                                            opt.parameter = opt
                                                .parameter
                                                .with_new_string_value("".to_string())
                                                .expect("can not set string as value");
                                        }
                                    }
                                },
                                OptionParameter::Duration(v) => match v {
                                    Some(val) => {
                                        let val = val.clone();
                                        ui.vertical(|ui| {
                                            let units = match val {
                                                DurationUnit::Seconds(mut s) => {
                                                    if ui.add(DragValue::new(&mut s)).changed(){
                                                        opt.parameter = OptionParameter::Duration(Some(DurationUnit::Seconds(s)));
                                                    };
                                                    "seconds (s)"
                                                }
                                                DurationUnit::Milliseconds(mut ms) => {
                                                    if ui.add(DragValue::new(&mut ms)).changed(){
                                                        opt.parameter = OptionParameter::Duration(Some(DurationUnit::Milliseconds(ms)));
                                                    };
                                                    "milliseconds (ms)"
                                                }
                                                DurationUnit::Microseconds(mut us) => {
                                                    if ui.add(DragValue::new(&mut us)).changed(){
                                                        opt.parameter = OptionParameter::Duration(Some(DurationUnit::Microseconds(us)));
                                                    };
                                                    "microseconds (us)"
                                                }
                                                DurationUnit::Timestamp {
                                                    mut hours,
                                                    mut minutes,
                                                    mut seconds,
                                                } => {
                                                    ui.horizontal(|ui| {
                                                        let mut changed = false;
                                                        if ui
                                                            .add(DragValue::new(&mut hours)).on_hover_text("hours")
                                                            .changed()
                                                        {
                                                            changed = true
                                                        };
                                                        if ui
                                                            .add(DragValue::new(&mut minutes)).on_hover_text("minutes")
                                                            .changed()
                                                        {
                                                            changed = true
                                                        };
                                                        if ui
                                                            .add(DragValue::new(&mut seconds)).on_hover_text("seconds")
                                                            .changed()
                                                        {
                                                            changed = true
                                                        };
                                                        if changed {
                                                            opt.parameter =
                                                                OptionParameter::Duration(Some(
                                                                   DurationUnit::Timestamp { hours, minutes, seconds },
                                                                ));
                                                        }
                                                    });
                                                    "timestamp (HH:MM:SS.mmm)"
                                                }
                                            };
                                            ComboBox::from_id_salt("duration")
                                                .selected_text(units)
                                                .show_ui(ui, |ui| {
                                                    if ui
                                                        .selectable_label(
                                                            "seconds (s)" == units,
                                                            "seconds (s)",
                                                        )
                                                        .clicked()
                                                    {
                                                        opt.parameter = OptionParameter::Duration(
                                                            Some(val.as_seconds()),
                                                        );
                                                    }
                                                    if ui
                                                        .selectable_label(
                                                            "milliseconds (ms)" == units,
                                                            "milliseconds (ms)",
                                                        )
                                                        .clicked()
                                                    {
                                                        opt.parameter = OptionParameter::Duration(
                                                            Some(val.as_milliseconds()),
                                                        );
                                                    }
                                                    if ui
                                                        .selectable_label(
                                                            "microseconds (us)" == units,
                                                            "microseconds (us)",
                                                        )
                                                        .clicked()
                                                    {
                                                        opt.parameter = OptionParameter::Duration(
                                                            Some(val.as_microseconds()),
                                                        );
                                                    }
                                                    if ui
                                                        .selectable_label(
                                                            "timestamp (HH:MM:SS.mmm)" == units,
                                                            "timestamp (HH:MM:SS.mmm)",
                                                        )
                                                        .clicked()
                                                    {
                                                        opt.parameter = OptionParameter::Duration(
                                                            Some(val.as_timestamp()),
                                                        );
                                                    }
                                                });
                                            if ui.button("clear parameter").clicked() {
                                                opt.parameter = OptionParameter::Duration(None);
                                            }
                                        });
                                    }
                                    None => {
                                        if ui.button("use parameter").clicked() {
                                            opt.parameter =
                                                OptionParameter::Duration(Some(DurationUnit::Seconds(0.0)));
                                        }
                                    }
                                },
                                OptionParameter::Enum {
                                    items,
                                    selected_idx,
                                } => match selected_idx {
                                    Some(mut val) => {
                                        let text = items[val as usize].clone();
                                        let id_salt =  &opt.name.clone();
                                        let cloned_items = items.clone();
                                        ui.vertical(|ui| {
                                            ComboBox::from_id_salt(id_salt)
                                                .selected_text(text)
                                                .show_ui(ui, |ui| {
                                                    for (idx, item) in
                                                        cloned_items.iter().enumerate()
                                                    {
                                                        if ui
                                                            .selectable_value(&mut val, idx, item)
                                                            .clicked()
                                                        {
                                                            opt.parameter = OptionParameter::Enum {
                                                                items: cloned_items.clone(),
                                                                selected_idx: Some(idx),
                                                            }
                                                        }
                                                    }
                                                });
                                            if ui.button("clear parameter").clicked() {
                                                opt.parameter = OptionParameter::Enum {
                                                    items: cloned_items,
                                                    selected_idx: None,
                                                };
                                            }
                                            if ui.button("enter raw string").clicked() {
                                                opt.parameter = OptionParameter::String(Some(
                                                    String::default(),
                                                ));
                                            }
                                        });
                                    }
                                    None => {
                                        if ui.button("use parameter").clicked() {
                                            opt.parameter = OptionParameter::Enum {
                                                items: items.clone(),
                                                selected_idx: Some(usize::default()),
                                            };
                                        }
                                    }
                                },
                                OptionParameter::Flags { items, selected } => {
                                    match selected.as_ref() {
                                        Some(vector) => {
                                            let cloned_items = items.clone();
                                            let mut cloned_vector = vector.clone();
                                            ui.vertical(|ui| {
                                                let mut changed = false;
                                                for (item, val) in cloned_items
                                                    .iter()
                                                    .zip(cloned_vector.iter_mut())
                                                {
                                                    if ui.checkbox(val, item).changed() {
                                                        changed = true;
                                                    }
                                                }
                                                if changed {
                                                    opt.parameter = OptionParameter::Flags {
                                                        items: cloned_items,
                                                        selected: Some(cloned_vector),
                                                    };
                                                    return;
                                                }
                                                if ui.button("clear parameter").clicked() {
                                                    opt.parameter = OptionParameter::Flags {
                                                        items: cloned_items,
                                                        selected: None,
                                                    };
                                                    return;
                                                }
                                                if ui.button("enter raw string").clicked() {
                                                    opt.parameter = OptionParameter::String(Some(
                                                        String::default(),
                                                    ));
                                                }
                                            });
                                        }
                                        None => {
                                            if ui.button("use parameter").clicked() {
                                                opt.parameter = OptionParameter::Flags {
                                                    items: items.clone(),
                                                    selected: Some(vec![false; items.len()]),
                                                };
                                            }
                                        }
                                    }
                                }
                            };
                            ui.end_row();
                        }
                    });
                ui.set_height(ui.available_height());
            });
        });
    }
}
