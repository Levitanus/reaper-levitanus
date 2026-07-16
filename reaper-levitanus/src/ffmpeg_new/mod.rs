use std::{
    cell::Cell,
    cell::RefCell,
    collections::VecDeque,
    error::Error,
    fmt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use anyhow::anyhow;
use log::debug;
use rea_rs::{
    gui::{self, DockableEguiWindow},
    CommandId, ControlSurface, ExtState, MessageBoxType, MessageBoxValue, Mutable, Project,
    Reaper, Track,
};
use serde::{Deserialize, Serialize};

mod render;
mod render_targets;

use render::{RenderEngine, RenderJobDefinition, RenderJobStatus, RenderStateSnapshot};
use render_targets::{
    build_render_targets, AvailbleForRender, RenderTarget, DEFAULT_RENDER_TARGETS_BUF_SIZE,
};

use crate::ffmpeg_new::render_targets::FALLBACK_RENDER_TARGETS_BUF_SIZE;

pub static BACKEND_ID_STRING: &str = "LevitanusFfmpegGuiNew";
const AUTO_REFRESH_INTERVAL: Duration = Duration::from_secs(1);
const RENDER_PROJECT_USING_LAST_SETTINGS_ACTION: u32 = 41824;

#[derive(Debug, Clone)]
enum GuiToSurfaceMessage {
    CloseRequested,
    SaveGuiState(GuiPersistedState),
    RequestRenderTargets,
    RequestRender,
    CancelRender,
}

const GUI_STATE_EXT_SECTION: &str = "Levitanus";
const GUI_STATE_EXT_KEY: &str = "FFMPEG_NEW_GUI_STATE";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
struct GuiPersistedState {
    selected_muxer: usize,
    selected_video_codec: usize,
    selected_audio_codec: usize,
    parallel_render: bool,
    render_workers: usize,
    use_rendered_video: bool,
}

impl GuiPersistedState {
    fn sanitize_for_elements(&mut self, elements: &GuiElementsState) {
        self.selected_muxer = clamp_selected_idx(self.selected_muxer, elements.muxers.len());
        self.selected_video_codec =
            clamp_selected_idx(self.selected_video_codec, elements.video_codecs.len());
        self.selected_audio_codec =
            clamp_selected_idx(self.selected_audio_codec, elements.audio_codecs.len());
        self.render_workers = self.render_workers.clamp(1, 64);
    }
}

#[derive(Debug, Clone)]
struct GuiElementsState {
    error: Option<String>,
    render_targets: Option<Vec<RenderTarget>>,
    render_targets_revision: u64,
    render_state: RenderStateSnapshot,
    muxers: Vec<String>,
    video_codecs: Vec<String>,
    audio_codecs: Vec<String>,
}

impl Default for GuiElementsState {
    fn default() -> Self {
        Self {
            error: None,
            render_targets: None,
            render_targets_revision: 0,
            render_state: RenderStateSnapshot::default(),
            muxers: vec!["mkv".to_string(), "mp4".to_string(), "mov".to_string()],
            video_codecs: vec![
                "copy".to_string(),
                "libx264".to_string(),
                "libx265".to_string(),
                "prores_ks".to_string(),
            ],
            audio_codecs: vec![
                "copy".to_string(),
                "aac".to_string(),
                "pcm_s16le".to_string(),
                "flac".to_string(),
            ],
        }
    }
}

fn clamp_selected_idx(idx: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        idx.min(len - 1)
    }
}

fn render_target_name(path: &PathBuf) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| stem.to_string())
        .unwrap_or_else(|| "<unnamed>".to_string())
}

fn render_status(status: &AvailbleForRender) -> (&'static str, gui::egui::Color32) {
    match status {
        AvailbleForRender::Ok => ("Ok", gui::egui::Color32::GREEN),
        AvailbleForRender::NoVideo => ("NoVideo", gui::egui::Color32::YELLOW),
        AvailbleForRender::OutOfBounds(_) => ("OutOfBounds", gui::egui::Color32::RED),
    }
}

