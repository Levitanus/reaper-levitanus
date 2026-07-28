use std::{cell::RefCell, collections::HashSet, error::Error, sync::Arc, time::Duration};

use rea_rs::{ActionHook, ControlSurface, ExtState, Reaper, Timer};
use serde::{Deserialize, Serialize};

use crate::{
    background_render::track_management::{
        align_all_instrument_track_orders, rebuild_instrument_list, resolve_unpaired_tracks,
        TrackRole,
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

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
enum Task {
    RebuildInstrumentList = 0,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct TaskQueue(HashSet<Task>);

impl TaskQueue {
    fn insert(&mut self, task: Task) {
        self.0.insert(task);
    }

    fn drain(&mut self) -> Vec<Task> {
        self.0.drain().collect()
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

    pub(crate) fn queue_task(task: Task) {
        let mut state = Self::ext_state();
        let mut queue = state.get().unwrap_or(None).unwrap_or(TaskQueue::default());
        queue.insert(task);
        state.set(queue);
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
    fn save(&self) {
        Self::ext_state().set(self.clone());
    }
}

#[derive(Debug)]
struct BGRControlSurface {}

#[derive(Debug)]
struct MainLoop {}

impl Timer for MainLoop {
    fn run(&mut self) -> Result<(), Box<dyn Error>> {
        let rpr = Reaper::get_mut();
        let pr = rpr.current_project();
        if !pr.is_stopped() {
            return Ok(());
        }
        let mut state = BackgroundRendererState::load()?;

        state.is_performing = true;
        state.save();
        let tasks: Vec<Task> = TaskQueue::load().drain();

        for task in tasks {
            match task {
                Task::RebuildInstrumentList => {
                    let (mut instruments, unpaired_tracks) = rebuild_instrument_list()?;
                    align_all_instrument_track_orders(&mut instruments)?;
                    state.instruments = instruments;
                    state.unpaired_tracks = unpaired_tracks;
                }
            }
        }

        let mut local_unpaired_tracks = std::mem::take(&mut state.unpaired_tracks);

        if !local_unpaired_tracks.is_empty() {
            resolve_unpaired_tracks(&mut local_unpaired_tracks, rpr, pr)?;
        }

        state.is_performing = false;
        state.save();
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
        if !BackgroundRendererState::load()?.is_performing {
            TaskQueue::queue_task(Task::RebuildInstrumentList);
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct RenderedInstrument {
    uuid: u128,
    bus: CachedTrack,
    rendered: CachedTrack,
    instrument: CachedTrack,
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
        TaskQueue::queue_task(Task::RebuildInstrumentList);
        let cs = BGRControlSurface {};
        let timer = MainLoop {};
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
