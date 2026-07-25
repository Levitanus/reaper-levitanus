use rea_rs::{Immutable, Mutable, Reaper, Track, GUID};
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
