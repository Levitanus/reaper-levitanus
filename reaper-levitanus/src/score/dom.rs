use std::collections::HashMap;

use fraction::{ConstOne, Fraction, Ratio, ToPrimitive};
use musical_note::ResolvedNote;
use rea_rs::{quantize, Project, ReaRsError, Time, Track, GUID};
use serde::{Deserialize, Serialize};

mod timeline;

pub(super) use timeline::Timeline;

use crate::LevitanusResult;

pub(super) type PartId = uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Score {
    timeline: Timeline,
    parts: HashMap<String, Part>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Part {
    spec: PartSpec,
    staves: Vec<Staff>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PartSpec {
    part_id: PartId,
    visual_quantize: Option<VisualQuantize>,
    transposition: Option<Transposition>,
    tracks: Vec<GUID>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash, Default)]
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
    pub unit: Fraction,
    pub ratio: Fraction,
}
impl Default for VisualQuantize {
    fn default() -> Self {
        Self {
            unit: Fraction::new(1_u64, 16_u64),
            ratio: Fraction::ONE,
        }
    }
}
impl VisualQuantize {
    pub fn as_quarters(&self) -> LevitanusResult<f64> {
        let final_ratio = self.unit * self.ratio * 4;
        final_ratio
            .to_f64()
            .ok_or(crate::LevitanusError::Unexpected(format!(
                "Can not convert VisualQuantize to quarters. Self is: {:#?}",
                self
            )))
    }
}