#[derive(Debug, Default)]
struct MessageBus {
    to_surface: Mutex<VecDeque<GuiToSurfaceMessage>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SurfaceOperation {
    RefreshRenderTargets,
}

impl MessageBus {
    fn send_to_surface(&self, msg: GuiToSurfaceMessage) {
        if let Ok(mut queue) = self.to_surface.lock() {
            queue.push_back(msg);
        }
    }

    fn drain_for_surface(&self) -> Vec<GuiToSurfaceMessage> {
        match self.to_surface.lock() {
            Ok(mut queue) => queue.drain(..).collect(),
            Err(_) => Vec::new(),
        }
    }
}

struct FfmpegGuiSurface {
    window: DockableEguiWindow,
    is_window_alive: Arc<AtomicBool>,
    message_bus: Arc<MessageBus>,
    pending_ops: Arc<Mutex<VecDeque<SurfaceOperation>>>,
    gui_elements_state: Arc<Mutex<GuiElementsState>>,
    gui_persisted_state: Arc<Mutex<GuiPersistedState>>,
    render_engine: RenderEngine,
    render_targets_buf_size: Cell<usize>,
    last_auto_refresh_at: Mutex<Instant>,
}

impl fmt::Debug for FfmpegGuiSurface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FfmpegGuiSurface").finish()
    }
}

