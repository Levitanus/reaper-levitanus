use std::hash;

use egui::{Color32, ComboBox, Context, RichText, Ui};
use log::debug;
use serde::{Deserialize, Serialize};

use crate::ffmpeg::{
    base::{SerializedFilter, SerializedOption},
    options::Opt,
};

use super::{Front, FrontMessage};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FlitersWidget {
    pub selected_video_item: Option<SelectedVideoItem>,
    pub filter_chain: FilterChain,
}
impl FlitersWidget {
    pub fn new() -> Self {
        Self {
            selected_video_item: None,
            filter_chain: FilterChain::Item,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SelectedVideoItem {
    pub track_name: String,
    pub track_guid: String,
    pub track_filters: Vec<SerializedFilter>,
    pub item_name: String,
    pub item_guid: String,
    pub item_filters: Vec<SerializedFilter>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, PartialOrd)]
pub enum FilterChain {
    Item,
    Track,
    Master,
}

impl Front {
    pub(crate) fn widget_filters(&mut self, ctx: &Context, ui: &mut Ui) {
        let widget = &mut self.filters_widget;
        if let Some(item) = &mut widget.selected_video_item {
            ui.horizontal(|ui| {
                ui.label(RichText::new("track: ").strong());
                ui.label(&item.track_name);
                ui.label(RichText::new("video item: ").strong());
                ui.label(&item.item_name);
            });
            let filters = match widget.filter_chain {
                FilterChain::Item => item.item_filters.clone(),
                FilterChain::Track => item.track_filters.clone(),
                FilterChain::Master => self.state.master_filters.clone(),
            };
            ui.horizontal(|ui| {
                ui.radio_value(
                    &mut widget.filter_chain,
                    FilterChain::Item,
                    RichText::new("item filter chain").strong(),
                );
                ui.radio_value(
                    &mut widget.filter_chain,
                    FilterChain::Track,
                    RichText::new("track filter chain").strong(),
                );
                ui.radio_value(
                    &mut widget.filter_chain,
                    FilterChain::Master,
                    RichText::new("master filter chain").strong(),
                );
            });
            let mut swap = None;
            let mut new_filters = Vec::new();
            let mut updated = false;
            for (index, filter) in filters.iter().enumerate() {
                ui.push_id(&filter.name, |ui| {
                    let mut filter = filter.clone();
                    let ui_filter = match self.filters.iter().find(|f| f.name == filter.name) {
                        Some(f) => f.clone(),
                        None => todo!(),
                    };
                    let mut assigned_options: Vec<Opt> = ui_filter
                        .options
                        .iter()
                        .filter_map(|opt| {
                            for f_opt in filter.options.iter() {
                                if opt.name == f_opt.name {
                                    let mut opt = opt.clone();
                                    opt.parameter = f_opt.value.clone();
                                    return Some(opt);
                                }
                            }
                            None
                        })
                        .collect();
                    let mut push = true;
                    ui.horizontal(|ui| {
                        if filters.len() > 1 {
                            if index > 0 {
                                if ui.button(RichText::new("⬆").strong()).clicked() {
                                    swap = Some((index, index - 1));
                                    updated = true;
                                }
                            }
                            if index < filters.len() - 1 {
                                if ui.button(RichText::new("⬇").strong()).clicked() {
                                    swap = Some((index, index + 1));
                                    updated = true;
                                }
                            }
                        }
                        if ui
                            .button(RichText::new("🗙").color(Color32::RED).strong())
                            .clicked()
                        {
                            push = false;
                            updated = true;
                        }
                        ui.label(RichText::new(&filter.name).strong());
                    });
                    ui.collapsing("filter options", |ui| {
                        if Self::widget_options_wrapper(
                            ui,
                            &format!("{} filter options", filter.name),
                            &mut assigned_options,
                            ui_filter.options,
                        ) {
                            updated = true;
                        }
                    });
                    filter.options = assigned_options
                        .into_iter()
                        .map(|opt| SerializedOption {
                            name: opt.name,
                            value: opt.parameter,
                        })
                        .collect();
                    if push {
                        new_filters.push(filter);
                    }
                });
            }
            ComboBox::from_id_salt("add filter")
                .selected_text(RichText::new("add filter"))
                .show_ui(ui, |ui| {
                    for filter in self.filters.iter().filter(|f| f.n_sockets.0 == 1) {
                        if ui.button(&filter.name).clicked() {
                            new_filters.push(SerializedFilter {
                                name: filter.name.clone(),
                                options: Vec::new(),
                            });
                            updated = true;
                        }
                        ui.label(RichText::new(&filter.description).weak());
                    }
                });
            if let Some((a, b)) = swap {
                new_filters.swap(a, b);
            }
            if updated {
                match widget.filter_chain {
                    FilterChain::Item => {
                        item.item_filters = new_filters;
                        self.emit(FrontMessage::UpdateFilters(FilterChain::Item));
                    }
                    FilterChain::Track => {
                        item.track_filters = new_filters;
                        self.emit(FrontMessage::UpdateFilters(FilterChain::Track));
                    }
                    FilterChain::Master => {
                        self.state.master_filters = new_filters;
                        self.emit(FrontMessage::UpdateFilters(FilterChain::Master));
                    }
                }
            }
        } else {
            ui.label(
                RichText::new("Select item with video for starting working with filters.").strong(),
            );
        }
    }
}
