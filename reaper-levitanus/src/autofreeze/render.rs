use anyhow::Error;
use log::debug;
use rea_rs::{
    ActionHook, BoundsMode, EnvelopePoint, ExtState, FXParent, FullRenderSettings, Item, Position,
    Reaper, RenderFormat, RenderMode, RenderNormalize, RenderSettings, RenderTail, Source, FX,
};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, time::Duration};

use crate::{
    autofreeze::{
        load_default_state,
        track_management::{BGRenderTrack, TrackRole},
        RenderedInstrument, EXT_SECTION,
    },
    LevitanusError,
};
const FEEZED_AUDIO_ITEM_KEY: &'static str = "freezed_take_guid";
const FREEZEDITEM_KEY: &'static str = "FreezedItem";

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, Hash)]
struct FreezedItem {
    instrument: RenderedInstrument,
    item_guid: String,
    take_guid: String,
    midi_hash: String,
    filename: Option<PathBuf>,
    filename_mask: String,
    bounds: (Position, Position),
}
impl FreezedItem {
    /// Safety: caller has to validate that Item belongs to Instrument track
    fn from_reaper_item(item: Item) -> Result<Self, Error> {
        let uuid = match item.track()?.belongs_to_bgr() {
            None => {
                return Err(
                    LevitanusError::Unexpected("Should be Instrument track".to_string()).into(),
                );
            }
            Some((uuid, role)) => {
                if role != TrackRole::Instrument {
                    return Err(LevitanusError::Unexpected(
                        "Should be Instrument track".to_string(),
                    )
                    .into());
                };
                uuid
            }
        };

        let take = item.active_take()?;

        let instrument = RenderedInstrument::from_uuid(uuid)?.ok_or(LevitanusError::Unexpected(
            "should be valid instrument".to_string(),
        ))?;

        let item_guid = item.guid()?.to_string();
        let take_guid = take.guid()?.to_string();

        let bounds = (item.position()?, item.end_position()?);

        let filename_mask = format!(
            "{} {}",
            item.track()?.name()?,
            take_guid.replace("{", "").replace("}", "")
        );

        let obj =
            ExtState::load_value(EXT_SECTION, FREEZEDITEM_KEY, &item, None)?.unwrap_or(Self {
                instrument,
                item_guid,
                take_guid,
                midi_hash: take.midi_hash(false, None)?.unwrap_or("".to_string()),
                filename: None,
                filename_mask,
                bounds,
            });

        Ok(obj)
    }

    fn get_freeze_filename(&self) -> anyhow::Result<PathBuf> {
        match &self.filename {
            Some(f) => Ok(f.clone()),
            None => {
                let state = load_default_state()?;
                Ok(state
                    .freeze_directory()?
                    .join(&self.filename_mask)
                    .with_extension(state.render_format.extension()))
            }
        }
    }

    fn get_reaper_item(&mut self) -> Result<Item, Error> {
        self.instrument.instrument.with_reaper_track(|track| {
            let mut index = 0;
            while let Some(item) = track.get_item(index)? {
                index += 1;
                if item.guid()?.to_string() == self.item_guid {
                    return Ok(item);
                }
            }
            Err(LevitanusError::InvalidObject.into())
        })
    }

    fn get_freezed_audio_item(&mut self) -> Result<Option<Item>, Error> {
        let filename = self.get_freeze_filename()?;

        if !filename.exists() {
            return Ok(None);
        }

        let audio_item = self.instrument.rendered.with_reaper_track(|track| {
            let mut index = 0;
            while let Some(item) = track.get_item(index)? {
                index += 1;
                let Some(guid) = ExtState::<String, Item>::load_value(
                    EXT_SECTION,
                    FEEZED_AUDIO_ITEM_KEY,
                    &item,
                    None,
                )?
                else {
                    continue;
                };

                if guid == self.item_guid {
                    return Ok(Some(item));
                }
            }

            Ok(None)
        })?;

        Ok(audio_item)
    }

