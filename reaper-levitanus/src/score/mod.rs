use anyhow::anyhow;
use rea_rs::{
    keys::{FVirt, KeyBinding, VKeys},
    ActionKind, ActionRegistrationOptions, ActionSections, ExtState, Reaper, Section,
};

pub use crate::score::midi_processing::get_notations_of_item;
use crate::score::{
    midi_editor_overlay::toggle_midi_editor_overlay,
    midi_processing::{mark_selected_notes_as_staff, mark_selected_notes_as_voice},
    visual_quantize_editor::toggle_visual_quantize_editor,
};

mod dom;
mod midi_editor_overlay;
mod midi_processing;
mod musicxml_example;
mod visual_quantize_editor;

pub const EXT_SEXTION: &'static str = "REASCORE";
pub const QUANTIZE_EDITOR_COMMAND_ID: &'static str = "quantize_editor_command_id";

pub fn register_score_actions(rpr: &mut Reaper) -> anyhow::Result<()> {
    rpr.register_action(
        "LEVITANUS_SCORE_MARK_VOICE_1",
        "rea-score: move selected notes to the voice 1",
        rea_rs::ActionKind::NotToggleable,
        |_| mark_selected_notes_as_voice(None),
        ActionRegistrationOptions::new(ActionSections::Sections(vec![
            Section::Main,
            Section::MidiEditor,
        ]))
        .with_default_key_binding(KeyBinding::new(
            FVirt::FVIRTKEY | FVirt::FALT,
            VKeys::VK_F3 as u16,
        )),
    )?;
    rpr.register_action(
        "LEVITANUS_SCORE_MARK_VOICE_2",
        "rea-score: move selected notes to the voice 2",
        rea_rs::ActionKind::NotToggleable,
        |_| mark_selected_notes_as_voice(Some(2)),
        ActionRegistrationOptions::new(ActionSections::Sections(vec![
            Section::Main,
            Section::MidiEditor,
        ]))
        .with_default_key_binding(KeyBinding::new(FVirt::FVIRTKEY, VKeys::VK_F3 as u16)),
    )?;
    rpr.register_action(
        "LEVITANUS_SCORE_MARK_STAFF_1",
        "rea-score: move selected notes to the staff 1",
        rea_rs::ActionKind::NotToggleable,
        |_| mark_selected_notes_as_staff(None),
        None,
    )?;
    rpr.register_action(
        "LEVITANUS_SCORE_MARK_STAFF_2",
        "rea-score: move selected notes to the staff 2",
        rea_rs::ActionKind::NotToggleable,
        |_| mark_selected_notes_as_staff(Some(2)),
        None,
    )?;

    let id = rpr.register_action(
        "LEVITANUS_SCORE_VQUANTIZE_EDITOR",
        "rea-score: visual quantize editor",
        ActionKind::Toggleable(false),
        toggle_visual_quantize_editor,
        ActionRegistrationOptions::new(ActionSections::Sections(vec![
            Section::MidiEditor,
            Section::MidiInlineEditor,
        ])),
    )?;
    ExtState::new(
        EXT_SEXTION,
        QUANTIZE_EDITOR_COMMAND_ID,
        id.command_id,
        false,
        rpr,
        None,
    )?;
    rpr.register_action(
        "LEVITANUS_SCORE_EDITOR_OVERLAY",
        "rea-score: toggle MIDI Editor overaly",
        ActionKind::Toggleable(false),
        toggle_midi_editor_overlay,
        ActionRegistrationOptions::new(ActionSections::Section(Section::MidiEditor)),
    )?;
    Ok(())
}

pub fn register_score_test_actions(rpr: &mut Reaper) -> anyhow::Result<()> {
    rpr.register_action(
        "LEVITANUS_SCORE_PRINT_ITEM_NOTATIONS",
        "rea-score: print item notations",
        rea_rs::ActionKind::NotToggleable,
        |_| {
            let item = Reaper::get()
                .current_project()
                .get_selected_item(0)?
                .ok_or(anyhow!("no selected item"))?;
            let res = get_notations_of_item(item)?;
            println!("{:#?}", res);
            Ok(())
        },
        None,
    )?;
    Ok(())
}