impl FfmpegGuiSurface {
    fn new() -> Result<Self, Box<dyn Error>> {
        debug!("ffmpeg_new: creating GUI surface");
        let is_window_alive = Arc::new(AtomicBool::new(true));
        let message_bus = Arc::new(MessageBus::default());
        let pending_ops = Arc::new(Mutex::new(VecDeque::new()));
        let gui_elements_state = Arc::new(Mutex::new(GuiElementsState::default()));
        let initial_persisted_state = {
            let mut state = Self::load_gui_state_from_project();
            let elements = gui_elements_state
                .lock()
                .map(|s| s.clone())
                .unwrap_or_default();
            state.sanitize_for_elements(&elements);
            state
        };
        let gui_persisted_state = Arc::new(Mutex::new(initial_persisted_state));
        let mut window = DockableEguiWindow::new(
            "Levitanus FFMPEG GUI",
            "levitanus_ffmpeg_gui",
            gui::baseview::dpi::Size::Logical(gui::baseview::dpi::LogicalSize::new(640.0, 460.0)),
        );

        window.set_dock(
            None,
            FfmpegWindowState {
                is_alive: Arc::clone(&is_window_alive),
                message_bus: Arc::clone(&message_bus),
                gui_elements_state: Arc::clone(&gui_elements_state),
                gui_persisted_state: Arc::clone(&gui_persisted_state),
                last_gui_state_snapshot: None,
                last_render_targets_revision: None,
            },
            |_ctx, _queue, state| {
                let gui_state_snapshot = state
                    .gui_persisted_state
                    .lock()
                    .map(|gui_state| gui_state.clone())
                    .unwrap_or_default();

                if state.last_gui_state_snapshot.as_ref() != Some(&gui_state_snapshot) {
                    state
                        .message_bus
                        .send_to_surface(GuiToSurfaceMessage::SaveGuiState(
                            gui_state_snapshot.clone(),
                        ));
                    state.last_gui_state_snapshot = Some(gui_state_snapshot);
                }
            },
            |ui, _queue, state| {
                let gui_elements_snapshot = state
                    .gui_elements_state
                    .lock()
                    .map(|gui_state| gui_state.clone())
                    .unwrap_or_default();
                let mut gui_persisted_snapshot = state
                    .gui_persisted_state
                    .lock()
                    .map(|gui_state| gui_state.clone())
                    .unwrap_or_default();
                gui_persisted_snapshot.sanitize_for_elements(&gui_elements_snapshot);

                if state.last_render_targets_revision
                    != Some(gui_elements_snapshot.render_targets_revision)
                {
                    ui.ctx().request_repaint();
                    state.last_render_targets_revision =
                        Some(gui_elements_snapshot.render_targets_revision);
                }

                gui::egui::CentralPanel::default().show(ui, |ui| {
                    gui::egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.heading("FFmpeg render");
                            ui.separator();

                            ui_render_settings(
                                &gui_elements_snapshot,
                                &mut gui_persisted_snapshot,
                                ui,
                            );
                            ui.separator();
                            ui_render_queue_controls(state, &gui_elements_snapshot, ui);

                            ui.separator();
                            if ui.button("Refresh render targets").clicked() {
                                state
                                    .message_bus
                                    .send_to_surface(GuiToSurfaceMessage::RequestRenderTargets);
                            }
                            ui_render_targets_list(&gui_elements_snapshot, ui);

                            if let Some(error) = &gui_elements_snapshot.error {
                                ui.colored_label(gui::egui::Color32::RED, error);
                            }
                            ui.separator();
                            if ui.button("Close").clicked() {
                                state
                                    .message_bus
                                    .send_to_surface(GuiToSurfaceMessage::CloseRequested);
                                ui.ctx()
                                    .send_viewport_cmd(gui::egui::ViewportCommand::Close);
                            }
                        });
                });

                if let Ok(mut shared_state) = state.gui_persisted_state.lock() {
                    *shared_state = gui_persisted_snapshot;
                }
            },
        );

        let surface = Self {
            window,
            is_window_alive,
            message_bus,
            pending_ops,
            gui_elements_state,
            gui_persisted_state,
            render_engine: RenderEngine::default(),
            render_targets_buf_size: Cell::new(DEFAULT_RENDER_TARGETS_BUF_SIZE),
            last_auto_refresh_at: Mutex::new(Instant::now()),
        };
        surface.enqueue_surface_operation(SurfaceOperation::RefreshRenderTargets);
        Ok(surface)
    }

    fn refresh_render_targets(&self) {
        let mut pr = Reaper::get().current_project();
        let cached_size = self.render_targets_buf_size.get().max(2);
        let effective_size = cached_size.max(FALLBACK_RENDER_TARGETS_BUF_SIZE);

        debug!(
            "ffmpeg_new: refresh_render_targets start buffer_size={} effective_size={}",
            cached_size, effective_size
        );

        match build_render_targets(&mut pr, effective_size) {
            Ok(result) => {
                debug!(
                    "ffmpeg_new: refresh_render_targets done, count={} required_buffer_size={} ",
                    result.targets.len(),
                    result.required_buffer_size
                );
                self.render_targets_buf_size.set(effective_size);
                if let Ok(mut gui_state) = self.gui_elements_state.lock() {
                    gui_state.error = None;
                    gui_state.render_targets = Some(result.targets);
                    gui_state.render_targets_revision =
                        gui_state.render_targets_revision.saturating_add(1);
                }
            }
            Err(error) => {
                debug!("ffmpeg_new: refresh_render_targets error: {error}");
                if let Ok(mut gui_state) = self.gui_elements_state.lock() {
                    gui_state.error = Some(error.to_string());
                    gui_state.render_targets_revision =
                        gui_state.render_targets_revision.saturating_add(1);
                }
            }
        }
    }

    fn load_gui_state_from_project() -> GuiPersistedState {
        let pr = Reaper::get().current_project();
        let ext_state: ExtState<GuiPersistedState, Project> = ExtState::new(
            GUI_STATE_EXT_SECTION,
            GUI_STATE_EXT_KEY,
            None,
            true,
            &pr,
            None,
        );

        match ext_state.get() {
            Ok(Some(state)) => state,
            Ok(None) => GuiPersistedState {
                parallel_render: true,
                render_workers: 4,
                ..GuiPersistedState::default()
            },
            Err(error) => {
                debug!("ffmpeg_new: failed to load GUI state from ExtState: {error}");
                GuiPersistedState {
                    parallel_render: true,
                    render_workers: 4,
                    ..GuiPersistedState::default()
                }
            }
        }
    }

    fn save_gui_state_to_project(&self, gui_state: GuiPersistedState) {
        let pr = Reaper::get().current_project();
        let mut ext_state: ExtState<GuiPersistedState, Project> = ExtState::new(
            GUI_STATE_EXT_SECTION,
            GUI_STATE_EXT_KEY,
            Some(gui_state.clone()),
            true,
            &pr,
            None,
        );
        ext_state.set(gui_state);
    }

    fn process_gui_messages(&mut self) {
        for msg in self.message_bus.drain_for_surface() {
            match msg {
                GuiToSurfaceMessage::CloseRequested => {
                    self.stop();
                }
                GuiToSurfaceMessage::SaveGuiState(state) => {
                    self.save_gui_state_to_project(state);
                }
                GuiToSurfaceMessage::RequestRenderTargets => {
                    self.enqueue_surface_operation(SurfaceOperation::RefreshRenderTargets);
                }
                GuiToSurfaceMessage::RequestRender => {
                    if let Err(error) = self.start_render() {
                        if let Ok(mut gui_state) = self.gui_elements_state.lock() {
                            gui_state.error = Some(error.to_string());
                            gui_state.render_targets_revision =
                                gui_state.render_targets_revision.saturating_add(1);
                        }
                    }
                }
                GuiToSurfaceMessage::CancelRender => {
                    self.render_engine.request_cancel();
                }
            }
        }
    }

    fn start_render(&mut self) -> anyhow::Result<()> {
        let elements = self
            .gui_elements_state
            .lock()
            .map(|state| state.clone())
            .map_err(|_| anyhow!("gui elements lock poisoned"))?;
        let mut persisted = self
            .gui_persisted_state
            .lock()
            .map(|state| state.clone())
            .map_err(|_| anyhow!("gui persisted lock poisoned"))?;
        persisted.sanitize_for_elements(&elements);

        let Some(render_targets) = elements.render_targets else {
            return Err(anyhow!("render targets are not ready yet"));
        };
        if render_targets.is_empty() {
            return Err(anyhow!("render targets list is empty"));
        }

        let missing_audio = render_targets
            .iter()
            .filter(|target| !target.path.exists())
            .map(|target| target.path.clone())
            .collect::<Vec<_>>();

        if !missing_audio.is_empty() {
            let response = Reaper::get().show_message_box(
                "FFmpeg render",
                format!(
                    "Found {} missing audio render files.\nRender project in Reaper first?",
                    missing_audio.len()
                ),
                MessageBoxType::YesNo,
            )?;

            if response == MessageBoxValue::Yes {
                let pr = Reaper::get().current_project();
                Reaper::get().perform_action(
                    CommandId::new(RENDER_PROJECT_USING_LAST_SETTINGS_ACTION),
                    0,
                    Some(&pr),
                );
            } else {
                return Err(anyhow!(
                    "audio files are missing: render project in Reaper before ffmpeg render"
                ));
            }

            let still_missing = render_targets.iter().any(|target| !target.path.exists());
            if still_missing {
                return Err(anyhow!(
                    "audio files are still missing after Reaper render request"
                ));
            }
        }

        let invalid_targets = render_targets
            .iter()
            .filter(|target| !matches!(target.availble_for_render, AvailbleForRender::Ok))
            .count();
        if invalid_targets > 0 {
            return Err(anyhow!(
                "{invalid_targets} render targets are not ready for video render (NoVideo/OutOfBounds)"
            ));
        }

        let muxer = elements
            .muxers
            .get(persisted.selected_muxer)
            .cloned()
            .ok_or_else(|| anyhow!("selected muxer is invalid"))?;
        let selected_video_codec = elements
            .video_codecs
            .get(persisted.selected_video_codec)
            .cloned()
            .ok_or_else(|| anyhow!("selected video codec is invalid"))?;
        let audio_codec = elements
            .audio_codecs
            .get(persisted.selected_audio_codec)
            .cloned()
            .ok_or_else(|| anyhow!("selected audio codec is invalid"))?;

        let mut jobs = Vec::new();
        for target in render_targets {
            if !matches!(target.availble_for_render, AvailbleForRender::Ok) {
                continue;
            }

            let (input_video_path, output_path, use_source_window, video_codec) =
                if persisted.use_rendered_video {
                    let rendered_video_path = target.path.with_extension(&muxer);
                    if !rendered_video_path.exists() {
                        return Err(anyhow!(
                            "use rendered video enabled, but source video does not exist: {}",
                            rendered_video_path.display()
                        ));
                    }
                    let output_path = with_suffix_before_extension(
                        &rendered_video_path,
                        " new_audio",
                        Some(&muxer),
                    );
                    (
                        rendered_video_path,
                        output_path,
                        false,
                        "copy".to_string(),
                    )
                } else {
                    let input_video_path = target
                        .video_source
                        .as_ref()
                        .ok_or_else(|| anyhow!("render target has no linked video source"))?
                        .clone();
                    if !input_video_path.exists() {
                        return Err(anyhow!(
                            "video source does not exist: {}",
                            input_video_path.display()
                        ));
                    }
                    (
                        input_video_path,
                        target.path.with_extension(&muxer),
                        true,
                        selected_video_codec.clone(),
                    )
                };

            jobs.push(RenderJobDefinition {
                render_target: target,
                output_path,
                input_video_path,
                use_source_window,
                video_codec,
                audio_codec: audio_codec.clone(),
            });
        }

        if jobs.is_empty() {
            return Err(anyhow!("no valid render jobs were generated"));
        }

        let jobs = self.confirm_overwrite_and_filter_jobs(jobs)?;
        if jobs.is_empty() {
            return Err(anyhow!("all render jobs were skipped by overwrite dialog"));
        }

        self.render_engine
            .start(jobs, persisted.parallel_render, persisted.render_workers)?;

        if let Ok(mut gui_state) = self.gui_elements_state.lock() {
            gui_state.error = None;
            gui_state.render_targets_revision = gui_state.render_targets_revision.saturating_add(1);
        }

        Ok(())
    }

    fn confirm_overwrite_and_filter_jobs(
        &self,
        jobs: Vec<RenderJobDefinition>,
    ) -> anyhow::Result<Vec<RenderJobDefinition>> {
        let mut approved = Vec::new();
        let mut replace_all_existing = false;
        let mut asked_replace_all = false;

        for job in jobs {
            if !job.output_path.exists() || replace_all_existing {
                approved.push(job);
                continue;
            }

            let replace_this = Reaper::get().show_message_box(
                "FFmpeg render overwrite",
                format!(
                    "Output file already exists:\n{}\n\nReplace this file?",
                    job.output_path.display()
                ),
                MessageBoxType::YesNo,
            )?;

            if replace_this != MessageBoxValue::Yes {
                continue;
            }

            approved.push(job);

            if !asked_replace_all {
                let apply_all = Reaper::get().show_message_box(
                    "FFmpeg render overwrite",
                    "Replace all remaining existing files without asking?",
                    MessageBoxType::YesNo,
                )?;
                replace_all_existing = apply_all == MessageBoxValue::Yes;
                asked_replace_all = true;
            }
        }

        Ok(approved)
    }

    fn enqueue_surface_operation(&self, op: SurfaceOperation) {
        if let Ok(mut queue) = self.pending_ops.lock() {
            if !queue.contains(&op) {
                queue.push_back(op);
            }
        }
    }

    fn process_surface_operations(&self) {
        let operations = match self.pending_ops.lock() {
            Ok(mut queue) => queue.drain(..).collect::<Vec<_>>(),
            Err(_) => {
                debug!("ffmpeg_new: surface operation queue is poisoned");
                return;
            }
        };

        for operation in operations {
            match operation {
                SurfaceOperation::RefreshRenderTargets => self.refresh_render_targets(),
            }
        }
    }

    fn sync_render_state_to_gui(&self) {
        let render_state = self.render_engine.snapshot();
        if let Ok(mut gui_state) = self.gui_elements_state.lock() {
            if gui_state.render_state != render_state {
                gui_state.render_state = render_state;
                gui_state.render_targets_revision =
                    gui_state.render_targets_revision.saturating_add(1);
            }
        }
    }
}

