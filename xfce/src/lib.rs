// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! The XFCE panel plugin. design.md, XFCE plugin, has the why of doing this
//! with hand-declared calls instead of headers.

use std::any::Any;
use std::ffi::{CStr, CString, c_char, c_int, c_uint, c_ulong, c_void};
use std::io::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant, SystemTime};

use tukzmon_core::config::{self, Look};
use tukzmon_core::graphs::Graphs;
use tukzmon_core::keep;

mod sys;
#[allow(clippy::wildcard_imports, reason = "sys is only the C declarations")]
use sys::*;

struct Plugin {
	xpp: *mut c_void,
	area: *mut c_void,
	graphs: Graphs,
	/// The one-shot timer to the next graph's read.
	timer: c_uint,
	/// A one-shot redraw while an axis eases in, or 0.
	frame: c_uint,
	config: PathBuf,
	config_time: Option<(SystemTime, u64)>,
	settings: Option<Child>,
	history: PathBuf,
	reset: keep::ResetWatch,
	/// File monitors on the settings file and the reset mark.
	watches: Vec<*mut c_void>,
	saved_at: Instant,
	moved_at: Instant,
	/// Taken off the panel for good, so its files stay gone.
	removed: bool,
	/// The last failed read or save of the history file, shown in the tip
	/// until a save works.
	trouble: Option<String>,
	/// Why the settings file can't be read, shown in the tip until it can.
	unreadable: Option<String>,
	/// Why a panic stopped this item. It draws nothing and does nothing after.
	stopped: Option<String>,
}

/// How often to draw while an axis eases in.
const FRAME_MS: c_uint = 33;

/// A crash loses at most this much history.
const SAVE_EVERY: Duration = Duration::from_secs(15 * 60);

/// GTK waits 500 ms for a still pointer before it shows a tip, and asking
/// again restarts that wait. So the tick leaves the tip alone until then.
const TIP_SHOWN: Duration = Duration::from_secs(1);

/// eprintln! panics on a failed write, such as to /dev/full or a hung-up tty,
/// and on Wayland the panel runs the plugin in its own process, so that abort
/// takes the whole panel down.
///
/// A panel that `xfce4-panel --restart` started has stderr on /dev/null, so
/// the line goes to the system log too, unless stderr is the journal already.
fn complain(what: std::fmt::Arguments) {
	let line = format!("tukzmon: {what}");
	let _ = writeln!(std::io::stderr().lock(), "{line}");
	// a test run would fill the host's log
	if !cfg!(test) {
		to_syslog(&line);
	}
}

/// With no openlog, since on Wayland the process and its name are the panel's.
fn to_syslog(line: &str) {
	// SAFETY: fd 2 is only looked at.
	if unsafe { g_log_writer_is_journald(2) } != 0 {
		return;
	}
	let Ok(c) = CString::new(line) else {
		return;
	};
	// SAFETY: a literal format with the one string it takes.
	unsafe { syslog(LOG_USER | LOG_WARNING, c"%s".as_ptr(), c.as_ptr()) };
}

/// The text a panic was given, as panic! and the std asserts make it.
fn panic_text(payload: &(dyn Any + Send)) -> &str {
	payload
		.downcast_ref::<&str>()
		.copied()
		.or_else(|| payload.downcast_ref::<String>().map(String::as_str))
		.unwrap_or("a panic")
}

/// One line per panic through complain, rather than std's 3 on stderr alone.
/// The hook is this library's own, since each cdylib has its own std.
fn report_panics() {
	static ONCE: std::sync::Once = std::sync::Once::new();
	ONCE.call_once(|| {
		std::panic::set_hook(Box::new(|info| {
			let at = info
				.location()
				.map(|l| format!(" at {}:{}", l.file(), l.line()))
				.unwrap_or_default();
			complain(format_args!(
				"stopped by a bug{at}: {}",
				panic_text(info.payload()).replace('\n', " ")
			));
		}));
	});
}

/// Runs a handler's body on the Plugin at `data`, or `stopped` once a panic
/// has stopped the item. A panic stops only this item, where on Wayland it
/// would take down the panel and every plugin in it.
fn guarded<R>(
	data: *mut c_void,
	stopped: impl FnOnce(&str) -> R,
	body: impl FnOnce(&mut Plugin) -> R,
) -> R {
	// SAFETY: data is the Plugin the handler was connected with, alive until
	// free-data.
	let p = unsafe { &mut *data.cast::<Plugin>() };
	if p.stopped.is_none() {
		match catch_unwind(AssertUnwindSafe(|| body(&mut *p))) {
			Ok(r) => return r,
			Err(e) => p.stop(panic_text(e.as_ref())),
		}
	}
	stopped(p.stopped.as_deref().unwrap_or_default())
}

