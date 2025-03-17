use std::{
    fs::File,
    io::BufReader,
    path::PathBuf,
    process::{Child, Command},
    sync::{
        mpsc::{self, Receiver},
        Arc, Mutex,
    },
    thread::spawn,
};

use log::{debug, error};
use rea_rs::{
    socket::{Broadcaster, SocketHandle},
    ControlSurface, ExtState, Reaper,
};
use render_settings::render_settings;
use serde::{Deserialize, Serialize};
use vizia::prelude::*;
use vizia::{model::Model, Application, ApplicationError};

use super::{
    options::Muxer,
    parser::{check_parsed_paths, muxers_path, parse_all, ParsingProgress},
    RenderSettings,
};
use crate::LevitanusError;
use small_widgets::widget_parser;

mod render_settings;
mod small_widgets;

pub static BACKEND_ID_STRING: &str = "LevitanusFfmpegGui";
pub static SOCKET_ADDRESS: &str = "127.0.0.1:49332";
static PERSIST: bool = false;
pub static EXT_SECTION: &str = "Levitanus";
pub static EXT_STATE_KEY: &str = "FFMPEG_FrontState";

#[derive(Debug, Serialize, Deserialize, Clone)]
enum IppMessage {
    Init,
    State(State),
    Mutate(StateMessage),
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
        let rpr = Reaper::get_mut();
        let (sockets, broadcaster) = rea_rs::socket::spawn_server(SOCKET_ADDRESS);
        let pr = rpr.current_project();
        rea_rs::ExtState::new(EXT_SECTION, EXT_STATE_KEY, State::default(), PERSIST, &pr);
        Ok(Backend {
            front,
            sockets,
            broadcaster,
        })
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

        // let status = self.front.try_wait();
        // if clients.len() > 0 && status.is_err() || status.ok().is_some() {
        //     drop(clients);
        //     self.stop();
        //     return Ok(());
        // }

        let rpr = Reaper::get();
        let pr = rpr.current_project();
        let mut ext_state = ExtState::new(EXT_SECTION, EXT_STATE_KEY, None, PERSIST, &pr);
        let mut state = ext_state.get().unwrap_or(State::default());

        let mut shutdown = false;

        for client in clients.iter_mut() {
            for message in client.try_iter() {
                debug!("server recieved a message: {:?}", message);
                match message {
                    IppMessage::Init => client.send(IppMessage::State(state.clone()))?,
                    IppMessage::State(state) => ext_state.set(state),
                    IppMessage::Mutate(msg) => state.update(msg),
                    IppMessage::Shutdown => shutdown = true,
                }
            }
        }
        ext_state.set(state);
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
        "ffmpeg back-end front-end subprocess, that communicates with front-end".to_string()
    }
}

#[derive(Debug, Lens)]
struct FrontState {
    gui_state: State,
    socket: SocketHandle<IppMessage>,
    parsing_progress: ParsingProgress,
    parsing_progress_f32: f32,
    parser_channel: Option<Receiver<ParsingProgress>>,
    muxers: Vec<Muxer>,
}
impl FrontState {
    fn new(gui_state: State, socket: SocketHandle<IppMessage>) -> Self {
        let parsing_progress = match check_parsed_paths(gui_state.json_path.clone()) {
            true => ParsingProgress::Result(Ok(())),
            false => ParsingProgress::Unparsed,
        };
        let muxers = Self::build_muxers_list(&gui_state.json_path, &parsing_progress)
            .expect("can not build muxers list");
        Self {
            gui_state,
            socket,
            parsing_progress,
            parsing_progress_f32: f32::default(),
            parser_channel: None,
            muxers,
        }
    }
    fn parse(&mut self) {
        let (tx, rx) = mpsc::channel::<ParsingProgress>();
        self.parser_channel = Some(rx);
        let path = self.gui_state.json_path.clone();
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
            _ => Ok(vec![Muxer::default()]),
        }
    }
}
impl Model for FrontState {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|app_event, meta| match app_event {
            FrontMessage::Frame => {
                for msg in self.socket.try_iter() {
                    debug!("Client recieved a message: {:?}", msg);
                    match msg {
                        IppMessage::Init => panic!(
                            "Init message is recieved by the client. This has not to be happened."
                        ),
                        IppMessage::State(state) => self.gui_state = state,
                        IppMessage::Mutate(msg) => self.gui_state.update(msg),
                        IppMessage::Shutdown => cx.close_window(),
                    }
                }
                if let Some(rx) = self.parser_channel.as_ref() {
                    match rx.try_recv() {
                        Ok(v) => {
                            match v {
                                ParsingProgress::Progress(p) => self.parsing_progress_f32 = p,
                                _ => (),
                            }
                            self.parsing_progress = v;
                        }
                        Err(e) => match e {
                            mpsc::TryRecvError::Empty => (),
                            mpsc::TryRecvError::Disconnected => {
                                // self.parsing_progress = ParsingProgress::Result(Ok(()));
                                self.parser_channel = None
                            }
                        },
                    }
                }
            }
            FrontMessage::Mutate(msg) => {
                self.gui_state.update(msg.clone());
                match self.socket.send(IppMessage::Mutate(msg.clone())) {
                    Ok(_) => (),
                    Err(e) => error!("Can not send mutate message: {}", e),
                };
            }
            FrontMessage::Closed => {
                self.socket.send(IppMessage::Shutdown).ok();
                self.socket.shutdown_all().ok();
            }
            FrontMessage::Parse => {
                debug!("Recieved Parse message.");
                self.parsing_progress = ParsingProgress::Progress(0.001);
                self.parse();
            }
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
enum StateMessage {
    VideoMuxer(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, Lens)]
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
impl State {
    fn update(&mut self, msg: StateMessage) {
        match msg {
            StateMessage::VideoMuxer(json) => {
                let muxer: Muxer =
                    serde_json::from_str(&json).expect("can not deserialize current muxer");
                self.render_settings.muxer = muxer;
            }
        }
    }
}

enum FrontMessage {
    Frame,
    Mutate(StateMessage),
    Closed,
    Parse,
}

pub fn front() -> Result<(), ApplicationError> {
    Application::new(|cx| {
        let socket = match rea_rs::socket::spawn_client(SOCKET_ADDRESS) {
            Ok(s) => s,
            Err(e) => {
                VStack::new(cx, |cx| {
                    Label::new(cx, "Socket is not connected. The error is:");
                    Label::new(cx, e.to_string());
                });
                return;
            }
        };
        debug!("Front is sending Init Message");
        socket.send(IppMessage::Init).unwrap();
        debug!("Front is waiting for Init state");
        let gui_state = match socket.recv().unwrap() {
            IppMessage::State(s) => s,
            _ => panic!("not a state"),
        };
        debug!("front is building state");
        FrontState::new(gui_state, socket).build(cx);
        cx.emit(EnvironmentEvent::SetThemeMode(AppTheme::BuiltIn(
            ThemeMode::DarkMode,
        )));

        VStack::new(cx, |cx| {
            VStack::new(cx, |cx| {
                render_settings(cx);
            });
            // Parser Block
            widget_parser(cx);
        });
    })
    .title("FFMPEG render")
    .should_poll()
    .on_idle(|cx| {
        cx.emit(FrontMessage::Frame);
    })
    .on_close(|ex| {
        ex.emit(FrontMessage::Closed);
    })
    .run()
}