impl Drop for FfmpegGuiSurface {
    fn drop(&mut self) {
        self.window.close();
    }
}

impl ControlSurface for FfmpegGuiSurface {
    fn run(&mut self) -> anyhow::Result<()> {
        self.window.poll_resize();

        if let Ok(mut last_auto_refresh_at) = self.last_auto_refresh_at.lock() {
            if last_auto_refresh_at.elapsed() >= AUTO_REFRESH_INTERVAL {
                self.enqueue_surface_operation(SurfaceOperation::RefreshRenderTargets);
                *last_auto_refresh_at = Instant::now();
            }
        }

        self.process_gui_messages();
        self.process_surface_operations();
        self.sync_render_state_to_gui();

        if !self.is_window_alive.load(Ordering::Relaxed) {
            self.stop();
        }
        Ok(())
    }

    fn get_type_string(&self) -> String {
        BACKEND_ID_STRING.to_string()
    }

    fn get_desc_string(&self) -> String {
        "ffmpeg gui control surface".to_string()
    }

    fn on_track_selection(&self, _track: &mut Track<Mutable>) -> anyhow::Result<()> {
        self.enqueue_surface_operation(SurfaceOperation::RefreshRenderTargets);
        Ok(())
    }
}

fn ui_render_settings(
    elements: &GuiElementsState,
    persisted: &mut GuiPersistedState,
    ui: &mut gui::egui::Ui,
) {
    ui.horizontal(|ui| {
        let selected_muxer = elements
            .muxers
            .get(persisted.selected_muxer)
            .map(String::as_str)
            .unwrap_or("-");
        gui::egui::ComboBox::from_label("Muxer")
            .selected_text(selected_muxer)
            .show_ui(ui, |ui| {
                for (idx, muxer) in elements.muxers.iter().enumerate() {
                    ui.selectable_value(&mut persisted.selected_muxer, idx, muxer);
                }
            });

        let selected_video_codec = elements
            .video_codecs
            .get(persisted.selected_video_codec)
            .map(String::as_str)
            .unwrap_or("-");
        gui::egui::ComboBox::from_label("Video codec")
            .selected_text(selected_video_codec)
            .show_ui(ui, |ui| {
                for (idx, codec) in elements.video_codecs.iter().enumerate() {
                    ui.selectable_value(&mut persisted.selected_video_codec, idx, codec);
                }
            });

        let selected_audio_codec = elements
            .audio_codecs
            .get(persisted.selected_audio_codec)
            .map(String::as_str)
            .unwrap_or("-");
        gui::egui::ComboBox::from_label("Audio codec")
            .selected_text(selected_audio_codec)
            .show_ui(ui, |ui| {
                for (idx, codec) in elements.audio_codecs.iter().enumerate() {
                    ui.selectable_value(&mut persisted.selected_audio_codec, idx, codec);
                }
            });
    });

    ui.horizontal(|ui| {
        ui.checkbox(&mut persisted.parallel_render, "Parallel render");
        ui.label("Render workers");
        ui.add(gui::egui::DragValue::new(&mut persisted.render_workers).range(1..=64));
        ui.checkbox(&mut persisted.use_rendered_video, "Use rendered video");
    });
}

