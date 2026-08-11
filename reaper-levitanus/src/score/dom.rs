use std::collections::HashMap;

use fraction::Fraction;
use musical_note::{ResolvedNote, Scale};
use rea_rs::TimeSignature;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Score {
    timeline: Timeline,
    parts: HashMap<String, Part>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Part {
    staves: Vec<Staff>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Staff {
    voices: Vec<Voice>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Voice {
    timeline: Timeline,
    transposition: Option<Transposition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Transposition {
    semitones: u32,
}

type MeasureID = u32;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Timeline {
    scale: HashMap<MeasureID, Scale>,
    time_signatures: HashMap<MeasureID, TimeSignature>,
    visual_quantize: HashMap<ScorePosition, VisualQuantize>,
    measures: Vec<Measure>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Measure {
    id: MeasureID,
    events: Vec<ScoreEvent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ScoreEvent {
    event_type: ScoreEventType,
    position: ScorePosition,
    length: ScoreLength,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum ScoreEventType {
    Rest,
    Note(ResolvedNote),
    Chord(Vec<ResolvedNote>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
struct ScorePosition {
    measure: MeasureID,
    position_in_measure: Fraction,
    position_absolute: Fraction,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ScoreLength {
    length_absolute: Fraction,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct VisualQuantize {
    ratio: Fraction,
}
