use std::{
    fs::File,
    io::BufReader,
    path::PathBuf,
    process::{Child, Command},
    sync::{
        mpsc::{self, channel, Receiver, Sender},
        Arc, Mutex,
    },
    thread::spawn,
    time::Duration,
};

use anyhow::Error;
use egui::{style::ScrollStyle, ScrollArea};
use filters_widget::{FilterChain, FlitersWidget, SelectedVideoItem};
use log::{debug, error};
use rea_rs::{
    socket::{self, Broadcaster, SocketHandle},
    ControlSurface, ExtState, Mutable, Project, Reaper, Track, WithReaperPtr, GUID,
};
use render_widget::RenderJob;
use serde::{Deserialize, Serialize};

use super::{
    base::{get_filters, SerializedFilter, TimeLine},
    base_types::{framerate_from_video, Resolution},
    options::{Encoder, Muxer, Opt, ParsedFilter},
    parser::{
        check_parsed_paths, encoders_path, filters_path, muxers_path, parse_all, ParsingProgress,
    },
    RenderSettings,
};
use crate::{
    ffmpeg::base::{build_render_timelines, set_filters},
    LevitanusError,
};

mod filters_widget;
mod render_settings;
mod render_widget;
mod small_widgets;

pub static PERSIST: bool = true;
pub static BACKEND_ID_STRING: &str = "LevitanusFfmpegGui";
pub static SOCKET_ADDRESS: &str = "127.0.0.1:49332";
pub static EXT_SECTION: &str = "Levitanus";
pub static EXT_STATE_KEY: &str = "FFMPEG_FrontState";

#[derive(Debug, Serialize, Deserialize, Clone)]
enum IppMessage {
    Init,
    State(State),
    Shutdown,
    GetCurrentVideoItem,
    SetCurrentVideoItem(PathBuf),
    BuildRenderSequence(RenderSettings),
    RenderSequence(Vec<TimeLine>),
    OnSelectedVideoItem(SelectedVideoItem),
    UpdateFilters(SelectedVideoItem),
}