fn ui_render_queue_controls(
    state: &FfmpegWindowState,
    elements: &GuiElementsState,
    ui: &mut gui::egui::Ui,
) {
    ui.horizontal(|ui| {
        let can_start = !elements.render_state.active;
        if ui
            .add_enabled(
                can_start,
                gui::egui::Button::new(gui::egui::RichText::new("Render").strong()),
            )
            .clicked()
        {
            state
                .message_bus
                .send_to_surface(GuiToSurfaceMessage::RequestRender);
        }

        if ui
            .add_enabled(
                elements.render_state.active,
                gui::egui::Button::new("Cancel"),
            )
            .clicked()
        {
            state
                .message_bus
                .send_to_surface(GuiToSurfaceMessage::CancelRender);
        }

        ui.label(format!(
            "{}/{}",
            elements.render_state.finished_jobs,
            elements.render_state.total_jobs
        ));
    });

    if !elements.render_state.jobs.is_empty() {
        gui::egui::ScrollArea::vertical()
            .max_height(180.0)
            .show(ui, |ui| {
                for job in &elements.render_state.jobs {
                    ui.group(|ui| {
                        ui.label(gui::egui::RichText::new(&job.name).strong());
                        ui.label(&job.output);
                        let (status, color) = match job.status {
                            RenderJobStatus::Queued => ("queued", gui::egui::Color32::LIGHT_BLUE),
                            RenderJobStatus::Running => ("rendering", gui::egui::Color32::YELLOW),
                            RenderJobStatus::Done => ("done", gui::egui::Color32::GREEN),
                            RenderJobStatus::Failed => ("failed", gui::egui::Color32::RED),
                            RenderJobStatus::Canceled => ("canceled", gui::egui::Color32::GRAY),
                        };
                        ui.colored_label(color, status);
                        ui.add(gui::egui::ProgressBar::new(job.progress.clamp(0.0, 1.0)));
                        if let Some(error) = &job.error {
                            ui.colored_label(gui::egui::Color32::RED, error);
                        }
                    });
                }
            });
    }
}

