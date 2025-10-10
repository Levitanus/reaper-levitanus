use anyhow::Error;
use dasp_rs::estimate_tuning;
use egui::style::ScrollStyle;
use log::{debug, warn};
use rea_rs::{
    socket::{self, Broadcaster, SocketHandle},
    ControlSurface, ExtState, Project, Reaper,
};
use serde::{Deserialize, Serialize};
use std::{
    fmt::format,
    sync::{Arc, Mutex},
    time::Duration,
};
use std::{
    path::Component,
    sync::mpsc::{self, Receiver, Sender},
};

use crate::{
    gui::{get_front_socket_address, widget_error_box, ExitCode},
    sample_editor::{get_regions_in_time_selection, Region, RegionInfo, TrackInfo},
    LevitanusError, EXT_SECTION,
};

pub static BACKEND_ID_STRING: &str = "SAMPLE_EDITOR_BACKEND";
static EXT_STATE_KEY: &str = "SAMPLE_EDITOR_FrontState";
pub static PERSIST: bool = true;
pub static SOCKET_PORT: u16 = 49338;
pub static DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);

// === Message enum for sample editor protocol ===
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IppMessage {
    Init,
    State(State),
    Shutdown,
    GetRegionsInTimeSelection,
    SendRegionsInTimeSelection(Vec<RegionInfo>),
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
    analyze_tracks: Vec<TrackInfo>,
}

impl Backend {
    pub fn new() -> anyhow::Result<Backend> {
        let socket_address =
            crate::gui::get_front_socket_address(crate::gui::ComponentType::SampleEditor);
        let (sockets, broadcaster) = rea_rs::socket::spawn_server(socket_address);
        let mut analyze_tracks = Vec::new();
        for idx in [2, 3] {
            if let Some(track) = Reaper::get().current_project().get_track(idx) {
                analyze_tracks.push(TrackInfo::from(track));
            }
        }
        Ok(Backend {
            sockets,
            broadcaster,
            analyze_tracks,
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
                    IppMessage::GetRegionsInTimeSelection => {
                        let analyze_tracks = match self.analyze_tracks.is_empty() {
                            true => None,
                            false => Some(&self.analyze_tracks),
                        };
                        client.send(IppMessage::SendRegionsInTimeSelection(
                            get_regions_in_time_selection(analyze_tracks.map(|v| &**v)),
                        ))?;
                    }
                    IppMessage::SendRegionsInTimeSelection(_) => (),
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
    PrintRootNotes,
    PrintLegatoIntervals,
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
    pub fn emit(&mut self, msg: FrontMessage) {
        if let Err(e) = self.msg_tx.send(msg) {
            self.exit_code = Some(ExitCode::Error(format!(
                "No connection in internal gui message channel: {}",
                e
            )));
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
                IppMessage::GetRegionsInTimeSelection => (),
                IppMessage::SendRegionsInTimeSelection(regions) => {
                    debug!("Received regions in time selection: {:#?}", regions);
                }
            }
        }
        // Collect UI messages first to avoid borrow checker issues
        let ui_msgs: Vec<_> = self.msg_rx.try_iter().collect();
        for msg in ui_msgs {
            match msg {
                FrontMessage::Exit => {
                    // Handle UI exit event
                    self.exit_code = Some(ExitCode::Shutdown);
                }
                FrontMessage::Error(e) => {
                    self.exit_code = Some(ExitCode::Error(e));
                } // Add more UI message handling as needed
                FrontMessage::PrintRootNotes => {
                    let regions = self.get_regions_in_time_selection()?;
                    for region in regions {
                        let mut region = Region::from(region);
                        println!("{:#?}", region.estimate_root_note(None, None, None, None));
                    }
                }
                FrontMessage::PrintLegatoIntervals => {
                    let regions = self.get_regions_in_time_selection()?;
                    for region in regions {
                        let mut region = Region::from(region);
                        println!(
                            "{:#?}",
                            region.estimate_legato_interval(None, None, None, 512)
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// Retrieves regions in time selection from the backend via the socket.
    fn get_regions_in_time_selection(&mut self) -> anyhow::Result<Vec<RegionInfo>> {
        self.socket.send(IppMessage::GetRegionsInTimeSelection)?;
        match self.socket.recv_timeout(DEFAULT_TIMEOUT) {
            Ok(msg) => {
                if let IppMessage::SendRegionsInTimeSelection(regions) = msg {
                    Ok(regions)
                } else {
                    Err(LevitanusError::Unexpected(format!(
                        "Unexpected message on get_regions_in_time_selection: {:#?}",
                        msg
                    ))
                    .into())
                }
            }
            Err(e) => Err(e.into()),
        }
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
            if ui
                .button("print root notes of regions in time selection")
                .clicked()
            {
                self.emit(FrontMessage::PrintRootNotes);
            }
            if ui
                .button("print legato_intervals of regions in time selection")
                .clicked()
            {
                self.emit(FrontMessage::PrintLegatoIntervals);
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
