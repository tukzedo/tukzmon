// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

// The few GLib, GTK 3, cairo, and libxfce4panel calls the plugin makes, by
// hand. Every widget is an opaque pointer, so no struct layouts are copied.

use std::ffi::{c_char, c_int, c_uint, c_ulong, c_void};

pub type GType = usize;
pub type Gboolean = c_int;

pub const G_CONNECT_AFTER: c_uint = 1;
pub const G_SIGNAL_MATCH_FUNC: c_uint = 1 << 3;
pub const CAIRO_FORMAT_ARGB32: c_int = 0;
pub const MODE_HORIZONTAL: c_int = 0;
pub const GDK_BUTTON_PRESS_MASK: c_int = 1 << 8;
pub const GDK_BUTTON_RELEASE_MASK: c_int = 1 << 9;

#[repr(C)]
#[allow(clippy::struct_field_names, reason = "glibc's Dl_info, as named there")]
pub struct DlInfo {
	pub dli_fname: *const c_char,
	pub dli_fbase: *mut c_void,
	pub dli_sname: *const c_char,
	pub dli_saddr: *mut c_void,
}

impl Default for DlInfo {
	fn default() -> Self {
		DlInfo {
			dli_fname: std::ptr::null(),
			dli_fbase: std::ptr::null_mut(),
			dli_sname: std::ptr::null(),
			dli_saddr: std::ptr::null_mut(),
		}
	}
}

unsafe extern "C" {
	pub fn dladdr(addr: *const c_void, info: *mut DlInfo) -> c_int;

	pub fn g_object_new(t: GType, first: *const c_char, ...) -> *mut c_void;
	pub fn g_signal_connect_data(
		instance: *mut c_void,
		signal: *const c_char,
		handler: *const c_void,
		data: *mut c_void,
		destroy: *const c_void,
		flags: c_uint,
	) -> c_ulong;
	pub fn g_signal_handlers_disconnect_matched(
		instance: *mut c_void,
		mask: c_uint,
		signal_id: c_uint,
		detail: u32,
		closure: *mut c_void,
		func: *const c_void,
		data: *mut c_void,
	) -> c_uint;
	pub fn g_timeout_add(
		ms: c_uint,
		f: extern "C" fn(*mut c_void) -> Gboolean,
		data: *mut c_void,
	) -> c_uint;
	pub fn g_source_remove(id: c_uint) -> Gboolean;

	pub fn gtk_drawing_area_new() -> *mut c_void;
	pub fn gtk_container_add(container: *mut c_void, widget: *mut c_void);
	pub fn gtk_widget_show_all(widget: *mut c_void);
	pub fn gtk_widget_set_size_request(widget: *mut c_void, w: c_int, h: c_int);
	pub fn gtk_widget_queue_draw(widget: *mut c_void);
	pub fn gtk_widget_get_allocated_width(widget: *mut c_void) -> c_int;
	pub fn gtk_widget_get_allocated_height(widget: *mut c_void) -> c_int;
	pub fn gtk_widget_get_scale_factor(widget: *mut c_void) -> c_int;
	pub fn gtk_widget_set_tooltip_text(widget: *mut c_void, text: *const c_char);
	pub fn gtk_widget_add_events(widget: *mut c_void, events: c_int);

	pub fn cairo_image_surface_create_for_data(
		data: *mut u8,
		format: c_int,
		w: c_int,
		h: c_int,
		stride: c_int,
	) -> *mut c_void;
	pub fn cairo_surface_set_device_scale(surface: *mut c_void, x: f64, y: f64);
	pub fn cairo_set_source_surface(cr: *mut c_void, surface: *mut c_void, x: f64, y: f64);
	pub fn cairo_paint(cr: *mut c_void);
	pub fn cairo_surface_destroy(surface: *mut c_void);

	pub fn xfce_panel_plugin_get_type() -> GType;
	pub fn xfce_panel_plugin_get_size(plugin: *mut c_void) -> c_int;
	pub fn xfce_panel_plugin_get_mode(plugin: *mut c_void) -> c_int;
	pub fn xfce_panel_plugin_get_unique_id(plugin: *mut c_void) -> c_int;
	pub fn xfce_panel_plugin_menu_show_configure(plugin: *mut c_void);
	pub fn xfce_panel_plugin_add_action_widget(plugin: *mut c_void, widget: *mut c_void);
}
