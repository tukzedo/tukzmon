// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

// Whether the screen is locked or blanked, so the graphs keep reading but
// don't draw while nobody can see them. See design.md Drawing.

use std::cell::Cell;
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::fmt::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

#[allow(clippy::wildcard_imports, reason = "sys is only the C declarations")]
use crate::sys::*;

const SAVER: &CStr = c"org.xfce.ScreenSaver";
const SAVER_PATH: &CStr = c"/org/xfce/ScreenSaver";
const LOGIND: &CStr = c"org.freedesktop.login1";
const LOGIND_SESSION: &CStr = c"org.freedesktop.login1.Session";
const PROPERTIES: &CStr = c"org.freedesktop.DBus.Properties";
const BUS: &CStr = c"org.freedesktop.DBus";
const BUS_PATH: &CStr = c"/org/freedesktop/DBus";

#[derive(Clone, Copy, Debug)]
enum Source {
	/// xfce4-screensaver's `ActiveChanged`, blanked or locked.
	Saver,
	/// logind's `LockedHint`, which light-locker and others set.
	Hint,
}

/// What the buses said, shared with the D-Bus callbacks, which can run
/// after the [`Locks`] that started them is gone.
#[derive(Debug)]
struct Seen {
	saver: Cell<bool>,
	hint: Cell<bool>,
	/// Drawn again on unlock. Null once the item is gone.
	area: Cell<*mut c_void>,
	#[cfg(test)]
	draws: Cell<u32>,
}

impl Seen {
	fn new(area: *mut c_void) -> Self {
		Self {
			saver: Cell::new(false),
			hint: Cell::new(false),
			area: Cell::new(area),
			#[cfg(test)]
			draws: Cell::new(0),
		}
	}

	fn hidden(&self) -> bool {
		self.saver.get() || self.hint.get()
	}

	fn draw(&self) {
		if self.hidden() {
			return;
		}
		#[cfg(test)]
		self.draws.set(self.draws.get() + 1);
		let area = self.area.get();
		if !area.is_null() {
			// SAFETY: Locks::stop nulls it before the widget goes.
			unsafe { gtk_widget_queue_draw(area) };
		}
	}

	/// The history kept every sample, so one draw on unlock catches up.
	fn set(&self, source: Source, on: bool) {
		let was = self.hidden();
		match source {
			Source::Saver => self.saver.set(on),
			Source::Hint => self.hint.set(on),
		}
		if was && !self.hidden() {
			self.draw();
		}
	}
}

#[derive(Debug)]
struct Bus {
	connection: *mut c_void,
	subscriptions: Vec<std::ffi::c_uint>,
}

/// Follows xfce4-screensaver on the session bus, and this session's
/// `LockedHint` on the system bus.
#[derive(Debug)]
pub struct Locks {
	seen: Rc<Seen>,
	buses: Vec<Bus>,
}

impl Locks {
	/// Never hidden, for an item not started yet, or a test.
	pub fn none() -> Self {
		Self {
			seen: Rc::new(Seen::new(std::ptr::null_mut())),
			buses: Vec::new(),
		}
	}

	/// # Safety
	///
	/// `area` is a live widget until [`Self::stop`].
	pub unsafe fn start(area: *mut c_void) -> Self {
		let mut locks = Self::none();
		locks.seen.area.set(area);
		if let Some(session) = bus(G_BUS_TYPE_SESSION) {
			// SAFETY: a live connection, whose ref this takes.
			unsafe { locks.follow_saver(session) };
		}
		// a session with no id, such as one started by hand, has no logind
		// object to watch
		let id = std::env::var("XDG_SESSION_ID").unwrap_or_default();
		if !id.is_empty()
			&& let Some(system) = bus(G_BUS_TYPE_SYSTEM)
		{
			// SAFETY: as above.
			unsafe { locks.follow_logind(system, &id) };
		}
		locks
	}

	/// True while the screen is locked or blanked.
	pub fn hidden(&self) -> bool {
		self.seen.hidden()
	}

	/// Asks for a draw, unless the screen is locked or blanked.
	pub fn draw(&self) {
		self.seen.draw();
	}

