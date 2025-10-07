use anyhow::Error;
use egui::style::ScrollStyle;
use log::{debug, warn};
use rea_rs::{
    socket::{self, Broadcaster, SocketHandle},
    ControlSurface, ExtState, Project, Reaper,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::{
    path::Component,
    sync::mpsc::{self, Receiver, Sender},
};

use crate::{
    gui::{get_front_socket_address, widget_error_box, ExitCode},
    LevitanusError, EXT_SECTION,
};

pub static BACKEND_ID_STRING: &str = "SAMPLE_EDITOR_BACKEND";
static EXT_STATE_KEY: &str = "SAMPLE_EDITOR_FrontState";
pub static PERSIST: bool = true;
pub static SOCKET_PORT: u16 = 49338;

// === Message enum for sample editor protocol ===
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IppMessage {
    Init,
    State(State),
    Shutdown,
    // Add more messages as needed
}

// === State struct for sample editor ===
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct State {}

// === Backend for sample editor ===
#[derive(Debug)]
pub struct Backend {
    sockets: Arc<Mutex<Vec<SocketHandle<IppMessage>>>>,
    broadcaster: Broadcaster,
}

impl Backend {
    pub fn new() -> anyhow::Result<Backend> {
        let socket_address =
            crate::gui::get_front_socket_address(crate::gui::ComponentType::SampleEditor);
        let (sockets, broadcaster) = rea_rs::socket::spawn_server(socket_address);
        Ok(Backend {
            sockets,
            broadcaster,
        })
    }
    fn ext_state(pr: &Project) -> ExtState<State, Project> {
        ExtState::new(EXT_SECTION, EXT_STATE_KEY, None, PERSIST, pr, None)
    }
}

impl ControlSurface for Backend {
    fn run(&mut self) -> anyhow::Result<()> {
        let mut clients = match self.sockets.lock() {
            Ok(m) => m,
            Err(e) => return Err(anyhow::anyhow!(format!("Poisoned lock: {}", e))),
        };
        let rpr = Reaper::get();
        let pr = rpr.current_project();
        let mut shutdown = false;
        for client in clients.iter_mut() {
            for message in client.try_iter() {
                match message {
                    IppMessage::Init => {
                        client.send(IppMessage::State(
                            Self::ext_state(&pr).get()?.unwrap_or_default(),
                        ))?;
                    }
                    IppMessage::State(state) => {
                        Self::ext_state(&pr).set(state);
                    }
                    IppMessage::Shutdown => {
                        shutdown = true;
                    } // Add more message handling as needed
                }
            }
        }
        if shutdown {
            drop(clients); // Release the immutable borrow before mutable borrow
            self.stop();
        }
        Ok(())
    }
    fn get_type_string(&self) -> String {
        BACKEND_ID_STRING.to_string()
    }
    fn get_desc_string(&self) -> String {
        "sample editor back-end subprocess, that communicates with front-end".to_string()
    }
}

// === Frontend for sample editor ===
#[derive(Debug)]
pub struct Front {
    pub state: State,
    pub socket: SocketHandle<IppMessage>,
    pub exit_code: Option<ExitCode>,
    pub msg_rx: Receiver<FrontMessage>,
    pub msg_tx: Sender<FrontMessage>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum FrontMessage {
    Exit,
    Error(String),
    // Add more as needed
}

impl Front {
    pub fn new(state: State, socket: SocketHandle<IppMessage>) -> Self {
        let (msg_tx, msg_rx) = mpsc::channel();
        Self {
            state,
            socket,
            exit_code: None,
            msg_rx,
            msg_tx,
        }
    }
    pub fn poll_messages(&mut self) -> anyhow::Result<()> {
        // Poll messages from backend (socket)
        for msg in self.socket.try_iter() {
            match msg {
                IppMessage::Init => {
                    warn!("Received Init message in frontend, which is unexpected");
                }
                IppMessage::State(state) => {
                    self.state = state;
                }
                IppMessage::Shutdown => {
                    self.exit_code = Some(ExitCode::Shutdown);
                } // Add more message handling as needed
            }
        }
        // Poll messages from UI (msg_rx) if needed
        Ok(())
    }
}
impl eframe::App for Front {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll messages from backend
        if let Err(e) = self.poll_messages() {
            self.exit_code = Some(ExitCode::Error(e.to_string()));
        }
        if let Some(code) = &self.exit_code {
            match code {
                ExitCode::Shutdown => return ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                ExitCode::Error(e) => {
                    widget_error_box(ctx, e);
                    return;
                }
            }
        }

        // Draw UI
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Sample Editor");
            if ui.button("Exit").clicked() {
                self.msg_tx.send(FrontMessage::Exit).unwrap_or(());
            }
        });

        // Request a repaint to keep the UI responsive
        ctx.request_repaint();
    }
}

// === Launch the sample editor frontend ===
pub fn front() -> anyhow::Result<()> {
    let native_options = eframe::NativeOptions::default();
    let address = get_front_socket_address(crate::gui::ComponentType::SampleEditor);
    let socket = socket::spawn_client(address)?;
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
        "Levitanus sample editor",
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
