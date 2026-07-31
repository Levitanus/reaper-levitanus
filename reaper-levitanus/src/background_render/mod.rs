use std::{cell::RefCell, collections::HashSet, error::Error, sync::Arc, time::Duration};

use log::{debug, warn};
use rea_rs::{ActionHook, ControlSurface, ExtState, FXParent, Reaper, ReaperResult, Timer, FX};
use serde::{Deserialize, Serialize};

use crate::{
    background_render::{
        track_management::{
            align_all_instrument_track_orders, rebuild_instrument_list, resolve_unpaired_tracks,
            BGRenderTrack, TrackRole,
        },
        Task::MonitorIntrument,
    },
    utils::CachedTrack,
};

const ID_STRING: &str = "BackgroudRenderer";
const TIMER_ID_STRING: &str = "BackgroudRendererTimer";
const EXT_SECTION: &str = "Levitanus_BackgroundRenderer";
const EXT_KEY: &str = "enabled";

const UUID_KEY: &str = "uuid";
const ROLE_KEY: &str = "role";

mod track_management;
pub use track_management::{create_bg_instrument, make_track_rendered};

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
enum Task {
    RebuildInstrumentList,
    MonitorIntrument(RenderedInstrument),
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct TaskQueue(HashSet<Task>);

impl TaskQueue {
    fn insert(&mut self, task: Task) {
        self.0.insert(task);
    }

    fn drain(&mut self) -> Vec<Task> {
        let tasks = self.0.drain().collect();
        if let Err(e) = Self::ext_state().set(self.clone()) {
            log::error!("can't save ExtState: {}", e);
        }
        tasks
    }
    fn ext_state() -> ExtState<'static, Self, Reaper> {
        ExtState::<BackgroundRendererState, Reaper>::existing(
            EXT_SECTION,
            "task_queue",
            false,
            Reaper::get(),
            None,
        )
    }

    pub(crate) fn queue_task(task: Task) -> ReaperResult<()> {
        let mut state = Self::ext_state();
        let mut queue = state.get().unwrap_or(None).unwrap_or(TaskQueue::default());
        queue.insert(task);
        state.set(queue)
    }

    pub(crate) fn load() -> Self {
        Self::ext_state()
            .get()
            .unwrap_or(None)
            .unwrap_or(Self::default())
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
struct BackgroundRendererState {
    instruments: Vec<RenderedInstrument>,
    unpaired_tracks: Vec<(u128, TrackRole, CachedTrack)>,
    is_performing: bool,
}
impl BackgroundRendererState {
    fn load() -> anyhow::Result<Self> {
        Ok(Self::ext_state().get()?.unwrap_or(Self::default()))
    }
    fn ext_state() -> ExtState<'static, Self, Reaper> {
        ExtState::<BackgroundRendererState, Reaper>::existing(
            EXT_SECTION,
            "BackgroundRendererState",
            false,
            Reaper::get(),
            1024 * 10,
        )
    }
    fn save(&self) -> ReaperResult<()> {
        Self::ext_state().set(self.clone())
    }
}

#[derive(Debug)]
struct BGRControlSurface {}

#[derive(Debug)]
struct MainLoop {}

impl Timer for MainLoop {
    fn run(&mut self) -> Result<(), anyhow::Error> {
        // debug!("run");
        let rpr = Reaper::get_mut();
        let pr = rpr.current_project();
        if !pr.is_stopped()? {
            return Ok(());
        }
        let mut state = BackgroundRendererState::load()?;
        // debug!("{:#?}", state);

        state.is_performing = true;
        state.save()?;
        let tasks: Vec<Task> = TaskQueue::load().drain();
        // debug!("tasks: {:#?}", tasks);

        for task in tasks {
            match task {
                Task::RebuildInstrumentList => {
                    let (mut instruments, unpaired_tracks) = rebuild_instrument_list()?;
                    align_all_instrument_track_orders(&mut instruments)?;
                    state.instruments = instruments;
                    state.unpaired_tracks = unpaired_tracks;
                }
                MonitorIntrument(mut instrument) => instrument.set_monitoring()?,
            }
        }

        let mut local_unpaired_tracks = std::mem::take(&mut state.unpaired_tracks);

        if !local_unpaired_tracks.is_empty() {
            resolve_unpaired_tracks(&mut local_unpaired_tracks, rpr, pr)?;
        }

        state.is_performing = false;
        state.save()?;
        Ok(())
    }

    fn id_string(&self) -> String {
        TIMER_ID_STRING.to_string()
    }

    fn interval(&self) -> Duration {
        Duration::from_millis(100)
    }
}

impl ControlSurface for BGRControlSurface {
    fn get_type_string(&self) -> String {
        ID_STRING.to_string()
    }

    fn get_desc_string(&self) -> String {
        "background render control surface".to_string()
    }