	/// # Safety
	///
	/// `connection` is a live `GDBusConnection`, whose ref this takes.
	unsafe fn follow_saver(&mut self, connection: *mut c_void) {
		let mut bus = Bus {
			connection,
			subscriptions: Vec::new(),
		};
		// SAFETY: per the caller, with literal names.
		unsafe {
			self.subscribe(
				&mut bus,
				SAVER,
				SAVER,
				c"ActiveChanged",
				SAVER_PATH,
				None,
				saver_changed,
			);
			// a screensaver that quits or crashes while up blanks nothing
			self.subscribe(
				&mut bus,
				BUS,
				BUS,
				c"NameOwnerChanged",
				BUS_PATH,
				Some(SAVER),
				saver_gone,
			);
			// for an item started while it's up, as by a panel restart
			self.ask(
				connection,
				SAVER,
				SAVER_PATH,
				SAVER,
				c"GetActive",
				std::ptr::null_mut(),
				saver_answer,
			);
		}
		self.buses.push(bus);
	}

	/// # Safety
	///
	/// As [`Self::follow_saver`].
	unsafe fn follow_logind(&mut self, connection: *mut c_void, id: &str) {
		let Ok(path) = CString::new(session_path(id)) else {
			// SAFETY: the ref the caller handed over.
			unsafe { g_object_unref(connection) };
			return;
		};
		let mut bus = Bus {
			connection,
			subscriptions: Vec::new(),
		};
		// SAFETY: per the caller. The tuple takes the floating strings, and
		// the call takes the tuple.
		unsafe {
			self.subscribe(
				&mut bus,
				LOGIND,
				PROPERTIES,
				c"PropertiesChanged",
				&path,
				Some(LOGIND_SESSION),
				hint_changed,
			);
			let wanted = [
				g_variant_new_string(LOGIND_SESSION.as_ptr()),
				g_variant_new_string(c"LockedHint".as_ptr()),
			];
			self.ask(
				connection,
				LOGIND,
				&path,
				PROPERTIES,
				c"Get",
				g_variant_new_tuple(wanted.as_ptr(), wanted.len()),
				hint_answer,
			);
		}
		self.buses.push(bus);
	}

	/// # Safety
	///
	/// `bus` has a live connection.
	#[allow(clippy::too_many_arguments, reason = "the D-Bus match rule")]
	unsafe fn subscribe(
		&self,
		bus: &mut Bus,
		sender: &CStr,
		interface: &CStr,
		member: &CStr,
		path: &CStr,
		arg0: Option<&CStr>,
		callback: SignalFn,
	) {
		let data = Rc::into_raw(Rc::clone(&self.seen)).cast_mut().cast();
		// SAFETY: per the caller. release takes back data's Rc.
		let id = unsafe {
			g_dbus_connection_signal_subscribe(
				bus.connection,
				sender.as_ptr(),
				interface.as_ptr(),
				member.as_ptr(),
				path.as_ptr(),
				arg0.map_or(std::ptr::null(), CStr::as_ptr),
				0,
				callback,
				data,
				release,
			)
		};
		bus.subscriptions.push(id);
	}

	/// # Safety
	///
	/// `connection` is live, and `parameters` null or a tuple it can take.
	#[allow(clippy::too_many_arguments, reason = "the D-Bus call")]
	unsafe fn ask(
		&self,
		connection: *mut c_void,
		name: &CStr,
		path: &CStr,
		interface: &CStr,
		method: &CStr,
		parameters: *mut c_void,
		done: ReadyFn,
	) {
		let data = Rc::into_raw(Rc::clone(&self.seen)).cast_mut().cast();
		// SAFETY: per the caller. done takes back data's Rc.
		unsafe {
			g_dbus_connection_call(
				connection,
				name.as_ptr(),
				path.as_ptr(),
				interface.as_ptr(),
				method.as_ptr(),
				parameters,
				std::ptr::null(),
				// a box with no screensaver doesn't get one started
				G_DBUS_CALL_FLAGS_NO_AUTO_START,
				-1,
				std::ptr::null_mut(),
				done,
				data,
			);
		}
	}

