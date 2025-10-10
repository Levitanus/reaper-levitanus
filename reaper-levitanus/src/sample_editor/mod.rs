use std::{
    cell::RefCell,
    cmp::{max, min},
    collections::HashMap,
    error::Error,
    hash::Hash,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use dasp_rs::{frames_to_time, hz_to_midi, midi_to_hz, AudioData};
use itertools::Itertools;
use log::{debug, warn};
use musical_note::{midi_to_note, Accidental, Key, ResolvedNote};
use rea_rs::{Immutable, MarkerRegionInfo, Position, Reaper, SourceOffset, Track};
use serde::{Deserialize, Serialize};
use statrs::statistics::{Data, Median};

use crate::{
    gui::{launch_frontend, stop_backend},
    sample_editor::gui::{Backend, BACKEND_ID_STRING},
    SampleEditorError,
};

mod gui;
pub use gui::{front, SOCKET_PORT};

static DEFAULT_SR: u32 = 22050;

pub fn sample_editor_gui() -> Result<(), Box<dyn Error>> {
    let rpr = Reaper::get_mut();
    if !rpr.has_control_surface(&BACKEND_ID_STRING.to_string()) {
        let backend = Backend::new()?;
        rpr.register_control_surface(Arc::new(RefCell::new(backend)));
    }
    launch_frontend(crate::gui::ComponentType::SampleEditor)?;
    Ok(())
}

pub fn stop_sample_editor_gui() -> Result<(), Box<dyn Error>> {
    stop_backend(BACKEND_ID_STRING)
}

fn get_regions_in_time_selection(tracks: Option<&[TrackInfo]>) -> Vec<RegionInfo> {
    let rpr = Reaper::get();
    let pr = rpr.current_project();
    let ts = pr.get_time_selection();

    let mut regions = vec![];

    for region in pr
        .iter_markers_and_regions()
        .filter(|r| r.is_region && ts.contains(r.position) && ts.contains(r.rgn_end))
    {
        let mut audio_sources = vec![];
        let timerange = region.position..region.rgn_end;
        for item in pr.iter_items() {
            if let Some(tracks) = tracks {
                if !tracks
                    .iter()
                    .any(|t| t.compare_to_reaper_track(&item.track()))
                {
                    continue;
                }
            }
            if item.position() > timerange.end {
                continue;
            }
            if item.end_position() < timerange.start {
                continue;
            }
            let take = item.active_take();
            if let Some(source) = take.source() {
                let file = source.filename();
                let mut source_offset = take.start_offset();
                let start = max(item.position(), timerange.start);
                let end = min(item.end_position(), timerange.end);
                if item.position() < timerange.start {
                    source_offset = source_offset + timerange.start.as_duration()
                        - item.position().as_duration();
                }
                let region_offset = if start > timerange.start {
                    start - timerange.start
                } else {
                    Position::default()
                };
                let length: Duration = { (end - start).as_duration() };
                audio_sources.push(AudioSourceInfo {
                    filename: file,
                    source_offset,
                    region_offset,
                    length,
                });
            }
        }
        regions.push(RegionInfo {
            info: region,
            audio_sources,
        });
    }
    regions
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionInfo {
    pub info: MarkerRegionInfo,
    pub audio_sources: Vec<AudioSourceInfo>,
}

#[derive(Debug)]
struct Region {
    info: RegionInfo,
    audio: Option<AudioData>,
    sr: Option<u32>,
    frame_length: Option<usize>,
    pitch_envelope: Option<Vec<f32>>,
    midi_envelope: Option<Vec<u8>>,
}
impl From<RegionInfo> for Region {
    fn from(value: RegionInfo) -> Self {
        Self {
            info: value,
            audio: None,
            sr: None,
            frame_length: None,
            pitch_envelope: None,
            midi_envelope: None,
        }
    }
}
impl Into<RegionInfo> for Region {
    fn into(self) -> RegionInfo {
        self.info
    }
}
impl Region {
    fn unwrap_sr(&self, sr: impl Into<Option<u32>>) -> u32 {
        let sr = sr.into();
        match sr {
            Some(sr) => sr,
            None => match self.sr {
                Some(sr) => sr,
                None => DEFAULT_SR,
            },
        }
    }
    fn unwrap_frame_length(&self, frame_length: impl Into<Option<usize>>) -> Option<usize> {
        let frame_length = frame_length.into();
        match frame_length {
            Some(frame_length) => Some(frame_length),
            None => match self.frame_length {
                Some(frame_length) => Some(frame_length),
                None => None,
            },
        }
    }

    pub fn load_audio(&mut self, sr: impl Into<Option<u32>>) -> Result<&AudioData, anyhow::Error> {
        if self.audio.is_some() {
            return Ok(self.audio.as_ref().unwrap());
        }
        if self.info.audio_sources.is_empty() {
            return Err(SampleEditorError::EmptyRegion.into());
        }
        let sr = self.unwrap_sr(sr);
        self.sr = Some(sr);
        let mut audios = vec![];
        let channels = self.info.audio_sources.len();
        for source in &self.info.audio_sources {
            let audio = dasp_rs::io::load(
                source.filename.clone(),
                Some(sr),
                Some(true),
                Some(source.source_offset.as_secs_f64() as f32),
                Some(source.length.as_secs_f32()),
            )?;
            audios.push(audio);
        }
        let audio_refs: Vec<&AudioData> = audios.iter().collect();
        let audio = dasp_rs::mixing::multi_channel_mix(&audio_refs, audio_refs.len() as u16)?;
        drop(audios);
        self.audio = Some(AudioData::new(
            dasp_rs::signal_processing::to_mono(&audio.samples, channels),
            DEFAULT_SR,
            1,
        ));
        Ok(self.audio.as_ref().unwrap())
    }

    pub fn compute_pitch_envelope(
        &mut self,
        sr: impl Into<Option<u32>>,
        fmin: impl Into<Option<f32>>,
        fmax: impl Into<Option<f32>>,
        frame_length: impl Into<Option<usize>>,
    ) -> Result<&Vec<f32>, anyhow::Error> {
        if self.pitch_envelope.is_some() {
            return Ok(self.pitch_envelope.as_ref().unwrap());
        }
        let sr = self.unwrap_sr(sr);
        let frame_length = self.unwrap_frame_length(frame_length);
        let audio = self.load_audio(sr)?;
        let fmin = match fmin.into() {
            Some(fmin) => fmin,
            None => 60.0,
        };
        let fmax = match fmax.into() {
            Some(fmax) => fmax,
            None => 1500.0,
        };
        let pitch_envelope =
            dasp_rs::tuning::pyin(&audio.samples, fmin, fmax, Some(sr), frame_length)?;
        self.pitch_envelope = Some(pitch_envelope);
        Ok(self.pitch_envelope.as_ref().unwrap())
    }

    fn compute_midi_envelope(
        &mut self,
        sr: impl Into<Option<u32>>,
        lower_note: impl Into<Option<musical_note::ResolvedNote>>,
        upper_note: impl Into<Option<musical_note::ResolvedNote>>,
        frame_length: impl Into<Option<usize>>,
    ) -> Result<&Vec<u8>, anyhow::Error> {
        if self.midi_envelope.is_some() {
            return Ok(self.midi_envelope.as_ref().unwrap());
        }
        let lower_note: Option<musical_note::ResolvedNote> = lower_note.into();
        let upper_note: Option<musical_note::ResolvedNote> = upper_note.into();
        let fmin = match lower_note {
            None => None,
            Some(note) => Some(midi_to_hz(&[note.midi as f32])[0]),
        };
        let fmax = match upper_note {
            None => None,
            Some(note) => Some(midi_to_hz(&[note.midi as f32])[0]),
        };
        let pitch_envelope = self.compute_pitch_envelope(sr, fmin, fmax, frame_length)?;
        let midi_envelope: Vec<u8> = hz_to_midi(pitch_envelope.as_slice())
            .iter()
            .map(|m| m.round() as u8)
            .collect();
        self.midi_envelope = Some(midi_envelope);
        Ok(self.midi_envelope.as_ref().unwrap())
    }

    pub fn estimate_root_note(
        &mut self,
        sr: impl Into<Option<u32>>,
        lower_note: impl Into<Option<musical_note::ResolvedNote>>,
        upper_note: impl Into<Option<musical_note::ResolvedNote>>,
        frame_length: impl Into<Option<usize>>,
    ) -> Result<ResolvedNote, anyhow::Error> {
        let midi_envelope = self.compute_midi_envelope(sr, lower_note, upper_note, frame_length)?;
        let filtered: Vec<u8> = midi_envelope
            .into_iter()
            .filter(|m| *m > &0)
            .map(|v| v.clone())
            .collect();
        let mode = mode(&filtered).unwrap_or(&0);
        let note = midi_to_note(*mode, Key::chromatic(), None);
        Ok(note)
    }

    pub fn estimate_legato_interval(
        &mut self,
        sr: impl Into<Option<u32>> + Clone,
        lower_note: impl Into<Option<musical_note::ResolvedNote>>,
        upper_note: impl Into<Option<musical_note::ResolvedNote>>,
        frame_length: impl Into<Option<usize>>,
    ) -> Result<LegatoInterval, anyhow::Error> {
        let sr = self.unwrap_sr(sr);
        let frame_length = self.unwrap_frame_length(frame_length);
        let hop_length = match frame_length {
            None => None,
            Some(l) => Some(l / 4),
        };
        debug!("sr: {:?}, frame_length: {:?}", sr, frame_length);
        let midi_envelope = self.compute_midi_envelope(sr, lower_note, upper_note, frame_length)?;
        let filtered: Vec<u8> = midi_envelope
            .iter()
            .filter(|m| *m > &0)
            .map(|v| v.clone())
            .collect();
        let start_mode = mode(&filtered[..(filtered.len() / 4)]).unwrap_or(&0);
        let end_mode = mode(&filtered[(filtered.len() / 4 * 3)..]).unwrap_or(&0);
        debug!("midi_envelope: {:?}", midi_envelope);
        debug!("start and end modes: ({:?}, {:?})", start_mode, end_mode);
        let (mut start_frame, mut end_frame) = (0, 0);
        for (frame, midi) in midi_envelope.iter().enumerate() {
            if midi == start_mode {
                start_frame = frame;
            }
            if midi == end_mode {
                end_frame = frame;
                break;
            }
        }
        let positions = frames_to_time(&[start_frame, end_frame], Some(sr), hop_length);
        let (transition_start, transition_end) = (
            Position::from(Duration::from_secs_f32(positions[0])),
            Position::from(Duration::from_secs_f32(positions[1])),
        );
        debug!("start frame: {start_frame:?}, {transition_start:?}; end frame: {end_frame:?}, {transition_end:?}");
        Ok(LegatoInterval {
            start_note: midi_to_note(*start_mode, Key::chromatic(), None),
            end_note: midi_to_note(*end_mode, Key::chromatic(), None),
            transition_start,
            transition_end,
            transition_center: Position::from(
                (transition_start + transition_end).as_duration() / 2,
            ),
        })
    }
}

fn mode<T: Eq + Hash>(seq: &[T]) -> Option<&T> {
    seq.iter()
        .counts()
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(val, _)| val)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSourceInfo {
    pub filename: PathBuf,
    pub source_offset: SourceOffset,
    pub region_offset: Position,
    pub length: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackInfo {
    pub guid: String,
    pub name: String,
}
impl TrackInfo {
    fn compare_to_reaper_track(&self, track: &rea_rs::Track<Immutable>) -> bool {
        track.guid().to_string() == self.guid
    }
}
impl From<Track<'_, Immutable>> for TrackInfo {
    fn from(track: Track<Immutable>) -> Self {
        Self {
            guid: track.guid().to_string(),
            name: track.name().clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegatoInterval {
    pub start_note: ResolvedNote,
    pub end_note: ResolvedNote,
    pub transition_start: Position,
    pub transition_end: Position,
    pub transition_center: Position,
}