    fn set_track_list_change(&self) -> anyhow::Result<()> {
        debug!("set tracklist change");
        if !BackgroundRendererState::load()?.is_performing {
            TaskQueue::queue_task(Task::RebuildInstrumentList)?;
        }
        Ok(())
    }
    fn run(&mut self) -> anyhow::Result<()> {
        let edited_tracks = if let Some(editor) = Reaper::get().active_midi_editor() {
            let mut edited_tracks = HashSet::new();
            for take in editor.enum_takes(true) {
                edited_tracks.insert(CachedTrack::from_reaper_track(take.parent_track()?)?);
            }
            Some(edited_tracks)
        } else {
            None
        };
        let mut state = BackgroundRendererState::load()?;
        for instrument in state.instruments.iter_mut() {
            let changed = match &edited_tracks {
                Some(edited_tracks) => {
                    let (opened, was_opened) = (
                        edited_tracks.contains(&instrument.instrument),
                        instrument.opened_in_editor,
                    );
                    (opened && !was_opened) || (!opened && was_opened)
                }
                None => instrument.opened_in_editor,
            };
            if changed {
                instrument.opened_in_editor = !instrument.opened_in_editor;
                TaskQueue::queue_task(MonitorIntrument(instrument.clone()))?;
                debug!(
                    "MonitorIntrument({}) by midi_editor: {}",
                    instrument.uuid, instrument.opened_in_editor
                );
            }
        }
        state.save()?;

        Ok(())
    }
    fn set_surface_recarm(&self, track: &mut rea_rs::Track, recarm: bool) -> anyhow::Result<()> {
        debug!("set_surface_recarm");
        if let Some((uuid, role)) = track.belongs_to_bgr() {
            // debug!("track belongs to bgr, getting params");
            if role != TrackRole::Instrument {
                return Ok(());
            }
            let rec_monitor = recarm && track.rec_monitoring()?.mode > 0;
            // debug!("looking up for instrument");
            let Some(mut instrument) = RenderedInstrument::from_uuid(uuid)? else {
                warn!(
                    "No Rendered instrument, but with instrument track found. \
                Could be project initialization, could be an error."
                );
                return Ok(());
            };
            if instrument.rec_monitor != rec_monitor {
                instrument.rec_monitor = rec_monitor;
                TaskQueue::queue_task(Task::MonitorIntrument(instrument.clone()))?;
                let mut state = BackgroundRendererState::load()?;
                for state_instr in state.instruments.iter_mut() {
                    if state_instr.uuid == uuid {
                        state_instr.rec_monitor = rec_monitor;
                        break;
                    }
                }
                state.save()?;
                debug!(
                    "MonitorIntrument({}) by rec_arm: {}",
                    instrument.uuid, rec_monitor
                );
            }
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, Hash)]
struct RenderedInstrument {
    pub uuid: u128,
    pub bus: CachedTrack,
    pub rendered: CachedTrack,
    pub instrument: CachedTrack,
    #[serde(default)]
    pub opened_in_editor: bool,
    #[serde(default)]
    pub rec_monitor: bool,
}
impl RenderedInstrument {
    pub(crate) fn new(
        uuid: u128,
        bus: CachedTrack,
        rendered: CachedTrack,
        instrument: CachedTrack,
    ) -> Self {
        Self {
            uuid,
            bus,
            rendered,
            instrument,
            opened_in_editor: false,
            rec_monitor: false,
        }
    }
    pub(crate) fn from_uuid(uuid: u128) -> anyhow::Result<Option<Self>> {
        let state = BackgroundRendererState::load()?;
        Ok(state
            .instruments
            .iter()
            .find(|instrument| instrument.uuid == uuid)
            .cloned())
    }
    pub(crate) fn set_monitoring(&mut self) -> anyhow::Result<()> {
        let monitoring = self.opened_in_editor | self.rec_monitor;
        debug!(
            "Setting monitoring for track: opened={}, rec_monitor={}, monitoring={}",
            self.opened_in_editor, self.rec_monitor, monitoring
        );
        self.rendered
            .with_reaper_track(|mut track| track.set_muted(monitoring))?;
        self.instrument.with_reaper_track(|track| {
            for mut fx in track.iter_fx() {
                if !fx.is_instrument()? {
                    continue;
                }
                if !fx.is_online()? {
                    fx.set_online(monitoring)?
                }
                for param in fx.iter_params() {
                    // debug!("FX param name: {}", param.name()?);
                    if param.name()? == "Bypass" {
                        if let Some(mut env) = param.envelope(false)? {
                            env.set_active(!monitoring)?;
                        }
                    }
                }
                fx.set_enabled(monitoring)?;
            }
            Ok(())
        })?;
        Ok(())
    }
}

pub fn load_default_state() -> ReaperResult<bool> {
    let rpr = Reaper::get();
    let ext_state: ExtState<bool, Reaper> =
        ExtState::new(EXT_SECTION, EXT_KEY, None, true, rpr, None)?;

    match ext_state.get() {
        Ok(Some(value)) => Ok(value),
        Ok(None) => Ok(false),
        Err(_) => Ok(false),
    }
}

pub fn save_default_state(enabled: bool) -> ReaperResult<()> {
    let rpr = Reaper::get();
    let mut ext_state = ExtState::new(EXT_SECTION, EXT_KEY, Some(enabled), true, rpr, None)?;
    ext_state.set(enabled)
}

pub fn is_running() -> bool {
    let id = ID_STRING.to_string();
    Reaper::get().has_control_surface(&id)
}

pub fn set_enabled(enabled: bool) -> Result<(), anyhow::Error> {
    let rpr = Reaper::get_mut();
    let id = ID_STRING.to_string();
    let running = rpr.has_control_surface(&id);

    if enabled && !running {
        TaskQueue::queue_task(Task::RebuildInstrumentList)?;
        let cs = BGRControlSurface {};
        let timer = MainLoop {};
        rpr.register_control_surface(Arc::new(RefCell::new(cs)));
        rpr.register_timer(Arc::new(RefCell::new(timer)));
    } else if !enabled && running {
        rpr.unregister_control_surface(id)?;
        rpr.unregister_timer(TIMER_ID_STRING.to_string())?;
    }

    save_default_state(enabled)?;
    Ok(())
}

pub fn restore_default_state() -> Result<bool, anyhow::Error> {
    let enabled = load_default_state()?;
    set_enabled(enabled)?;
    Ok(enabled)
}

pub fn toggle_action(hook: &mut ActionHook) -> Result<(), anyhow::Error> {
    let next_state = !is_running();
    set_enabled(next_state)?;
    hook.set_toggle_state(next_state);
    Ok(())
}
