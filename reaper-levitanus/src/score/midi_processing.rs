use core::fmt;
use std::{fmt::Display, str::FromStr};

use anyhow::Result;
use fraction::Fraction;
use itertools::Itertools;
use log::{debug, error, warn};
use rea_rs::{
    flatten_midi_notes, CcShapeKind, Item, MidiEvent, MidiEventBuilder, MidiEventConsumer,
    MidiMessage, MidiNoteEvent, Notation, NotationMessage, NoteOffMessage, NoteOnMessage, Reaper,
    Take,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{
    score::dom::{VisualQuantize, VoicePosition},
    LevitanusError, LevitanusResult,
};

pub(super) fn mark_selected_notes_as_voice(voice: u8) -> anyhow::Result<()> {
    debug!("mark selected notes as voice {:?}", voice);
    MidiProcessor::process_notes_in_editor(|notes_with_notations| {
        for note_w_n in notes_with_notations {
            if !note_w_n.note.is_selected {
                continue;
            }
            note_w_n.edit_notation(|notation| {
                let mut notation = notation.unwrap_or_default();
                let mut position = notation.voice.unwrap_or_default();
                position.voice = voice;
                notation.voice = Some(position);
                Ok(Some(notation))
            })?;
        }
        Ok(())
    })
}
pub(super) fn mark_selected_notes_as_staff(staff: u8) -> anyhow::Result<()> {
    debug!("mark selected notes as staff {:?}", staff);
    MidiProcessor::process_notes_in_editor(|notes_with_notations| {
        for note_w_n in notes_with_notations {
            if !note_w_n.note.is_selected {
                continue;
            }
            note_w_n.edit_notation(|notation| {
                let mut notation = notation.unwrap_or_default();
                let mut position = notation.voice.unwrap_or_default();
                position.staff = staff;
                notation.voice = Some(position);
                Ok(Some(notation))
            })?;
        }
        Ok(())
    })
}
pub(super) fn clear_voice_position() -> anyhow::Result<()> {
    debug!("clear_voice_position");
    MidiProcessor::process_notes_in_editor(|notes_with_notations| {
        for note_w_n in notes_with_notations {
            if !note_w_n.note.is_selected {
                continue;
            }
            note_w_n.edit_notation(|notation| {
                let mut notation = notation.unwrap_or_default();
                notation.voice = None;
                Ok(Some(notation))
            })?;
        }
        Ok(())
    })
}

#[derive(Clone, Serialize, Deserialize)]
pub struct NoteWithNotation {
    note: MidiNoteEvent,
    notation: Option<MidiEvent<NotationMessage>>,
}
impl fmt::Debug for NoteWithNotation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NoteWithNotation")
            .field("note", &self.note)
            .field("notation", &self.notation.as_ref().map(DisplayAsDebug))
            .finish()
    }
}
impl NoteWithNotation {
    fn edit_tokens(&mut self, f: impl FnMut(&mut Vec<String>) -> Result<()>) -> Result<()> {
        let mut notation_ev = match &self.notation {
            Some(n) => n.clone(),
            None => MidiEvent::new(
                self.note.start_in_ppq,
                self.note.is_selected,
                self.note.is_muted,
                rea_rs::CcShapeKind::default(),
                Notation::Note {
                    channel: self.note.channel,
                    note: self.note.note,
                    tokens: Vec::new(),
                }
                .into(),
            ),
        };
        match notation_ev.message_mut().edit_tokens(f)? {
            true => self.notation = Some(notation_ev),
            false => self.notation = None,
        }

        Ok(())
    }
    fn edit_notation(
        &mut self,
        mut f: impl FnMut(Option<NoteNotation>) -> Result<Option<NoteNotation>>,
    ) -> Result<()> {
        self.edit_tokens(|tokens| match f(NoteNotation::from_token_stream(tokens))? {
            None => Ok(()),
            Some(notation) => {
                if notation == NoteNotation::default() {
                    return Ok(());
                }
                notation.to_token_stream(tokens)?;
                Ok(())
            }
        })
    }
}