/// The look in the settings file, or None while it can't be read, which
/// is logged once per error and kept in `unreadable` for the tip.
fn read_look(path: &Path, unreadable: &mut Option<String>) -> Option<Look> {
	match Look::load(path) {
		Ok(look) => {
			*unreadable = None;
			Some(look)
		}
		Err(e) => {
			let what = format!("can't read {}: {e}", path.display());
			if unreadable.as_ref() != Some(&what) {
				complain(format_args!("{what}"));
			}
			*unreadable = Some(what);
			None
		}
	}
}

// with the size, since a file watch can wake on a write still under way, and
// the rest of it may come within the same tick of the file clock
fn modified(p: &Path) -> Option<(SystemTime, u64)> {
	let m = std::fs::metadata(p).ok()?;
	Some((m.modified().ok()?, m.len()))
}

impl Plugin {
	fn fit(&mut self) {
		// SAFETY: plugin is the live XfcePanelPlugin this state belongs to.
		unsafe {
			let size = xfce_panel_plugin_get_size(self.xpp);
			// a deskbar is a vertical panel too
			self.graphs.stacked = xfce_panel_plugin_get_mode(self.xpp) != MODE_HORIZONTAL;
			let (w, h) = self.graphs.wanted(u32::try_from(size).unwrap_or(0));
			let px = |v: u32| c_int::try_from(v).unwrap_or(c_int::MAX);
			gtk_widget_set_size_request(self.xpp, px(w), px(h));
		}
	}

	/// Each graph keeps its own rate, so the timer is set again each time,
	/// for whichever is due next.
	fn start_timer(&mut self, ms: u32) {
		// SAFETY: self lives in a Box until free-data, which stops this timer.
		self.timer = unsafe { g_timeout_add(ms, tick, std::ptr::from_mut(self).cast()) };
	}

	/// Logged, and kept for the tip, since a restarted panel's stderr is
	/// /dev/null.
	fn history_trouble(&mut self, doing: &str, e: &std::io::Error) {
		let what = format!("can't {doing} {}: {e}", self.history.display());
		complain(format_args!("{what}"));
		self.trouble = Some(what);
	}

	fn load_history(&mut self) {
		match self.graphs.load(&self.history) {
			Ok(()) => {}
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
			Err(e) => self.history_trouble("read", &e),
		}
		self.saved_at = Instant::now();
	}

	fn save_history(&mut self) {
		if self.removed {
			return;
		}
		self.graphs.trim();
		match self.graphs.save(&self.history) {
			Ok(()) => self.trouble = None,
			Err(e) => self.history_trouble("save", &e),
		}
		self.saved_at = Instant::now();
	}

	/// The graphs' tip, with any settings or history trouble under it.
	fn tip(&self, graphs: String) -> String {
		let mut tip = graphs;
		for (head, t) in [("Settings", &self.unreadable), ("History", &self.trouble)] {
			if let Some(t) = t {
				tip = format!("{tip}\n\n{head}: {t}");
			}
		}
		tip
	}

	/// Ends the timers and file watches, which hold a pointer to this.
	fn quiet(&mut self) {
		for id in [&mut self.timer, &mut self.frame] {
			if *id != 0 {
				// SAFETY: the id is ours and still running, since each
				// callback zeroes or replaces it.
				unsafe { g_source_remove(*id) };
				*id = 0;
			}
		}
		for w in self.watches.drain(..) {
			// SAFETY: each is from watch, and isn't used again.
			unsafe { unwatch(w) };
		}
	}

	/// After a panic, since what it was changing may be half done. The
	/// history file isn't saved again, and the tip says why it stopped.
	fn stop(&mut self, why: &str) {
		self.stopped = Some(why.replace('\n', " "));
		self.quiet();
		// the area is made after the Plugin
		if !self.area.is_null() {
			// SAFETY: area is a child of the live plugin.
			unsafe { gtk_widget_queue_draw(self.area) };
		}
	}

	fn tick(&mut self) {
		// reap a closed settings window
		if self
			.settings
			.as_mut()
			.is_some_and(|c| !matches!(c.try_wait(), Ok(None)))
		{
			self.settings = None;
		}
		self.reload_if_changed();
		self.reset_if_asked();
		let next = self.graphs.tick();
		if self.graphs.look().keep && self.saved_at.elapsed() >= SAVE_EVERY {
			self.save_history();
		}
		// SAFETY: area is a child of the live plugin.
		unsafe {
			gtk_widget_queue_draw(self.area);
			if self.graphs.hovered() && self.moved_at.elapsed() >= TIP_SHOWN {
				// a still pointer's time moves on too
				gtk_widget_trigger_tooltip_query(self.area);
			}
		}
		self.start_timer(next);
	}

