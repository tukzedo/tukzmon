// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! The XFCE panel plugin. design.md, XFCE plugin, has the why of doing this
//! with hand-declared calls instead of headers.

use std::ffi::{CStr, CString, c_char, c_int, c_uint, c_ulong, c_void};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::SystemTime;

use tukzmon_core::Graph;
use tukzmon_core::config::{self, Look};

mod sys;
#[allow(clippy::wildcard_imports, reason = "sys is only the C declarations")]
use sys::*;

struct Plugin {
	xpp: *mut c_void,
	area: *mut c_void,
	graph: Graph,
	timer: c_uint,
	timer_ms: u32,
	config: PathBuf,
	config_time: Option<SystemTime>,
	tip: String,
	settings: Option<Child>,
}

fn modified(p: &Path) -> Option<SystemTime> {
	std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

impl Plugin {
	fn fit(&self) {
		// SAFETY: plugin is the live XfcePanelPlugin this state belongs to.
		unsafe {
			let size = xfce_panel_plugin_get_size(self.xpp);
			let w = self.graph.look.width as c_int;
			if xfce_panel_plugin_get_mode(self.xpp) == MODE_HORIZONTAL {
				gtk_widget_set_size_request(self.xpp, w, size);
			} else {
				gtk_widget_set_size_request(self.xpp, size, (size / 2).max(16));
			}
		}
	}

	fn start_timer(&mut self) {
		self.timer_ms = self.graph.look.sample_ms.max(50);
		// SAFETY: self lives in a Box until free-data, which stops this timer.
		self.timer = unsafe { g_timeout_add(self.timer_ms, tick, std::ptr::from_mut(self).cast()) };
	}

	/// Pick up a save from the settings window.
	fn reload_if_changed(&mut self) {
		let t = modified(&self.config);
		if t == self.config_time {
			return;
		}
		self.config_time = t;
		let look = Look::load(&self.config);
		if look != self.graph.look {
			let refit = look.width != self.graph.look.width;
			self.graph.look = look;
			if refit {
				self.fit();
			}
		}
	}
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
	p.graph.sample();
	let tip = p.graph.tooltip();
	if tip != p.tip {
		if let Ok(c) = CString::new(tip.as_str()) {
			// SAFETY: plugin is live and the text is copied by GTK.
			unsafe { gtk_widget_set_tooltip_text(p.xpp, c.as_ptr()) };
		}
		p.tip = tip;
	}
	// SAFETY: area is a child of the live plugin.
	unsafe { gtk_widget_queue_draw(p.area) };
	if p.graph.look.sample_ms.max(50) != p.timer_ms {
		p.start_timer();
		return 0;
	}
	1
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
		let px = p.graph.render(w as usize, h as usize, scale as usize);
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
	}
	1
}

extern "C" fn size_changed(_plugin: *mut c_void, _size: c_int, data: *mut c_void) -> Gboolean {
	// SAFETY: data is the connected Plugin.
	unsafe { &*data.cast::<Plugin>() }.fit();
	1
}

extern "C" fn mode_changed(_plugin: *mut c_void, _mode: c_int, data: *mut c_void) {
	// SAFETY: data is the connected Plugin.
	unsafe { &*data.cast::<Plugin>() }.fit();
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
		Err(e) => eprintln!("tukzmon: can't start {}: {e}", program.display()),
	}
}

extern "C" fn free_data(_plugin: *mut c_void, data: *mut c_void) {
	// SAFETY: data came from Box::into_raw in construct, and this is its last use.
	let p = unsafe { Box::from_raw(data.cast::<Plugin>()) };
	if p.timer != 0 {
		// SAFETY: the timer id is ours and still running.
		unsafe { g_source_remove(p.timer) };
	}
}

fn construct(plugin: *mut c_void) {
	let config = config::config_file();
	let mut p = Box::new(Plugin {
		xpp: plugin,
		area: std::ptr::null_mut(),
		graph: Graph::new(Look::load(&config)),
		timer: 0,
		timer_ms: 0,
		config_time: modified(&config),
		config,
		tip: String::new(),
		settings: None,
	});
	// SAFETY: plugin is a fresh XfcePanelPlugin, and every handler gets the
	// boxed Plugin, which outlives them all until free-data.
	unsafe {
		p.area = gtk_drawing_area_new();
		gtk_container_add(plugin, p.area);
		// the area has a window of its own, so clicks stop there unless it takes
		// them and hands right-clicks on to the panel's menu
		gtk_widget_add_events(p.area, GDK_BUTTON_PRESS_MASK | GDK_BUTTON_RELEASE_MASK);
		xfce_panel_plugin_add_action_widget(plugin, p.area);
		let data: *mut c_void = std::ptr::from_mut(p.as_mut()).cast();
		connect(p.area, c"draw", draw as *const c_void, data);
		connect(plugin, c"size-changed", size_changed as *const c_void, data);
		connect(plugin, c"mode-changed", mode_changed as *const c_void, data);
		connect(
			plugin,
			c"configure-plugin",
			configure as *const c_void,
			data,
		);
		xfce_panel_plugin_menu_show_configure(plugin);
		p.graph.sample();
		p.fit();
		p.start_timer();
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
