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
use egui::style::ScrollStyle;
use log::{debug, error};
use rea_rs::{
    socket::{self, Broadcaster, SocketHandle},
    ControlSurface, ExtState, Project, Reaper,
};
use serde::{Deserialize, Serialize};

use super::{
    options::{Encoder, Muxer, Opt},
    parser::{check_parsed_paths, encoders_path, muxers_path, parse_all, ParsingProgress},
    RenderSettings,
};
use crate::LevitanusError;

mod render_settings;
mod small_widgets;

static PERSIST: bool = true;
pub static BACKEND_ID_STRING: &str = "LevitanusFfmpegGui";
pub static SOCKET_ADDRESS: &str = "127.0.0.1:49332";
pub static EXT_SECTION: &str = "Levitanus";
pub static EXT_STATE_KEY: &str = "FFMPEG_FrontState";

#[derive(Debug, Serialize, Deserialize, Clone)]
enum IppMessage {
    Init,
    State(State),
    Shutdown,
}

#[derive(Debug)]
pub struct Backend {
    front: Child,
    sockets: Arc<Mutex<Vec<SocketHandle<IppMessage>>>>,
    broadcaster: Broadcaster,
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
        })
    }
    fn ext_state(pr: &Project) -> ExtState<State, Project> {
        ExtState::new(EXT_SECTION, EXT_STATE_KEY, None, PERSIST, pr, None)
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
        let mut clients = match self.sockets.lock() {
            Ok(m) => m,
            Err(e) => return Err(LevitanusError::Poison(e.to_string()).into()),
        };

        let rpr = Reaper::get();
        let pr = rpr.current_project();

        let mut shutdown = false;

        for client in clients.iter_mut() {
            for message in client.try_iter() {
                debug!("server recieved a message: {:#?}", message);
                match message {
                    IppMessage::Init => client.send(IppMessage::State(
                        Self::ext_state(&pr).get()?.unwrap_or(State::default()),
                    ))?,
                    IppMessage::State(msg) => Self::ext_state(&pr).set(msg),
                    IppMessage::Shutdown => shutdown = true,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct State {
    json_path: PathBuf,
    render_settings: RenderSettings,
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
    muxers: Vec<Muxer>,
    encoders: Vec<Encoder>,
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

        let (msg_tx, msg_rx) = channel();
        Self {
            state: gui_state,
            socket,
            exit_code: None,
            msg_rx,
            msg_tx,
            parsing_progress,
            parser_channel: None,
            muxers,
            encoders,
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
    fn poll_messages(&mut self) -> anyhow::Result<()> {
        for msg in self.msg_rx.try_iter().collect::<Vec<FrontMessage>>() {
            match msg {
                FrontMessage::Parse => self.parse(),
                FrontMessage::Exit => self.exit_code = Some(ExitCode::Shutdown),
                FrontMessage::Error(e) => return Err(Error::msg(e)),
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
        for msg in self.socket.try_iter() {
            match msg {
                IppMessage::Init => panic!("recieved init message during the loop."),
                IppMessage::State(s) => self.state = s,
                IppMessage::Shutdown => self.exit_code = Some(ExitCode::Shutdown),
            }
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
        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| self.widget_parser(ctx, ui));
        egui::CentralPanel::default().show(ctx, |ui| {
            self.widget_render_settings(ctx, ui);
        });
        ctx.request_repaint_after(Duration::from_millis(200));
    }
    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        debug!("on save ()");
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
    let state = match socket.recv()? {
        IppMessage::State(s) => s,
        msg => {
            return Err(LevitanusError::FrontInitialization(format!(
                "Recieved another message instead of front initialization state: {:?}",
                msg
            ))
            .into())
        }
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