	/// Pick up a save from the settings window.
	fn reload_if_changed(&mut self) {
		let t = modified(&self.config);
		if t == self.config_time {
			return;
		}
		// the time stays old until it reads, since a chmod back doesn't move it
		let Some(look) = read_look(&self.config, &mut self.unreadable) else {
			return;
		};
		self.config_time = t;
		if look != *self.graphs.look() {
			let widths: Vec<u32> = self.graphs.widths().collect();
			// the margins change the size too
			let along = self.graphs.wanted(0);
			let keep = (self.graphs.look().keep, look.keep);
			let dropped = self.graphs.drops(&look);
			// what came in while it was on still counts
			if keep.0 && (!keep.1 || dropped) {
				self.save_history();
			}
			let from = keep.1.then_some(self.history.as_path());
			if let Err(e) = self.graphs.set_look(look, from) {
				self.history_trouble("read", &e);
			}
			let refit = !self.graphs.widths().eq(widths) || self.graphs.wanted(0) != along;
			if keep == (false, true) {
				// nothing seen while it was off is logged, per backlog 2026102408304572
				self.load_history();
			}
			if refit {
				self.fit();
			}
		}
	}

	/// Reset history in the settings window starts the graphs over, and
	/// deletes the file when it's kept.
	fn reset_if_asked(&mut self) {
		if !self.reset.asked() {
			return;
		}
		let saved = self.graphs.look().keep.then_some(self.history.as_path());
		match self.graphs.reset(saved) {
			// what the trouble was about is gone
			Ok(()) => self.trouble = None,
			Err(e) => self.history_trouble("delete", &e),
		}
		self.saved_at = Instant::now();
	}

	/// The panel's Remove says the item's configuration is lost, so its
	/// files go, and a later item given the same id starts fresh.
	fn forget(&mut self) {
		self.removed = true;
		// deleting the settings file would wake its watch
		self.quiet();
		// its window would write the file back at Apply
		if let Some(mut c) = self.settings.take() {
			let _ = c.kill();
			let _ = c.wait();
		}
		for f in [&self.config, &self.history] {
			if let Err(e) = keep::remove(f) {
				complain(format_args!("can't delete {}: {e}", f.display()));
			}
		}
	}
}

/// Calls `changed` with `data` whenever `path` is written, made, or removed.
/// Null where it can't be watched, and the tick still checks it then.
///
/// # Safety
///
/// `data` must outlive the monitor, which [`unwatch`] ends.
unsafe fn watch(
	path: &Path,
	changed: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, c_int, *mut c_void),
	data: *mut c_void,
) -> *mut c_void {
	use std::os::unix::ffi::OsStrExt as _;
	let Ok(c) = CString::new(path.as_os_str().as_bytes()) else {
		return std::ptr::null_mut();
	};
	// SAFETY: c is a path, the file is ours until the unref, and a null
	// error is allowed.
	unsafe {
		let file = g_file_new_for_path(c.as_ptr());
		let monitor = g_file_monitor_file(file, 0, std::ptr::null_mut(), std::ptr::null_mut());
		g_object_unref(file);
		if !monitor.is_null() {
			connect(monitor, c"changed", changed as *const c_void, data);
		}
		monitor
	}
}

/// # Safety
///
/// `monitor` is from [`watch`], or null, and isn't used again.
unsafe fn unwatch(monitor: *mut c_void) {
	if !monitor.is_null() {
		// SAFETY: as said above.
		unsafe {
			g_file_monitor_cancel(monitor);
			g_object_unref(monitor);
		}
	}
}

/// A save in the settings window, or Reset history, shows at once rather than
/// at the next graph's read, which can be a minute away.
unsafe extern "C" fn changed(
	_monitor: *mut c_void,
	_file: *mut c_void,
	_other: *mut c_void,
	_event: c_int,
	data: *mut c_void,
) {
	guarded(
		data,
		|_| (),
		|p| {
			if p.timer != 0 {
				// SAFETY: the timer id is ours and still running, since tick
				// sets a new one each time it runs.
				unsafe { g_source_remove(p.timer) };
				p.timer = 0;
			}
			p.tick();
		},
	);
}

extern "C" fn tick(data: *mut c_void) -> Gboolean {
	guarded(data, |_| (), Plugin::tick);
	0
}

extern "C" fn draw(widget: *mut c_void, cr: *mut c_void, data: *mut c_void) -> Gboolean {
	guarded(data, |_| 0, |p| draw_graphs(p, widget, cr))
}

