use std::fmt::Display;

use anyhow::Result;
use fraction::Fraction;
use log::{debug, error, warn};
use rea_rs::{
    swell_gui::{
        layout::{Insets, WidgetFills, WidgetSize},
        EditFieldOptions,
    },
    ActionHook, Button, ComboBox, ComboBoxOptions, CommandId, EditField, ExtState, Reaper,
    ReaperWindow, SwellId, WindowHandler, WindowSpec,
};
use serde::{Deserialize, Serialize};

use crate::{
    score::{dom::VisualQuantize, EXT_SEXTION, QUANTIZE_EDITOR_COMMAND_ID},
    LevitanusError,
};

const WINDOW_ID: &'static str = "RSVisualQuantizeEditor";

pub(super) fn toggle_visual_quantize_editor(hook: &mut ActionHook) -> Result<()> {
    let rpr = Reaper::get_mut();
    match hook.toggle_state().unwrap_or(false) {
        false => {
            rpr.register_window_handler(Box::new(VisualQuantizeEditor::new()?))?;
            hook.set_toggle_state(true);
        }
        true => {
            rpr.unregister_window_handler(&WINDOW_ID.into())?;
            hook.set_toggle_state(false);
        }
    }
    Ok(())
}

pub(super) struct VisualQuantizeEditor {
    window: ReaperWindow,
    grid_units: ComboBox,
    grid_units_list: Vec<Fraction>,
    unit_type: ComboBox,
    unit_types_list: Vec<UnitType>,
    tuplet_num: EditField,
    tuplet_denom: EditField,
    commit_button: Button,
    apply_button: Button,
    cancel_button: Button,
}
impl VisualQuantizeEditor {
    fn new() -> Result<Self> {
        let rpr = Reaper::get();
        let row_heigh = 25;
        let window = rpr.create_window(
            &WindowSpec::new("RS Visual Quantize Editor")
                .size(300, row_heigh * 6)
                .resizable(false),
        )?;
        let ui = window.build_ui()?.with_insets(Insets {
            left: 15,
            top: 15,
            right: 15,
            bottom: 15,
        });
        let row = ui.row(
            SwellId::new_control(),
            WidgetSize::new_fill_x(270, row_heigh),
        )?;
        let default_size =
            WidgetSize::new_fill_x(80, row_heigh).set_fill_x(WidgetFills::FillPortion(1));
        let grid_units = row.combo_box(
            SwellId::new_control(),
            default_size,
            ComboBoxOptions::default(),
        )?;
        let grid_units_list = vec![
            Fraction::new(4_u64, 1_u64),
            Fraction::new(2_u64, 1_u64),
            Fraction::new(1_u64, 1_u64),
            Fraction::new(1_u64, 2_u64),
            Fraction::new(1_u64, 4_u64),
            Fraction::new(1_u64, 8_u64),
            Fraction::new(1_u64, 16_u64),
            Fraction::new(1_u64, 32_u64),
            Fraction::new(1_u64, 64_u64),
            Fraction::new(1_u64, 128_u64),
        ];
        for unit in grid_units_list.iter() {
            grid_units.add_item(&unit.to_string())?;
        }
        let unit_type = row.combo_box(
            SwellId::new_control(),
            default_size,
            ComboBoxOptions::default(),
        )?;
        let unit_types_list = UnitType::list();
        for t in unit_types_list.iter() {
            unit_type.add_item(&t.to_string())?;
        }
        let tuplet_group = ui.group_box(
            SwellId::new_control(),
            "tuplet options",
            WidgetSize::new_fill_x(270, row_heigh * 2),
        )?;
        let row = tuplet_group.row(
            SwellId::new_control(),
            WidgetSize::new_fill_x(250, row_heigh),
        )?;
        let tuplet_num = row.edit_field(
            SwellId::new_control(),
            default_size,
            EditFieldOptions::default(),
        )?;
        let tuplet_denom = row.edit_field(
            SwellId::new_control(),
            default_size,
            EditFieldOptions::default(),
        )?;

        let row = ui.row(SwellId::new_control(), default_size)?;
        let commit_button = row.button(SwellId::new_control(), "Commit", default_size)?;
        let apply_button = row.button(SwellId::new_control(), "Apply", default_size)?;
        let cancel_button = row.button(SwellId::new_control(), "Cancel", default_size)?;

        window.show()?;

        Ok(Self {
            window,
            grid_units,
            grid_units_list,
            unit_type,
            unit_types_list,
            tuplet_num,
            tuplet_denom,
            commit_button,
            apply_button,
            cancel_button,
        })
    }