fn ui_render_targets_list(state: &GuiElementsState, ui: &mut gui::egui::Ui) {
    ui.heading("Render targets");

    let table_revision = state
        .render_targets
        .as_ref()
        .map(|targets| {
            targets
                .iter()
                .map(|target| target.path.to_string_lossy())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| "<none>".to_string());

    gui::egui::ScrollArea::vertical()
        .id_salt(("render_targets_list", table_revision.as_str()))
        .max_height(280.0)
        .show(ui, |ui| {
            gui::egui::Grid::new(("render_targets_table", table_revision.as_str()))
                .num_columns(3)
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Name");
                    ui.strong("Path");
                    ui.strong("Available for render");
                    ui.end_row();

                    if let Some(targets) = &state.render_targets {
                        for target in targets {
                            let (status_text, status_color) =
                                render_status(&target.availble_for_render);
                            ui.label(render_target_name(&target.path));
                            let path_text = target.path.to_string_lossy();
                            ui.add(
                                gui::egui::Label::new(path_text.as_ref())
                                    .truncate()
                                    .sense(gui::egui::Sense::hover()),
                            )
                            .on_hover_text(path_text.as_ref());
                            ui.label(gui::egui::RichText::new(status_text).color(status_color));
                            ui.end_row();
                        }
                    } else {
                        ui.label("No targets");
                        ui.label("-");
                        ui.label("-");
                        ui.end_row();
                    }
                });
        });
}