fn draw_graphs(p: &mut Plugin, widget: *mut c_void, cr: *mut c_void) -> Gboolean {
	// SAFETY: widget and cr come from GTK for this draw.
	unsafe {
		let scale = gtk_widget_get_scale_factor(widget).max(1);
		let w = gtk_widget_get_allocated_width(widget).max(0) * scale;
		let h = gtk_widget_get_allocated_height(widget).max(0) * scale;
		if w == 0 || h == 0 {
			return 0;
		}
		let px = p.graphs.render(w as usize, h as usize, scale as usize);
		// cairo only reads a source surface, so the const cast is fine
		let surf = cairo_image_surface_create_for_data(
			px.as_ptr().cast_mut().cast(),
			CAIRO_FORMAT_ARGB32,
			w,
			h,
			w * 4,
		);
		cairo_surface_set_device_scale(surf, scale as f64, scale as f64);
		cairo_set_source_surface(cr, surf, 0.0, 0.0);
		cairo_paint(cr);
		cairo_surface_destroy(surf);
		if p.frame == 0 && p.graphs.easing() {
			// SAFETY: as start_timer, and free-data stops this one too.
			p.frame = g_timeout_add(FRAME_MS, frame, std::ptr::from_mut(p).cast());
		}
	}
	1
}

extern "C" fn frame(data: *mut c_void) -> Gboolean {
	guarded(
		data,
		|_| (),
		|p| {
			p.frame = 0;
			// SAFETY: area is a child of the live plugin.
			unsafe { gtk_widget_queue_draw(p.area) };
		},
	);
	0
}

extern "C" fn motion(widget: *mut c_void, event: *mut c_void, data: *mut c_void) -> Gboolean {
	guarded(
		data,
		|_| 0,
		|p| {
			let (mut x, mut y) = (0.0, 0.0);
			// SAFETY: event is the motion event GTK is handing us, and widget its widget.
			let (got, scale, w, h) = unsafe {
				gdk_event_request_motions(event);
				(
					gdk_event_get_coords(event, &raw mut x, &raw mut y) != 0,
					gtk_widget_get_scale_factor(widget).max(1) as usize,
					gtk_widget_get_allocated_width(widget).max(0) as usize,
					gtk_widget_get_allocated_height(widget).max(0) as usize,
				)
			};
			if got {
				p.moved_at = Instant::now();
				let px = |v: f64| v.max(0.0).floor() as usize * scale;
				if p.graphs
					.hover(Some((px(x), px(y))), w * scale, h * scale, scale)
				{
					// SAFETY: widget is live for this event.
					unsafe { gtk_widget_queue_draw(widget) };
				}
			}
			0
		},
	)
}

extern "C" fn leave(widget: *mut c_void, _event: *mut c_void, data: *mut c_void) -> Gboolean {
	guarded(
		data,
		|_| 0,
		|p| {
			if p.graphs.hover(None, 0, 0, 1) {
				// SAFETY: widget is live for this event.
				unsafe { gtk_widget_queue_draw(widget) };
			}
			0
		},
	)
}

extern "C" fn query_tooltip(
	widget: *mut c_void,
	x: c_int,
	y: c_int,
	keyboard: Gboolean,
	tooltip: *mut c_void,
	data: *mut c_void,
) -> Gboolean {
	let tip = guarded(
		data,
		|why| format!("tukzmon stopped after a bug: {why}\nRestart the panel to start it again."),
		|p| {
			let graphs = if keyboard == 0 {
				// SAFETY: widget is the live drawing area GTK is asking about.
				let (scale, w, h) = unsafe {
					(
						gtk_widget_get_scale_factor(widget).max(1),
						gtk_widget_get_allocated_width(widget).max(0),
						gtk_widget_get_allocated_height(widget).max(0),
					)
				};
				p.graphs.hover_tip(
					(x.max(0) * scale) as usize,
					(y.max(0) * scale) as usize,
					(w * scale) as usize,
					(h * scale) as usize,
					scale as usize,
				)
			} else {
				p.graphs.tooltip()
			};
			p.tip(graphs)
		},
	);
	let Ok(c) = CString::new(tip) else {
		return 0;
	};
	// SAFETY: tooltip is GTK's for this query, and it copies the text.
	unsafe { gtk_tooltip_set_text(tooltip, c.as_ptr()) };
	1
}

extern "C" fn size_changed(_plugin: *mut c_void, _size: c_int, data: *mut c_void) -> Gboolean {
	guarded(data, |_| (), Plugin::fit);
	1
}

extern "C" fn mode_changed(_plugin: *mut c_void, _mode: c_int, data: *mut c_void) {
	guarded(data, |_| (), Plugin::fit);
}

