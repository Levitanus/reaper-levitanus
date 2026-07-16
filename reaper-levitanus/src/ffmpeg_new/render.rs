use std::{
    collections::VecDeque,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
};

use anyhow::anyhow;

use super::render_targets::RenderTarget;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RenderJobStatus {
    Queued,
    Running,
    Done,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct RenderJobSnapshot {
    pub(super) name: String,
    pub(super) output: String,
    pub(super) progress: f32,
    pub(super) status: RenderJobStatus,
    pub(super) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct RenderStateSnapshot {
    pub(super) active: bool,
    pub(super) canceled: bool,
    pub(super) total_jobs: usize,
    pub(super) finished_jobs: usize,
    pub(super) jobs: Vec<RenderJobSnapshot>,
}

impl Default for RenderStateSnapshot {
    fn default() -> Self {
        Self {
            active: false,
            canceled: false,
            total_jobs: 0,
            finished_jobs: 0,
            jobs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct RenderJobDefinition {
    pub(super) render_target: RenderTarget,
    pub(super) output_path: PathBuf,
    pub(super) input_video_path: PathBuf,
    pub(super) use_source_window: bool,
    pub(super) video_codec: String,
    pub(super) audio_codec: String,
}

#[derive(Debug)]
enum RenderWorkerEvent {
    Started(usize),
    Progress(usize, f32),
    Finished(usize, Option<String>, bool),
}

#[derive(Debug, Clone)]
pub(super) struct RenderEngine {
    state: Arc<Mutex<RenderStateSnapshot>>,
    cancel_flag: Arc<AtomicBool>,
}

impl Default for RenderEngine {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(RenderStateSnapshot::default())),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl RenderEngine {
    pub(super) fn snapshot(&self) -> RenderStateSnapshot {
        self.state
            .lock()
            .map(|state| state.clone())
            .unwrap_or_default()
    }

    pub(super) fn request_cancel(&self) {
        self.cancel_flag.store(true, Ordering::Relaxed);
    }

    pub(super) fn start(
        &self,
        jobs: Vec<RenderJobDefinition>,
        parallel: bool,
        workers: usize,
    ) -> anyhow::Result<()> {
        let total_jobs = jobs.len();
        if total_jobs == 0 {
            return Err(anyhow!("render queue is empty"));
        }

        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow!("render state lock poisoned"))?;
            if state.active {
                return Err(anyhow!("render is already running"));
            }
            state.active = true;
            state.canceled = false;
            state.total_jobs = total_jobs;
            state.finished_jobs = 0;
            state.jobs = jobs
                .iter()
                .map(|job| RenderJobSnapshot {
                    name: render_target_name(&job.render_target.path),
                    output: job.output_path.to_string_lossy().to_string(),
                    progress: 0.0,
                    status: RenderJobStatus::Queued,
                    error: None,
                })
                .collect();
        }

        self.cancel_flag.store(false, Ordering::Relaxed);

        let state = Arc::clone(&self.state);
        let cancel_flag = Arc::clone(&self.cancel_flag);
        thread::spawn(move || {
            let queue = Arc::new(Mutex::new(
                jobs.into_iter().enumerate().collect::<VecDeque<_>>(),
            ));
            let (tx, rx) = mpsc::channel::<RenderWorkerEvent>();

            let worker_count = if parallel {
                workers.max(1).min(total_jobs)
            } else {
                1
            };
            let mut handles = Vec::with_capacity(worker_count);

            for _ in 0..worker_count {
                let queue = Arc::clone(&queue);
                let tx = tx.clone();
                let cancel_flag = Arc::clone(&cancel_flag);
                handles.push(thread::spawn(move || {
                    worker_loop(queue, tx, cancel_flag);
                }));
            }
            drop(tx);

            while let Ok(event) = rx.recv() {
                let Ok(mut shared) = state.lock() else {
                    break;
                };
                match event {
                    RenderWorkerEvent::Started(idx) => {
                        if let Some(job) = shared.jobs.get_mut(idx) {
                            job.status = RenderJobStatus::Running;
                        }
                    }
                    RenderWorkerEvent::Progress(idx, progress) => {
                        if let Some(job) = shared.jobs.get_mut(idx) {
                            job.progress = progress.clamp(0.0, 1.0);
                        }
                    }
                    RenderWorkerEvent::Finished(idx, err, canceled) => {
                        if let Some(job) = shared.jobs.get_mut(idx) {
                            if canceled {
                                job.status = RenderJobStatus::Canceled;
                                shared.canceled = true;
                            } else if let Some(error) = err {
                                job.status = RenderJobStatus::Failed;
                                job.error = Some(error);
                            } else {
                                job.status = RenderJobStatus::Done;
                                job.progress = 1.0;
                            }
                        }
                        shared.finished_jobs = shared.finished_jobs.saturating_add(1);
                    }
                }
            }

            for handle in handles {
                let _ = handle.join();
            }

            if let Ok(mut shared) = state.lock() {
                shared.active = false;
                shared.canceled = shared.canceled || cancel_flag.load(Ordering::Relaxed);
            }
        });

        Ok(())
    }
}

fn worker_loop(
    queue: Arc<Mutex<VecDeque<(usize, RenderJobDefinition)>>>,
    tx: mpsc::Sender<RenderWorkerEvent>,
    cancel_flag: Arc<AtomicBool>,
) {
    loop {
        if cancel_flag.load(Ordering::Relaxed) {
            break;
        }

        let Some((idx, job)) = queue.lock().ok().and_then(|mut q| q.pop_front()) else {
            break;
        };

        let _ = tx.send(RenderWorkerEvent::Started(idx));
        let result = execute_render_job(&job, &tx, idx, &cancel_flag);
        let _ = tx.send(RenderWorkerEvent::Finished(idx, result.0, result.1));
    }
}

fn execute_render_job(
    job: &RenderJobDefinition,
    tx: &mpsc::Sender<RenderWorkerEvent>,
    idx: usize,
    cancel_flag: &AtomicBool,
) -> (Option<String>, bool) {
    let (mut command, command_str) = match build_ffmpeg_command(job) {
        Ok(value) => value,
        Err(error) => return (Some(error.to_string()), false),
    };

    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let Ok(mut child) = command.spawn() else {
        return (
            Some(format!(
                "failed to spawn ffmpeg for '{}': {}",
                job.output_path.display(),
                command_str
            )),
            false,
        );
    };

    let stderr_reader = child.stderr.take().map(BufReader::new);
    let stderr_handle = stderr_reader.map(|reader| {
        thread::spawn(move || {
            let mut log = String::new();
            for line in reader.lines() {
                if let Ok(line) = line {
                    log.push_str(&line);
                    log.push('\n');
                }
            }
            log
        })
    });

    let mut canceled = false;
    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if cancel_flag.load(Ordering::Relaxed) {
                canceled = true;
                let _ = child.kill();
                break;
            }

            let Ok(line) = line else {
                continue;
            };
            if let Some(progress) =
                parse_ffmpeg_progress(&line, render_duration_secs(&job.render_target.duration))
            {
                let _ = tx.send(RenderWorkerEvent::Progress(idx, progress));
            }
        }
    }

    let wait_result = child.wait();
    let stderr_log = match stderr_handle {
        Some(handle) => handle.join().unwrap_or_default(),
        None => String::new(),
    };

    if canceled || cancel_flag.load(Ordering::Relaxed) {
        return (None, true);
    }

    match wait_result {
        Ok(status) if status.success() => (None, false),
        Ok(status) => {
            let log_tail = stderr_log
                .lines()
                .rev()
                .take(12)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
            (
                Some(format!(
                    "ffmpeg exited with status {}{}",
                    status,
                    if log_tail.is_empty() {
                        "".to_string()
                    } else {
                        format!("\n{}", log_tail)
                    }
                )),
                false,
            )
        }
        Err(error) => (Some(format!("failed waiting ffmpeg process: {error}")), false),
    }
}

fn render_duration_secs(duration: &rea_rs::Duration) -> f64 {
    duration
        .num_microseconds()
        .map(|value| (value.max(0) as f64) / 1_000_000.0)
        .unwrap_or(0.0)
}

fn parse_ffmpeg_progress(line: &str, duration_secs: f64) -> Option<f32> {
    if duration_secs <= f64::EPSILON {
        return Some(1.0);
    }

    if let Some(us) = line.strip_prefix("out_time_us=") {
        let micros = us.trim().parse::<f64>().ok()?;
        return Some((micros / 1_000_000.0 / duration_secs) as f32);
    }

    if let Some(ms) = line.strip_prefix("out_time_ms=") {
        let value = ms.trim().parse::<f64>().ok()?;
        let seconds = if value > 10_000_000.0 {
            value / 1_000_000.0
        } else {
            value / 1000.0
        };
        return Some((seconds / duration_secs) as f32);
    }

    if let Some(ts) = line.strip_prefix("out_time=") {
        let seconds = parse_ffmpeg_timestamp(ts.trim())?;
        return Some((seconds / duration_secs) as f32);
    }

    None
}

fn parse_ffmpeg_timestamp(value: &str) -> Option<f64> {
    let (hms, fraction) = value.split_once('.').unwrap_or((value, "0"));
    let mut hms_iter = hms.split(':');
    let hours = hms_iter.next()?.parse::<f64>().ok()?;
    let minutes = hms_iter.next()?.parse::<f64>().ok()?;
    let seconds = hms_iter.next()?.parse::<f64>().ok()?;
    let frac = format!("0.{fraction}").parse::<f64>().ok()?;
    Some(hours * 3600.0 + minutes * 60.0 + seconds + frac)
}

fn timestamp_string(seconds: f64) -> String {
    let safe = seconds.max(0.0);
    let hours = (safe / 3600.0).floor();
    let minutes = ((safe % 3600.0) / 60.0).floor();
    let secs = safe % 60.0;
    format!("{:02}:{:02}:{:06.3}", hours as u64, minutes as u64, secs)
}

fn build_ffmpeg_command(job: &RenderJobDefinition) -> anyhow::Result<(Command, String)> {
    let mut args = vec![
        "-hide_banner".to_string(),
        "-y".to_string(),
        "-nostats".to_string(),
        "-progress".to_string(),
        "pipe:1".to_string(),
    ];

    if job.use_source_window {
        let offset = timestamp_string(job.render_target.source_offset.as_secs_f64());
        let duration = timestamp_string(render_duration_secs(&job.render_target.duration));
        args.extend([
            "-ss".to_string(),
            offset,
            "-t".to_string(),
            duration,
        ]);
    }

    args.extend([
        "-i".to_string(),
        job.input_video_path.to_string_lossy().to_string(),
        "-i".to_string(),
        job.render_target.path.to_string_lossy().to_string(),
        "-map".to_string(),
        "0:v:0".to_string(),
        "-map".to_string(),
        "1:a:0".to_string(),
        "-c:v".to_string(),
        job.video_codec.clone(),
    ]);

    if job.video_codec == "libx264" || job.video_codec == "h264" {
        args.extend(["-crf".to_string(), "15".to_string()]);
    }

    args.extend(["-c:a".to_string(), job.audio_codec.clone()]);
    if job.audio_codec == "aac" {
        args.extend(["-b:a".to_string(), "384K".to_string()]);
    }

    args.push("-shortest".to_string());
    args.push(job.output_path.to_string_lossy().to_string());

    let mut command = Command::new("ffmpeg");
    command.args(&args);

    let command_str = format!("ffmpeg {}", args.join(" "));
    Ok((command, command_str))
}

fn render_target_name(path: &PathBuf) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| stem.to_string())
        .unwrap_or_else(|| "<unnamed>".to_string())
}
