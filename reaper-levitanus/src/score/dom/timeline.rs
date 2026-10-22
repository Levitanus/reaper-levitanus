use fraction::ToPrimitive;
use indexmap::IndexMap;
use itertools::Itertools;
use log::{debug, error};
use musical_note::Key;
use rea_rs::{
    quantize, MidiEvent, Project, RawMidiMessage, ReaRsError, ReaperResult,
    TempoTimeSigMarkerFlags, Time, TimeSignature, Track,
};
use serde::{Deserialize, Serialize};

use crate::{
    score::{
        dom::{Measure, MeasureID, PartSpec, VoicePosition},
        midi_processing::{SerializedNotation, TrackNotation},
        ProjectSettings, VisualQuantize,
    },
    LevitanusError, LevitanusResult,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Timeline {
    key: IndexMap<Time, Key>,
    time_signatures: IndexMap<MeasureID, TimeSignature>,
    visual_quantize: IndexMap<Time, Vec<VisualQuantize>>,
    measures: Vec<Measure>,
}
impl Timeline {
    pub fn parse_global(project: &Project) -> LevitanusResult<Self> {
        let project_settings = ProjectSettings::get(project)?;
        let measures = Vec::new();
        let mut visual_quantize = IndexMap::new();
        visual_quantize.insert(
            Time::default(),
            vec![project_settings.visual_quantize.clone()],
        );
        let mut time_signatures = IndexMap::new();
        time_signatures.insert(0, project.time_signature_at_position(Time::default())?.0);
        for marker in project.iter_tempo_markers() {
            if marker
                .flags
                .contains(TempoTimeSigMarkerFlags::SET_TIME_SIGNATURE)
            {
                if let Some(ts) = marker.time_signature {
                    time_signatures.insert(marker.beat_position.relative(project)?.0, ts);
                }
            }
        }

        let mut key_signatures = IndexMap::new();
        key_signatures.insert(Time::default(), project_settings.key);
        let vquantize = project_settings.visual_quantize;
        for notation in GlobalNotation::iter_global_notations(project) {
            if let Some(key) = notation.notation.key {
                let position = quantize::by_quarters(
                    0.0,
                    vquantize.as_quarters()?,
                    notation.position.as_quarters_abs(project)?,
                    project,
                )?;
                key_signatures.insert(Time::from_quarters_abs(position, project)?, key);
            }
        }
        let obj = Self {
            key: key_signatures,
            time_signatures,
            visual_quantize,
            measures,
        };
        debug!("Parsed global TimeLine: {:#?}", obj);
        Ok(obj)
    }

    pub fn build_for_part(
        part: &mut PartSpec,
        project: &Project,
        global_timeline: impl Into<Option<Timeline>>,
    ) -> LevitanusResult<Self> {
        let mut timelines = PartTimelines::new(
            global_timeline
                .into()
                .unwrap_or(Timeline::parse_global(project)?),
        );
        let part_midi = get_part_midi(part, project)?;
        let sorted = part_midi;
        for (time, event) in sorted {
            let (specs, msg) = event.into_raw_parts();
            match msg.parse()? {
                rea_rs::MidiMessageKind::NoteOn(msg) => todo!(),
                rea_rs::MidiMessageKind::NoteOff(msg) => todo!(),
                rea_rs::MidiMessageKind::Notation(msg) => match msg.notation() {
                    rea_rs::Notation::Note {
                        channel,
                        note,
                        tokens,
                    } => todo!(),
                    rea_rs::Notation::Track(mut tokens) => {
                        let Some(track_notation) = TrackNotation::from_token_stream(&mut tokens)
                        else {
                            continue;
                        };
                        if let Some(vquantize) = track_notation.visual_quantize()? {
                            timelines
                                .with_timeline_mut(&vquantize.voice_position, |timeline| {
                                    let quantized_time =
                                        timeline.quantize_position(time, project)?;
                                    let quantized_end = match &vquantize.length_qn {
                                        None => None,
                                        Some(qn) => {
                                            let start_qn = time.as_quarters_abs(project)?;
                                            let end_qn = start_qn + qn;
                                            Some(Time::from_quarters_abs(end_qn, project)?)
                                        }
                                    };
                                    Ok(timeline.insert_visual_quantize(
                                        vquantize.quantize.clone(),
                                        quantized_time,
                                        quantized_end,
                                    )?)
                                })
                                .map_err(|e| {
                                    LevitanusError::Unexpected(format!(
                                        "can not insert visual quantize: {:?}",
                                        e
                                    ))
                                })?;
                        }
                    }
                    rea_rs::Notation::Unknown(tokens) => todo!(),
                },
                _ => (),
            }
        }
        todo!();
    }

    pub fn insert_visual_quantize(
        &mut self,
        vquantize: VisualQuantize,
        position: Time,
        end: Option<Time>,
    ) -> LevitanusResult<()> {
        todo!()
    }

    pub fn quantize_position(&self, position: Time, project: &Project) -> ReaperResult<Time> {
        for (time, vquantize) in self.visual_quantize.iter() {
            if time < &position {
                continue;
            }
            let mut vquantize = vquantize.clone().into_iter();
            let mut interval = vquantize.next().expect("should be at least one vquantize");
            for q in vquantize {
                interval.unit = q.unit;
                interval.ratio *= q.ratio;
            }
            let interval = (interval.unit * interval.ratio * 4).to_f64().ok_or({
                ReaRsError::UnsuccessfulOperationString(format!(
                    "Can not convert interval into quarter notes: {:?}",
                    interval
                ))
            })?;
            let quantized = quantize::by_quarters(
                *time,
                interval,
                position.as_quarters_abs(project)?,
                project,
            )?;
            return Time::from_quarters_abs(quantized, project);
        }
        Err(ReaRsError::UnsuccessfulOperation(
            "Given time is probably less than project start",
        ))
    }
}

fn get_part_midi(
    part: &mut PartSpec,
    project: &Project,
) -> Result<impl Iterator<Item = (Time, MidiEvent<RawMidiMessage>)>, LevitanusError> {
    let mut keep = Vec::new();
    let mut tracks_midi = Vec::new();
    for track_guid in part.tracks.iter() {
        let track = match Track::from_guid(project, track_guid)? {
            Some(t) => {
                keep.push(true);
                t
            }
            None => {
                keep.push(false);
                continue;
            }
        };
        tracks_midi.push(iter_track_midi(&track)?);
    }
    let mut keep = keep.into_iter();
    part.tracks.retain(|_| keep.next().unwrap());
    let part_midi = tracks_midi.into_iter().kmerge_by(|a, b| a.0 < b.0);
    Ok(part_midi)
}

fn iter_track_midi(
    track: &Track,
) -> LevitanusResult<impl Iterator<Item = (Time, MidiEvent<RawMidiMessage>)>> {
    let mut iterators = Vec::new();
    let mut takes = Vec::new();

    for i in 0..track.n_items()? {
        let take = track
            .get_item(i)?
            .ok_or(LevitanusError::InvalidObject)?
            .active_take()?;

        iterators.push(take.iter_midi(None)?);
        takes.push(take);
    }

    Ok(iterators
        .into_iter()
        .zip(takes)
        .flat_map(|(builder, take)| {
            builder.filter_map(move |event| {
                match Time::from_ppq(event.ppq_position(), &take){
                    Ok(time) => Some((time,event)),
                    Err(e) => {error!("Can not make Time from the Event ppqo Position. Error is: {:?}\nRetruning None",e);None}
                }
            })
        }))
}

/*
TODO:
- parse global project events: time signatures, key signatures
- check existence of tracks inside the part and modify tracklist
- collect all reqired events (notes, notations) from all tracks, converting their positions and lengthes into quarter notes split them by categories.
- build quantisation grid with nested tuples
- quaintize events, respecting the grid (separate function)
- build measures *** TODO
*/

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlobalNotation {
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<Key>,
}
impl GlobalNotation {
    const PREFIX: &'static str = "RS";
    pub fn try_deserialize(payload: &str) -> Option<Self> {
        serde_json::from_str(payload.strip_prefix(Self::PREFIX)?).ok()
    }
    pub fn serialize(&self) -> LevitanusResult<String> {
        Ok(Self::PREFIX.to_owned()
            + &serde_json::to_string(self).map_err(|e| {
                LevitanusError::Unexpected(format!(
                    "Can not serialize GlobalNotation to json. Original error: {:?}",
                    e
                ))
            })?)
    }
    pub fn iter_global_notations<'a>(
        project: &'a Project,
    ) -> impl Iterator<Item = GlobalNotationEvent> + use<'a> {
        std::iter::from_fn(move || {
            let mut iterator = project.iter_markers_and_regions();

            while let Ok(marker) = iterator.next()? {
                if !marker.is_region {
                    let notation = Self::try_deserialize(&marker.name)?;
                    return Some(GlobalNotationEvent {
                        marker_user_index: marker.user_index,
                        position: marker.position,
                        notation,
                    });
                } else {
                    continue;
                }
            }
            None
        })
    }
}

