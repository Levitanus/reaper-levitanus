use std::fmt::Display;

use anyhow::Result;
use fraction::Fraction;
use log::{debug, error, warn};
use rea_rs::{
    swell_gui::{
        layout::{Insets, WidgetFills, WidgetSize},
        EditFieldOptions,
    },
    ActionHook, Button, ComboBox, ComboBoxOptions, CommandId, EditField, ExtState, MidiEvent,
    Project, Reaper, ReaperResult, ReaperWindow, SwellId, Take, Time, Track, WindowHandler,
    WindowSpec,
};
use serde::{Deserialize, Serialize};

use crate::{
    score::{
        dom::VisualQuantize,
        midi_processing::{
            MidiProcessor, SerializedNotation, TrackNotation, VisualQuantizeNotation,
        },
        ProjectSettings, RuntimeState, TrackSettings, EXT_SEXTION,
    },
    LevitanusError, LevitanusResult,
};

const WINDOW_ID: &'static str = "RSVisualQuantizeEditor";

pub(super) fn toggle_visual_quantize_editor(
    hook: &mut ActionHook,
    mode: VisualQuantizeMode,
) -> Result<()> {
    let rpr = Reaper::get_mut();
    if rpr.is_window_registered(&WINDOW_ID.into()) {
        rpr.unregister_window_handler(&WINDOW_ID.into())?;
    };
    match hook.toggle_state().unwrap_or(false) {
        false => {
            rpr.register_window_handler(Box::new(VisualQuantizeEditor::new(
                mode,
                hook.command_id(),
            )?))?;
            hook.set_toggle_state(true);
        }
        true => {
            hook.set_toggle_state(false);
        }
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct State {
    grid_units: usize,
    unit_type: usize,
    tuplet_num: String,
    tuplet_denom: String,
}
impl Default for State {
    fn default() -> Self {
        Self {
            grid_units: 6,
            unit_type: 0,
            tuplet_num: "5".to_owned(),
            tuplet_denom: "2".to_owned(),
        }
    }
}
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub(super) enum VisualQuantizeMode {
    #[default]
    ProjectDefault,
    PartDefault,
    Track,
    Staff,
    Voice,
}

pub(super) struct VisualQuantizeEditor {
    window: ReaperWindow,
    mode: VisualQuantizeMode,
    command_id: CommandId,
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
    fn new(mode: VisualQuantizeMode, command_id: CommandId) -> Result<Self> {
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

        let mut obj = Self {
            window,
            mode,
            command_id,
            grid_units,
            grid_units_list,
            unit_type,
            unit_types_list,
            tuplet_num,
            tuplet_denom,
            commit_button,
            apply_button,
            cancel_button,
        };
        obj.load_controls()?;
        Ok(obj)
    }

    fn get_quantize(&self) -> anyhow::Result<Option<VisualQuantize>> {
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
                        return Ok(None);
                    }
                    Ok(n) => n,
                };
                let denom: u32 = match self.tuplet_denom.text()?.parse() {
                    Err(e) => {
                        error!("Can not parse denominator: {:?}", e);
                        return Ok(None);
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

        Ok(Some(vq))
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
        let id = self.command_id;
        Reaper::get_mut().set_action_toggle_state(id, false);
        Ok(Reaper::get_mut().unregister_window_handler(&WINDOW_ID.to_string())?)
    }

    fn apply_quantize(&self, vq: VisualQuantize) -> Result<()> {
        debug!("Applying Visual Quantise: {:?}", vq);
        let rpr = Reaper::get();
        let pr = rpr.current_project();
        match self.mode {
            VisualQuantizeMode::ProjectDefault => todo!(),
            VisualQuantizeMode::PartDefault => todo!(),
            VisualQuantizeMode::Track => {
                let Some(editor) = rpr.active_midi_editor() else {
                    return Ok(());
                };
                let take = editor.get_active_take()?;
                let (length, position) = get_notation_params_from_time_selection(pr, &take)?;
                let notation =
                    TrackNotation::default().with_visual_quantize(&VisualQuantizeNotation {
                        quantize: vq,
                        voice_position: None,
                        length_qn: length,
                    });
                insert_quantize_into_take(take, position, notation)?;
            }
            VisualQuantizeMode::Staff => {
                todo!();
                // let Some(editor) = rpr.active_midi_editor() else {
                //     return Ok(());
                // };
                // let take = editor.get_active_take()?;
                // let (length, position) = get_notation_params_from_time_selection(pr, &take)?;
                // let notation =
                //     TrackNotation::default().with_visual_quantize(&VisualQuantizeNotation {
                //         quantize: vq,
                //         staff: Some(runtime_state.staff_selected),
                //         voice: None,
                //         length_ppq: length,
                //     });
                // insert_quantize_into_take(take, position, notation)?;
            }
            VisualQuantizeMode::Voice => {
                todo!();
                // let Some(editor) = rpr.active_midi_editor() else {
                //     return Ok(());
                // };
                // let take = editor.get_active_take()?;
                // let (length, position) = get_notation_params_from_time_selection(pr, &take)?;
                // let notation =
                //     TrackNotation::default().with_visual_quantize(&VisualQuantizeNotation {
                //         quantize: vq,
                //         staff: Some(runtime_state.staff_selected),
                //         voice: Some(runtime_state.voice_selected),
                //         length_ppq: length,
                //     });
                // insert_quantize_into_take(take, position, notation)?;
            }
        }

        Ok(())
    }
    fn save_controls(&self) -> Result<()> {
        let pr = &Reaper::get().current_project();
        let mut state = self.get_state(pr)?;
        let grid_units = self.grid_units.selection()?.unwrap_or_default();
        let unit_type = self.unit_type.selection()?.unwrap_or_default();
        let tuplet_num = self.tuplet_num.text()?;
        let tuplet_denom = self.tuplet_denom.text()?;
        state.set(State {
            grid_units,
            unit_type,
            tuplet_num,
            tuplet_denom,
        })?;
        Ok(())
    }
    fn load_controls(&mut self) -> Result<()> {
        let pr = &Reaper::get().current_project();
        let state = self.get_state(pr)?.get()?.unwrap_or_default();
        self.grid_units.select(state.grid_units as i32)?;
        self.unit_type.select(state.unit_type as i32)?;
        self.tuplet_num.set_text(&state.tuplet_num)?;
        self.tuplet_denom.set_text(&state.tuplet_denom)?;
        self.enable_tuplet_section()?;
        Ok(())
    }
    fn get_state<'a>(&self, project: &'a Project) -> ReaperResult<ExtState<'a, State, Project>> {
        ExtState::new(
            EXT_SEXTION,
            "quantize_editor_state",
            State::default(),
            true,
            project,
            None,
        )
    }

    fn enable_tuplet_section(&mut self) -> Result<(), anyhow::Error> {
        let unit = self.get_unit_type()?;
        Ok(match unit {
            UnitType::Tuplet => {
                self.tuplet_num.enable(true)?;
                self.tuplet_denom.enable(true)?;
            }
            _ => {
                self.tuplet_num.enable(false)?;
                self.tuplet_denom.enable(false)?;
            }
        })
    }
}