	/// Ends the subscriptions and forgets the area. A reply still on its
	/// way finds no area to draw.
	pub fn stop(&mut self) {
		self.seen.area.set(std::ptr::null_mut());
		for bus in self.buses.drain(..) {
			// SAFETY: each id is from subscribe on this connection, and the
			// connection's ref is ours.
			unsafe {
				for id in bus.subscriptions {
					g_dbus_connection_signal_unsubscribe(bus.connection, id);
				}
				g_object_unref(bus.connection);
			}
		}
	}
}

impl Drop for Locks {
	fn drop(&mut self) {
		self.stop();
	}
}

fn bus(kind: c_int) -> Option<*mut c_void> {
	let mut error = std::ptr::null_mut();
	// SAFETY: a plain lookup. The session bus is already open in the panel,
	// for xfconf.
	let connection = unsafe { g_bus_get_sync(kind, std::ptr::null_mut(), &raw mut error) };
	if !error.is_null() {
		// SAFETY: set by the call, and ours.
		unsafe { g_error_free(error) };
	}
	(!connection.is_null()).then_some(connection)
}

/// logind's object for session `id`, escaped as `sd_bus_path_encode` does:
/// a leading digit, and anything not a letter or digit, as `_` and 2 hex
/// digits.
fn session_path(id: &str) -> String {
	let mut path = String::from("/org/freedesktop/login1/session/");
	if id.is_empty() {
		path.push('_');
	}
	for (i, b) in id.bytes().enumerate() {
		if b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()) {
			path.push(char::from(b));
		} else {
			let _ = write!(path, "_{b:02x}");
		}
	}
	path
}

/// Runs `f` on `v`, a variant it doesn't keep, then lets it go.
///
/// # Safety
///
/// `v` is null or a variant whose ref is the caller's.
unsafe fn with<R>(v: *mut c_void, f: impl FnOnce(*mut c_void) -> Option<R>) -> Option<R> {
	if v.is_null() {
		return None;
	}
	let r = f(v);
	// SAFETY: per the caller.
	unsafe { g_variant_unref(v) };
	r
}

/// The one boolean in `b`, `v`, `(b)` or `(v)`.
///
/// # Safety
///
/// `v` is null or a live variant.
unsafe fn boolean_in(v: *mut c_void) -> Option<bool> {
	if v.is_null() {
		return None;
	}
	// SAFETY: a live variant, per the caller. Each child and inner value is
	// a new ref, which with lets go.
	unsafe {
		match CStr::from_ptr(g_variant_get_type_string(v)).to_bytes() {
			b"b" => Some(g_variant_get_boolean(v) != 0),
			b"v" => with(g_variant_get_variant(v), |inner| boolean_in(inner)),
			b"(b)" | b"(v)" => with(g_variant_get_child_value(v, 0), |child| boolean_in(child)),
			_ => None,
		}
	}
}

/// # Safety
///
/// `data` is a `Seen` from [`Rc::into_raw`], still held.
unsafe fn seen<'a>(data: *mut c_void) -> &'a Seen {
	// SAFETY: per the caller.
	unsafe { &*data.cast::<Seen>() }
}

extern "C" fn release(data: *mut c_void) {
	// SAFETY: the Rc subscribe gave GDBus, handed back once.
	let _ = catch_unwind(|| drop(unsafe { Rc::from_raw(data.cast::<Seen>()) }));
}

// ActiveChanged (b)
extern "C" fn saver_changed(
	_connection: *mut c_void,
	_sender: *const c_char,
	_path: *const c_char,
	_interface: *const c_char,
	_signal: *const c_char,
	parameters: *mut c_void,
	data: *mut c_void,
) {
	let _ = catch_unwind(AssertUnwindSafe(|| {
		// SAFETY: data is from subscribe, and GDBus owns parameters.
		unsafe {
			if let Some(on) = boolean_in(parameters) {
				seen(data).set(Source::Saver, on);
			}
		}
	}));
}

