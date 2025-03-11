use std::{
    process::{Child, Command},
    sync::{Arc, Mutex},
};

use log::{debug, error};
use rea_rs::{
    socket::{Broadcaster, SocketHandle},
    ControlSurface, ExtState, Reaper,
};
use serde::{Deserialize, Serialize};
use vizia::prelude::*;
use vizia::{model::Model, Application, ApplicationError};

use crate::LevitanusError;

pub static BACKEND_ID_STRING: &str = "LevitanusFfmpegGui";
pub static SOCKET_ADDRESS: &str = "127.0.0.1:49332";
static PERSIST: bool = true;
pub static EXT_SECTION: &str = "Levitanus";
pub static EXT_STATE_KEY: &str = "FFMPEG_FrontState";

#[derive(Debug, Serialize, Deserialize, Clone)]
enum IppMessage {
    Init,
    State(State),
    Mutate(StateMessage),
    Shutdown,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
enum StateMessage {
    Inc,
    Dec,
}

#[derive(Debug, Clone, Serialize, Deserialize, Lens)]
struct State {
    counter: u64,
}
impl Default for State {
    fn default() -> Self {
        State { counter: 0 }
    }
}
impl State {
    fn update(&mut self, msg: StateMessage) {
        match msg {
            StateMessage::Inc => self.counter += 1,
            StateMessage::Dec => self.counter -= 1,
        }
    }
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
        })
    }
}

enum FrontMessage {
    Frame,
    Mutate(StateMessage),
    Closed,
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
        FrontState { gui_state, socket }.build(cx);

        HStack::new(cx, |cx| {
            Button::new(cx, |cx| Label::new(cx, "Inc"))
                .on_press(|ex| ex.emit(FrontMessage::Mutate(StateMessage::Inc)));
            Label::new(cx, FrontState::gui_state.then(State::counter));
            Button::new(cx, |cx| Label::new(cx, "Dec"))
                .on_press(|ex| ex.emit(FrontMessage::Mutate(StateMessage::Dec)));
        });
    })
    .should_poll()
    .on_idle(|cx| {
        cx.emit(FrontMessage::Frame);
    })
    .on_close(|ex| {
        ex.emit(FrontMessage::Closed);
    })
    .run()
}
