use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    process::Stdio,
    sync::mpsc::{channel, Receiver, SendError, Sender},
    thread::spawn,
    time::Duration,
};

use egui::{Color32, Context, Id, Modal, ProgressBar, RichText, Ui};
use lazy_static::lazy_static;
use log::debug;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::{
    ffmpeg::{
        base::{Render, TimeLine},
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
            speed: Regex::new(r"speed=\s*(\w+)").expect("speed regex not compiled"),
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
    pub reciever: Option<Receiver<RenderMessage>>,
    pub sender: Option<Sender<RenderMessage>>,
}
impl RenderJob {
    pub fn poll(&mut self) -> Result<(), LevitanusError> {
        if let Some(recv) = &self.reciever {
            for msg in recv.try_iter() {
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
                                    self.progress = RenderProgress::Result(Ok(()))
                                }
                            }
                        }
                        self.last_status.progress = p;
                    }
                    RenderMessage::Stop => (),
                    RenderMessage::Err(e) => match &mut self.progress {
                        RenderProgress::Result(Err(old_e)) => {
                            self.progress = RenderProgress::Result(Err(format!("{}\n{}", old_e, e)))
                        }
                        _ => self.progress = RenderProgress::Result(Err(e)),
                    },
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
            let job = RenderJob {
                filename: tl
                    .outfile
                    .with_extension(&self.state.render_settings.extension)
                    .clone(),
                duration: tl.duration(),
                last_status: RenderStatus::default(),
                progress: RenderProgress::Progress(0.0),
                reciever: Some(reciever),
                sender: Some(thread_s),
                show_error: false,
            };
            let renderer = Render {
                render_settings: self.state.render_settings.clone(),
            };
            let mut ffmpeg = renderer.get_render_job(tl)?;
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
                let stdout = child.stdout.take().expect("handle present");
                let buf_reader = BufReader::new(stdout);
                for line in buf_reader.lines() {
                    if let Ok(msg) = thread_r.try_recv() {
                        match msg {
                            RenderMessage::Stop => {
                                child.kill().expect("can not kill child ffmpeg");
                                return;
                            }
                            _ => (),
                        }
                    }
                    if let Ok(line) = line {
                        if let Some(msg) = RenderMessage::from_string(line) {
                            if let Err(e) = sender.send(msg) {
                                child.kill().ok();
                                panic!("{:?}", e);
                            };
                        }
                    }
                }
                if let Some(stderr) = child.stderr {
                    let buf_reader = BufReader::new(stderr);
                    for line in buf_reader.lines() {
                        if let Ok(s) = line {
                            debug!("error string: {}", s);
                            if s.starts_with("Error") {
                                sender
                                    .send(RenderMessage::Err(s))
                                    .expect("can not send error message");
                            }
                        }
                    }
                }
            });
        }
        Ok(())
    }

    pub(crate) fn widget_render(&mut self, ctx: &Context, ui: &mut Ui) {
        Self::frame(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("render").clicked() {
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
                            ui.label(status);
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
                                                ui.heading("render error");
                                                ui.label(e);
                                                if ui.button("close").clicked() {
                                                    job.show_error = false;
                                                }
                                            },
                                        );
                                    }
                                }
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
                    }
                });
            }
        });
    }
}