// NameOwnerChanged (name, old owner, new owner)
extern "C" fn saver_gone(
	_connection: *mut c_void,
	_sender: *const c_char,
	_path: *const c_char,
	_interface: *const c_char,
	_signal: *const c_char,
	parameters: *mut c_void,
	data: *mut c_void,
) {
	let _ = catch_unwind(AssertUnwindSafe(|| {
		// SAFETY: as saver_changed. The string lives as long as its child.
		unsafe {
			let gone = with(g_variant_get_child_value(parameters, 2), |owner| {
				Some(*g_variant_get_string(owner, std::ptr::null_mut()) == 0)
			});
			if gone == Some(true) {
				seen(data).set(Source::Saver, false);
			}
		}
	}));
}

// PropertiesChanged (interface, changed, invalidated)
extern "C" fn hint_changed(
	_connection: *mut c_void,
	_sender: *const c_char,
	_path: *const c_char,
	_interface: *const c_char,
	_signal: *const c_char,
	parameters: *mut c_void,
	data: *mut c_void,
) {
	let _ = catch_unwind(AssertUnwindSafe(|| {
		// SAFETY: as saver_changed.
		unsafe {
			let hint = with(g_variant_get_child_value(parameters, 1), |changed| {
				with(
					g_variant_lookup_value(changed, c"LockedHint".as_ptr(), c"b".as_ptr()),
					|v| boolean_in(v),
				)
			});
			if let Some(on) = hint {
				seen(data).set(Source::Hint, on);
			}
		}
	}));
}

/// # Safety
///
/// As a `GAsyncReadyCallback` from [`Locks::ask`].
unsafe fn answered(source: *mut c_void, res: *mut c_void, data: *mut c_void, from: Source) {
	// SAFETY: the Rc ask leaked for this one reply.
	let seen = unsafe { Rc::from_raw(data.cast::<Seen>()) };
	let _ = catch_unwind(AssertUnwindSafe(|| {
		let mut error = std::ptr::null_mut();
		// SAFETY: source is the connection the call went on, and res its
		// result. No screensaver, or no session, is an error and no answer.
		unsafe {
			let reply = g_dbus_connection_call_finish(source, res, &raw mut error);
			if !error.is_null() {
				g_error_free(error);
			}
			if let Some(on) = with(reply, |r| boolean_in(r)) {
				seen.set(from, on);
			}
		}
	}));
}

extern "C" fn saver_answer(source: *mut c_void, res: *mut c_void, data: *mut c_void) {
	// SAFETY: GDBus calls this once for the call ask made.
	let _ = catch_unwind(|| unsafe { answered(source, res, data, Source::Saver) });
}

extern "C" fn hint_answer(source: *mut c_void, res: *mut c_void, data: *mut c_void) {
	// SAFETY: as saver_answer.
	let _ = catch_unwind(|| unsafe { answered(source, res, data, Source::Hint) });
}

#[cfg(test)]
mod tests {
	use std::ffi::CStr;
	use std::rc::Rc;
	use std::time::{Duration, Instant};

	use super::{Locks, Seen, boolean_in, session_path};
	#[allow(clippy::wildcard_imports, reason = "sys is only the C declarations")]
	use crate::sys::*;

	/// Hands `text`, a `GVariant` in its text form, to a signal callback.
	fn signal(callback: SignalFn, seen: &Rc<Seen>, text: &CStr) {
		// SAFETY: a literal that parses, sunk so it's ours to let go.
		unsafe {
			let v = g_variant_ref_sink(g_variant_new_parsed(text.as_ptr()));
			let null = std::ptr::null();
			callback(
				std::ptr::null_mut(),
				null,
				null,
				null,
				null,
				v,
				Rc::as_ptr(seen).cast_mut().cast(),
			);
			g_variant_unref(v);
		}
	}