/// tukzmon-settings beside the real library file, else on PATH.
fn settings_program() -> PathBuf {
	let mut info = DlInfo::default();
	// SAFETY: any address in this library works, and info is ours to fill.
	let found = unsafe { dladdr(settings_program as *const c_void, &raw mut info) } != 0;
	if found && !info.dli_fname.is_null() {
		// SAFETY: dladdr gives a NUL-terminated path that lives as long as the library.
		let lib = PathBuf::from(
			unsafe { CStr::from_ptr(info.dli_fname) }
				.to_string_lossy()
				.into_owned(),
		);
		if let Some(dir) = lib.canonicalize().ok().as_deref().and_then(Path::parent) {
			let beside = dir.join("tukzmon-settings");
			if beside.is_file() {
				return beside;
			}
		}
	}
	PathBuf::from("tukzmon-settings")
}

extern "C" fn configure(_plugin: *mut c_void, data: *mut c_void) {
	guarded(
		data,
		|_| (),
		|p| {
			if p.settings
				.as_mut()
				.is_some_and(|c| matches!(c.try_wait(), Ok(None)))
			{
				return;
			}
			let program = settings_program();
			match Command::new(&program)
				.arg("--config")
				.arg(&p.config)
				.spawn()
			{
				Ok(child) => p.settings = Some(child),
				Err(e) => complain(format_args!("can't start {}: {e}", program.display())),
			}
		},
	);
}

extern "C" fn removed(_plugin: *mut c_void, data: *mut c_void) {
	// SAFETY: data is the connected Plugin, and free-data comes after this.
	let p = unsafe { &mut *data.cast::<Plugin>() };
	// not guarded, since a stopped item's files go too
	if catch_unwind(AssertUnwindSafe(|| p.forget())).is_err() {
		p.quiet();
	}
}

extern "C" fn free_data(_plugin: *mut c_void, data: *mut c_void) {
	guarded(
		data,
		|_| (),
		|p| {
			if p.graphs.look().keep {
				p.save_history();
			}
		},
	);
	// SAFETY: data came from Box::into_raw in realize, and this is its last use.
	let mut p = unsafe { Box::from_raw(data.cast::<Plugin>()) };
	p.quiet();
}

impl Plugin {
	/// All but the widgets, so a panic here leaves nothing connected to it.
	fn new(plugin: *mut c_void) -> Self {
		// SAFETY: plugin is a live XfcePanelPlugin.
		let id = unsafe { xfce_panel_plugin_get_unique_id(plugin) };
		let config = config::instance_file(&config::config_dir(), id);
		if let Err(e) = config::start_from(&config, &config::config_file()) {
			complain(format_args!(
				"can't start {} from the shared file: {e}",
				config.display()
			));
		}
		let data = keep::data_dir();
		// so the reset mark's folder can be watched from the start
		if let Err(e) = std::fs::create_dir_all(&data) {
			complain(format_args!("can't make {}: {e}", data.display()));
		}
		let mut unreadable = None;
		let look = read_look(&config, &mut unreadable);
		// a time no file has, so each tick tries again, even once it's gone
		let config_time = match look {
			Some(_) => modified(&config),
			None => Some((SystemTime::UNIX_EPOCH, u64::MAX)),
		};
		let mut p = Plugin {
			xpp: plugin,
			area: std::ptr::null_mut(),
			graphs: Graphs::new(look.unwrap_or_default()),
			timer: 0,
			frame: 0,
			config_time,
			config,
			settings: None,
			history: keep::history_file(&data, id),
			reset: keep::ResetWatch::new(&data),
			watches: Vec::new(),
			saved_at: Instant::now(),
			moved_at: Instant::now(),
			removed: false,
			trouble: None,
			unreadable,
			stopped: None,
		};
		if p.graphs.look().keep {
			p.load_history();
		}
		p
	}

	/// The widgets and handlers, for the Plugin at `data`, which is this one.
	fn start(&mut self, data: *mut c_void) {
		let plugin = self.xpp;
		let mark = keep::reset_mark(&keep::data_dir());
		// SAFETY: plugin is a fresh XfcePanelPlugin, and every handler gets
		// data, which outlives them all until free-data.
		unsafe {
			self.area = gtk_drawing_area_new();
			gtk_container_add(plugin, self.area);
			// the area has a window of its own, so clicks stop there unless it takes
			// them and hands right-clicks on to the panel's menu
			gtk_widget_add_events(
				self.area,
				GDK_BUTTON_PRESS_MASK
					| GDK_BUTTON_RELEASE_MASK
					| GDK_POINTER_MOTION_MASK
					| GDK_LEAVE_NOTIFY_MASK,
			);
			xfce_panel_plugin_add_action_widget(plugin, self.area);
			gtk_widget_set_has_tooltip(self.area, 1);
			connect(self.area, c"draw", draw as *const c_void, data);
			connect(
				self.area,
				c"motion-notify-event",
				motion as *const c_void,
				data,
			);
			connect(
				self.area,
				c"leave-notify-event",
				leave as *const c_void,
				data,
			);
			connect(
				self.area,
				c"query-tooltip",
				query_tooltip as *const c_void,
				data,
			);
			connect(plugin, c"size-changed", size_changed as *const c_void, data);
			connect(plugin, c"mode-changed", mode_changed as *const c_void, data);
			connect(
				plugin,
				c"configure-plugin",
				configure as *const c_void,
				data,
			);
			connect(plugin, c"removed", removed as *const c_void, data);
			xfce_panel_plugin_menu_show_configure(plugin);
			// before anything below can panic, so a stopped item's tip shows
			gtk_widget_show_all(plugin);
			self.watches = vec![
				watch(&self.config, changed, data),
				watch(&mark, changed, data),
			];
		}
		let first = self.graphs.tick();
		self.fit();
		self.start_timer(first);
	}
}

