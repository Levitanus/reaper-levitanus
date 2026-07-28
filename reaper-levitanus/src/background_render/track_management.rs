use std::{
    collections::{HashMap, HashSet},
    error::Error,
    ffi::CString,
};

use log::debug;
use rea_rs::{
    ActionHook, ExtState, FXMut, FXParent, HasExtState, Mutable, Project, Reaper, Track, UndoFlags,
    FX,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    background_render::{RenderedInstrument, EXT_SECTION, ROLE_KEY, UUID_KEY},
    utils::{cache_and_remove_track_sends, create_send, CachedTrack},
    LevitanusError,
};

trait BGRenderTrack
where
    Self: HasExtState + Sized,
{
    /// If Track belongs to BGR, returns uuid and role
    fn belongs_to_bgr(&self) -> Option<(u128, TrackRole)> {
        if let Some(uid) = ExtState::load_value(EXT_SECTION, UUID_KEY, self, None).unwrap_or(None) {
            if let Some(role) =
                ExtState::load_value(EXT_SECTION, ROLE_KEY, self, None).unwrap_or(None)
            {
                return Some((uid, role));
            }
        }
        None
    }
}
impl<'a> BGRenderTrack for Track<'a, Mutable> {}

pub(crate) fn rebuild_instrument_list(
) -> anyhow::Result<(Vec<RenderedInstrument>, Vec<(u128, TrackRole, CachedTrack)>)> {
    let rpr = Reaper::get_mut();
    let mut pr = rpr.current_project();
    let mut buses = HashMap::new();
    let mut rendered = HashMap::new();
    let mut instruments = HashMap::new();
    pr.iter_tracks_mut(|track| {
        if let Some((uid, role)) = track.belongs_to_bgr() {
            match role {
                TrackRole::Bus => buses.insert(uid, CachedTrack::from(track)),
                TrackRole::Instrument => instruments.insert(uid, CachedTrack::from(track)),
                TrackRole::Rendered => rendered.insert(uid, CachedTrack::from(track)),
            };
        }

        Ok(())
    })?;

    let mut collected_instruments = Vec::new();
    let mut unmatched_buses = HashMap::new();
    for (uuid, bus) in buses {
        if let (Some(rendered), Some(instrument)) =
            (rendered.remove(&uuid), instruments.remove(&uuid))
        {
            collected_instruments.push(RenderedInstrument {
                uuid,
                bus,
                rendered,
                instrument,
            });
        } else {
            unmatched_buses.insert(uuid, bus);
        }
    }
    debug!("instruments list is built");
    debug!("collected_instruments: {:#?}", collected_instruments);
    debug!("unmatched_buses: {:#?}", unmatched_buses);
    debug!("rendered: {:#?}", rendered);
    debug!("instruments: {:#?}", instruments);

    let mut unpaired_tracks = unmatched_buses
        .into_iter()
        .map(|(uuid, track)| (uuid, TrackRole::Bus, track))
        .collect::<Vec<_>>();
    unpaired_tracks.extend(
        rendered
            .into_iter()
            .map(|(uuid, track)| (uuid, TrackRole::Rendered, track)),
    );
    unpaired_tracks.extend(
        instruments
            .into_iter()
            .map(|(uuid, track)| (uuid, TrackRole::Instrument, track)),
    );

    Ok((collected_instruments, unpaired_tracks))
}