fn with_suffix_before_extension(path: &Path, suffix: &str, ext: Option<&str>) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output")
        .to_string();
    let extension = ext
        .map(|value| value.to_string())
        .or_else(|| path.extension().and_then(|e| e.to_str()).map(|s| s.to_string()))
        .unwrap_or_default();

    let filename = if extension.is_empty() {
        format!("{}{}", stem, suffix)
    } else {
        format!("{}{}.{}", stem, suffix, extension)
    };
    parent.join(filename)
}

#[derive(Debug)]
struct FfmpegWindowState {
    is_alive: Arc<AtomicBool>,
    message_bus: Arc<MessageBus>,
    gui_elements_state: Arc<Mutex<GuiElementsState>>,
    gui_persisted_state: Arc<Mutex<GuiPersistedState>>,
    last_gui_state_snapshot: Option<GuiPersistedState>,
    last_render_targets_revision: Option<u64>,
}

impl Drop for FfmpegWindowState {
    fn drop(&mut self) {
        self.is_alive.store(false, Ordering::Relaxed);
    }
}

pub fn ffmpeg_gui() -> Result<(), Box<dyn Error>> {
    let rpr = Reaper::get_mut();
    let id_string = BACKEND_ID_STRING.to_string();

    if rpr.has_control_surface(&id_string) {
        rpr.unregister_control_surface(id_string)?;
        return Ok(());
    }

    let backend = FfmpegGuiSurface::new()?;
    rpr.register_control_surface(Arc::new(RefCell::new(backend)));
    Ok(())
}