/// # Safety
///
/// `instance` must be a `GObject` and `cb` a function matching the signal.
unsafe fn connect(
	instance: *mut c_void,
	signal: &CStr,
	cb: *const c_void,
	data: *mut c_void,
) -> c_ulong {
	// SAFETY: passed through from the caller.
	unsafe { g_signal_connect_data(instance, signal.as_ptr(), cb, data, std::ptr::null(), 0) }
}

extern "C" fn realize(plugin: *mut c_void, _data: *mut c_void) {
	// SAFETY: plugin is the instance this handler was connected to.
	unsafe {
		g_signal_handlers_disconnect_matched(
			plugin,
			G_SIGNAL_MATCH_FUNC,
			0,
			0,
			std::ptr::null_mut(),
			realize as *const c_void,
			std::ptr::null_mut(),
		);
	}
	// a panic here leaves the item empty, with nothing to free
	let Ok(p) = catch_unwind(|| Plugin::new(plugin)) else {
		return;
	};
	let data: *mut c_void = Box::into_raw(Box::new(p)).cast();
	// SAFETY: free-data is connected first, so the Plugin is freed whatever
	// start gets to.
	unsafe { connect(plugin, c"free-data", free_data as *const c_void, data) };
	guarded(data, |_| (), |p| p.start(data));
}

/// What `XFCE_PANEL_PLUGIN_REGISTER` would make: the panel's wrapper looks
/// this up by name and calls it to get the plugin object.
///
/// # Safety
///
/// Called by the panel with its own valid strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xfce_panel_module_construct(
	name: *const c_char,
	unique_id: c_int,
	display_name: *const c_char,
	comment: *const c_char,
	arguments: *mut *mut c_char,
	_screen: *mut c_void,
) -> *mut c_void {
	report_panics();
	// SAFETY: the property list matches XfcePanelPlugin's, ended by NULL.
	unsafe {
		let plugin = g_object_new(
			xfce_panel_plugin_get_type(),
			c"name".as_ptr(),
			name,
			c"unique-id".as_ptr(),
			unique_id,
			c"display-name".as_ptr(),
			display_name,
			c"comment".as_ptr(),
			comment,
			c"arguments".as_ptr(),
			arguments,
			std::ptr::null::<c_char>(),
		);
		g_signal_connect_data(
			plugin,
			c"realize".as_ptr(),
			realize as *const c_void,
			std::ptr::null_mut(),
			std::ptr::null(),
			G_CONNECT_AFTER,
		);
		plugin
	}
}

#[cfg(test)]
mod tests {
	use std::ffi::{c_int, c_void};
	use std::path::Path;
	use std::sync::atomic::{AtomicUsize, Ordering};
	use std::time::{Duration, Instant};

	use tukzmon_core::config::Rgb;
	use tukzmon_core::keep;

	/// A Plugin with no widgets, as the handlers get it, kept so free-data
	/// would save.
	fn plugin(dir: &Path, config: &Path, history: &Path) -> *mut c_void {
		let look = tukzmon_core::config::Look::default();
		assert!(look.keep, "free-data saves only when kept");
		let p = Box::new(super::Plugin {
			xpp: std::ptr::null_mut(),
			area: std::ptr::null_mut(),
			graphs: tukzmon_core::graphs::Graphs::new(look),
			timer: 0,
			frame: 0,
			config: config.to_path_buf(),
			config_time: None,
			settings: None,
			history: history.to_path_buf(),
			reset: keep::ResetWatch::new(dir),
			watches: Vec::new(),
			saved_at: Instant::now(),
			moved_at: Instant::now(),
			removed: false,
			trouble: None,
			unreadable: None,
			stopped: None,
		});
		Box::into_raw(p).cast()
	}