#[derive(Debug, Clone)]
pub struct GlobalNotationEvent {
    marker_user_index: usize,
    position: Time,
    notation: GlobalNotation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartTimelines {
    timelines: IndexMap<VoicePosition, Timeline>,
}
impl PartTimelines {
    pub fn new(global_timeline: Timeline) -> Self {
        let mut timelines = IndexMap::new();
        timelines.insert(VoicePosition::default(), global_timeline);
        Self { timelines }
    }
    #[inline]
    pub fn timeline(&self, voice_position: &VoicePosition) -> &Timeline {
        self.timelines.get(voice_position).unwrap_or_else(|| {
            self.timelines
                .get(&VoicePosition::default())
                .expect("has to be global timeline")
        })
    }
    #[inline]
    pub fn timeline_mut(&mut self, voice_position: &VoicePosition) -> &mut Timeline {
        match self.timelines.get_mut(voice_position) {
            Some(t) => t,
            None => {
                self.timelines.insert(
                    *voice_position,
                    self.timeline(&VoicePosition::default()).clone(),
                );
                self.timeline_mut(voice_position)
            }
        }
    }

    #[inline]
    pub fn with_timeline(
        &self,
        voice_position: &Option<VoicePosition>,
        mut f: impl FnMut(&Timeline) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        match voice_position {
            Some(v) => f(self.timeline(v)),
            None => {
                for timeline in self.timelines.values() {
                    f(timeline)?;
                }
                Ok(())
            }
        }
    }

    #[inline]
    pub fn with_timeline_mut(
        &mut self,
        voice_position: &Option<VoicePosition>,
        mut f: impl FnMut(&mut Timeline) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        match voice_position {
            Some(v) => f(self.timeline_mut(v)),
            None => {
                for timeline in self.timelines.values_mut() {
                    f(timeline)?;
                }
                Ok(())
            }
        }
    }
}