    fn apply_quantize(&self) -> anyhow::Result<()> {
        let grid_idx = self.grid_units.selection()?.unwrap_or_default();
        let grid = self
            .grid_units_list
            .get(grid_idx)
            .ok_or(LevitanusError::KeyError(
                "grid_units_list".to_string(),
                format!("{grid_idx}"),
            ))?;
        let unit_type = self.get_unit_type()?;
        let vq = match unit_type {
            UnitType::Straight => VisualQuantize {
                unit: *grid,
                ratio: Fraction::new(1_u64, 1_u64),
            },
            UnitType::Triplet => VisualQuantize {
                unit: *grid,
                ratio: Fraction::new(3_u64, 2_u64),
            },
            UnitType::Dotted => VisualQuantize {
                unit: *grid,
                ratio: Fraction::new(2_u64, 3_u64),
            },
            UnitType::Tuplet => {
                let num: u32 = match self.tuplet_num.text()?.parse() {
                    Err(e) => {
                        error!("Can not parse numerator: {:?}", e);
                        return Ok(());
                    }
                    Ok(n) => n,
                };
                let denom: u32 = match self.tuplet_denom.text()?.parse() {
                    Err(e) => {
                        error!("Can not parse denominator: {:?}", e);
                        return Ok(());
                    }
                    Ok(n) => n,
                };
                debug!("Tuple parsed: {:?}", Fraction::new(num, denom));
                VisualQuantize {
                    unit: *grid,
                    ratio: Fraction::new(num, denom),
                }
            }
        };
        debug!("{:?}", vq);

        Ok(())
    }

    fn get_unit_type(&self) -> Result<&UnitType, anyhow::Error> {
        let unit_idx = self.unit_type.selection()?.unwrap_or_default();
        let unit = self
            .unit_types_list
            .get(unit_idx)
            .ok_or(LevitanusError::KeyError(
                "unit_types_list".to_string(),
                format!("{unit_idx}"),
            ))?;

        Ok(unit)
    }

    fn close(&self) -> Result<()> {
        let rpr = Reaper::get_mut();
        let id = ExtState::<CommandId, Reaper>::load_value(
            EXT_SEXTION,
            QUANTIZE_EDITOR_COMMAND_ID,
            rpr,
            None,
        )?
        .ok_or(LevitanusError::KeyError(
            "Reaper".to_owned(),
            "quantize_editor_command_id".to_owned(),
        ))?;
        Reaper::get_mut().set_action_toggle_state(id, false);
        Ok(Reaper::get_mut().unregister_window_handler(&WINDOW_ID.to_string())?)
    }
}

impl Display for UnitType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Straight => f.write_str("straight"),
            Self::Triplet => f.write_str("triplet"),
            Self::Dotted => f.write_str("dotted (2/3)"),
            Self::Tuplet => f.write_str("tuplet (custom)"),
        }
    }
}

impl WindowHandler for VisualQuantizeEditor {
    fn window_id(&self) -> rea_rs::WindowId {
        WINDOW_ID.into()
    }

    fn window(&self) -> &rea_rs::ReaperWindow {
        &self.window
    }

    fn on_control_event(
        &mut self,
        event: rea_rs::swell_gui::events::ControlEvent,
    ) -> anyhow::Result<()> {
        match event {
            rea_rs::ControlEvent::ButtonClicked { control } => match control {
                val if val == self.commit_button.id() => {
                    debug!("Commiting:");
                    self.apply_quantize()?;
                    return Ok(());
                }
                val if val == self.apply_button.id() => {
                    debug!("Applying:");
                    self.apply_quantize()?;
                    return Ok(self.close()?);
                }
                val if val == self.cancel_button.id() => {
                    debug!("closing");
                    return Ok(self.close()?);
                }
                val => warn!("unknown button clicked: {:?}", val),
            },
            rea_rs::ControlEvent::ComboSelectionChanged { control } => {
                if control == self.unit_type.id() {
                    let unit = self.get_unit_type()?;
                    match unit {
                        UnitType::Tuplet => {
                            self.tuplet_num.enable(true)?;
                            self.tuplet_denom.enable(true)?;
                        }
                        _ => {
                            self.tuplet_num.enable(false)?;
                            self.tuplet_denom.enable(false)?;
                        }
                    }
                }
            }
            event => warn!("unexpected event: {:?}", event),
        }
        Ok(())
    }
}

impl Drop for VisualQuantizeEditor {
    fn drop(&mut self) {
        let rpr = Reaper::get_mut();
        _ = rpr.unregister_window_handler(&WINDOW_ID.to_string());
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
enum UnitType {
    Straight,
    Triplet,
    Dotted,
    Tuplet,
}
impl UnitType {
    fn list() -> Vec<UnitType> {
        vec![Self::Straight, Self::Triplet, Self::Dotted, Self::Tuplet]
    }
    fn from_index(index: u32) -> Option<Self> {
        match index {
            0 => Some(Self::Straight),
            1 => Some(Self::Triplet),
            2 => Some(Self::Dotted),
            3 => Some(Self::Tuplet),
            _ => None,
        }
    }
}