	// Test ID: dekps
	#[test]
	fn a_failed_history_save_shows_in_the_tip_until_one_works() {
		let dir =
			std::env::temp_dir().join(format!("test_tukzmon_xfce_{}_trouble", std::process::id()));
		std::fs::create_dir_all(&dir).unwrap();
		// a file where its folder should be
		let blocked = dir.join("blocked");
		std::fs::write(&blocked, "").unwrap();
		let data = plugin(&dir, &dir.join("own.shcl"), &blocked.join("history"));
		// SAFETY: from plugin, and freed below.
		let p = unsafe { &mut *data.cast::<super::Plugin>() };
		p.save_history();
		let failed = p.tip("graphs".into());
		p.history = dir.join("history");
		p.save_history();
		let saved = p.tip("graphs".into());
		super::free_data(std::ptr::null_mut(), data);
		for f in [&blocked, &dir.join("history")] {
			let _ = std::fs::remove_file(f);
		}
		std::fs::remove_dir(&dir).unwrap();
		assert!(
			failed.starts_with("graphs\n\nHistory: can't save ")
				&& failed.contains(&*blocked.display().to_string()),
			"{failed}"
		);
		assert_eq!(saved, "graphs");
	}

	// Test ID: dena9
	#[test]
	fn an_unreadable_settings_file_keeps_the_look_and_says_why() {
		let dir = std::env::temp_dir().join(format!(
			"test_tukzmon_xfce_{}_unreadable",
			std::process::id()
		));
		std::fs::create_dir_all(&dir).unwrap();
		let own = dir.join("own.shcl");
		std::fs::write(&own, "cpu:\n\tcolors:\n\t\tuser: \"#123456\"\n").unwrap();
		let data = plugin(&dir, &own, &dir.join("history"));
		// SAFETY: from plugin, and freed below.
		let p = unsafe { &mut *data.cast::<super::Plugin>() };
		p.reload_if_changed();
		let first = p.graphs.look().cpu[2];
		// a folder in its place, so it can't be read as root either
		std::fs::remove_file(&own).unwrap();
		std::fs::create_dir(&own).unwrap();
		p.reload_if_changed();
		let kept = p.graphs.look().cpu[2];
		let failed = p.tip("graphs".into());
		std::fs::remove_dir(&own).unwrap();
		std::fs::write(&own, "cpu:\n\tcolors:\n\t\tuser: \"#654321\"\n").unwrap();
		p.reload_if_changed();
		let fixed = (p.graphs.look().cpu[2], p.tip("graphs".into()));
		p.removed = true;
		super::free_data(std::ptr::null_mut(), data);
		std::fs::remove_file(&own).unwrap();
		std::fs::remove_dir(&dir).unwrap();
		// a color, since a new size would reach for the missing widgets
		let (old, new) = (Rgb(0x12, 0x34, 0x56), Rgb(0x65, 0x43, 0x21));
		assert_eq!((first, kept), (old, old));
		assert!(
			failed.starts_with("graphs\n\nSettings: can't read ")
				&& failed.contains(&*own.display().to_string()),
			"{failed}"
		);
		assert_eq!(fixed, (new, "graphs".to_string()));
	}

	// Test ID: dekpt
	#[test]
	fn a_panic_in_a_handler_stops_only_that_item() {
		let dir =
			std::env::temp_dir().join(format!("test_tukzmon_xfce_{}_stopped", std::process::id()));
		std::fs::create_dir_all(&dir).unwrap();
		let hist = dir.join("history");
		let data = plugin(&dir, &dir.join("own.shcl"), &hist);
		let first = super::guarded(data, str::to_owned, |_| panic!("boom\nat two"));
		// a handler that ran would reach GTK with a null area
		let ran = super::guarded(data, |_| false, |_| true);
		assert_eq!(super::tick(data), 0);
		super::frame(data);
		// a stopped item's history isn't saved
		super::free_data(std::ptr::null_mut(), data);
		let saved = hist.exists();
		let _ = std::fs::remove_file(&hist);
		std::fs::remove_dir(&dir).unwrap();
		assert_eq!(first, "boom at two");
		assert!(!ran);
		assert!(!saved);
	}

	// Test ID: dekpx
	#[test]
	fn every_handler_the_panel_calls_catches_a_panic() {
		let text = include_str!("lib.rs");
		let code = text.split("#[cfg(test)]\nmod tests").next().unwrap();
		let mut seen = Vec::new();
		let mut bare = Vec::new();
		for part in code.split("extern \"C\" fn ").skip(1) {
			let name = part.split('(').next().unwrap();
			// only C calls, before anything of ours runs
			if name == "xfce_panel_module_construct" {
				continue;
			}
			let body = part.split("\n}\n").next().unwrap();
			seen.push(name);
			if !body.contains("guarded(") && !body.contains("catch_unwind(") {
				bare.push(name);
			}
		}
		assert!(seen.len() >= 13, "only {seen:?}");
		assert!(
			bare.is_empty(),
			"a panic in these aborts the panel: {bare:?}"
		);
	}