    fn freeze(&mut self, restore_render_settings: bool) -> Result<bool, Error> {
        if let Some(audio_item) = self.get_freezed_audio_item()? {
            audio_item.delete()?;
        }
        let filename = self.get_freeze_filename()?;
        if filename.exists() {
            fs::remove_file(filename.clone())?;
        }

        let rpr = Reaper::get();
        let mut pr = rpr.current_project();
        let state = load_default_state()?;
        let item = self.get_reaper_item()?;
        let take = item.active_take()?;
        self.take_guid = take.guid()?.to_string();
        self.midi_hash = take
            .midi_hash(false, None)?
            .ok_or(LevitanusError::Unexpected(
                "Should be valid midi item".into(),
            ))?;

        self.bounds = (item.position()?, item.end_position()?);
        let old_render_settings = match restore_render_settings {
            false => None,
            true => Some(pr.get_full_render_settings()?),
        };
        let selected_tracks: Vec<usize> = match restore_render_settings {
            true => pr
                .iter_selected_tracks()
                .map(|track| track.index().expect("schould be valid track"))
                .collect(),
            false => Vec::new(),
        };

        self.instrument.instrument.with_reaper_track(|track| {
            track.make_only_selected_track()?;

            for mut fx in track.iter_fx() {
                fx.set_online(true)?;
                fx.set_enabled(true)?;
            }

            Ok(())
        })?;

        pr.apply_full_render_settings(&bounds_stem_render_settings(
            filename.clone(),
            self.bounds,
            state.render_tail,
            state.render_format,
        ))?;
        rpr.update_timeline();
        drop(pr);
        rpr.perform_action(42230, 0, None);

        let rpr = Reaper::get();
        let mut pr = rpr.current_project();

        if !filename.exists() {
            log::error!("FreezedItem::freeze: render is aborted, no valid file after render");
            return Ok(false);
        }

        if let Some(settings) = old_render_settings {
            pr.apply_full_render_settings(&settings)?;
        }
        if !selected_tracks.is_empty() {
            pr.select_all_tracks(false)?;
            for idx in selected_tracks {
                if let Some(mut tr) = pr.get_track(idx)? {
                    tr.set_selected(true)?;
                }
            }
        }

        let item = self.get_reaper_item()?;
        ExtState::<FreezedItem, Item>::existing(EXT_SECTION, FREEZEDITEM_KEY, true, &item, None)
            .set(self.clone())?;
        self.instrument.rendered.with_reaper_track(|mut track| {
            let mut item =
                track.add_item(self.bounds.0, self.bounds.1 + state.render_tail.tail.into())?;
            let mut take = item.add_take()?;
            let source = Source::create_from_file(filename, false)?;
            take.set_source(source)?;
            // item.set_ext_value(EXT_SECTION, FEEZED_AUDIO_ITEM_KEY, self.take_guid.clone())?;
            ExtState::new(
                EXT_SECTION,
                FEEZED_AUDIO_ITEM_KEY,
                self.take_guid.clone(),
                true,
                &item,
                None,
            )?;
            Ok(())
        })?;

        // Iterate through all FX on the instrument track and manipulate the "Bypass" envelope
        self.instrument.instrument.with_reaper_track(|track| {
            for mut fx in track.iter_fx() {
                // Look for the "Bypass" parameter in each FX

                for param in fx.iter_params() {
                    if param.name()? != "Bypass" {
                        continue;
                    }
                    // Get the envelope for the Bypass parameter
                    let Some(mut envelope) = param.envelope(true)? else {
                        continue;
                    };
                    // Remove all points within bounds
                    let mut points_to_remove = Vec::new();
                    for i in 0..envelope.n_points()? {
                        let point = envelope.get_point(i)?;
                        if point.position >= self.bounds.0 && point.position <= self.bounds.1 {
                            points_to_remove.push(i);
                        }
                    }

                    // Remove points in reverse order to avoid index shifting issue
                    for &index in points_to_remove.iter().rev() {
                        envelope.delete_point(index)?;
                    }
                    envelope.insert_point(
                        EnvelopePoint::new(
                            self.bounds.0,
                            1.0,
                            rea_rs::EnvelopePointShape::Square,
                            0.0,
                            false,
                        ),
                        false,
                    )?;
                    envelope.insert_point(
                        EnvelopePoint::new(
                            self.bounds.1,
                            0.0,
                            rea_rs::EnvelopePointShape::Square,
                            0.0,
                            false,
                        ),
                        false,
                    )?;
                    envelope.sort_points()?;
                }

                fx.set_enabled(false)?;
            }

            Ok(())
        })?;

        Ok(true)
    }
}