#[derive(Debug)]
pub struct Backend {
    front: Child,
    sockets: Arc<Mutex<Vec<SocketHandle<IppMessage>>>>,
    broadcaster: Broadcaster,
    last_video_item_guid: Option<GUID>,
    last_video_item_selection: bool,
}
impl Backend {
    pub fn new() -> anyhow::Result<Backend> {
        let front =
            Command::new("/home/levitanus/gits/reaper-levitanus/target/debug/front").spawn()?;
        let (sockets, broadcaster) = rea_rs::socket::spawn_server(SOCKET_ADDRESS);
        Ok(Backend {
            front,
            sockets,
            broadcaster,
            last_video_item_guid: None,
            last_video_item_selection: false,
        })
    }
    fn ext_state(pr: &Project) -> ExtState<State, Project> {
        ExtState::new(EXT_SECTION, EXT_STATE_KEY, None, PERSIST, pr, None)
    }
    fn get_current_video_item(&self) -> Option<PathBuf> {
        if let Some(guid) = self.last_video_item_guid {
            let rpr = Reaper::get();
            let pr = rpr.current_project();
            if let Some(item) = pr.iter_items().find(|it| it.guid() == guid) {
                if let Some(source) = item.active_take().source() {
                    return Some(source.filename());
                }
            }
        }
        None
    }
}
impl Drop for Backend {
    fn drop(&mut self) {
        self.broadcaster.shutdown().ok();
        self.front.kill().ok();
    }
}
impl Backend {}
impl ControlSurface for Backend {
    fn run(&mut self) -> anyhow::Result<()> {
        if self.sockets.is_poisoned() {
            self.stop();
            if let Err(e) = self.sockets.lock() {
                return Err(LevitanusError::Poison(e.to_string()).into());
            }
        }
        let mut clients = match self.sockets.lock() {
            Ok(m) => m,
            Err(e) => return Err(LevitanusError::Poison(e.to_string()).into()),
        };

        let rpr = Reaper::get();
        let mut pr = rpr.current_project();
        if let Some(item) = pr.get_selected_item_mut(0) {
            if match self.last_video_item_guid {
                None => true,
                Some(guid) => item.guid() != guid || self.last_video_item_selection == false,
            } {
                self.last_video_item_selection = true;
                if let Some(source) = item.active_take().source() {
                    if source.type_string() == "VIDEO" {
                        self.last_video_item_guid = Some(item.guid());
                        let imm_pr = rpr.current_project();
                        let track = Track::<Mutable>::new(&imm_pr, item.track().get());
                        let track_name = format!("{}: {}", track.index() + 1, track.name());
                        let track_guid = track.guid().to_string();
                        let item_name = item.active_take().name();
                        let item_guid = item.guid().to_string();
                        let track_filters = get_filters(&track);
                        let item_filters = get_filters(&item);
                        for client in clients.iter() {
                            client.send(IppMessage::OnSelectedVideoItem(SelectedVideoItem {
                                track_name: track_name.clone(),
                                track_guid: track_guid.clone(),
                                track_filters: track_filters.clone(),
                                item_name: item_name.clone(),
                                item_guid: item_guid.clone(),
                                item_filters: item_filters.clone(),
                            }))?;
                        }
                    }
                }
            }
        } else {
            self.last_video_item_selection = false;
        }

        let mut shutdown = false;

        for client in clients.iter_mut() {
            for message in client.try_iter() {
                // debug!("server recieved a message: {:#?}", message);
                match message {
                    IppMessage::Init => client.send(IppMessage::State(
                        Self::ext_state(&pr).get()?.unwrap_or(State::default()),
                    ))?,
                    IppMessage::State(msg) => {
                        let mut state = Self::ext_state(&pr);
                        if state.get()?.unwrap_or(State::default()) != msg {
                            state.set(msg);
                            pr.mark_dirty();
                        }
                    }
                    IppMessage::Shutdown => shutdown = true,
                    IppMessage::GetCurrentVideoItem => {
                        if let Some(file) = self.get_current_video_item() {
                            client.send(IppMessage::SetCurrentVideoItem(file))?
                        }
                    }
                    IppMessage::SetCurrentVideoItem(_) => error!("recieved current video item"),
                    IppMessage::BuildRenderSequence(s) => {
                        client.send(IppMessage::RenderSequence(build_render_timelines(&s)?))?
                    }
                    IppMessage::RenderSequence(_) => error!("recieved render_sequence on back-end"),
                    IppMessage::OnSelectedVideoItem(_) => {
                        error!("recieved OnSelectedVideoItem on back-end")
                    }
                    IppMessage::UpdateFilters(item) => {
                        // debug!("recieved UpdateFilters{:#?}", item);
                        for tr_index in 0..pr.n_tracks() {
                            // let tr_guid = GUID::from_string(item.track_guid.clone())?;
                            // let it_guid = GUID::from_string(item.item_guid.clone())?;
                            let mut tr = pr
                                .get_track_mut(tr_index)
                                .expect("no track with te given index");
                            // debug!("trck guid: {}", tr.guid().to_string());
                            if tr.guid().to_string() == item.track_guid {
                                for idx in 0..tr.n_items() {
                                    let mut tr_item = tr
                                        .get_item(idx)
                                        .expect(&format!("no item with index {}", idx));
                                    // debug!("item guid: {}", tr_item.guid().to_string());
                                    if tr_item.guid().to_string() == item.item_guid {
                                        set_filters(&mut tr_item, item.item_filters.clone());
                                        set_filters(&mut tr, item.track_filters.clone());
                                        return Ok(());
                                    }
                                }
                            }
                        }
                        return Err(LevitanusError::KeyError(
                            "Project".to_string(),
                            format!("track: {}, item: {}", item.track_name, item.item_name),
                        )
                        .into());
                    }
                }
            }
        }
        if shutdown {
            drop(clients);
            self.stop();
        }
        Ok(())
    }
    fn get_type_string(&self) -> String {
        BACKEND_ID_STRING.to_string()
    }
    fn get_desc_string(&self) -> String {
        "ffmpeg back-end subprocess, that communicates with front-end".to_string()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
enum StateMessage {
    Muxer(Muxer),
    MuxerOptions(Vec<Opt>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct State {
    json_path: PathBuf,
    render_settings: RenderSettings,
    parallel_render: bool,
    master_filters: Vec<SerializedFilter>,
}
impl Default for State {
    fn default() -> Self {
        let rpr = Reaper::get();
        let json_path = PathBuf::from(rpr.get_resource_path().expect("can not get resource path"))
            .join("Data")
            .join("reaper-levitanus");
        State {
            json_path,
            render_settings: RenderSettings::default(),
            parallel_render: true,
            master_filters: Vec::new(),
        }
    }
}

#[derive(Debug)]
enum ExitCode {
    Shutdown,
    Error(String),
}

#[derive(Debug)]
enum FrontMessage {
    Parse,
    Exit,
    Error(String),
    AlternativeValue(String),
    GetResolution,
    GetFrameRate,
    Render,
    UpdateFilters(FilterChain),
}

#[derive(Debug)]
struct Front {
    state: State,
    socket: SocketHandle<IppMessage>,
    exit_code: Option<ExitCode>,
    msg_rx: Receiver<FrontMessage>,
    msg_tx: Sender<FrontMessage>,
    parsing_progress: ParsingProgress,
    parser_channel: Option<Receiver<ParsingProgress>>,
    render_jobs: Vec<RenderJob>,
    alternative_value: String,
    muxers: Vec<Muxer>,
    encoders: Vec<Encoder>,
    filters: Vec<ParsedFilter>,
    filters_widget: FlitersWidget,
}
impl Front {
    fn new(gui_state: State, socket: SocketHandle<IppMessage>) -> Self {
        let parsing_progress = match check_parsed_paths(gui_state.json_path.clone()) {
            true => ParsingProgress::Result(Ok(())),
            false => ParsingProgress::Unparsed,
        };
        let muxers = Self::build_muxers_list(&gui_state.json_path, &parsing_progress)
            .expect("can not build muxers list")
            .into_iter()
            .filter(|mux| mux.video_codec.is_some() && mux.extensions.is_some())
            .collect();
        let encoders = Self::build_encoders_list(&gui_state.json_path, &parsing_progress)
            .expect("can not build encoders list");
        let filters = Self::build_filters_list(&gui_state.json_path, &parsing_progress)
            .expect("can not build filters list");

        let (msg_tx, msg_rx) = channel();
        Self {
            state: gui_state,
            socket,
            exit_code: None,
            msg_rx,
            msg_tx,
            parsing_progress,
            parser_channel: None,
            render_jobs: Vec::new(),
            alternative_value: String::default(),
            muxers,
            encoders,
            filters,
            filters_widget: FlitersWidget::new(),
        }
    }
    fn parse(&mut self) {
        let (tx, rx) = mpsc::channel::<ParsingProgress>();
        self.parser_channel = Some(rx);
        self.parsing_progress = ParsingProgress::Progress(0.0);
        let path = self.state.json_path.clone();
        spawn(move || {
            parse_all(path, tx).expect("can not parse all");
        });
    }
    fn build_muxers_list(
        json_path: &PathBuf,
        progress: &ParsingProgress,
    ) -> anyhow::Result<Vec<Muxer>> {
        match progress {
            ParsingProgress::Result(Ok(_)) => {
                let file = File::open(muxers_path(json_path))?;
                let reader = BufReader::new(file);
                Ok(serde_json::from_reader(reader)?)
            }
            _ => Ok(Vec::new()),
        }
    }
    fn build_encoders_list(
        json_path: &PathBuf,
        progress: &ParsingProgress,
    ) -> anyhow::Result<Vec<Encoder>> {
        match progress {
            ParsingProgress::Result(Ok(_)) => {
                let file = File::open(encoders_path(json_path))?;
                let reader = BufReader::new(file);
                Ok(serde_json::from_reader(reader)?)
            }
            _ => Ok(Vec::new()),
        }
    }
    fn build_filters_list(
        json_path: &PathBuf,
        progress: &ParsingProgress,
    ) -> anyhow::Result<Vec<ParsedFilter>> {
        match progress {
            ParsingProgress::Result(Ok(_)) => {
                let file = File::open(filters_path(json_path))?;
                let reader = BufReader::new(file);
                Ok(serde_json::from_reader(reader)?)
            }
            _ => Ok(Vec::new()),
        }
    }
    fn poll_messages(&mut self) -> anyhow::Result<()> {
        for msg in self.msg_rx.try_iter().collect::<Vec<FrontMessage>>() {
            match msg {
                FrontMessage::Parse => self.parse(),
                FrontMessage::Exit => self.exit_code = Some(ExitCode::Shutdown),
                FrontMessage::Error(e) => return Err(Error::msg(e)),
                FrontMessage::AlternativeValue(s) => self.alternative_value = s,
                FrontMessage::GetFrameRate => {
                    self.socket.send(IppMessage::GetCurrentVideoItem)?;
                    if let Ok(file) = self.socket.recv() {
                        if let IppMessage::SetCurrentVideoItem(file) = file {
                            self.state.render_settings.fps = framerate_from_video(file)?;
                        }
                    }
                }
                FrontMessage::GetResolution => {
                    self.socket.send(IppMessage::GetCurrentVideoItem)?;
                    if let Ok(file) = self.socket.recv() {
                        if let IppMessage::SetCurrentVideoItem(file) = file {
                            self.state.render_settings.resolution = Resolution::from_file(file)?;
                        }
                    }
                }
                FrontMessage::Render => self.socket.send(IppMessage::BuildRenderSequence(
                    self.state.render_settings.clone(),
                ))?,
                FrontMessage::UpdateFilters(chain) => {
                    // debug!("FrontMessage::UpdateFilters({:#?}) ", chain);
                    match chain {
                        FilterChain::Item | FilterChain::Track => {
                            match &self.filters_widget.selected_video_item {
                                Some(i) => {
                                    self.socket.send(IppMessage::UpdateFilters(i.clone()))?
                                }
                                None => self.emit(FrontMessage::Error(
                                    "empty selected video item on update filters".to_string(),
                                )),
                            }
                        }
                        FilterChain::Master => {
                            self.socket.send(IppMessage::State(self.state.clone()))?
                        }
                    }
                }
            }
        }
        if let Some(rx) = &self.parser_channel {
            for prg in rx.try_iter() {
                self.parsing_progress = prg;
                if let ParsingProgress::Result(Ok(_)) = self.parsing_progress {
                    self.muxers =
                        Self::build_muxers_list(&self.state.json_path, &self.parsing_progress)?;
                    self.encoders =
                        Self::build_encoders_list(&self.state.json_path, &self.parsing_progress)?;
                }
            }
        }
        for msg in self.socket.try_iter().collect::<Vec<IppMessage>>() {
            match msg {
                IppMessage::Init => panic!("recieved init message during the loop."),
                IppMessage::State(s) => self.state = s,
                IppMessage::Shutdown => self.exit_code = Some(ExitCode::Shutdown),
                IppMessage::BuildRenderSequence(_) => {
                    error!("recieved BuildRenderSequence message on front-end")
                }
                IppMessage::RenderSequence(s) => self.render(s)?,
                IppMessage::GetCurrentVideoItem => {
                    error!("recieved GetCurrentVideoItem mesge on font-end")
                }
                IppMessage::SetCurrentVideoItem(file) => {
                    error!("recieved SetCurrentVideoItem({:?}) in polling", file)
                }
                IppMessage::OnSelectedVideoItem(item) => {
                    self.filters_widget.selected_video_item = Some(item)
                }
                IppMessage::UpdateFilters(i) => {
                    error!("recieved UpdateFilters({:?}) in polling", i)
                }
            }
        }
        for job in self.render_jobs.iter_mut() {
            job.poll()?;
        }
        Ok(())
    }
    fn emit(&self, message: FrontMessage) {
        self.msg_tx
            .send(message)
            .expect("front message channel is corrupted");
    }
}
impl eframe::App for Front {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if let Err(e) = self.poll_messages() {
            self.exit_code = Some(ExitCode::Error(e.to_string()));
        }
        if let Some(code) = &self.exit_code {
            match code {
                ExitCode::Shutdown => return ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                ExitCode::Error(e) => {
                    self.widget_error_box(ctx, e);
                    return;
                }
            }
        }
        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
            ui.horizontal(|ui| {
                self.widget_render(ctx, ui);
                ui.separator();
                self.widget_parser(ctx, ui);
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    self.widget_filters(ctx, ui);
                    self.widget_render_settings(ctx, ui);
                });
        });
        ctx.request_repaint_after(Duration::from_millis(200));
    }
    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        // debug!("on save ()");
        match self.socket.send(IppMessage::State(self.state.clone())) {
            Ok(()) => (),
            Err(e) => {
                let msg = format!("Can not save state in reaper.\nThe error is: {}", e);
                error!("{}", msg);
                self.exit_code = Some(ExitCode::Error(msg))
            }
        }
    }
}

pub fn front() -> anyhow::Result<()> {
    let native_options = eframe::NativeOptions::default();
    let socket = socket::spawn_client(SOCKET_ADDRESS)?;
    socket.send(IppMessage::Init)?;
    let state = {
        let mut state: Result<State, LevitanusError> = Err(LevitanusError::FrontInitialization(
            "did't recieved any message from back-end".to_owned(),
        ));
        for msg in socket.iter() {
            if let IppMessage::State(s) = msg {
                state = Ok(s);
                break;
            } else {
                state = Err(LevitanusError::FrontInitialization(format!(
                    "Recieved another message instead of front initialization state: {:?}",
                    msg
                )));
            }
        }
        state?
    };
    debug!("state is: {:#?}", state);
    let app = Front::new(state, socket);
    match eframe::run_native(
        "Levitanus FFMPEG render",
        native_options,
        Box::new(|cc| {
            cc.egui_ctx.style_mut(|s| {
                let mut style = ScrollStyle::floating();
                style.floating_allocated_width = 10.0;
                s.spacing.scroll = style;
            });
            Ok(Box::new(app))
        }),
    ) {
        Ok(r) => Ok(r),
        Err(e) => Err(LevitanusError::Unexpected(e.to_string()).into()),
    }
}
