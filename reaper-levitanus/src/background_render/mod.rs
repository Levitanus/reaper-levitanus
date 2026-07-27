use std::{
    cell::RefCell,
    collections::HashSet,
    error::Error,
    sync::{Arc, Mutex},
    time::Duration,
};

use rea_rs::{ActionHook, ControlSurface, ExtState, Reaper, Timer};

use crate::{LevitanusError, background_render::track_management::{TrackRole, align_all_instrument_track_orders, rebuild_instrument_list, resolve_unpaired_tracks}, utils::CachedTrack};

const ID_STRING: &str = "BackgroudRenderer";
const TIMER_ID_STRING: &str = "BackgroudRendererTimer";
const EXT_SECTION: &str = "Levitanus_BackgroundRenderer";
const EXT_KEY: &str = "enabled";

const UUID_KEY: &str = "uuid";
const ROLE_KEY: &str = "role";

mod track_management;
pub use track_management::{create_bg_instrument, make_track_rendered};

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