fn insert_quantize_into_take(
    mut take: Take,
    position: u32,
    notation: TrackNotation,
) -> Result<(), anyhow::Error> {
    take.insert_midi_event(
        MidiEvent::new(
            position,
            false,
            false,
            rea_rs::CcShapeKind::default(),
            notation.as_message()?,
        ),
        None,
    )?;
    Ok(())
}

fn get_notation_params_from_time_selection(
    pr: Project,
    take: &Take,
) -> Result<(Option<f64>, u32), anyhow::Error> {
    match pr.get_time_selection()? {
        Some(ts) => {
            let range = ts.get()?;
            let start = range.start.as_ppq(take)?;
            let length = range.end.as_quarters_abs(&pr)? - range.start.as_quarters_abs(&pr)?;
            Ok((Some(length), start))
        }
        None => Ok((None, pr.get_cursor_position()?.as_ppq(take)?)),
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
                val if val == self.commit_button.id() || val == self.apply_button.id() => {
                    debug!("Commiting:");
                    if let Some(vq) = self.get_quantize()? {
                        self.apply_quantize(vq)?;
                        self.save_controls()?;
                    }
                    if val == self.apply_button.id() {
                        return Ok(self.close()?);
                    }
                    return Ok(());
                }

                val if val == self.cancel_button.id() => {
                    debug!("closing");
                    return Ok(self.close()?);
                }
                val => warn!("unknown button clicked: {:?}", val),
            },
            rea_rs::ControlEvent::ComboSelectionChanged { control } => {
                if control == self.unit_type.id() {
                    self.enable_tuplet_section()?;
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

/*fn get_visual_quantize_for_position(
    position: Time,
    track: &Track,
    staff: Option<u32>,
    voice: Option<u32>,
) -> LevitanusResult<VisualQuantize> {
    let mut last = None;
    for i in 0..track.n_items()? {
        let take = track
            .get_item(i)?
            .ok_or(LevitanusError::InvalidObject)?
            .active_take()?;
        let processor = MidiProcessor::load(&take)?;
        for mut event in processor.get_notations().track_notations {
            if Time::from_ppq(event.ppq_position(), &take)? > position {
                break;
            }
            event
                .message_mut()
                .edit_tokens(|tokens| {
                    let Some(notation) = TrackNotation::from_token_stream(tokens) else {
                        return Ok(());
                    };
                    if let Ok(Some(vq)) = notation.visual_quantize() {
                        match voice {
                            Some(voice) => {
                                let staff = match staff {
                                    Some(s) => s,
                                    None => 1,
                                };
                                if vq.
                            }
                            None => {}
                        };
                    };

                    Ok(())
                })
                .map_err(|e| LevitanusError::Unexpected(format!("{:?}", e)))?;
        }
    }
    let last = match last {
        Some(vq) => vq,
        None => {
            if let Some(vq) = TrackSettings::get(track)?.visual_quantize {
                vq
            } else {
                ProjectSettings::get()?.visual_quantize
            }
        }
    };
    Ok(last)
}
*/
