use std::{
	cell::RefCell,
	error::Error,
	fmt,
	sync::Arc,
	sync::atomic::{AtomicBool, Ordering},
};

use rea_rs::{
	gui::{self, DockableEguiWindow},
	ControlSurface, Reaper,
};

pub static BACKEND_ID_STRING: &str = "LevitanusFfmpegGuiNew";

struct FfmpegGuiSurface {
	window: DockableEguiWindow,
	is_window_alive: Arc<AtomicBool>,
}

impl fmt::Debug for FfmpegGuiSurface {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("FfmpegGuiSurface").finish()
	}
}

impl FfmpegGuiSurface {
	fn new() -> Result<Self, Box<dyn Error>> {
		let is_window_alive = Arc::new(AtomicBool::new(true));
		let mut window = DockableEguiWindow::new(
			"Levitanus FFMPEG GUI",
			"levitanus_ffmpeg_gui",
			gui::baseview::dpi::Size::Logical(gui::baseview::dpi::LogicalSize::new(640.0, 460.0)),
		);

		window.set_dock(
			None,
			FfmpegWindowState {
				is_alive: Arc::clone(&is_window_alive),
			},
			|_ctx, _queue, _state| {},
			|ui, _queue, _state| {
				gui::egui::CentralPanel::default().show(ui, |ui| {
					ui.heading("FFmpeg GUI (new)");
					ui.label(
						"Window lifecycle is now managed by rea-rs::gui without a separate process.",
					);
					if ui.button("Close").clicked() {
						ui.ctx()
							.send_viewport_cmd(gui::egui::ViewportCommand::Close);
					}
				});
			},
		);

		Ok(Self {
			window,
			is_window_alive,
		})
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
}

#[derive(Debug)]
struct FfmpegWindowState {
	is_alive: Arc<AtomicBool>,
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