	// Test ID: dekpv
	#[test]
	fn the_release_build_unwinds_so_a_panic_can_be_caught() {
		let toml =
			std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml"))
				.unwrap();
		let release = toml
			.split("\n[")
			.find(|s| s.starts_with("profile.release]"))
			.unwrap();
		let panic: Vec<&str> = release
			.lines()
			.filter(|l| l.trim_start().starts_with("panic"))
			.collect();
		assert_eq!(panic, ["panic = \"unwind\""]);
	}

	// Test ID: dejae
	#[test]
	#[cfg_attr(miri, ignore = "GIO is C")]
	fn a_reset_mark_written_calls_back_at_once() {
		static HITS: AtomicUsize = AtomicUsize::new(0);
		unsafe extern "C" fn hit(
			_: *mut c_void,
			_: *mut c_void,
			_: *mut c_void,
			_: c_int,
			_: *mut c_void,
		) {
			HITS.fetch_add(1, Ordering::SeqCst);
		}
		let dir =
			std::env::temp_dir().join(format!("test_tukzmon_xfce_{}_watch", std::process::id()));
		std::fs::create_dir_all(&dir).unwrap();
		// SAFETY: hit takes no data.
		let w = unsafe { super::watch(&keep::reset_mark(&dir), hit, std::ptr::null_mut()) };
		assert!(!w.is_null());
		keep::reset(&dir, false).unwrap();
		let t0 = Instant::now();
		while HITS.load(Ordering::SeqCst) == 0 && t0.elapsed() < Duration::from_secs(5) {
			// SAFETY: the default context, which only this test runs.
			unsafe { super::sys::g_main_context_iteration(std::ptr::null_mut(), 0) };
			std::thread::sleep(Duration::from_millis(2));
		}
		let took = t0.elapsed();
		// SAFETY: from watch, and not used again.
		unsafe { super::unwatch(w) };
		std::fs::remove_file(keep::reset_mark(&dir)).unwrap();
		std::fs::remove_dir(&dir).unwrap();
		assert!(HITS.load(Ordering::SeqCst) > 0, "no call in {took:?}");
		assert!(took < Duration::from_millis(500), "{took:?}");
	}

	// Test ID: dekmw
	#[test]
	fn a_removed_item_takes_its_files_and_free_data_saves_nothing() {
		use tukzmon_core::config;
		let dir =
			std::env::temp_dir().join(format!("test_tukzmon_xfce_{}_removed", std::process::id()));
		std::fs::create_dir_all(&dir).unwrap();
		let shared = dir.join("tukzmon.shcl");
		std::fs::write(&shared, "shared\n").unwrap();
		let own = config::instance_file(&dir, 7);
		let hist = keep::history_file(&dir, 7);
		std::fs::write(&own, "mine\n").unwrap();
		std::fs::write(&hist, "old").unwrap();
		let data = plugin(&dir, &own, &hist);
		// the panel's order: removed, then free-data
		super::removed(std::ptr::null_mut(), data);
		super::free_data(std::ptr::null_mut(), data);
		let left = (own.exists(), hist.exists());
		// a later item with the same id starts from the shared file
		config::start_from(&own, &shared).unwrap();
		let fresh = std::fs::read_to_string(&own).unwrap();
		for f in [&own, &shared] {
			std::fs::remove_file(f).unwrap();
		}
		let _ = std::fs::remove_file(&hist);
		std::fs::remove_dir(&dir).unwrap();
		assert_eq!(left, (false, false), "(settings, history) left behind");
		assert_eq!(fresh, "shared\n");
	}

	// Test ID: defyn
	#[test]
	fn nothing_the_panel_loads_can_panic_on_a_failed_print() {
		let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
		let macros = ["print", "println", "eprint", "eprintln", "dbg"].map(|m| format!("{m}!("));
		let mut seen = 0;
		let mut hits = Vec::new();
		for dir in ["xfce/src", "core/src", "knobs/src"] {
			for entry in std::fs::read_dir(root.join(dir)).unwrap() {
				let path = entry.unwrap().path();
				if path.extension().is_none_or(|e| e != "rs") || path.ends_with("tests.rs") {
					continue;
				}
				let text = std::fs::read_to_string(&path).unwrap();
				let code = text
					.split("#[cfg(test)]\nmod tests")
					.next()
					.unwrap_or_default();
				seen += 1;
				for (n, line) in code.lines().enumerate() {
					if macros.iter().any(|m| {
						line.match_indices(m.as_str()).any(|(i, _)| {
							!line[..i].ends_with(|c: char| c.is_alphanumeric() || c == '_')
						})
					}) {
						hits.push(format!("{}:{}", path.display(), n + 1));
					}
				}
			}
		}
		assert!(seen > 10, "only {seen} files read");
		assert!(
			hits.is_empty(),
			"print macros panic on a failed write: {hits:?}"
		);
	}
}