fn bounds_stem_render_settings(
    filename: PathBuf,
    bounds: (Position, Position),
    tail: impl Into<Option<RenderTail>>,
    format: RenderFormat,
) -> FullRenderSettings {
    let mut settings = FullRenderSettings::default();
    settings.directory = Some(
        filename
            .parent()
            .expect("shold be full path with valid directory")
            .to_path_buf(),
    );
    settings.file = Some(
        filename
            .file_name()
            .expect("should be valid filename")
            .to_string_lossy()
            .to_string(),
    );
    settings.bounds = Some(bounds);
    settings.bounds_mode = Some(BoundsMode::Custom);
    settings.tail = tail.into();
    let mut base_settings = RenderSettings::new(RenderMode::Stems);
    base_settings.multichannel_tracks_to_multichannel_files = true;
    base_settings.use_mono = true;
    base_settings.pre_fader_stems = true;
    settings.settings = Some(base_settings);
    settings.fade_in = Some(Duration::default());
    settings.fade_out = Some(Duration::default());
    settings.pad_end = Some(Duration::default());
    settings.pad_start = Some(Duration::default());
    settings.srate = Some(None);
    let mut normalize = RenderNormalize::default();
    normalize.disable_all_postprocessing = true;
    settings.normalize = Some(normalize);
    settings.primary_format = Some(format);
    settings.secondary_format = None;

    settings
}

pub fn action_freeze_selected_items(_hool: &mut ActionHook) -> anyhow::Result<()> {
    let res = freeze_selected_items();
    debug!("render finished, returning result");
    res
}

pub fn freeze_selected_items() -> Result<(), Error> {
    debug!("freeze_selected_items: starting function");
    let rpr = Reaper::get();
    let mut pr = rpr.current_project();
    debug!("freeze_selected_items: getting current render settings");
    let old_render_settings = pr.get_full_render_settings()?;
    debug!("olde render settings: {:#?}", old_render_settings);
    let mut freezed_items = Vec::new();
    debug!("freeze_selected_items: collecting selected items");
    for item in pr.iter_selected_items() {
        debug!("freeze_selected_items: processing item");
        freezed_items.push(FreezedItem::from_reaper_item(item)?);
    }
    debug!(
        "freeze_selected_items: collected {} items",
        freezed_items.len()
    );
    let selected_tracks: Vec<usize> = pr
        .iter_selected_tracks()
        .map(|track| track.index().expect("schould be valid track"))
        .collect();
    debug!(
        "freeze_selected_items: collected {} selected tracks",
        selected_tracks.len()
    );

    // debug!("freeze_selected_items: starting to freeze items");
    // for (index, mut item) in freezed_items.into_iter().enumerate() {
    //     debug!("freeze_selected_items: freezing item {}", index);
    //     if !item.freeze(false)? {
    //         debug!("freeze_selected_items: freeze returned false, breaking");
    //         break;
    //     }
    //     debug!("freeze_selected_items: finished freezing item {}", index);
    // }
    // debug!("freeze_selected_items: finished freezing all items");
    // std::thread::sleep(Duration::from_secs(5));
    // debug!("freeze_selected_items: restoring render settings");
    // let mut pr = rpr.current_project();
    pr.set_render_format(RenderFormat::WavePack, false)?;
    pr.apply_full_render_settings(&old_render_settings)?;
    return Err(LevitanusError::Render("upplied old render settings".into()).into());
    debug!("freeze_selected_items: clearing track selection");
    pr.select_all_tracks(false)?;
    debug!("freeze_selected_items: restoring track selection");
    for idx in selected_tracks {
        if let Some(mut tr) = pr.get_track(idx)? {
            tr.set_selected(true)?;
        }
    }
    debug!("freeze_selected_items: function completed successfully");

    Ok(())
}