	// Test ID: df3qr
	#[test]
	#[cfg_attr(miri, ignore = "GLib is C")]
	fn lock_hides_graphs_until_unlock() {
		let seen = Rc::new(Seen::new(std::ptr::null_mut()));
		let mut steps = Vec::new();
		let mut note = |seen: &Seen| steps.push((seen.hidden(), seen.draws.get()));
		seen.draw();
		note(&seen);
		signal(super::saver_changed, &seen, c"(true,)");
		seen.draw();
		note(&seen);
		let hint = |on: &str| {
			std::ffi::CString::new(format!(
				"('org.freedesktop.login1.Session', {{'LockedHint': <{on}>}}, @as [])"
			))
			.unwrap()
		};
		signal(super::hint_changed, &seen, &hint("true"));
		signal(super::saver_changed, &seen, c"(false,)");
		note(&seen);
		// another property leaves it
		signal(
			super::hint_changed,
			&seen,
			c"('org.freedesktop.login1.Session', {'Active': <true>}, @as [])",
		);
		note(&seen);
		signal(super::hint_changed, &seen, &hint("false"));
		note(&seen);
		signal(super::saver_changed, &seen, c"(true,)");
		// a new owner isn't gone
		signal(
			super::saver_gone,
			&seen,
			c"('org.xfce.ScreenSaver', ':1.5', ':1.6')",
		);
		note(&seen);
		signal(
			super::saver_gone,
			&seen,
			c"('org.xfce.ScreenSaver', ':1.6', '')",
		);
		note(&seen);
		assert_eq!(
			steps,
			[
				(false, 1),
				(true, 1),
				(true, 1),
				(true, 1),
				(false, 2),
				(true, 2),
				(false, 3)
			]
		);
		// SAFETY: literals that parse, sunk so they're ours to let go.
		let answers: Vec<Option<bool>> = ["(<true>,)", "(false,)", "('no',)", "(<'no'>,)"]
			.iter()
			.map(|t| unsafe {
				let text = std::ffi::CString::new(*t).unwrap();
				let v = g_variant_ref_sink(g_variant_new_parsed(text.as_ptr()));
				let b = boolean_in(v);
				g_variant_unref(v);
				b
			})
			.collect();
		assert_eq!(answers, [Some(true), Some(false), None, None]);
	}

	// Test ID: df3qs
	#[test]
	fn logind_path_escapes_digit() {
		let path = |id| session_path(id).rsplit('/').next().unwrap().to_owned();
		assert_eq!(path("15938"), "_315938");
		assert_eq!(path("c12"), "c12");
		assert_eq!(path("a-1"), "a_2d1");
		assert_eq!(path(""), "_");
	}

