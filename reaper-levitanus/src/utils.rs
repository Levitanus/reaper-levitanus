use log::debug;
use rea_rs::{
    AutomationMode, GenericSend, GenericSendMut, HardwareSend, Immutable, Mutable, Pan, PanLaw,
    ProbablyMutable, Reaper, SendDestChannels, SendIntType, SendMIDIProps, SendMode,
    SendSourceChannels, SendType, Track, TrackReceive, TrackSend, Volume, GUID,
};
use serde::{Deserialize, Serialize};

use crate::LevitanusError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CachedTrack {
    pub index: usize,
    pub guid: String,
}
impl CachedTrack {
    /// The simplest way to construct a track witout overhead
    pub fn new(index: usize, guid: GUID) -> Self {
        Self {
            index,
            guid: guid.to_string(),
        }
    }
    pub fn from_index(index: usize) -> Option<Self> {
        let guid = Reaper::get()
            .current_project()
            .get_track(index)?
            .guid()
            .to_string();
        Some(Self { index, guid })
    }
    pub fn from_guid(guid: GUID) -> Option<Self> {
        let index = Reaper::get()
            .current_project()
            .iter_tracks()
            .find(|track| track.guid() == guid)?
            .index();
        Some(Self {
            index,
            guid: guid.to_string(),
        })
    }
    pub fn validate(&mut self) -> Result<(), LevitanusError> {
        let rpr = Reaper::get();
        let pr = rpr.current_project();
        if let Some(track) = pr.get_track(self.index) {
            if track.guid().to_string() == self.guid {
                return Ok(());
            }
        }
        if let Some(track) = pr
            .iter_tracks()
            .find(|track| track.guid().to_string() == self.guid)
        {
            self.index = track.index();
            return Ok(());
        }
        Err(LevitanusError::TrackValidationError(self.guid.clone()))
    }

