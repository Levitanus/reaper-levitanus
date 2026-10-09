use std::collections::HashMap;

use fraction::{Fraction, Ratio};
use musical_note::{Key, ResolvedNote};
use rea_rs::TimeSignature;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Score {
    timeline: Timeline,
    parts: HashMap<String, Part>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Part {
    transposition: Option<Transposition>,
    staves: Vec<Staff>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Staff {
    voices: Vec<Voice>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Voice {
    timeline: Timeline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Transposition {
    semitones: u32,
}

type MeasureID = u32;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Timeline {
    key: HashMap<MeasureID, Key>,
    time_signatures: HashMap<MeasureID, TimeSignature>,
    visual_quantize: HashMap<ScorePosition, VisualQuantize>,
    measures: Vec<Measure>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Measure {
    id: MeasureID,
    events: Vec<ScoreEvent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct ScoreEvent {
    event_type: ScoreEventType,
    position: ScorePosition,
    length: ScoreLength,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum ScoreEventType {
    Rest,
    Note(ResolvedNote),
    Chord(Vec<ResolvedNote>),
    Tuple {
        ratio: Ratio<u32>,
        events: Vec<ScoreEvent>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub(super) struct ScorePosition {
    measure: MeasureID,
    position_in_measure: Fraction,
    position_absolute: Fraction,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ScoreLength {
    length_absolute: Fraction,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct VisualQuantize {
    ratio: Fraction,
}