	// Test ID: df3qt
	#[test]
	#[cfg_attr(miri, ignore = "GLib is C")]
	fn private_bus_lock_hides_graphs() {
		// Killed on a failed assert too, so no bus outlives the test.
		struct Daemon(std::process::Child, std::path::PathBuf);
		impl Drop for Daemon {
			fn drop(&mut self) {
				let _ = self.0.kill();
				let _ = self.0.wait();
				let _ = std::fs::remove_file(&self.1);
			}
		}
		// A config of its own, since the stock one, and EXTERNAL auth, look
		// the caller up in passwd, and the build image runs as a uid it
		// doesn't list. Its policy lets all through, as the stock one does.
		let address = format!("unix:abstract=tukzmon-lock-test-{}", std::process::id());
		let config =
			std::env::temp_dir().join(format!("tukzmon-lock-test-{}.conf", std::process::id()));
		std::fs::write(
			&config,
			format!(
				"<busconfig><type>session</type><listen>{address}</listen><auth>ANONYMOUS</auth><allow_anonymous/>\
				<policy context=\"default\"><allow send_destination=\"*\"/><allow receive_sender=\"*\"/>\
				<allow own=\"*\"/></policy>\
				</busconfig>"
			),
		)
		.unwrap();
		let mut daemon = Daemon(
			std::process::Command::new("dbus-daemon")
				.args(["--nofork", "--print-address"])
				.arg(format!("--config-file={}", config.display()))
				.stdout(std::process::Stdio::piped())
				.spawn()
				.expect("dbus-daemon, from the dbus-daemon package"),
			config,
		);
		// It prints the address once it listens, which beats a fixed wait
		// on a loaded box.
		let mut listening = String::new();
		std::io::BufRead::read_line(
			&mut std::io::BufReader::new(daemon.0.stdout.take().unwrap()),
			&mut listening,
		)
		.unwrap();
		assert!(
			listening.starts_with("unix:"),
			"dbus-daemon said {listening:?}"
		);
		let address = std::ffi::CString::new(address).unwrap();
		let session = std::ffi::CString::new(session_path("7")).unwrap();
		let other = std::ffi::CString::new(session_path("8")).unwrap();
		let mut steps = Vec::new();
		// SAFETY: GDBus calls with literal names, on connections and a
		// context this test makes and lets go.
		unsafe {
			let context = g_main_context_new();
			g_main_context_push_thread_default(context);
			let null = std::ptr::null_mut();
			let no_error = std::ptr::null_mut();
			// client, and a message bus connection
			let connect = || {
				let c = g_dbus_connection_new_for_address_sync(
					address.as_ptr(),
					1 | 8,
					null,
					null,
					no_error,
				);
				assert!(!c.is_null());
				c
			};
			let own = |connection, name: &CStr| {
				let wanted = [
					g_variant_new_string(name.as_ptr()),
					g_variant_new_parsed(c"uint32 0".as_ptr()),
				];
				let reply = g_dbus_connection_call_sync(
					connection,
					super::BUS.as_ptr(),
					super::BUS_PATH.as_ptr(),
					super::BUS.as_ptr(),
					c"RequestName".as_ptr(),
					g_variant_new_tuple(wanted.as_ptr(), wanted.len()),
					std::ptr::null(),
					0,
					-1,
					null,
					no_error,
				);
				g_variant_unref(reply);
			};
			let emit = |connection, path: &CStr, interface: &CStr, signal: &CStr, args: &CStr| {
				g_dbus_connection_emit_signal(
					connection,
					std::ptr::null(),
					path.as_ptr(),
					interface.as_ptr(),
					signal.as_ptr(),
					g_variant_new_parsed(args.as_ptr()),
					no_error,
				);
			};
			let (saver, logind, stranger) = (connect(), connect(), connect());
			own(saver, super::SAVER);
			own(logind, super::LOGIND);
			let mut locks = Locks::none();
			locks.follow_saver(connect());
			locks.follow_logind(connect(), "7");
			let settle = |locks: &Locks, want: bool| {
				let until = Instant::now() + Duration::from_secs(5);
				while locks.hidden() != want && Instant::now() < until {
					g_main_context_iteration(context, 0);
					std::thread::sleep(Duration::from_millis(5));
				}
				locks.hidden()
			};
			let active = |connection, on: &CStr| {
				emit(
					connection,
					super::SAVER_PATH,
					super::SAVER,
					c"ActiveChanged",
					on,
				);
			};
			let hint = |path: &CStr, on: &CStr| {
				emit(logind, path, super::PROPERTIES, c"PropertiesChanged", on);
			};
			let (on, off) = (
				c"('org.freedesktop.login1.Session', {'LockedHint': <true>}, @as [])",
				c"('org.freedesktop.login1.Session', {'LockedHint': <false>}, @as [])",
			);
			// a stranger, and another session, are let be
			active(stranger, c"(true,)");
			hint(&other, on);
			let until = Instant::now() + Duration::from_millis(300);
			while Instant::now() < until {
				g_main_context_iteration(context, 0);
				std::thread::sleep(Duration::from_millis(5));
			}
			steps.push(locks.hidden());
			active(saver, c"(true,)");
			steps.push(settle(&locks, true));
			active(saver, c"(false,)");
			steps.push(settle(&locks, false));
			hint(&session, on);
			steps.push(settle(&locks, true));
			hint(&session, off);
			steps.push(settle(&locks, false));
			active(saver, c"(true,)");
			steps.push(settle(&locks, true));
			g_dbus_connection_close_sync(saver, null, no_error);
			steps.push(settle(&locks, false));
			steps.push(locks.seen.draws.get() == 3);
			drop(locks);
			for c in [saver, logind, stranger] {
				g_object_unref(c);
			}
			for _ in 0..20 {
				g_main_context_iteration(context, 0);
			}
			g_main_context_pop_thread_default(context);
			g_main_context_unref(context);
		}
		drop(daemon);
		assert_eq!(steps, [false, true, false, true, false, true, false, true]);
	}
}
