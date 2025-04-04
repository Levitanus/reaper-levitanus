use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    process::Stdio,
    sync::mpsc::{channel, Receiver, SendError, Sender},
    thread::spawn,
    time::Duration,
};

use egui::{Area, Color32, Context, Id, Modal, ProgressBar, RichText, ScrollArea, Ui};
use itertools::Itertools;
use lazy_static::lazy_static;
use log::{debug, error};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::{
    ffmpeg::{
        base::{Render, TimeLine},
        base_types::Timestamp,
        options::DurationUnit,
    },
    LevitanusError,
};

use super::{Front, FrontMessage};

lazy_static! {
    static ref RENDER_RE: RenderRegex = RenderRegex::new();
}

#[derive(Debug)]
struct RenderRegex {
    frame: Regex,
    fps: Regex,
    time: Regex,
    speed: Regex,
    progress: Regex,
}
impl RenderRegex {
    fn new() -> Self {
        Self {
            frame: Regex::new(r"frame=(\d+)").expect("frame regex not compiled"),
            fps: Regex::new(r"fps=([\d.]+)").expect("fps regex not compiled"),
            time: Regex::new(r"out_time=([\d\.:]+)").expect("time regex not compiled"),
            speed: Regex::new(r"speed=\s*(\S+)").expect("speed regex not compiled"),
            progress: Regex::new(r"progress=(\w+)").expect("progress regex not compiled"),
        }
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RenderStatus {
    frame: u32,
    fps: f32,
    time: DurationUnit,
    speed: String,
    progress: Result<String, String>,
}
impl Default for RenderStatus {
    fn default() -> Self {
        Self {
            frame: u32::default(),
            fps: f32::default(),
            time: DurationUnit::Seconds(f64::default()),
            speed: String::default(),
            progress: Ok(String::default()),
        }
    }
}

#[derive(Debug)]
pub struct RenderJob {
    pub filename: PathBuf,
    pub show_error: bool,
    pub duration: Duration,
    pub progress: RenderProgress,
    pub last_status: RenderStatus,
    pub show_script: bool,
    pub render_script: String,
    pub reciever: Option<Receiver<RenderMessage>>,
    pub sender: Option<Sender<RenderMessage>>,
    pub error_log: String,
}
impl RenderJob {
    pub fn poll(&mut self) -> Result<(), LevitanusError> {
        if let Some(recv) = &self.reciever {
            for msg in recv.try_iter() {
                // debug!(
                //     "client for {:?}, polled message: {:?}",
                //     self.filename.file_name(),
                //     msg
                // );
                match msg {
                    RenderMessage::Frame(frame) => self.last_status.frame = frame,
                    RenderMessage::Fps(fps) => self.last_status.fps = fps,
                    RenderMessage::Time(t) => {
                        let progress =
                            (t.as_duration().as_secs_f64() / self.duration.as_secs_f64()) as f32;
                        debug!("{:?}, progress={}", t, progress);
                        self.progress = RenderProgress::Progress(progress);
                        self.last_status.time = t;
                    }
                    RenderMessage::Speed(s) => self.last_status.speed = s,
                    RenderMessage::Progress(p) => {
                        match &p {
                            Err(e) => self.progress = RenderProgress::Result(Err(e.clone())),
                            Ok(p) => {
                                if p == "end" {
                                    if let RenderProgress::Result(Err(_)) = &self.progress {
                                        return Ok(());
                                    }
                                    self.progress = RenderProgress::Result(Ok(()))
                                }
                            }
                        }
                        self.last_status.progress = p;
                    }
                    RenderMessage::Stop => (),
                    RenderMessage::Err(e) => {
                        match &mut self.progress {
                            RenderProgress::Result(Err(old_e)) => {
                                self.progress =
                                    RenderProgress::Result(Err(format!("{}\n{}", old_e, e)))
                            }
                            _ => self.progress = RenderProgress::Result(Err(e)),
                        };
                        if let Some(s) = &self.sender {
                            s.send(RenderMessage::Stop)
                                .map_err(|err| LevitanusError::Render(format!("{}", err)))?;
                        }
                    }
                    RenderMessage::LogError(s) => {
                        self.error_log.push_str(&format!("{}\n", s));
                        if s.contains("Error") {
                            self.progress = RenderProgress::Result(Err(s))
                        }
                    }
                }
            }
        }
        Ok(())
    }
    pub fn kill(&self) -> Result<(), SendError<RenderMessage>> {
        if let Some(sender) = &self.sender {
            sender.send(RenderMessage::Stop)?
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RenderProgress {
    Progress(f32),
    Result(Result<(), String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RenderMessage {
    Frame(u32),
    Fps(f32),
    Time(DurationUnit),
    Speed(String),
    Progress(Result<String, String>),
    Stop,
    Err(String),
    LogError(String),
}
impl RenderMessage {
    pub fn from_string(line: String) -> Option<Self> {
        if let Some(cap) = RENDER_RE.frame.captures(&line) {
            return Some(Self::Frame(
                cap[1].parse().expect("no frame group in regex"),
            ));
        }
        if let Some(cap) = RENDER_RE.fps.captures(&line) {
            return Some(Self::Fps(cap[1].parse().expect("no frame group in regex")));
        }
        if let Some(cap) = RENDER_RE.time.captures(&line) {
            let mut split = cap[1].split(":");
            let hours = split
                .next()
                .expect("no hours")
                .parse()
                .expect("can not parse hours");
            let minutes = split
                .next()
                .expect("no minutes")
                .parse()
                .expect("can not parse minutes");
            let seconds = split
                .next()
                .expect("no seconds")
                .parse()
                .expect("can not parse seconds");
            let time = DurationUnit::Timestamp {
                hours,
                minutes,
                seconds,
            };
            return Some(Self::Time(time));
        }
        if let Some(cap) = RENDER_RE.speed.captures(&line) {
            return Some(Self::Speed(cap[1].to_string()));
        }
        if let Some(cap) = RENDER_RE.progress.captures(&line) {
            if &cap[1] == "continue" || &cap[1] == "end" {
                return Some(Self::Progress(Ok(cap[1].to_string())));
            } else {
                return Some(Self::Progress(Err(cap[1].to_string())));
            }
        }
        None
    }
}

impl Front {
    pub fn render(&mut self, render_queue: Vec<TimeLine>) -> anyhow::Result<()> {
        for tl in render_queue {
            let (sender, reciever) = channel();
            let (thread_s, thread_r) = channel();
            let filename = tl
                .outfile
                .with_extension(&self.state.render_settings.extension)
                .clone();
            let duration = tl.duration();
            let renderer = Render {
                render_settings: self.state.render_settings.clone(),
            };
            let mut ffmpeg = renderer.get_render_job(tl, self.state.master_filters.clone())?;
            let render_script = format!("{:?}", ffmpeg);
            let job = RenderJob {
                filename,
                duration,
                last_status: RenderStatus::default(),
                progress: RenderProgress::Progress(0.0),
                reciever: Some(reciever),
                sender: Some(thread_s),
                show_error: false,
                show_script: false,
                render_script,
                error_log: String::default(),
            };
            self.render_jobs.push(job);
            spawn(move || {
                ffmpeg.stdout(Stdio::piped());
                // ffmpeg.stdin(Stdio::piped());
                ffmpeg.stderr(Stdio::piped());
                let mut child = match ffmpeg.spawn() {
                    Ok(p) => p,
                    Err(e) => {
                        let _ = sender.send(RenderMessage::Err(e.to_string()));
                        return;
                    }
                };
                // debug!("{:?}", child.wait_with_output());
                let stout_reader = BufReader::new(child.stdout.take().expect("handle present"));
                let stderr_reader = BufReader::new(child.stderr.take().expect("handle present"));
                let sender_clone = sender.clone();
                let thread = spawn(move || {
                    for line in stderr_reader.lines() {
                        if let Ok(s) = line {
                            debug!("stderr msg: {:?}", s);
                            sender_clone
                                .send(RenderMessage::LogError(s))
                                .expect("can not send eror log");
                        }
                    }
                });
                for line in stout_reader.lines() {
                    if let Ok(msg) = thread_r.try_recv() {
                        debug!("recieved render message: {:?}", msg);
                        match msg {
                            RenderMessage::Stop => {
                                child.kill().expect("can not kill child ffmpeg");
                                return;
                            }
                            _ => (),
                        }
                    }
                    if let Ok(line) = line {
                        debug!("line from child ffmpeg: {:?}", line);
                        if let Some(msg) = RenderMessage::from_string(line) {
                            if let Err(e) = sender.send(msg) {
                                error!("Can not send render message: {:?}", e);
                                child.kill().ok();
                                panic!("{:?}", e);
                            };
                        }
                    }
                }
                thread.join().expect("error on join");
            });
        }
        Ok(())
    }

    pub(crate) fn widget_render(&mut self, ctx: &Context, ui: &mut Ui) {
        Self::frame(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button(RichText::new("render").strong()).clicked() {
                    self.emit(FrontMessage::Render);
                }
                ui.checkbox(&mut self.state.parallel_render, "render files parallel");
            });
            if self.render_jobs.len() > 0 {
                Modal::new(Id::new("render")).show(ctx, |ui| {
                    let mut overal_progress = false;
                    for job in self.render_jobs.iter_mut() {
                        let (progress, status, error) = match &job.progress {
                            RenderProgress::Progress(p) => {
                                overal_progress = true;
                                (*p, RichText::new("rendering").color(Color32::YELLOW), None)
                            }
                            RenderProgress::Result(r) => match r {
                                Ok(()) => {
                                    (1.0, RichText::new("rendered").color(Color32::GREEN), None)
                                }
                                Err(e) => {
                                    (-1.0, RichText::new("error").color(Color32::RED), Some(e))
                                }
                            },
                        };
                        Self::frame(ui, |ui| {
                            ui.label(job.filename.to_string_lossy());
                            if ui.button("show render script").clicked() {
                                job.show_script = true;
                            }
                            ui.horizontal(|ui| {
                                ui.label(status);
                                ui.separator();
                                ui.label(format!("fps: {}", job.last_status.fps));
                                ui.label(format!("speed: {}", job.last_status.speed));
                                ui.label(format!(
                                    "time: {}",
                                    job.last_status.time.as_duration().timestump()
                                ));
                            });
                            match error {
                                None => {
                                    ui.add(ProgressBar::new(progress));
                                }
                                Some(e) => {
                                    if ui.button("show error").clicked() {
                                        job.show_error = true
                                    }
                                    if job.show_error {
                                        Modal::new(Id::new(job.filename.to_string_lossy())).show(
                                            ctx,
                                            |ui| {
                                                ui.set_max_height(
                                                    ctx.available_rect().height() - 100.0,
                                                );
                                                ui.heading("render error");
                                                ui.label(RichText::new(e).strong());
                                                ScrollArea::vertical()
                                                    .max_height(ui.available_height() - 100.0)
                                                    .show(ui, |ui| {
                                                        ui.heading("error log:");
                                                        ui.label(&job.error_log);
                                                    });
                                                if ui.button("close").clicked() {
                                                    job.show_error = false;
                                                }
                                            },
                                        );
                                    }
                                }
                            }
                            if job.show_script {
                                Modal::new(Id::new("render script")).show(ctx, |ui| {
                                    ui.set_max_height(ctx.available_rect().height() - 100.0);
                                    ScrollArea::vertical()
                                        .max_height(ui.available_height() - 100.0)
                                        .show(ui, |ui| {
                                            ui.label(&job.render_script);
                                        });
                                    if ui.button("close").clicked() {
                                        job.show_script = false
                                    }
                                });
                            }
                        });
                    }

                    match overal_progress {
                        true => {
                            if ui.button("stop").clicked() {
                                for job in self.render_jobs.iter() {
                                    if let Err(e) = job.kill() {
                                        self.emit(FrontMessage::Error(e.to_string()));
                                    };
                                }
                                self.render_jobs.clear();
                            }
                        }
                        false => {
                            if ui.button("close").clicked() {
                                self.render_jobs.clear();
                            }
                        }
                    };
                });
            }
        });
    }
}