pub(crate) fn resolve_unpaired_tracks(
    unpaired_tracks: &mut Vec<(u128, TrackRole, CachedTrack)>,
    rpr: &mut Reaper,
    mut pr: Project,
) -> Result<(), anyhow::Error> {
    pr.begin_undo_block();
    debug!("resolving unpaired tracks: {:#?}", unpaired_tracks);
    match rpr.show_message_box(
            "Missing tracks",
            "There are tracks, that were used for BackgroundRenderer, but now some \
                         tracks are missing.\n\n\
                         Do you want to restore the track structure (Yes) or delete the rest (No)?\n\n\
                         If you cancel the dialog, you'll be prompted for each track.",
                rea_rs::MessageBoxType::YesNoCancel
            )? {
                rea_rs::MessageBoxValue::Yes => {
                    for (uuid, _, _) in unpaired_tracks.iter_mut() {
                        restore_instrument_structure(*uuid)?;
                        align_instrument_track_order_by_uuid(*uuid)?;
                    }
                }
                rea_rs::MessageBoxValue::No => {
                    for (_, _, track) in unpaired_tracks.iter_mut() {
                        delete_track(track)?;
                    }
                }
                _ => {
                    for (uuid, role, track) in unpaired_tracks.iter_mut(){
                        let name = track.with_reaper_track_mut(|track|{
                            let name = track.name().clone();
                            match role{
                                TrackRole::Instrument => Ok(name),
                                TrackRole::Bus => Ok(name.strip_suffix(" bus").unwrap_or(&name).to_string()),
                                TrackRole::Rendered => Ok(name.strip_suffix(" rendered").unwrap_or(&name).to_string()),
                            }
                        })?;
                        match rpr.show_message_box(
                            "Missing tracks", 
                            format!("Do you want to restore track structure for {} (Yes),\
                                    delete it (No),\
                                    or exclude from BackroundRender (Cancel)?", 
                            name), rea_rs::MessageBoxType::YesNoCancel)?{
                            rea_rs::MessageBoxValue::Yes => {
                                restore_instrument_structure(*uuid)?;
                                align_instrument_track_order_by_uuid(*uuid)?;
                            }
                            rea_rs::MessageBoxValue::No => delete_track(track)?,
                            _ => forget_track(track)?
                        }
                    }
                },
            }
    unpaired_tracks.clear();
    pr.end_undo_block("resolve unpaired BackgroudRenderer track", UndoFlags::all());
    Ok(())
}

fn move_track_before(track: &mut CachedTrack, before_index: usize) -> anyhow::Result<()> {
    let selected_guids = {
        let pr = Reaper::get().current_project();
        pr.iter_selected_tracks()
            .map(|track| track.guid().to_string())
            .collect::<HashSet<_>>()
    };

    track.validate()?;
    track.with_reaper_track_mut(|track| {
        track.make_only_selected_track();
        Ok(())
    })?;
    let moved = Reaper::get()
        .low()
        .ReorderSelectedTracks(before_index as i32, 0);
    if !moved {
        return Err(LevitanusError::Reaper("Failed to reorder selected tracks".to_string()).into());
    }

    if !selected_guids.is_empty() {
        let mut pr = Reaper::get_mut().current_project();
        pr.iter_tracks_mut(|mut track| {
            track.set_selected(false)?;
            Ok(())
        })?;
        pr.iter_tracks_mut(|mut track| {
            if selected_guids.contains(&track.guid().to_string()) {
                track.set_selected(true)?;
            }
            Ok(())
        })?;
    }

    track.validate()?;
    Ok(())
}

fn align_instrument_track_order(instrument: &mut RenderedInstrument) -> anyhow::Result<()> {
    instrument.instrument.validate()?;
    instrument.rendered.validate()?;
    instrument.bus.validate()?;

    let instrument_index = instrument.instrument.index;
    if instrument.rendered.index + 1 != instrument_index {
        move_track_before(&mut instrument.rendered, instrument_index)?;
        instrument.instrument.validate()?;
    }

    let rendered_index = instrument.rendered.index;
    if instrument.bus.index + 1 != rendered_index {
        move_track_before(&mut instrument.bus, rendered_index)?;
    }

    instrument.instrument.validate()?;
    instrument.rendered.validate()?;
    instrument.bus.validate()?;
    Ok(())
}

pub(crate) fn align_all_instrument_track_orders(
    instruments: &mut [RenderedInstrument],
) -> anyhow::Result<()> {
    for instrument in instruments.iter_mut() {
        align_instrument_track_order(instrument)?;
    }
    Ok(())
}

