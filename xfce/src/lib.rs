// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! The XFCE panel plugin. design.md, XFCE plugin, has the why of doing this
//! with hand-declared calls instead of headers.

use std::ffi::{CStr, CString, c_char, c_int, c_uint, c_ulong, c_void};
use std::io::Write as _;
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
fn complain(what: std::fmt::Arguments) {
	let _ = writeln!(std::io::stderr().lock(), "tukzmon: {what}");
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

	fn load_history(&mut self) {
		match self.graphs.load(&self.history) {
			Ok(()) => {}
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
			Err(e) => complain(format_args!("can't read {}: {e}", self.history.display())),
		}
		self.saved_at = Instant::now();
	}

	fn save_history(&mut self) {
		if self.removed {
			return;
		}
		self.graphs.trim();
		if let Err(e) = self.graphs.save(&self.history) {
			complain(format_args!("can't save {}: {e}", self.history.display()));
		}
		self.saved_at = Instant::now();
	}

	/// Pick up a save from the settings window.
	fn reload_if_changed(&mut self) {
		let t = modified(&self.config);
		if t == self.config_time {
			return;
		}
		self.config_time = t;
		let look = Look::load(&self.config);
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
				complain(format_args!("can't read {}: {e}", self.history.display()));
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
		if let Err(e) = self.graphs.reset(saved) {
			complain(format_args!("can't delete {}: {e}", self.history.display()));
		}
		self.saved_at = Instant::now();
	}

	/// The panel's Remove says the item's configuration is lost, so its
	/// files go, and a later item given the same id starts fresh.
	fn forget(&mut self) {
		self.removed = true;
		if self.timer != 0 {
			// SAFETY: the timer id is ours and still running.
			unsafe { g_source_remove(self.timer) };
			self.timer = 0;
		}
		// deleting the settings file would wake its watch
		for w in self.watches.drain(..) {
			// SAFETY: each is from watch, and isn't used again.
			unsafe { unwatch(w) };
		}
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
	// SAFETY: data is the Plugin the monitor was made for, still alive.
	let p = unsafe { &mut *data.cast::<Plugin>() };
	if p.timer != 0 {
		// SAFETY: the timer id is ours and still running, since tick sets a
		// new one each time it runs.
		unsafe { g_source_remove(p.timer) };
		p.timer = 0;
	}
	tick(data);
}

extern "C" fn tick(data: *mut c_void) -> Gboolean {
	// SAFETY: data is the Plugin the timer was started with, still alive.
	let p = unsafe { &mut *data.cast::<Plugin>() };
	// reap a closed settings window
	if p.settings
		.as_mut()
		.is_some_and(|c| !matches!(c.try_wait(), Ok(None)))
	{
		p.settings = None;
	}
	p.reload_if_changed();
	p.reset_if_asked();
	let next = p.graphs.tick();
	if p.graphs.look().keep && p.saved_at.elapsed() >= SAVE_EVERY {
		p.save_history();
	}
	// SAFETY: area is a child of the live plugin.
	unsafe {
		gtk_widget_queue_draw(p.area);
		if p.graphs.hovered() && p.moved_at.elapsed() >= TIP_SHOWN {
			// a still pointer's time moves on too
			gtk_widget_trigger_tooltip_query(p.area);
		}
	}
	p.start_timer(next);
	0
}

extern "C" fn draw(widget: *mut c_void, cr: *mut c_void, data: *mut c_void) -> Gboolean {
	// SAFETY: data is the Plugin connected with this handler.
	let p = unsafe { &mut *data.cast::<Plugin>() };
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
	// SAFETY: data is the Plugin the timer was started with, still alive.
	let p = unsafe { &mut *data.cast::<Plugin>() };
	p.frame = 0;
	// SAFETY: area is a child of the live plugin.
	unsafe { gtk_widget_queue_draw(p.area) };
	0
}

extern "C" fn motion(widget: *mut c_void, event: *mut c_void, data: *mut c_void) -> Gboolean {
	// SAFETY: data is the connected Plugin.
	let p = unsafe { &mut *data.cast::<Plugin>() };
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
}

extern "C" fn leave(widget: *mut c_void, _event: *mut c_void, data: *mut c_void) -> Gboolean {
	// SAFETY: data is the connected Plugin.
	let p = unsafe { &mut *data.cast::<Plugin>() };
	if p.graphs.hover(None, 0, 0, 1) {
		// SAFETY: widget is live for this event.
		unsafe { gtk_widget_queue_draw(widget) };
	}
	0
}

extern "C" fn query_tooltip(
	widget: *mut c_void,
	x: c_int,
	y: c_int,
	keyboard: Gboolean,
	tooltip: *mut c_void,
	data: *mut c_void,
) -> Gboolean {
	// SAFETY: data is the connected Plugin.
	let p = unsafe { &*data.cast::<Plugin>() };
	let tip = if keyboard == 0 {
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
	let Ok(c) = CString::new(tip) else {
		return 0;
	};
	// SAFETY: tooltip is GTK's for this query, and it copies the text.
	unsafe { gtk_tooltip_set_text(tooltip, c.as_ptr()) };
	1
}

extern "C" fn size_changed(_plugin: *mut c_void, _size: c_int, data: *mut c_void) -> Gboolean {
	// SAFETY: data is the connected Plugin.
	unsafe { &mut *data.cast::<Plugin>() }.fit();
	1
}

extern "C" fn mode_changed(_plugin: *mut c_void, _mode: c_int, data: *mut c_void) {
	// SAFETY: data is the connected Plugin.
	unsafe { &mut *data.cast::<Plugin>() }.fit();
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
	// SAFETY: data is the connected Plugin.
	let p = unsafe { &mut *data.cast::<Plugin>() };
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
}

extern "C" fn removed(_plugin: *mut c_void, data: *mut c_void) {
	// SAFETY: data is the connected Plugin, and free-data comes after this.
	unsafe { &mut *data.cast::<Plugin>() }.forget();
}

extern "C" fn free_data(_plugin: *mut c_void, data: *mut c_void) {
	// SAFETY: data came from Box::into_raw in construct, and this is its last use.
	let mut p = unsafe { Box::from_raw(data.cast::<Plugin>()) };
	if p.graphs.look().keep {
		p.save_history();
	}
	if p.timer != 0 {
		// SAFETY: the timer id is ours and still running.
		unsafe { g_source_remove(p.timer) };
	}
	if p.frame != 0 {
		// SAFETY: as above.
		unsafe { g_source_remove(p.frame) };
	}
	for w in p.watches.drain(..) {
		// SAFETY: each is from watch, and the plugin goes with this call.
		unsafe { unwatch(w) };
	}
}

fn construct(plugin: *mut c_void) {
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
	let mark = keep::reset_mark(&data);
	let mut p = Box::new(Plugin {
		xpp: plugin,
		area: std::ptr::null_mut(),
		graphs: Graphs::new(Look::load(&config)),
		timer: 0,
		frame: 0,
		config_time: modified(&config),
		config,
		settings: None,
		history: keep::history_file(&data, id),
		reset: keep::ResetWatch::new(&data),
		watches: Vec::new(),
		saved_at: Instant::now(),
		moved_at: Instant::now(),
		removed: false,
	});
	if p.graphs.look().keep {
		p.load_history();
	}
	// SAFETY: plugin is a fresh XfcePanelPlugin, and every handler gets the
	// boxed Plugin, which outlives them all until free-data.
	unsafe {
		p.area = gtk_drawing_area_new();
		gtk_container_add(plugin, p.area);
		// the area has a window of its own, so clicks stop there unless it takes
		// them and hands right-clicks on to the panel's menu
		gtk_widget_add_events(
			p.area,
			GDK_BUTTON_PRESS_MASK
				| GDK_BUTTON_RELEASE_MASK
				| GDK_POINTER_MOTION_MASK
				| GDK_LEAVE_NOTIFY_MASK,
		);
		xfce_panel_plugin_add_action_widget(plugin, p.area);
		gtk_widget_set_has_tooltip(p.area, 1);
		let data: *mut c_void = std::ptr::from_mut(p.as_mut()).cast();
		connect(p.area, c"draw", draw as *const c_void, data);
		connect(
			p.area,
			c"motion-notify-event",
			motion as *const c_void,
			data,
		);
		connect(p.area, c"leave-notify-event", leave as *const c_void, data);
		connect(
			p.area,
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
		p.watches = vec![watch(&p.config, changed, data), watch(&mark, changed, data)];
		let first = p.graphs.tick();
		p.fit();
		p.start_timer(first);
		gtk_widget_show_all(plugin);
		connect(
			plugin,
			c"free-data",
			free_data as *const c_void,
			Box::into_raw(p).cast(),
		);
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
	construct(plugin);
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

	use tukzmon_core::keep;

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
		use tukzmon_core::config::{self, Look};
		let dir =
			std::env::temp_dir().join(format!("test_tukzmon_xfce_{}_removed", std::process::id()));
		std::fs::create_dir_all(&dir).unwrap();
		let shared = dir.join("tukzmon.shcl");
		std::fs::write(&shared, "shared\n").unwrap();
		let own = config::instance_file(&dir, 7);
		let hist = keep::history_file(&dir, 7);
		std::fs::write(&own, "mine\n").unwrap();
		std::fs::write(&hist, "old").unwrap();
		let look = Look::default();
		assert!(look.keep, "free-data saves only when kept");
		let p = Box::new(super::Plugin {
			xpp: std::ptr::null_mut(),
			area: std::ptr::null_mut(),
			graphs: tukzmon_core::graphs::Graphs::new(look),
			timer: 0,
			frame: 0,
			config: own.clone(),
			config_time: None,
			settings: None,
			history: hist.clone(),
			reset: keep::ResetWatch::new(&dir),
			watches: Vec::new(),
			saved_at: Instant::now(),
			moved_at: Instant::now(),
			removed: false,
		});
		let data: *mut c_void = Box::into_raw(p).cast();
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
