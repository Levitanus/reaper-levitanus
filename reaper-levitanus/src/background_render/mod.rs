use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    error::Error,
    ffi::CString,
    sync::{Arc, Mutex},
    time::Duration,
};

use rea_rs::{
    ptr_wrappers::MediaTrack, ActionHook, ControlSurface, ExtState, HasExtState, Immutable,
    Mutable, Project, Reaper, Timer, Track, TrackSend, UndoFlags, WithReaperPtr,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{utils::CachedTrack, LevitanusError};

const ID_STRING: &str = "BackgroudRenderer";
const TIMER_ID_STRING: &str = "BackgroudRendererTimer";
const EXT_SECTION: &str = "Levitanus_BackgroundRenderer";
const EXT_KEY: &str = "enabled";

const UUID_KEY: &str = "uuid";
const ROLE_KEY: &str = "role";

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
enum Task {
    RebuildInstrumentList = 0,
}

#[derive(Debug, Default)]
struct BackgroundRendererState {
    instruments: Vec<RenderedInstrument>,
    unpaired_tracks: Vec<(u128, TrackRole, CachedTrack)>,
    task_queue: Mutex<HashSet<Task>>,
    is_performing: bool,
}

#[derive(Debug)]
struct BackgroundRendererSurface {
    state: Arc<Mutex<BackgroundRendererState>>,
}

#[derive(Debug)]
struct BackgroundRendererTimer {
    state: Arc<Mutex<BackgroundRendererState>>,
}

impl BackgroundRendererState {
    fn queue_task(&self, task: Task) {
        if let Ok(mut queue) = self.task_queue.lock() {
            queue.insert(task);
        }
    }
}

fn rebuild_instrument_list(
) -> anyhow::Result<(Vec<RenderedInstrument>, Vec<(u128, TrackRole, CachedTrack)>)> {
    let rpr = Reaper::get_mut();
    let mut pr = rpr.current_project();
    let mut buses = HashMap::new();
    let mut rendered = HashMap::new();
    let mut instruments = HashMap::new();
    pr.iter_tracks_mut(|track| {
        if let Some(uid) =
            ExtState::<u128, Track<Mutable>>::new(EXT_SECTION, "uuid", None, true, &track, None)
                .get()?
        {
            if let Some(role) = ExtState::<TrackRole, Track<Mutable>>::new(
                EXT_SECTION,
                "role",
                None,
                true,
                &track,
                None,
            )
            .get()?
            {
                match role {
                    TrackRole::Bus => buses.insert(uid, CachedTrack::from(track)),
                    TrackRole::Instrument => instruments.insert(uid, CachedTrack::from(track)),
                    TrackRole::Rendered => rendered.insert(uid, CachedTrack::from(track)),
                };
            }
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

fn resolve_unpaired_tracks(
    unpaired_tracks: &mut Vec<(u128, TrackRole, CachedTrack)>,
    rpr: &mut Reaper,
    mut pr: Project,
) -> Result<(), anyhow::Error> {
    pr.begin_undo_block();
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
                        let name = track.with_reaper_track(|track|{
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

impl Timer for BackgroundRendererTimer {
    fn run(&mut self) -> Result<(), Box<dyn Error>> {
        let rpr = Reaper::get_mut();
        let pr = rpr.current_project();
        if !pr.is_stopped() {
            return Ok(());
        }

        let tasks: Vec<Task> = {
            let mut state = self
                .state
                .lock()
                .map_err(|e| LevitanusError::Poison(e.to_string()))?;
            state.is_performing = true;
            state
                .task_queue
                .lock()
                .map(|mut queue| queue.drain().collect())
                .unwrap_or_default()
        };

        for task in tasks {
            match task {
                Task::RebuildInstrumentList => {
                    let (mut instruments, unpaired_tracks) = rebuild_instrument_list()?;
                    align_all_instrument_track_orders(&mut instruments)?;
                    let mut state = self
                        .state
                        .lock()
                        .map_err(|e| LevitanusError::Poison(e.to_string()))?;
                    state.instruments = instruments;
                    state.unpaired_tracks = unpaired_tracks;
                }
            }
        }

        let mut local_unpaired_tracks = {
            let mut state = self
                .state
                .lock()
                .map_err(|e| LevitanusError::Poison(e.to_string()))?;
            std::mem::take(&mut state.unpaired_tracks)
        };

        if !local_unpaired_tracks.is_empty() {
            resolve_unpaired_tracks(&mut local_unpaired_tracks, rpr, pr)?;
        }

        let mut state = self
            .state
            .lock()
            .map_err(|e| LevitanusError::Poison(e.to_string()))?;
        state.is_performing = false;
        Ok(())
    }

    fn id_string(&self) -> String {
        TIMER_ID_STRING.to_string()
    }

    fn interval(&self) -> Duration {
        Duration::from_millis(100)
    }
}

fn move_track_before(track: &mut CachedTrack, before_index: usize) -> anyhow::Result<()> {
    let selected_guids = {
        let pr = Reaper::get().current_project();
        pr.iter_selected_tracks()
            .map(|track| track.guid().to_string())
            .collect::<HashSet<_>>()
    };

    track.validate()?;
    track.with_reaper_track(|track| {
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

fn align_all_instrument_track_orders(instruments: &mut [RenderedInstrument]) -> anyhow::Result<()> {
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

impl ControlSurface for BackgroundRendererSurface {
    fn run(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn get_type_string(&self) -> String {
        ID_STRING.to_string()
    }

    fn get_desc_string(&self) -> String {
        "background render control surface".to_string()
    }

    fn set_track_list_change(&self) -> anyhow::Result<()> {
        let state = self
            .state
            .lock()
            .map_err(|e| LevitanusError::Poison(e.to_string()))?;
        if !state.is_performing {
            state.queue_task(Task::RebuildInstrumentList);
        }
        Ok(())
    }
}

#[derive(Debug)]
struct RenderedInstrument {
    uuid: u128,
    bus: CachedTrack,
    rendered: CachedTrack,
    instrument: CachedTrack,
}

#[derive(Debug, Clone)]
struct SendSnapshot {
    source_guid: String,
    source_send_index: usize,
    dest_guid: String,
    mute: f64,
    phase: f64,
    mono: f64,
    volume: f64,
    pan: f64,
    pan_law: f64,
    send_mode: f64,
    automation_mode: f64,
    source_channels: f64,
    dest_channels: f64,
    midi_flags: f64,
}

pub fn load_default_state() -> bool {
    let rpr = Reaper::get();
    let ext_state: ExtState<bool, Reaper> =
        ExtState::new(EXT_SECTION, EXT_KEY, None, true, rpr, None);

    match ext_state.get() {
        Ok(Some(value)) => value,
        Ok(None) => false,
        Err(_) => false,
    }
}

pub fn save_default_state(enabled: bool) {
    let rpr = Reaper::get();
    let mut ext_state = ExtState::new(EXT_SECTION, EXT_KEY, Some(enabled), true, rpr, None);
    ext_state.set(enabled);
}

pub fn is_running() -> bool {
    let id = ID_STRING.to_string();
    Reaper::get().has_control_surface(&id)
}

pub fn set_enabled(enabled: bool) -> Result<(), Box<dyn Error>> {
    let rpr = Reaper::get_mut();
    let id = ID_STRING.to_string();
    let running = rpr.has_control_surface(&id);

    if enabled && !running {
        let state = Arc::new(Mutex::new(BackgroundRendererState {
            instruments: Vec::new(),
            unpaired_tracks: Vec::new(),
            task_queue: Mutex::new(HashSet::new()),
            is_performing: false,
        }));
        if let Ok(state) = state.lock() {
            state.queue_task(Task::RebuildInstrumentList);
        }
        let cs = BackgroundRendererSurface {
            state: state.clone(),
        };
        let timer = BackgroundRendererTimer { state };
        rpr.register_control_surface(Arc::new(RefCell::new(cs)));
        rpr.register_timer(Arc::new(RefCell::new(timer)));
    } else if !enabled && running {
        rpr.unregister_control_surface(id)?;
        rpr.unregister_timer(TIMER_ID_STRING.to_string())?;
    }

    save_default_state(enabled);
    Ok(())
}

pub fn restore_default_state() -> Result<bool, Box<dyn Error>> {
    let enabled = load_default_state();
    set_enabled(enabled)?;
    Ok(enabled)
}

pub fn toggle_action(hook: &mut ActionHook) -> Result<(), Box<dyn Error>> {
    let next_state = !is_running();
    set_enabled(next_state)?;
    hook.set_toggle_state(next_state);
    Ok(())
}

pub fn add_selected_track_to_background_renderer(_: &mut ActionHook) -> Result<(), Box<dyn Error>> {
    let rpr = Reaper::get_mut();
    let already_assigned = {
        let mut pr = rpr.current_project();
        let instrument = pr
            .get_selected_track_mut(0)
            .ok_or(LevitanusError::Unexpected(
                "Select a track first".to_string(),
            ))?;
        ExtState::<TrackRole, Track<Mutable>>::new(
            EXT_SECTION,
            ROLE_KEY,
            None,
            true,
            &instrument,
            None,
        )
        .get()?
        .is_some()
    };

    if already_assigned {
        return Err(LevitanusError::Unexpected(
            "Selected track is already assigned to BackgroundRenderer".to_string(),
        )
        .into());
    }

    let mut pr = rpr.current_project();
    pr.begin_undo_block();
    let result = (|| -> anyhow::Result<()> {
        let uid = Uuid::new_v4().as_u128();

        let mut pr = Reaper::get_mut().current_project();
        let instrument = pr
            .get_selected_track_mut(0)
            .ok_or(LevitanusError::Unexpected(
                "Select a track first".to_string(),
            ))?;

        ExtState::new(EXT_SECTION, UUID_KEY, uid, true, &instrument, None);
        ExtState::new(
            EXT_SECTION,
            ROLE_KEY,
            TrackRole::Instrument,
            true,
            &instrument,
            None,
        );
        let mut instrument_cached = CachedTrack::from(instrument);
        drop(pr);
        let snapshots = collect_related_send_snapshots(&instrument_cached.guid)?;

        let mut bus = create_bus_track(uid, instrument_cached.clone(), None)?;
        create_rendered_track(uid, instrument_cached.clone(), Some(bus.clone()))?;

        // Match default BackgroundRenderer instrument behavior.
        instrument_cached.with_reaper_track(|mut track| {
            track.set_parent_send(None)?;
            Ok(())
        })?;

        move_non_instrument_fx_to_bus(&mut instrument_cached, &mut bus)?;
        migrate_related_sends(&instrument_cached.guid, &bus.guid, snapshots)?;
        purge_instrument_routing(&instrument_cached.guid, &bus.guid)?;
        align_instrument_track_order_by_uuid(uid)?;
        Ok(())
    })();
    pr.end_undo_block("Add track to BackgroundRenderer", UndoFlags::all());
    result?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum TrackRole {
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

fn collect_related_send_snapshots(instrument_guid: &str) -> anyhow::Result<Vec<SendSnapshot>> {
    let rpr = Reaper::get();
    let pr = rpr.current_project();
    let low = rpr.low();
    let mut snapshots = Vec::new();

    for source_track in pr.iter_tracks() {
        let source_guid = source_track.guid().to_string();
        let source_ptr = source_track.get().as_ptr();
        for send_index in 0..source_track.n_sends() {
            let dest_ptr = unsafe {
                low.GetSetTrackSendInfo(
                    source_ptr,
                    0,
                    send_index as i32,
                    CString::new("P_DESTTRACK").expect("cstring").as_ptr(),
                    std::ptr::null_mut(),
                ) as *mut rea_rs_low::raw::MediaTrack
            };
            let Some(dest_ptr) = MediaTrack::new(dest_ptr) else {
                continue;
            };
            let dest_guid = Track::<Immutable>::new(&pr, dest_ptr).guid().to_string();
            if source_guid != instrument_guid && dest_guid != instrument_guid {
                continue;
            }

            snapshots.push(SendSnapshot {
                source_guid: source_guid.clone(),
                source_send_index: send_index,
                dest_guid,
                mute: unsafe {
                    low.GetTrackSendInfo_Value(
                        source_ptr,
                        0,
                        send_index as i32,
                        CString::new("B_MUTE").expect("cstring").as_ptr(),
                    )
                },
                phase: unsafe {
                    low.GetTrackSendInfo_Value(
                        source_ptr,
                        0,
                        send_index as i32,
                        CString::new("B_PHASE").expect("cstring").as_ptr(),
                    )
                },
                mono: unsafe {
                    low.GetTrackSendInfo_Value(
                        source_ptr,
                        0,
                        send_index as i32,
                        CString::new("B_MONO").expect("cstring").as_ptr(),
                    )
                },
                volume: unsafe {
                    low.GetTrackSendInfo_Value(
                        source_ptr,
                        0,
                        send_index as i32,
                        CString::new("D_VOL").expect("cstring").as_ptr(),
                    )
                },
                pan: unsafe {
                    low.GetTrackSendInfo_Value(
                        source_ptr,
                        0,
                        send_index as i32,
                        CString::new("D_PAN").expect("cstring").as_ptr(),
                    )
                },
                pan_law: unsafe {
                    low.GetTrackSendInfo_Value(
                        source_ptr,
                        0,
                        send_index as i32,
                        CString::new("D_PANLAW").expect("cstring").as_ptr(),
                    )
                },
                send_mode: unsafe {
                    low.GetTrackSendInfo_Value(
                        source_ptr,
                        0,
                        send_index as i32,
                        CString::new("I_SENDMODE").expect("cstring").as_ptr(),
                    )
                },
                automation_mode: unsafe {
                    low.GetTrackSendInfo_Value(
                        source_ptr,
                        0,
                        send_index as i32,
                        CString::new("I_AUTOMODE").expect("cstring").as_ptr(),
                    )
                },
                source_channels: unsafe {
                    low.GetTrackSendInfo_Value(
                        source_ptr,
                        0,
                        send_index as i32,
                        CString::new("I_SRCCHAN").expect("cstring").as_ptr(),
                    )
                },
                dest_channels: unsafe {
                    low.GetTrackSendInfo_Value(
                        source_ptr,
                        0,
                        send_index as i32,
                        CString::new("I_DSTCHAN").expect("cstring").as_ptr(),
                    )
                },
                midi_flags: unsafe {
                    low.GetTrackSendInfo_Value(
                        source_ptr,
                        0,
                        send_index as i32,
                        CString::new("I_MIDIFLAGS").expect("cstring").as_ptr(),
                    )
                },
            });
        }
    }

    Ok(snapshots)
}

fn migrate_related_sends(
    instrument_guid: &str,
    bus_guid: &str,
    snapshots: Vec<SendSnapshot>,
) -> anyhow::Result<()> {
    let mut snapshots_to_remove = snapshots.clone();
    snapshots_to_remove.sort_by(|a, b| {
        a.source_guid
            .cmp(&b.source_guid)
            .then(b.source_send_index.cmp(&a.source_send_index))
    });

    for snapshot in snapshots_to_remove {
        remove_send_by_source_and_index(&snapshot.source_guid, snapshot.source_send_index)?;
    }

    for snapshot in snapshots {
        let new_source_guid = if snapshot.source_guid == instrument_guid {
            bus_guid
        } else {
            &snapshot.source_guid
        };
        let new_dest_guid = if snapshot.dest_guid == instrument_guid {
            bus_guid
        } else {
            &snapshot.dest_guid
        };

        if new_source_guid != new_dest_guid {
            create_send_with_params(new_source_guid, new_dest_guid, &snapshot)?;
        }
    }
    Ok(())
}

fn purge_instrument_routing(instrument_guid: &str, bus_guid: &str) -> anyhow::Result<()> {
    let rpr = Reaper::get();
    let pr = rpr.current_project();
    let low = rpr.low();

    let mut to_remove = Vec::new();
    for source_track in pr.iter_tracks() {
        let source_guid = source_track.guid().to_string();
        let source_ptr = source_track.get().as_ptr();
        for send_index in 0..source_track.n_sends() {
            let dest_ptr = unsafe {
                low.GetSetTrackSendInfo(
                    source_ptr,
                    0,
                    send_index as i32,
                    CString::new("P_DESTTRACK").expect("cstring").as_ptr(),
                    std::ptr::null_mut(),
                ) as *mut rea_rs_low::raw::MediaTrack
            };
            let Some(dest_ptr) = MediaTrack::new(dest_ptr) else {
                continue;
            };
            let dest_guid = Track::<Immutable>::new(&pr, dest_ptr).guid().to_string();

            // Instrument should only keep the intentional autosend to bus.
            let remove = if source_guid == instrument_guid {
                dest_guid != bus_guid
            } else {
                dest_guid == instrument_guid
            };
            if remove {
                to_remove.push((source_guid.clone(), send_index));
            }
        }
    }

    to_remove.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    for (source_guid, send_index) in to_remove {
        remove_send_by_source_and_index(&source_guid, send_index)?;
    }
    Ok(())
}

fn create_send_with_params(
    source_guid: &str,
    dest_guid: &str,
    snapshot: &SendSnapshot,
) -> anyhow::Result<()> {
    let rpr = Reaper::get();
    let pr = rpr.current_project();
    let source = find_track_by_guid(&pr, source_guid).ok_or(LevitanusError::Unexpected(
        format!("Source track not found: {source_guid}"),
    ))?;
    let dest = find_track_by_guid(&pr, dest_guid).ok_or(LevitanusError::Unexpected(format!(
        "Destination track not found: {dest_guid}"
    )))?;

    let low = rpr.low();
    let send_index = unsafe { low.CreateTrackSend(source.get().as_ptr(), dest.get().as_ptr()) };
    if send_index < 0 {
        return Err(LevitanusError::Reaper("Failed to create send".to_string()).into());
    }
    let source_ptr = source.get().as_ptr();
    let send_index = send_index as i32;

    set_send_value(low, source_ptr, send_index, "B_MUTE", snapshot.mute);
    set_send_value(low, source_ptr, send_index, "B_PHASE", snapshot.phase);
    set_send_value(low, source_ptr, send_index, "B_MONO", snapshot.mono);
    set_send_value(low, source_ptr, send_index, "D_VOL", snapshot.volume);
    set_send_value(low, source_ptr, send_index, "D_PAN", snapshot.pan);
    set_send_value(low, source_ptr, send_index, "D_PANLAW", snapshot.pan_law);
    set_send_value(
        low,
        source_ptr,
        send_index,
        "I_SENDMODE",
        snapshot.send_mode,
    );
    set_send_value(
        low,
        source_ptr,
        send_index,
        "I_AUTOMODE",
        snapshot.automation_mode,
    );
    set_send_value(
        low,
        source_ptr,
        send_index,
        "I_SRCCHAN",
        snapshot.source_channels,
    );
    set_send_value(
        low,
        source_ptr,
        send_index,
        "I_DSTCHAN",
        snapshot.dest_channels,
    );
    set_send_value(
        low,
        source_ptr,
        send_index,
        "I_MIDIFLAGS",
        snapshot.midi_flags,
    );
    Ok(())
}

fn remove_send_by_source_and_index(source_guid: &str, send_index: usize) -> anyhow::Result<()> {
    let rpr = Reaper::get();
    let pr = rpr.current_project();
    let source = find_track_by_guid(&pr, source_guid).ok_or(LevitanusError::Unexpected(
        format!("Source track not found: {source_guid}"),
    ))?;

    let removed = unsafe {
        rpr.low()
            .RemoveTrackSend(source.get().as_ptr(), 0, send_index as i32)
    };
    if !removed {
        return Err(LevitanusError::Reaper("Failed to remove old send".to_string()).into());
    }
    Ok(())
}

fn set_send_value(
    low: &rea_rs_low::Reaper,
    source_ptr: *mut rea_rs_low::raw::MediaTrack,
    send_index: i32,
    key: &str,
    value: f64,
) {
    let key = CString::new(key).expect("send key CString");
    unsafe {
        low.SetTrackSendInfo_Value(source_ptr, 0, send_index, key.as_ptr(), value);
    }
}

fn find_track_by_guid<'a>(pr: &'a Project, guid: &str) -> Option<Track<'a, Immutable>> {
    pr.iter_tracks()
        .find(|track| track.guid().to_string() == guid)
}

fn move_non_instrument_fx_to_bus(
    instrument: &mut CachedTrack,
    bus: &mut CachedTrack,
) -> anyhow::Result<()> {
    instrument.validate()?;
    bus.validate()?;
    let rpr = Reaper::get();
    let pr = rpr.current_project();
    let instrument_track = pr
        .get_track(instrument.index)
        .ok_or(LevitanusError::Unexpected(
            "No instrument track".to_string(),
        ))?;
    let bus_track = pr
        .get_track(bus.index)
        .ok_or(LevitanusError::Unexpected("No bus track".to_string()))?;

    let low = rpr.low();
    let instrument_ptr = instrument_track.get().as_ptr();
    let bus_ptr = bus_track.get().as_ptr();
    let n_fx = unsafe { low.TrackFX_GetCount(instrument_ptr) as usize };
    let instrument_fx_idx = unsafe { low.TrackFX_GetInstrument(instrument_ptr) };
    let mut to_move = Vec::new();
    for index in 0..n_fx {
        if instrument_fx_idx >= 0 && index == instrument_fx_idx as usize {
            continue;
        }
        to_move.push(index);
    }

    if to_move.is_empty() {
        return Ok(());
    }

    let insert_index = unsafe { low.TrackFX_GetCount(bus_ptr) };
    for index in to_move.into_iter().rev() {
        unsafe {
            low.TrackFX_CopyToTrack(instrument_ptr, index as i32, bus_ptr, insert_index, true);
        }
    }

    Ok(())
}

fn create_instrument_track(
    uid: u128,
    index: usize,
    instrument_name: String,
    bus: Option<CachedTrack>,
) -> anyhow::Result<CachedTrack> {
    let mut pr = Reaper::get().current_project();
    let mut instr = pr.add_track(index, instrument_name.clone());
    instr.set_parent_send(None)?;
    ExtState::new(EXT_SECTION, UUID_KEY, uid, true, &instr, None);
    ExtState::new(
        EXT_SECTION,
        ROLE_KEY,
        TrackRole::Instrument,
        true,
        &instr,
        None,
    );
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
    let mut rendered_track = instrument.with_reaper_track(|track| {
        let index = track.index();
        let instrument_name = track.name();
        let mut pr = Reaper::get().current_project();
        let mut rndr = pr.add_track(index, instrument_name.clone() + " rendered");
        rndr.set_visible_in_tcp(false)?;
        rndr.set_visible_in_mcp(false)?;
        rndr.set_parent_send(None)?;
        ExtState::new(EXT_SECTION, UUID_KEY, uid, true, &rndr, None);
        ExtState::new(
            EXT_SECTION,
            ROLE_KEY,
            TrackRole::Rendered,
            true,
            &rndr,
            None,
        );
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
    let mut bus_track = instrument.with_reaper_track(|track| {
        let index = track.index();
        let instrument_name = track.name();
        let mut pr = Reaper::get().current_project();
        let mut bus = pr.add_track(index, instrument_name.clone() + " bus");
        bus.set_visible_in_tcp(false)?;
        ExtState::new(EXT_SECTION, UUID_KEY, uid, true, &bus, None);
        ExtState::new(EXT_SECTION, ROLE_KEY, TrackRole::Bus, true, &bus, None);
        Ok(CachedTrack::from(bus))
    })?;
    create_send(&mut instrument, &mut bus_track)?;
    if let Some(mut rendered) = rendered {
        create_send(&mut rendered, &mut bus_track)?;
    }
    Ok(bus_track)
}

fn create_send(source: &mut CachedTrack, dest: &mut CachedTrack) -> anyhow::Result<()> {
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

fn delete_track(track: &mut CachedTrack) -> anyhow::Result<()> {
    track.with_reaper_track(|track| {
        track.delete();
        Ok(())
    })
}

fn forget_track(track: &mut CachedTrack) -> anyhow::Result<()> {
    let section = CString::new(EXT_SECTION).unwrap();
    let uuid_key = CString::new(UUID_KEY).unwrap();
    let role_key = CString::new(ROLE_KEY).unwrap();
    track.with_reaper_track(|track| {
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
            let instrument_name = bus.with_reaper_track(|track| {
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
            let instrument_name = rendered.with_reaper_track(|track| {
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