fn align_instrument_track_order_by_uuid(uuid: u128) -> anyhow::Result<()> {
    let rpr = Reaper::get_mut();
    let mut pr = rpr.current_project();
    let mut instrument = None;
    let mut bus = None;
    let mut rendered = None;
    pr.iter_tracks_mut(|track| {
        if let Some((track_uuid, track_role)) = track.belongs_to_bgr() {
            if track_uuid != uuid {
                return Ok(());
            }
            match track_role {
                TrackRole::Bus => bus = Some(CachedTrack::from(track)),
                TrackRole::Instrument => instrument = Some(CachedTrack::from(track)),
                TrackRole::Rendered => rendered = Some(CachedTrack::from(track)),
            }
        }
        Ok(())
    })?;

    if let (Some(instrument), Some(rendered), Some(bus)) = (instrument, rendered, bus) {
        let mut set = RenderedInstrument {
            uuid,
            bus,
            rendered,
            instrument,
        };
        align_instrument_track_order(&mut set)?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum TrackRole {
    Instrument = 0,
    Rendered = 1,
    Bus = 2,
}

pub fn create_bg_instrument(_: &mut ActionHook) -> Result<(), Box<dyn Error>> {
    let rpr = Reaper::get_mut();
    static KEY: &str = "Instrument";
    if let Ok(values) = rpr.get_user_inputs("Enter Instrument name", vec![KEY], 512) {
        let instrument_name = values.get(KEY).unwrap_or(&KEY.to_string()).to_owned();
        let uid = Uuid::new_v4();
        let mut pr = rpr.current_project();
        pr.begin_undo_block();
        let index = match pr.get_selected_track(0) {
            Some(track) => track.index() + 1,
            None => pr.n_tracks(),
        };
        let instrument = create_instrument_track(uid.as_u128(), index, instrument_name, None)?;
        let bus = create_bus_track(uid.as_u128(), instrument.clone(), None)?;
        let _ = create_rendered_track(uid.as_u128(), instrument, Some(bus))?;
        pr.end_undo_block("Create BackgroundRenderer Instrument", UndoFlags::all());
    }
    Ok(())
}

pub fn make_track_rendered(_: &mut ActionHook) -> Result<(), Box<dyn Error>> {
    let rpr = Reaper::get_mut();
    let mut pr = rpr.current_project();
    pr.begin_undo_block();
    let mut track = match pr.get_selected_track_mut(0) {
        Some(track) => track,
        None => {
            log::error!("no selected track for adding to background render");
            return Ok(());
        }
    };
    let sends = cache_and_remove_track_sends(&mut track);

    let uid = Uuid::new_v4().as_u128();
    create_bgr_ext_state(uid, &track, TrackRole::Instrument);
    let cached_instr = CachedTrack {
        index: track.index(),
        guid: track.guid().to_string(),
    };
    let mut bus = create_bus_track(uid, cached_instr.clone(), None)?;
    let _rendered = create_rendered_track(uid, cached_instr, Some(bus.clone()))?;
    debug!("transfering fx from instrument to bus");
    bus.with_reaper_track_mut(|mut bus_track| {
        let mut idx = 0;
        while let Some(fx) = track.get_fx_mut(idx) {
            if fx.is_instrument() {
                idx += 1;
                continue;
            }
            let bus_idx = bus_track.n_fx();
            fx.move_to_track(&mut bus_track, bus_idx);
        }
        debug!("applying sends to bus");
        for send in sends {
            send.apply_to_track(&mut bus_track)?;
        }
        Ok(())
    })?;

    pr.end_undo_block("add track to BackgroundRenderer", UndoFlags::all());
    Ok(())
}

fn create_instrument_track(
    uid: u128,
    index: usize,
    instrument_name: String,
    bus: Option<CachedTrack>,
) -> anyhow::Result<CachedTrack> {
    debug!(
        "creating instrument track for BackgroundRenderer {}\nwith uuid {}",
        instrument_name, uid
    );
    let mut pr = Reaper::get().current_project();
    let mut instr = pr.add_track(index, instrument_name.clone());
    instr.set_parent_send(None)?;
    create_bgr_ext_state(uid, &instr, TrackRole::Instrument);
    let mut instrument_track = CachedTrack::from(instr);
    if let Some(mut bus) = bus {
        create_send(&mut instrument_track, &mut bus)?;
    }
    Ok(instrument_track)
}

fn create_rendered_track(
    uid: u128,
    mut instrument: CachedTrack,
    bus: Option<CachedTrack>,
) -> anyhow::Result<CachedTrack> {
    debug!(
        "creating rendered track for BackgroundRenderer instrument {:#?}\nwith uuid {}",
        instrument, uid
    );
    let mut rendered_track = instrument.with_reaper_track_mut(|track| {
        let index = track.index();
        let instrument_name = track.name();
        let mut pr = Reaper::get().current_project();
        let mut rndr = pr.add_track(index, instrument_name.clone() + " rendered");
        rndr.set_visible_in_tcp(false)?;
        rndr.set_visible_in_mcp(false)?;
        rndr.set_parent_send(None)?;
        create_bgr_ext_state(uid, &rndr, TrackRole::Rendered);
        Ok(CachedTrack::from(rndr))
    })?;
    if let Some(mut bus) = bus {
        create_send(&mut rendered_track, &mut bus)?;
    }
    Ok(rendered_track)
}

fn create_bus_track(
    uid: u128,
    mut instrument: CachedTrack,
    rendered: Option<CachedTrack>,
) -> anyhow::Result<CachedTrack> {
    debug!(
        "creating bus track for BackgroundRenderer instrument {:#?}\nwith uuid {}",
        instrument, uid
    );
    let mut bus_track = instrument.with_reaper_track_mut(|track| {
        let index = track.index();
        let instrument_name = track.name();
        let mut pr = Reaper::get().current_project();
        let mut bus = pr.add_track(index, instrument_name.clone() + " bus");
        bus.set_visible_in_tcp(false)?;
        create_bgr_ext_state(uid, &bus, TrackRole::Bus);
        Ok(CachedTrack::from(bus))
    })?;
    create_send(&mut instrument, &mut bus_track)?;
    if let Some(mut rendered) = rendered {
        create_send(&mut rendered, &mut bus_track)?;
    }
    Ok(bus_track)
}

fn create_bgr_ext_state(uid: u128, track: &Track<'_, Mutable>, role: TrackRole) {
    debug!(
        "creating BackgroundRenderer ExtState for track {}, with role {:?} and uuid {}",
        track.name(),
        role,
        uid
    );
    ExtState::new(EXT_SECTION, UUID_KEY, uid, true, track, None);
    ExtState::new(EXT_SECTION, ROLE_KEY, role, true, track, None);
}

fn delete_track(track: &mut CachedTrack) -> anyhow::Result<()> {
    track.with_reaper_track_mut(|track| {
        debug!("deleting track {}", track.name());
        track.delete();
        Ok(())
    })
}

fn forget_track(track: &mut CachedTrack) -> anyhow::Result<()> {
    let section = CString::new(EXT_SECTION).unwrap();
    let uuid_key = CString::new(UUID_KEY).unwrap();
    let role_key = CString::new(ROLE_KEY).unwrap();
    track.with_reaper_track_mut(|track| {
        track.delete_ext_value(&section, &uuid_key);
        track.delete_ext_value(&section, &role_key);
        Ok(())
    })
}

fn restore_instrument_structure(uuid: u128) -> anyhow::Result<()> {
    let rpr = Reaper::get_mut();
    let mut pr = rpr.current_project();
    let mut instrument = None;
    let mut bus = None;
    let mut rendered = None;
    pr.iter_tracks_mut(|track| {
        if let Some(track_uuid) =
            ExtState::<u128, Track<Mutable>>::new(EXT_SECTION, UUID_KEY, None, true, &track, None)
                .get()?
        {
            if track_uuid != uuid {
                return Ok(());
            }
            match ExtState::<TrackRole, Track<Mutable>>::new(
                EXT_SECTION,
                ROLE_KEY,
                None,
                true,
                &track,
                None,
            )
            .get()?
            .ok_or(LevitanusError::Unexpected(
                "no ExtState for role on the backgroundrendered track".to_string(),
            ))? {
                TrackRole::Bus => bus = Some(CachedTrack::from(track)),
                TrackRole::Instrument => instrument = Some(CachedTrack::from(track)),
                TrackRole::Rendered => rendered = Some(CachedTrack::from(track)),
            }
        }
        Ok(())
    })?;

    if let Some(instrument) = instrument {
        if bus.is_none() {
            bus.replace(create_bus_track(
                uuid,
                instrument.clone(),
                rendered.clone(),
            )?);
        }
        if rendered.is_none() {
            create_rendered_track(uuid, instrument, bus)?;
        }
        return Ok(());
    }
    if let Some(mut bus) = bus {
        if instrument.is_none() {
            let instrument_name = bus.with_reaper_track_mut(|track| {
                let name = track.name().clone();
                let name = name.strip_suffix(" bus").unwrap_or(&name).to_string();
                Ok(name)
            })?;
            instrument.replace(create_instrument_track(
                uuid,
                bus.index,
                instrument_name.into(),
                Some(bus.clone()),
            )?);
        }
        if rendered.is_none() {
            create_rendered_track(
                uuid,
                instrument.expect("no instrument after creating one"),
                Some(bus),
            )?;
        }
        return Ok(());
    }
    if let Some(mut rendered) = rendered {
        if instrument.is_none() {
            let instrument_name = rendered.with_reaper_track_mut(|track| {
                let name = track.name().clone();
                let name = name.strip_suffix(" rendered").unwrap_or(&name).to_string();
                Ok(name)
            })?;
            instrument.replace(create_instrument_track(
                uuid,
                rendered.index,
                instrument_name.into(),
                bus.clone(),
            )?);
        }
        if bus.is_none() {
            bus.replace(create_bus_track(
                uuid,
                instrument.expect("no instrument after creating one"),
                Some(rendered),
            )?);
        }
    }

    Ok(())
}
