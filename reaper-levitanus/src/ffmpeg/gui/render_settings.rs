use egui::{
    CollapsingHeader, Color32, ComboBox, Context, DragValue, Grid, RichText, ScrollArea, Ui,
};
use fraction::Fraction;
use itertools::Itertools;

use super::{Front, FrontMessage};
use crate::ffmpeg::{
    base_types::Resolution,
    options::{DurationUnit, Encoder, EncoderType, FfmpegColor, Muxer, Opt, OptionParameter},
    parser::ParsingProgress,
    RenderSettings,
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
                let current_muxer = match self
                    .muxers
                    .iter()
                    .find(|mux| mux.name == self.state.render_settings.muxer)
                {
                    Some(m) => m.clone(),
                    None => {
                        if let Some(s) = self.alternative_value(
                            ctx,
                            "muxer",
                            &self.state.render_settings.muxer,
                            self.muxers.iter().map(|mux| mux.name.clone()),
                        ) {
                            self.state.render_settings.muxer = s;
                        }
                        return;
                    }
                };
                let current_video_encoder = match self
                    .encoders
                    .iter()
                    .find(|enc| enc.name == self.state.render_settings.video_encoder)
                {
                    Some(enc) => enc.clone(),
                    None => {
                        let result = self.alternative_value(
                            ctx,
                            "video encoder",
                            &self.state.render_settings.video_encoder,
                            self.encoders
                                .iter()
                                .filter(|enc| enc.encoder_type == EncoderType::Video)
                                .map(|enc| enc.name.clone()),
                        );
                        if let Some(s) = result {
                            self.state.render_settings.video_encoder = s;
                        }
                        return;
                    }
                };

                let current_audio_encoder = match self.state.render_settings.audio_encoder.as_ref()
                {
                    None => None,
                    Some(c) => match self.encoders.iter().find(|enc| enc.name == *c) {
                        Some(enc) => Some(enc.clone()),
                        None => {
                            let result = self.alternative_value(
                                ctx,
                                "audio encoder",
                                &c,
                                self.encoders
                                    .iter()
                                    .filter(|enc| enc.encoder_type == EncoderType::Audio)
                                    .map(|enc| enc.name.clone()),
                            );
                            if let Some(s) = result {
                                self.state.render_settings.audio_encoder = Some(s);
                            }
                            return;
                        }
                    },
                };

                let current_subtitle_encoder =
                    match self.state.render_settings.subtitle_encoder.as_ref() {
                        None => None,
                        Some(c) => match self.encoders.iter().find(|enc| enc.name == *c) {
                            Some(enc) => Some(enc.clone()),
                            None => {
                                let result = self.alternative_value(
                                    ctx,
                                    "subtitle encoder",
                                    &c,
                                    self.encoders
                                        .iter()
                                        .filter(|enc| enc.encoder_type == EncoderType::Subtitle)
                                        .map(|enc| enc.name.clone()),
                                );
                                if let Some(s) = result {
                                    self.state.render_settings.subtitle_encoder = Some(s);
                                }
                                return;
                            }
                        },
                    };

                // GUI
                ui.horizontal(|ui| {
                    self.widget_muxer(ui, &current_muxer);
                    self.widget_video_encoder(ui, &current_video_encoder);
                    if let Some(enc) = &current_audio_encoder {
                        self.widget_audio_encoder(ui, enc);
                    }
                    if let Some(enc) = &current_subtitle_encoder {
                        self.widget_subtitle_encoder(ui, enc);
                    }
                });
                ui.separator();
                self.widget_small_render_settings(ui);
                CollapsingHeader::new("muxer options").show_unindented(ui, |ui| {
                    Self::widget_options_wrapper(
                        ui,
                        "muxer",
                        &mut self.state.render_settings.muxer_options,
                        current_muxer.options,
                    );
                });
                CollapsingHeader::new("video encoder options").show_unindented(ui, |ui| {
                    Self::widget_options_wrapper(
                        ui,
                        "video encoder",
                        &mut self.state.render_settings.video_encoder_options,
                        current_video_encoder.options,
                    );
                });
                if let Some(enc) = &current_audio_encoder {
                    CollapsingHeader::new("audio encoder options").show_unindented(ui, |ui| {
                        Self::widget_options_wrapper(
                            ui,
                            "audio encoder",
                            &mut self.state.render_settings.audio_encoder_options,
                            enc.options.clone(),
                        );
                    });
                }
                if let Some(enc) = &current_subtitle_encoder {
                    CollapsingHeader::new("subtitle encoder options").show_unindented(ui, |ui| {
                        Self::widget_options_wrapper(
                            ui,
                            "subtitle encoder",
                            &mut self.state.render_settings.subtitle_encoder_options,
                            enc.options.clone(),
                        );
                    });
                }
            });
    }

    fn widget_small_render_settings(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("resolution: ").strong());
            ui.add(DragValue::new(
                &mut self.state.render_settings.resolution.width,
            ));
            ui.label("x");
            ui.add(DragValue::new(
                &mut self.state.render_settings.resolution.height,
            ));
            ui.add_space(20.0);
            ComboBox::from_id_salt("default resolutions")
                .selected_text("built-in resolutions")
                .show_ui(ui, |ui| {
                    for (name, res) in built_in_resolutions() {
                        if ui
                            .selectable_label(res == self.state.render_settings.resolution, name)
                            .clicked()
                        {
                            self.state.render_settings.resolution = res.clone()
                        }
                    }
                });
            if ui.button("get from current video item").clicked() {
                self.emit(FrontMessage::GetResolution);
            }
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("background color: ").strong());
            let mut color: Color32 = self.state.render_settings.pad_color.clone().into();
            if ui.color_edit_button_srgba(&mut color).changed() {
                self.state.render_settings.pad_color = FfmpegColor::from(color);
            };
            ComboBox::from_id_salt("default colors")
                .selected_text("built-in colors")
                .show_ui(ui, |ui| {
                    for (s, hex) in FfmpegColor::built_in_colors() {
                        if ui
                            .selectable_label(hex == self.state.render_settings.pad_color.color, s)
                            .clicked()
                        {
                            self.state.render_settings.pad_color.color = hex;
                        }
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("framerate").strong());
            let mut num = *self
                .state
                .render_settings
                .fps
                .numer()
                .expect("fps numerator is none");
            let mut den = *self
                .state
                .render_settings
                .fps
                .denom()
                .expect("fps denominator is none");
            if ui.add(DragValue::new(&mut num)).changed() {
                self.state.render_settings.fps = Fraction::new(num, den)
            };
            ui.label("/");
            if ui.add(DragValue::new(&mut den)).changed() {
                self.state.render_settings.fps = Fraction::new(num, den)
            };
            ui.add_space(20.0);
            ComboBox::from_id_salt("default framerates")
                .selected_text("built-in framerates")
                .show_ui(ui, |ui| {
                    for (name, fps) in built_in_framerates() {
                        if ui
                            .selectable_label(fps == self.state.render_settings.fps, name)
                            .clicked()
                        {
                            self.state.render_settings.fps = fps.clone()
                        }
                    }
                });
            if ui.button("get from current video item").clicked() {
                self.emit(FrontMessage::GetFrameRate);
            }
        });
    }

    fn widget_muxer(&mut self, ui: &mut Ui, current_muxer: &Muxer) {
        // Self::frame(ui, |ui| {
        ui.vertical(|ui| {
            ui.set_max_width(140.0);
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
                                let c = c.replace("h264", "libx264").replace("flv1", "flv");
                                self.state.render_settings.video_encoder = c;
                            }
                            if let Some(c) = &mux.audio_codec {
                                let c = c.replace("vorbis", "libvorbis");
                                self.state.render_settings.audio_encoder = Some(c);
                            } else {
                                self.state.render_settings.audio_encoder = None;
                            }
                            self.state.render_settings.subtitle_encoder =
                                mux.subtitle_codec.clone();
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
        // });
    }

    fn widget_video_encoder(&mut self, ui: &mut Ui, current_encoder: &Encoder) {
        // Self::frame(ui, |ui| {
        ui.vertical(|ui| {
            ui.set_max_width(140.0);
            ui.label(RichText::new("video encoder:").strong());
            ComboBox::from_id_salt("video encoder")
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
            ui.menu_button("\u{2139} encoder info", |ui| {
                ui.label(&current_encoder.description);
                Self::encoder_flag(
                    ui,
                    "frame level multithreading",
                    current_encoder.frame_level_multithreading,
                    false,
                );
                Self::encoder_flag(
                    ui,
                    "slice level multithreading",
                    current_encoder.slice_level_multithreading,
                    false,
                );
                Self::encoder_flag(ui, "is experimental", current_encoder.is_experimenal, true);
                Self::encoder_flag(
                    ui,
                    "supports draw horiz band",
                    current_encoder.supports_draw_horiz_band,
                    false,
                );
                Self::encoder_flag(
                    ui,
                    "supports direct rendering method 1",
                    current_encoder.supports_direct_rendering_method_1,
                    false,
                );
            });
            ui.label(RichText::new("pixel format:").strong());
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
        // });
    }

    fn widget_audio_encoder(&mut self, ui: &mut Ui, current_encoder: &Encoder) {
        // Self::frame(ui, |ui| {
        ui.vertical(|ui| {
            ui.set_max_width(140.0);
            ui.label(RichText::new("audio encoder:").strong());
            ComboBox::from_id_salt("audio encoder")
                .selected_text(&current_encoder.name)
                .show_ui(ui, |ui| {
                    for enc in self
                        .encoders
                        .iter()
                        .filter(|e| e.encoder_type == EncoderType::Audio)
                    {
                        if ui
                            .selectable_label(enc.name == current_encoder.name, &enc.name)
                            .clicked()
                        {
                            self.state.render_settings.audio_encoder = Some(enc.name.clone());
                            self.state.render_settings.audio_encoder_options = Vec::new();
                        }
                    }
                });
            ui.label(&current_encoder.description);
            ui.horizontal(|ui| {
                let desc = "audio offset in seconds";
                ui.label(RichText::new("audio offset").strong())
                    .on_hover_text(desc);
                ui.add(DragValue::new(&mut self.state.render_settings.audio_offset))
                    .on_hover_text(desc);
            });
        });
        // });
    }

    fn widget_subtitle_encoder(&mut self, ui: &mut Ui, current_encoder: &Encoder) {
        // Self::frame(ui, |ui| {
        ui.vertical(|ui| {
            ui.set_max_width(140.0);
            ui.label(RichText::new("subtitle encoder:").strong());
            ComboBox::from_id_salt("subtitle encoder")
                .selected_text(&current_encoder.name)
                .show_ui(ui, |ui| {
                    for enc in self
                        .encoders
                        .iter()
                        .filter(|e| e.encoder_type == EncoderType::Subtitle)
                    {
                        if ui
                            .selectable_label(enc.name == current_encoder.name, &enc.name)
                            .clicked()
                        {
                            self.state.render_settings.subtitle_encoder = Some(enc.name.clone());
                            self.state.render_settings.subtitle_encoder_options = Vec::new();
                        }
                    }
                });
            ui.label(&current_encoder.description);
        });
        // });
    }

    pub fn widget_options_wrapper(
        ui: &mut Ui,
        id: &str,
        assigned_options: &mut Vec<Opt>,
        full_options: Vec<Opt>,
    ) -> bool {
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
        let result = Self::widget_options(ui, id, &mut options);
        *assigned_options = options
            .into_iter()
            .filter(|opt| opt.parameter.is_assigned())
            .collect();
        result
    }
    fn widget_options(ui: &mut Ui, id: &str, options: &mut Vec<Opt>) -> bool {
        let mut updated = false;
        ui.push_id(&id, |ui|{
            ScrollArea::vertical()
            .max_height(300.0).auto_shrink([false,true])
            // .min_scrolled_height(100.0)
            .show(ui, |ui| {
                Grid::new("options")
                    .min_col_width(100.0)
                    .max_col_width(300.0)
                    .num_columns(3)
                    .striped(true)
                    .spacing((10.0, 10.0))
                    .show(ui, |ui| {
                        for opt in options {
                            let old_opt = opt.clone();
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
                            if opt != &old_opt{
                                updated = true;
                            }
                        }
                    });
                ui.set_height(ui.available_height());
            });
        });
        updated
    }
}

fn built_in_resolutions() -> Vec<(&'static str, Resolution)> {
    vec![
        (
            "ntsc",
            Resolution {
                width: 720,
                height: 480,
            },
        ),
        (
            "pal",
            Resolution {
                width: 720,
                height: 576,
            },
        ),
        (
            "qntsc",
            Resolution {
                width: 352,
                height: 240,
            },
        ),
        (
            "qpal",
            Resolution {
                width: 352,
                height: 288,
            },
        ),
        (
            "sntsc",
            Resolution {
                width: 640,
                height: 480,
            },
        ),
        (
            "spal",
            Resolution {
                width: 768,
                height: 576,
            },
        ),
        (
            "film",
            Resolution {
                width: 352,
                height: 240,
            },
        ),
        (
            "ntsc-film",
            Resolution {
                width: 352,
                height: 240,
            },
        ),
        (
            "sqcif",
            Resolution {
                width: 128,
                height: 96,
            },
        ),
        (
            "qcif",
            Resolution {
                width: 176,
                height: 144,
            },
        ),
        (
            "cif",
            Resolution {
                width: 352,
                height: 288,
            },
        ),
        (
            "4cif",
            Resolution {
                width: 704,
                height: 576,
            },
        ),
        (
            "16cif",
            Resolution {
                width: 1408,
                height: 1152,
            },
        ),
        (
            "qqvga",
            Resolution {
                width: 160,
                height: 120,
            },
        ),
        (
            "qvga",
            Resolution {
                width: 320,
                height: 240,
            },
        ),
        (
            "vga",
            Resolution {
                width: 640,
                height: 480,
            },
        ),
        (
            "svga",
            Resolution {
                width: 800,
                height: 600,
            },
        ),
        (
            "xga",
            Resolution {
                width: 1024,
                height: 768,
            },
        ),
        (
            "uxga",
            Resolution {
                width: 1600,
                height: 1200,
            },
        ),
        (
            "qxga",
            Resolution {
                width: 2048,
                height: 1536,
            },
        ),
        (
            "sxga",
            Resolution {
                width: 1280,
                height: 1024,
            },
        ),
        (
            "qsxga",
            Resolution {
                width: 2560,
                height: 2048,
            },
        ),
        (
            "hsxga",
            Resolution {
                width: 5120,
                height: 4096,
            },
        ),
        (
            "wvga",
            Resolution {
                width: 852,
                height: 480,
            },
        ),
        (
            "wxga",
            Resolution {
                width: 1366,
                height: 768,
            },
        ),
        (
            "wsxga",
            Resolution {
                width: 1600,
                height: 1024,
            },
        ),
        (
            "wuxga",
            Resolution {
                width: 1920,
                height: 1200,
            },
        ),
        (
            "woxga",
            Resolution {
                width: 2560,
                height: 1600,
            },
        ),
        (
            "wqsxga",
            Resolution {
                width: 3200,
                height: 2048,
            },
        ),
        (
            "wquxga",
            Resolution {
                width: 3840,
                height: 2400,
            },
        ),
        (
            "whsxga",
            Resolution {
                width: 6400,
                height: 4096,
            },
        ),
        (
            "whuxga",
            Resolution {
                width: 7680,
                height: 4800,
            },
        ),
        (
            "cga",
            Resolution {
                width: 320,
                height: 200,
            },
        ),
        (
            "ega",
            Resolution {
                width: 640,
                height: 350,
            },
        ),
        (
            "hd480",
            Resolution {
                width: 852,
                height: 480,
            },
        ),
        (
            "hd720",
            Resolution {
                width: 1280,
                height: 720,
            },
        ),
        (
            "hd1080",
            Resolution {
                width: 1920,
                height: 1080,
            },
        ),
        (
            "2k",
            Resolution {
                width: 2048,
                height: 1080,
            },
        ),
        (
            "2kflat",
            Resolution {
                width: 1998,
                height: 1080,
            },
        ),
        (
            "2kscope",
            Resolution {
                width: 2048,
                height: 858,
            },
        ),
        (
            "4k",
            Resolution {
                width: 4096,
                height: 2160,
            },
        ),
        (
            "4kflat",
            Resolution {
                width: 3996,
                height: 2160,
            },
        ),
        (
            "4kscope",
            Resolution {
                width: 4096,
                height: 1716,
            },
        ),
        (
            "nhd",
            Resolution {
                width: 640,
                height: 360,
            },
        ),
        (
            "hqvga",
            Resolution {
                width: 240,
                height: 160,
            },
        ),
        (
            "wqvga",
            Resolution {
                width: 400,
                height: 240,
            },
        ),
        (
            "fwqvga",
            Resolution {
                width: 432,
                height: 240,
            },
        ),
        (
            "hvga",
            Resolution {
                width: 480,
                height: 320,
            },
        ),
        (
            "qhd",
            Resolution {
                width: 960,
                height: 540,
            },
        ),
        (
            "2kdci",
            Resolution {
                width: 2048,
                height: 1080,
            },
        ),
        (
            "4kdci",
            Resolution {
                width: 4096,
                height: 2160,
            },
        ),
        (
            "uhd2160",
            Resolution {
                width: 3840,
                height: 2160,
            },
        ),
        (
            "uhd4320",
            Resolution {
                width: 7680,
                height: 4320,
            },
        ),
    ]
}

fn built_in_framerates() -> Vec<(&'static str, Fraction)> {
    vec![
        ("ntsc", Fraction::new(30000_u64, 1001_u64)),
        ("pal", Fraction::new(25_u64, 1_u64)),
        ("qntsc", Fraction::new(30000_u64, 1001_u64)),
        ("qpal", Fraction::new(25_u64, 1_u64)),
        ("sntsc", Fraction::new(30000_u64, 1001_u64)),
        ("spal", Fraction::new(25_u64, 1_u64)),
        ("film", Fraction::new(24_u64, 1_u64)),
        ("ntsc - film", Fraction::new(24000_u64, 1001_u64)),
    ]
}