    /// Work with the mutable track using a fallible callback.
    pub fn with_reaper_track<F, T>(&mut self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(Track<Immutable>) -> anyhow::Result<T>,
    {
        self.validate()?;
        f(Reaper::get()
            .current_project()
            .get_track(self.index)
            .expect("there is no track at the given index"))
    }

    /// Work with the mutable track using a fallible callback.
    pub fn with_reaper_track_mut<F, T>(&mut self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(Track<Mutable>) -> anyhow::Result<T>,
    {
        self.validate()?;
        f(Reaper::get()
            .current_project()
            .get_track_mut(self.index)
            .expect("there is no track at the given index"))
    }
}
impl From<Track<'_, Immutable>> for CachedTrack {
    fn from(value: Track<'_, Immutable>) -> Self {
        Self {
            index: value.index(),
            guid: value.guid().to_string(),
        }
    }
}
impl From<Track<'_, Mutable>> for CachedTrack {
    fn from(value: Track<'_, Mutable>) -> Self {
        Self {
            index: value.index(),
            guid: value.guid().to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CachedSend {
    index: usize,
    send_type: SendType,
    source_track: Option<CachedTrack>,
    dest_track: Option<CachedTrack>,
    source_channels: Option<SendSourceChannels>,
    dest_channels: Option<SendDestChannels>,
    midi_properties: Option<SendMIDIProps>,
    volume: Volume,
    pan: Pan,
    pan_law: PanLaw,
    mute: bool,
    mono: bool,
    send_mode: SendMode,
    phase_flipped: bool,
    automation_mode: AutomationMode,
}
impl CachedSend {
    pub fn apply_to_track(self, track: &mut Track<Mutable>) -> anyhow::Result<()> {
        match self.send_type {
            SendType::HardwareSend => track.add_hardware_send().apply_cache(self),
            SendType::Receive => {
                if let Some(mut source) = self.source_track.clone() {
                    source.with_reaper_track(|source| track.add_receive(&source).apply_cache(self))
                } else {
                    return Err(LevitanusError::Unexpected(
                        "There is no Source track on cached Recieve".to_string(),
                    )
                    .into());
                }
            }
            SendType::Send => {
                if let Some(mut dest) = self.dest_track.clone() {
                    dest.with_reaper_track(|dest| track.add_send(&dest).apply_cache(self))
                } else {
                    return Err(LevitanusError::Unexpected(
                        "There is no Destination track on cached Send".to_string(),
                    )
                    .into());
                }
            }
        }
    }
}

trait SendToCache<'a, T: ProbablyMutable + 'a>: SendIntType + Sized + GenericSend<'a, T> {
    fn cache(&'a self) -> CachedSend {
        debug!(
            "caching send {} from track {:?}",
            self.index(),
            self.parent_track().name()
        );
        let source_track = match self.source_track() {
            Some(tr) => Some(CachedTrack {
                index: tr.index(),
                guid: tr.guid().to_string(),
            }),
            None => None,
        };
        let dest_track = match self.dest_track() {
            Some(tr) => Some(CachedTrack {
                index: tr.index(),
                guid: tr.guid().to_string(),
            }),
            None => None,
        };
        CachedSend {
            index: self.index(),
            send_type: self.send_type(),
            source_track,
            dest_track,
            source_channels: self.source_channels(),
            dest_channels: self.dest_channels(),
            midi_properties: self.midi_properties(),
            volume: self.volume(),
            pan: self.pan(),
            pan_law: self.pan_law(),
            mute: self.is_mute(),
            mono: self.is_mono(),
            send_mode: self.send_mode(),
            phase_flipped: self.phase_flipped(),
            automation_mode: self.automation_mode(),
        }
    }
}
impl<'a, T: ProbablyMutable + 'a> SendToCache<'a, T> for TrackSend<'a, T> {}
impl<'a, T: ProbablyMutable + 'a> SendToCache<'a, T> for TrackReceive<'a, T> {}
impl<'a, T: ProbablyMutable + 'a> SendToCache<'a, T> for HardwareSend<'a, T> {}

trait CacheToSend<'a>: SendIntType + Sized + GenericSendMut<'a> {
    fn apply_cache(&mut self, cache: CachedSend) -> anyhow::Result<()> {
        debug!(
            "applying send cache {:?}\nto track {:?}",
            cache,
            self.parent_track().name()
        );
        self.set_automation_mode(cache.automation_mode)?;
        if let Some(dest_channels) = cache.dest_channels {
            self.set_dest_channels(dest_channels)?;
        }
        self.set_source_channels(cache.source_channels)?;
        self.set_midi_properties(cache.midi_properties)?;
        self.set_mono(cache.mono)?;
        self.set_mute(cache.mute)?;
        self.set_pan(cache.pan)?;
        self.set_pan_law(cache.pan_law)?;
        self.set_phase(cache.phase_flipped)?;
        self.set_send_mode(cache.send_mode)?;
        self.set_volume(cache.volume)?;

        Ok(())
    }
}
impl<'a> CacheToSend<'a> for TrackSend<'a, Mutable> {}
impl<'a> CacheToSend<'a> for TrackReceive<'a, Mutable> {}
impl<'a> CacheToSend<'a> for HardwareSend<'a, Mutable> {}

pub(crate) fn create_send(source: &mut CachedTrack, dest: &mut CachedTrack) -> anyhow::Result<()> {
    dest.validate()?;
    source.validate()?;
    let rpr = Reaper::get();
    let pr = rpr.current_project();
    let rendered = pr
        .get_track(source.index)
        .ok_or(LevitanusError::Unexpected("No track at given index".into()))?;
    let bus = pr
        .get_track(dest.index)
        .ok_or(LevitanusError::Unexpected("No track at given index".into()))?;
    TrackSend::create_new(&rendered, &bus);
    Ok(())
}

pub(crate) fn cache_and_remove_track_sends(track: &mut Track<Mutable>) -> Vec<CachedSend> {
    let mut sends = Vec::new();
    debug!("caching track sends");
    for idx in (0..track.n_sends()).rev() {
        let send = match track.get_send(idx) {
            Some(s) => s,
            None => break,
        };
        sends.push(send.cache());
        debug!(
            "cached {:#?}\n deleting",
            sends.last().expect("no send is pushed")
        );
        let _ = send.delete();
    }
    debug!("caching track receives");
    for idx in (0..track.n_receives()).rev() {
        let send = match track.get_recieve(idx) {
            Some(s) => s,
            None => break,
        };
        sends.push(send.cache());
        debug!(
            "cached {:#?}\n deleting",
            sends.last().expect("no send is pushed")
        );
        let _ = send.delete();
    }
    debug!("caching track hardware sends");
    for idx in (0..track.n_hardware_sends()).rev() {
        let send = match track.get_hardware_send(idx) {
            Some(s) => s,
            None => break,
        };
        sends.push(send.cache());
        debug!(
            "cached {:#?}\n deleting",
            sends.last().expect("no send is pushed")
        );
        let _ = send.delete();
    }
    sends
}
