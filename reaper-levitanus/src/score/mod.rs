use anyhow::anyhow;
use musical_note::Key;
use rea_rs::{
    keys::{FVirt, KeyBinding, VKeys},
    ptr_wrappers::MediaTrack,
    ActionKind, ActionRegistrationOptions, ActionSections, ExtState, Project, Reaper, ReaperResult,
    Section, Track,
};
use serde::{Deserialize, Serialize};

pub use crate::score::midi_processing::get_notations_of_item;
use crate::score::{
    dom::{PartId, VisualQuantize},
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

pub fn register_score_actions(rpr: &mut Reaper) -> anyhow::Result<()> {
    rpr.register_action(
        "LEVITANUS_SCORE_MARK_VOICE_1",
        "ReaScore: move selected notes to the voice 1",
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
        "ReaScore: move selected notes to the voice 2",
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
        "ReaScore: move selected notes to the staff 1",
        rea_rs::ActionKind::NotToggleable,
        |_| mark_selected_notes_as_staff(None),
        None,
    )?;
    rpr.register_action(
        "LEVITANUS_SCORE_MARK_STAFF_2",
        "ReaScore: move selected notes to the staff 2",
        rea_rs::ActionKind::NotToggleable,
        |_| mark_selected_notes_as_staff(Some(2)),
        None,
    )?;

    rpr.register_action(
        "LEVITANUS_SCORE_VQUANTIZE_EDITOR",
        "ReaScore: visual quantize part (track)",
        ActionKind::Toggleable(false),
        |hook| {
            toggle_visual_quantize_editor(hook, visual_quantize_editor::VisualQuantizeMode::Track)
        },
        ActionRegistrationOptions::new(ActionSections::Sections(vec![
            Section::MidiEditor,
            Section::MidiInlineEditor,
        ])),
    )?;
    rpr.register_action(
        "LEVITANUS_SCORE_VQUANTIZE_EDITOR",
        "ReaScore: visual quantize default",
        ActionKind::Toggleable(false),
        |hook| {
            toggle_visual_quantize_editor(
                hook,
                visual_quantize_editor::VisualQuantizeMode::ProjectDefault,
            )
        },
        ActionRegistrationOptions::new(ActionSections::Section(Section::Main)),
    )?;
    rpr.register_action(
        "LEVITANUS_SCORE_EDITOR_OVERLAY",
        "ReaScore: toggle MIDI Editor overaly",
        ActionKind::Toggleable(false),
        toggle_midi_editor_overlay,
        ActionRegistrationOptions::new(ActionSections::Section(Section::MidiEditor)),
    )?;
    Ok(())
}

pub fn register_score_test_actions(rpr: &mut Reaper) -> anyhow::Result<()> {
    rpr.register_action(
        "LEVITANUS_SCORE_PRINT_ITEM_NOTATIONS",
        "ReaScore: print item notations",
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

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct RuntimeState {
    staff_selected: u8,
    voice_selected: u8,
}
impl RuntimeState {
    pub(super) fn get() -> ReaperResult<Self> {
        let rpr = Reaper::get();
        Ok(Self::state(rpr).get()?.unwrap_or_default())
    }
    pub(super) fn set(&self) -> ReaperResult<()> {
        let rpr = Reaper::get();
        Self::state(rpr).set(self.clone())
    }
    fn state(reaper: &Reaper) -> ExtState<'_, Self, Reaper> {
        ExtState::<RuntimeState, Reaper>::existing(
            EXT_SEXTION,
            "runtime_state",
            false,
            reaper,
            None,
        )
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct ProjectSettings {
    visual_quantize: VisualQuantize,
    key: Key,
}
impl ProjectSettings {
    pub(super) fn get(project: &Project) -> ReaperResult<Self> {
        Ok(Self::state(project).get()?.unwrap_or_default())
    }
    pub(super) fn set(&self, project: &Project) -> ReaperResult<()> {
        Self::state(project).set(self.clone())
    }
    fn state(project: &Project) -> ExtState<'_, Self, Project> {
        ExtState::<Self, Project>::existing(EXT_SEXTION, "settings", false, project, None)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct TrackSettings {
    part: PartId,
}
impl TrackSettings {
    pub(super) fn get(track: &Track) -> ReaperResult<Self> {
        Ok(Self::state(track).get()?.unwrap_or_default())
    }
    pub(super) fn set(&self, track: &Track) -> ReaperResult<()> {
        Self::state(track).set(self.clone())
    }
    fn state(track: &Track) -> ExtState<'_, Self, Track> {
        ExtState::<Self, Track>::existing(EXT_SEXTION, "settings", false, track, None)
    }
}