pub(super) trait SerializedNotation:
    Sized + Serialize + DeserializeOwned + Default + PartialEq
{
    const PREFIX: &str = "LS";
    fn try_deserialize(token: &str) -> Option<Self> {
        let json = token.strip_prefix(Self::PREFIX)?;
        serde_json::from_str(json).ok()
    }
    fn from_token_stream(tokens: &mut Vec<String>) -> Option<Self> {
        let payload_index = tokens
            .iter()
            .position(|token| Self::try_deserialize(token).is_some())?;

        if payload_index == 0 || tokens[payload_index - 1] != "custom" {
            return None;
        }

        let payload = tokens.remove(payload_index);
        tokens.remove(payload_index - 1);

        Self::try_deserialize(&payload)
    }

    fn to_token_stream(
        &self,
        tokens: &mut Vec<String>,
    ) -> std::result::Result<(), serde_json::Error> {
        let mut i = 0;

        while i + 1 < tokens.len() {
            if tokens[i] == "custom" && Self::try_deserialize(&tokens[i + 1]).is_some() {
                tokens.drain(i..=i + 1);
            } else {
                i += 1;
            }
        }
        // Don't put tokens if it's a default notation
        if self == &Self::default() {
            return Ok(());
        }

        let payload = serde_json::to_string(self)?;

        tokens.push("custom".to_owned());
        tokens.push(format!("{}{payload}", Self::PREFIX));
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct TrackNotation {
    #[serde(rename = "vquantize", skip_serializing_if = "Option::is_none")]
    visual_quintize: Option<(String, String, Option<u8>, Option<u8>, Option<f64>)>,
}
impl SerializedNotation for TrackNotation {}
impl TrackNotation {
    pub fn visual_quantize(&self) -> LevitanusResult<Option<VisualQuantizeNotation>> {
        let (unit, ratio, staff, voice, length) = match &self.visual_quintize {
            None => return Ok(None),
            Some(q) => q,
        };
        let voice_position = match (staff, voice) {
            (None, None) => None,
            (Some(staff), Some(voice)) => Some(VoicePosition {
                staff: *staff,
                voice: *voice,
            }),
            e => {
                error!(
                    "Unsupported variant of VoicePosition: {:?}. Returning default",
                    e
                );
                Some(VoicePosition::default())
            }
        };
        Ok(Some(VisualQuantizeNotation {
            quantize: VisualQuantize {
                unit: Fraction::from_str(&unit)
                    .map_err(|e| LevitanusError::Unexpected(format!("ParseError: {:?}", e)))?,
                ratio: Fraction::from_str(&ratio)
                    .map_err(|e| LevitanusError::Unexpected(format!("ParseError: {:?}", e)))?,
            },
            voice_position,
            length_qn: *length,
        }))
    }
    pub fn with_visual_quantize(mut self, quantize: &VisualQuantizeNotation) -> Self {
        let (staff, voice) = match &quantize.voice_position {
            Some(q) => (Some(q.staff), Some(q.voice)),
            None => (None, None),
        };
        self.visual_quintize = Some((
            quantize.quantize.unit.to_string(),
            quantize.quantize.ratio.to_string(),
            staff,
            voice,
            quantize.length_qn,
        ));
        self
    }
    pub fn as_message(&self) -> Result<NotationMessage> {
        let mut tokens = Vec::new();
        self.to_token_stream(&mut tokens)?;
        Ok(Notation::Track(tokens).into())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisualQuantizeNotation {
    pub quantize: VisualQuantize,
    pub voice_position: Option<VoicePosition>,
    pub length_qn: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct NoteNotation {
    #[serde(rename = "vquantize", skip_serializing_if = "Option::is_none")]
    pub voice: Option<VoicePosition>,
}
impl SerializedNotation for NoteNotation {}

struct DisplayAsDebug<'a, T: ?Sized>(&'a T);

impl<T: Display + ?Sized> fmt::Debug for DisplayAsDebug<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(self.0, f)
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct TakeNotations {
    pub notes_with_notation: Vec<NoteWithNotation>,
    pub track_notations: Vec<MidiEvent<NotationMessage>>,
    pub unknown_notations: Vec<MidiEvent<NotationMessage>>,
}
impl fmt::Debug for TakeNotations {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TakeNotations")
            .field("notes_with_notation", &self.notes_with_notation)
            .field(
                "track_notations",
                &self
                    .track_notations
                    .iter()
                    .map(DisplayAsDebug)
                    .collect::<Vec<_>>(),
            )
            .field(
                "unknown_notations",
                &self
                    .unknown_notations
                    .iter()
                    .map(DisplayAsDebug)
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

#[derive(Debug)]
pub struct MidiProcessor {
    buf: Vec<u8>,
}
impl MidiProcessor {
    pub fn process_notes_in_editor(
        mut f: impl FnMut(&mut Vec<NoteWithNotation>) -> Result<()>,
    ) -> Result<()> {
        let rpr = Reaper::get();
        let Some(editor) = rpr.active_midi_editor() else {
            warn!("no opened MIDI editor");
            return Ok(());
        };

        for mut take in editor.enum_takes(true) {
            let mut processor = MidiProcessor::load(&take)?;
            let mut notations = processor.get_notations();
            f(&mut notations.notes_with_notation)?;
            processor.apply_notations(notations);
            processor.set(&mut take)?;
        }
        Ok(())
    }
    pub fn load(take: &Take) -> LevitanusResult<Self> {
        if !take.is_midi()? {
            return Err(LevitanusError::NoMidi.into());
        }
        debug!("getting midi from take: {:?}", take);
        let buf = take.get_midi(None)?;
        Ok(Self { buf })
    }
    pub fn set(self, take: &mut Take) -> LevitanusResult<()> {
        debug!("applying midi to take");
        Ok(take.set_midi(self.buf)?)
    }
    pub fn events(&self) -> MidiEventBuilder<impl Iterator<Item = u8> + '_> {
        MidiEventBuilder::new(self.buf.iter().copied())
    }
    pub fn get_notations(&self) -> TakeNotations {
        let mut notes_with_notation: Vec<NoteWithNotation> = self
            .events()
            .filter_notes()
            .map(|event| NoteWithNotation {
                note: event,
                notation: None,
            })
            .collect();
        let mut track_notations = Vec::new();
        let mut unknown_notations = Vec::new();
        for event in self.events() {
            let (spec, msg) = event.into_raw_parts();
            let Some(msg) = NotationMessage::from_raw(msg.into_raw()) else {
                continue;
            };
            match msg.notation() {
                Notation::Note {
                    channel,
                    note: n_note,
                    tokens: _,
                } => {
                    for note in notes_with_notation.iter_mut() {
                        if note.note.start_in_ppq == spec.ppq_position()
                            && note.note.channel == channel
                            && note.note.note == n_note
                        {
                            let notation_event = MidiEvent::from_raw_parts(spec, msg);
                            note.notation = Some(notation_event);
                            break;
                        }
                    }
                }
                Notation::Track(_) => track_notations.push(MidiEvent::from_raw_parts(spec, msg)),
                Notation::Unknown(_) => {
                    unknown_notations.push(MidiEvent::from_raw_parts(spec, msg))
                }
            }
        }
        TakeNotations {
            notes_with_notation,
            track_notations,
            unknown_notations,
        }
    }
    pub fn apply_notations(&mut self, notations: TakeNotations) {
        // let note_events = ;
        let events =
            self.events()
                .filter(|event| {
                    let msg = event.message();
                    if NotationMessage::from_raw(msg.get_raw()).is_some() {
                        return false;
                    }
                    if NoteOnMessage::from_raw(msg.get_raw()).is_some() {
                        return false;
                    }
                    if NoteOffMessage::from_raw(msg.get_raw()).is_some() {
                        return false;
                    }
                    true
                })
                .chain(flatten_midi_notes(
                    notations
                        .notes_with_notation
                        .iter()
                        .map(|item| item.note.clone()),
                ))
                // .chain(note_events)
                .chain(notations.notes_with_notation.iter().filter_map(
                    |item| match &item.notation {
                        None => None,
                        Some(event) => Some(event.clone().into_raw_event()),
                    },
                ))
                .chain(
                    notations
                        .track_notations
                        .iter()
                        .map(|event| event.clone().into_raw_event()),
                )
                .chain(
                    notations
                        .unknown_notations
                        .iter()
                        .map(|event| event.clone().into_raw_event()),
                )
                .sorted_by(|first, other| {
                    first
                        .partial_cmp(&other)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
        self.buf = MidiEventConsumer::new(events).collect();
    }
}

pub fn get_notations_of_item(item: Item) -> LevitanusResult<TakeNotations> {
    let take = item.active_take()?;
    Ok(MidiProcessor::load(&take)?.get_notations())
}

// pub fn insert_track_notation_in_opened_editor(
//     notation: TrackNotation,
//     position: Time,
// ) -> Result<()> {
//     let Some(editor) = Reaper::get().active_midi_editor() else {
//         return Ok(());
//     };
//     let mut take = editor.get_active_take()?;
//     let mut notations = Vec::new();
//     notation.to_token_stream(&mut notations)?;
//     let event = MidiEvent::new(
//         position.as_ppq(&take)?,
//         false,
//         false,
//         CcShapeKind::default(),
//         NotationMessage::from(Notation::Track(notations)),
//     );

//     take.insert_midi_event(event, None)?;
//     Ok(())
// }
