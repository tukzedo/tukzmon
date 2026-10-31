// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

// The calls between the Rust model and the Qt window in qt.cpp.

use knobs::{Control, Value};

use crate::dialog::Dialog;
use crate::live::Live;
use crate::pick::{Field, Hsv};

#[cxx::bridge(namespace = "tukzmon")]
pub mod ffi {
	struct TabInfo {
		label: String,
		parent: i32,
	}

	/// A line of the window's tab list, as drawn, the rows it shows, and the
	/// lines lit dimmer beside it.
	struct TabShown {
		label: String,
		rows: Vec<String>,
		dim: Vec<String>,
	}

	/// How a row is drawn. A dropdown with `shown_as: radio` is Radio.
	#[derive(Debug)]
	enum ControlKind {
		Heading,
		Checkbox,
		Slider,
		Number,
		Dropdown,
		Radio,
		Text,
		Color,
		File,
	}

	struct RowInfo {
		id: String,
		label: String,
		tab: i32,
		control: ControlKind,
		whole: bool,
		min: f64,
		max: f64,
		options: Vec<String>,
		option_labels: Vec<String>,
		indent: i32,
		/// The graph whose section it's in, or "" outside one.
		section: String,
		/// It sits to the right of the rest of its section.
		beside: bool,
		/// It picks a theme, so the theme buttons go beside it.
		chooser: bool,
		/// The graph list draws it, so it gets no row of its own.
		in_list: bool,
	}

	/// The graph list's row for one graph: the ids of the settings it shows.
	struct GraphRow {
		name: String,
		show: String,
		width: String,
		sample: String,
		time: String,
	}

	struct RowState {
		text: String,
		on: bool,
		number: f64,
		slider: f64,
		can_reset: bool,
		counts: bool,
		tip: String,
		choice: String,
		options: Vec<String>,
		option_labels: Vec<String>,
		shown: bool,
	}

	struct ThemeButtons {
		save: bool,
		rename: bool,
		remove: bool,
	}

	/// The open picker's color, and what each value box shows for it: a
	/// whole percent in each but the last, which is the hex box.
	struct PickState {
		h: f64,
		s: f64,
		v: f64,
		hex: String,
		labels: Vec<String>,
		percents: Vec<i32>,
	}

	extern "Rust" {
		type Dialog;
		fn dialog_title(d: &Dialog) -> String;
		fn dialog_tabs(d: &Dialog) -> Vec<TabInfo>;
		fn dialog_rows(d: &Dialog) -> Vec<RowInfo>;
		fn row_state(d: &Dialog, id: &str) -> RowState;
		fn set_bool(d: &mut Dialog, id: &str, on: bool);
		fn set_number(d: &mut Dialog, id: &str, v: f64);
		fn set_slider(d: &mut Dialog, id: &str, t: f64);
		fn set_text(d: &mut Dialog, id: &str, text: &str) -> bool;
		fn reset_row(d: &mut Dialog, id: &str);
		fn apply(d: &mut Dialog) -> String;
		fn is_dirty(d: &Dialog) -> bool;
		fn theme_buttons(d: &Dialog) -> ThemeButtons;
		fn theme_name(d: &Dialog) -> String;
		fn save_theme(d: &mut Dialog);
		fn save_theme_as(d: &mut Dialog, name: &str) -> String;
		fn rename_theme(d: &mut Dialog, name: &str) -> String;
		fn delete_theme(d: &mut Dialog);
		fn randomize_theme(d: &mut Dialog);
		fn pick_open(d: &mut Dialog, id: &str) -> bool;
		fn pick_state(d: &Dialog) -> PickState;
		fn pick_set(d: &mut Dialog, h: f64, s: f64, v: f64);
		fn pick_type(d: &mut Dialog, field: usize, text: &str) -> bool;
		fn pick_close(d: &mut Dialog, keep: bool);
		fn graph_rows(d: &Dialog) -> Vec<GraphRow>;
		fn move_graph(d: &mut Dialog, from: usize, to: usize) -> bool;
		fn section_order(d: &Dialog) -> Vec<String>;
		fn reset_history(d: &Dialog) -> String;
		fn reset_deletes_saved(d: &Dialog) -> bool;

		type Live;
		fn live_new(d: &Dialog) -> Box<Live>;
		fn live_look(l: &mut Live, d: &Dialog);
		fn live_clear(l: &mut Live);
		/// Reads each graph whose beat has come, and gives the ms to the next
		/// one's.
		fn live_sample(l: &mut Live) -> u32;
		fn live_width(l: &Live, name: &str) -> u32;
		/// The pixels live until the next call on `l`. cxx asks for `unsafe`
		/// to name the lifetime, and the body is safe.
		unsafe fn live_draw<'a>(
			l: &'a mut Live,
			name: &str,
			w: usize,
			h: usize,
			scale: usize,
		) -> &'a [u32];
		/// The tip at the graph's hover line, as the panel gives it.
		fn live_tip(l: &Live, name: &str, w: usize, h: usize, scale: usize) -> String;
		/// Puts the hover line at device column `x` of the graph drawn `w`
		/// wide. True when it moved.
		fn live_hover(l: &mut Live, name: &str, x: usize, w: usize) -> bool;
		/// Takes the hover line off every graph. True when one had it.
		fn live_leave(l: &mut Live) -> bool;
		/// A tip broken into lines of at most `TIP_CHARS` characters.
		fn wrap_tip(text: &str) -> String;
		/// True while the graph's time axis eases in, so it wants drawing again soon.
		fn live_easing(l: &Live, name: &str) -> bool;
	}

	unsafe extern "C++" {
		include!("tukzmon-settings/src/qt.h");
		fn run_dialog(d: &mut Dialog, snapshot: &str) -> i32;
		/// A message box, with no window behind it, for a settings file that
		/// can't be read.
		fn say_unreadable(text: &str);
		#[allow(dead_code, reason = "only the tests ask")]
		fn greyed_rows(d: &mut Dialog) -> Vec<String>;
		/// Each graph list row's graph width and x, and its width box's x.
		#[allow(dead_code, reason = "only the tests ask")]
		fn graph_list_places(d: &mut Dialog) -> Vec<i32>;
		#[allow(dead_code, reason = "only the tests ask")]
		fn tabs_shown(d: &mut Dialog) -> Vec<TabShown>;
		/// Picks the tab named `pick`, moves graph list row `from` to `to`, and
		/// gives the tab picked then, followed by each tab as `tabs_shown`
		/// does.
		#[allow(dead_code, reason = "only the tests ask")]
		fn tabs_after_move(d: &mut Dialog, pick: &str, from: usize, to: usize) -> Vec<TabShown>;
		/// Moves graph list row `from` to `to`, and gives each section on its
		/// page from the top, as "name left right": the right edge of what's
		/// in its left column, and the left edge of what's beside it.
		#[allow(dead_code, reason = "only the tests ask")]
		fn sections_after_move(d: &mut Dialog, from: usize, to: usize) -> Vec<String>;
		/// The line picked at the start and after each Ctrl+Tab, or
		/// Ctrl+Shift+Tab with `back`, pressed in the tab list or, with
		/// `from_page`, in a number box on the page. `page_keys` sends
		/// Ctrl+PgDn and Ctrl+PgUp instead.
		#[allow(dead_code, reason = "only the tests ask")]
		fn tabs_by_key(d: &mut Dialog, back: bool, from_page: bool, page_keys: bool)
		-> Vec<String>;
		/// Per section on Themes/Colors, "name left right": how far down its
		/// box the left column's first row under the colors mode starts, and
		/// the right column's first row. -1 for a column with none shown.
		#[allow(dead_code, reason = "only the tests ask")]
		fn section_tops(d: &mut Dialog) -> Vec<String>;
		#[allow(dead_code, reason = "only the tests ask")]
		fn bottom_buttons(d: &mut Dialog) -> Vec<String>;
		/// Whether Apply is on before and after typing into the first color
		/// box, or number box, and whether the dialog has the change once
		/// the box gives it up.
		#[allow(dead_code, reason = "only the tests ask")]
		fn apply_while_typing(d: &mut Dialog, number: bool) -> Vec<bool>;
		/// The window color, then per tab "label scroll rest": the most common
		/// color in the page that scrolls and in the rest of the window, past
		/// the tab list.
		#[allow(dead_code, reason = "only the tests ask")]
		fn page_shades(d: &mut Dialog, dark: bool) -> Vec<String>;
		/// Opens the window on the offscreen screens set in the file named, and
		/// gives "room x y w h" for the screen less its panels, "frame x y w h"
		/// for the window with its frame, then per tab with no tabs under it
		/// "label view content": the height its page shows, and the height the
		/// page asks for.
		#[allow(dead_code, reason = "only the tests ask")]
		fn window_fit(d: &mut Dialog, screens: &str) -> Vec<String>;
		/// The most contrast an empty checkbox has with the window, as WCAG
		/// measures it.
		#[allow(dead_code, reason = "only the tests ask")]
		fn empty_box_contrast(dark: bool) -> f64;
		/// The color chips restyled by a refresh with nothing changed, then by
		/// one after the first color row is set to `color`.
		#[allow(dead_code, reason = "only the tests ask")]
		fn chip_restyles(d: &mut Dialog, color: &str) -> Vec<i32>;
		/// Microseconds per call of `times` refreshes, then of `times` steps
		/// of the first slider, each with the events it posts.
		#[allow(dead_code, reason = "only the tests ask")]
		fn refresh_micros(d: &mut Dialog, times: usize) -> Vec<f64>;
	}
}

fn dialog_title(d: &Dialog) -> String {
	format!("tukzmon settings - {}", d.path().display())
}

// cxx's structs own their text, so the spec's is copied into them
fn dialog_tabs(d: &Dialog) -> Vec<ffi::TabInfo> {
	d.spec()
		.tabs
		.iter()
		.map(|t| ffi::TabInfo {
			label: t.label.clone(),
			parent: t.parent.map_or(-1, |p| p as i32),
		})
		.collect()
}

// None for a setting with no row
fn control_kind(s: &knobs::Setting) -> Option<ffi::ControlKind> {
	Some(match s.control {
		Control::Heading => ffi::ControlKind::Heading,
		Control::Checkbox => ffi::ControlKind::Checkbox,
		Control::Slider => ffi::ControlKind::Slider,
		Control::Number => ffi::ControlKind::Number,
		Control::Dropdown if tukzmon_core::config::shown_as_radio(&s.id) => ffi::ControlKind::Radio,
		Control::Dropdown => ffi::ControlKind::Dropdown,
		Control::Text => ffi::ControlKind::Text,
		Control::Color => ffi::ControlKind::Color,
		Control::File => ffi::ControlKind::File,
		Control::None => return None,
	})
}

fn dialog_rows(d: &Dialog) -> Vec<ffi::RowInfo> {
	d.spec()
		.settings
		.iter()
		.zip(d.sections())
		.filter_map(|(s, section)| Some((s, section, control_kind(s)?)))
		.map(|(s, section, control)| ffi::RowInfo {
			id: s.id.clone(),
			label: s.label.clone(),
			tab: s.tab as i32,
			control,
			whole: s.kind == knobs::Kind::Int,
			min: s.min,
			max: s.max,
			options: s.options.iter().map(|(k, _)| k.clone()).collect(),
			option_labels: s.options.iter().map(|(_, l)| l.clone()).collect(),
			indent: s.indent as i32,
			section: section.map_or_else(String::new, |(g, _)| g.to_string()),
			beside: section.is_some_and(|(_, b)| b),
			chooser: d.spec().chooser_of(&s.id).is_some(),
			in_list: d.in_graph_list(&s.id),
		})
		.collect()
}

fn row_state(d: &Dialog, id: &str) -> ffi::RowState {
	let s = d.state(id);
	let (options, option_labels) = s.options.into_iter().unzip();
	ffi::RowState {
		text: s.text,
		on: s.on,
		number: s.number,
		slider: s.slider,
		can_reset: s.can_reset,
		counts: s.counts,
		tip: s.tip,
		choice: s.choice,
		shown: s.shown,
		options,
		option_labels,
	}
}

fn set_bool(d: &mut Dialog, id: &str, on: bool) {
	d.set(id, &Value::Bool(on));
}

fn set_number(d: &mut Dialog, id: &str, v: f64) {
	d.set(id, &Value::Float(v));
}

fn set_slider(d: &mut Dialog, id: &str, t: f64) {
	d.set_slider(id, t);
}

fn set_text(d: &mut Dialog, id: &str, text: &str) -> bool {
	d.set_text(id, text)
}

fn reset_row(d: &mut Dialog, id: &str) {
	d.reset(id);
}

fn apply(d: &mut Dialog) -> String {
	d.apply().err().unwrap_or_default()
}

fn is_dirty(d: &Dialog) -> bool {
	d.dirty()
}

fn theme_buttons(d: &Dialog) -> ffi::ThemeButtons {
	let b = d.theme_buttons();
	ffi::ThemeButtons {
		save: b.save,
		rename: b.rename,
		remove: b.delete,
	}
}

fn theme_name(d: &Dialog) -> String {
	d.theme_name()
}

fn save_theme(d: &mut Dialog) {
	d.save_theme();
}

fn save_theme_as(d: &mut Dialog, name: &str) -> String {
	d.save_theme_as(name).err().unwrap_or_default()
}

fn rename_theme(d: &mut Dialog, name: &str) -> String {
	d.rename_theme(name).err().unwrap_or_default()
}

fn delete_theme(d: &mut Dialog) {
	d.delete_theme();
}

fn randomize_theme(d: &mut Dialog) {
	d.randomize();
}

fn pick_open(d: &mut Dialog, id: &str) -> bool {
	d.pick_open(id)
}

fn pick_state(d: &Dialog) -> ffi::PickState {
	let hsv = d.pick_hsv().unwrap_or_default();
	ffi::PickState {
		h: hsv.h,
		s: hsv.s,
		v: hsv.v,
		hex: crate::pick::hex(crate::pick::to_rgb(hsv)),
		labels: Field::ALL.iter().map(|f| f.label().to_string()).collect(),
		percents: Field::ALL.iter().filter_map(|f| f.percent(hsv)).collect(),
	}
}

fn pick_set(d: &mut Dialog, h: f64, s: f64, v: f64) {
	d.pick_set(Hsv { h, s, v });
}

fn pick_type(d: &mut Dialog, field: usize, text: &str) -> bool {
	Field::ALL.get(field).is_some_and(|&f| d.pick_type(f, text))
}

fn pick_close(d: &mut Dialog, keep: bool) {
	d.pick_close(keep);
}

fn graph_rows(d: &Dialog) -> Vec<ffi::GraphRow> {
	d.graph_rows()
		.into_iter()
		.map(|r| ffi::GraphRow {
			name: r.name,
			show: r.show,
			width: r.width,
			sample: r.sample,
			time: r.time,
		})
		.collect()
}

fn move_graph(d: &mut Dialog, from: usize, to: usize) -> bool {
	d.move_graph(from, to)
}

fn section_order(d: &Dialog) -> Vec<String> {
	d.section_order()
}

fn reset_history(d: &Dialog) -> String {
	d.reset_history().err().unwrap_or_default()
}

fn reset_deletes_saved(d: &Dialog) -> bool {
	d.reset_deletes_saved()
}

fn live_clear(l: &mut Live) {
	l.clear();
}

fn live_new(d: &Dialog) -> Box<Live> {
	Box::new(Live::new(d.look()))
}

fn live_look(l: &mut Live, d: &Dialog) {
	l.set_look(d.look());
}

fn live_sample(l: &mut Live) -> u32 {
	l.tick()
}

fn live_width(l: &Live, name: &str) -> u32 {
	l.width(name)
}

fn live_draw<'a>(l: &'a mut Live, name: &str, w: usize, h: usize, scale: usize) -> &'a [u32] {
	l.render(name, w, h, scale)
}

fn live_tip(l: &Live, name: &str, w: usize, h: usize, scale: usize) -> String {
	l.hover_tip(name, w, h, scale)
}

fn live_hover(l: &mut Live, name: &str, x: usize, w: usize) -> bool {
	l.hover(name, Some(x), w)
}

fn live_leave(l: &mut Live) -> bool {
	l.hover("", None, 0)
}

/// GTK's own tooltips wrap at 70 characters, so the panel's and these match.
pub const TIP_CHARS: usize = 70;

fn wrap_tip(text: &str) -> String {
	wrapped(text, TIP_CHARS)
}

/// Breaks each line of `text` longer than `width` characters between words.
/// Shorter lines, and the breaks already there, are kept as they are, so a
/// wrapped text wraps to itself. A word longer than `width` gets a line of
/// its own.
pub fn wrapped(text: &str, width: usize) -> String {
	let mut out = String::with_capacity(text.len() + text.len() / width.max(1));
	for (i, line) in text.split('\n').enumerate() {
		if i > 0 {
			out.push('\n');
		}
		if line.chars().count() <= width {
			out.push_str(line);
			continue;
		}
		let mut used = 0;
		for word in line.split_whitespace() {
			let n = word.chars().count();
			if used > 0 && used + 1 + n > width {
				out.push('\n');
				used = 0;
			} else if used > 0 {
				out.push(' ');
				used += 1;
			}
			out.push_str(word);
			used += n;
		}
	}
	out
}

fn live_easing(l: &Live, name: &str) -> bool {
	l.easing(name)
}

pub fn run(d: &mut Dialog, snapshot: &str) -> i32 {
	ffi::run_dialog(d, snapshot)
}

pub fn say_unreadable(text: &str) {
	ffi::say_unreadable(text);
}

#[cfg(test)]
mod tests {
	use super::*;
	use tukzmon_core::config::THEME;

	// One QApplication at a time, or 2 tests' windows crash each other.
	static QT: std::sync::Mutex<()> = std::sync::Mutex::new(());

	fn qt_turn() -> std::sync::MutexGuard<'static, ()> {
		QT.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
	}

	// Test ID: defw3
	#[test]
	fn every_tip_wraps_at_a_readable_width_and_keeps_its_words() {
		let short = "User 30%  System 10%\nNice 0%";
		assert_eq!(wrapped(short, 70), short);
		assert_eq!(wrapped("aa bb cc", 5), "aa bb\ncc");
		assert_eq!(wrapped("a\n\nbb cc dd", 5), "a\n\nbb cc\ndd");
		assert_eq!(wrapped("abcdefgh ij", 5), "abcdefgh\nij");
		assert_eq!(wrapped("", 5), "");
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_tips_never_written.shcl",
			std::process::id()
		));
		let d = Dialog::open(path).unwrap();
		let mut long = 0;
		for r in dialog_rows(&d) {
			let tip = row_state(&d, &r.id).tip;
			let w = wrap_tip(&tip);
			long += usize::from(tip.lines().any(|l| l.chars().count() > TIP_CHARS));
			assert!(
				w.lines()
					.all(|l| l.chars().count() <= TIP_CHARS || !l.contains(' ')),
				"{}: {w}",
				r.id
			);
			let words = |t: &str| t.split_whitespace().map(str::to_owned).collect::<Vec<_>>();
			assert_eq!(words(&w), words(&tip), "{}", r.id);
			assert_eq!(
				w.matches("\n\n").count(),
				tip.matches("\n\n").count(),
				"{}",
				r.id
			);
			assert_eq!(wrap_tip(&w), w, "{}", r.id);
		}
		assert!(long > 10, "only {long} tips needed wrapping");
	}

	// Test ID: dendh
	#[test]
	fn rows_send_their_control_as_a_kind_and_the_picker_its_percents() {
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_kinds_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path).unwrap();
		let rows = dialog_rows(&d);
		let kind = |id: &str| rows.iter().find(|r| r.id == id).unwrap().control;
		assert_eq!(kind("cpu_mode"), ffi::ControlKind::Radio);
		assert_eq!(kind("back_bottom"), ffi::ControlKind::Color);
		assert_eq!(kind("back_all"), ffi::ControlKind::Checkbox);
		assert_eq!(kind("border"), ffi::ControlKind::Slider);
		assert_eq!(kind("disk_drives"), ffi::ControlKind::Text);
		for k in [
			ffi::ControlKind::Heading,
			ffi::ControlKind::Number,
			ffi::ControlKind::Dropdown,
		] {
			assert!(rows.iter().any(|r| r.control == k), "{k:?}");
		}
		assert!(d.pick_open("back_bottom"));
		let p = pick_state(&d);
		let hsv = d.pick_hsv().unwrap();
		assert_eq!(p.labels.len(), Field::ALL.len());
		assert_eq!(
			p.percents.len(),
			Field::ALL.len() - 1,
			"all but the hex box"
		);
		for (f, n) in Field::ALL.iter().zip(&p.percents) {
			assert_eq!(f.percent(hsv), Some(*n), "{f:?}");
		}
		assert_eq!(
			p.labels.last().map(String::as_str),
			Some(Field::Hex.label())
		);
		assert_eq!(crate::pick::hex(crate::pick::to_rgb(hsv)), p.hex);
		pick_close(&mut d, false);
	}

	// Test ID: defm1
	#[test]
	fn the_graph_list_sits_under_graphs_and_takes_its_rows() {
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_list_never_written.shcl",
			std::process::id()
		));
		let d = Dialog::open(path).unwrap();
		let rows = dialog_rows(&d);
		let listed: Vec<&str> = rows
			.iter()
			.filter(|r| r.in_list)
			.map(|r| r.id.as_str())
			.collect();
		let mut want: Vec<String> = graph_rows(&d)
			.into_iter()
			.flat_map(|g| [g.show, g.width, g.sample, g.time])
			.collect();
		want.push(crate::dialog::ORDER.to_string());
		assert_eq!(listed, want);
		// qt.cpp puts the list where its first row would be
		let first = rows.iter().position(|r| r.in_list).unwrap();
		assert_eq!(rows[first - 1].id, "graphshead");
		assert!(
			rows.iter()
				.filter(|r| r.in_list)
				.all(|r| r.tab == rows[first].tab)
		);
	}

	// Test ID: defmk
	#[test]
	#[cfg_attr(miri, ignore = "it builds the Qt window, and Miri can't call C++")]
	fn the_longest_graph_sets_the_column_so_the_rest_line_up() {
		let _qt = qt_turn();
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_places_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path).unwrap();
		// at 100 px, a width in percent is the same number
		d.set("width", &Value::Float(100.0));
		d.set("cpu_width", &Value::Float(40.0));
		d.set("mem_width", &Value::Float(100.0));
		let rows = graph_rows(&d);
		let places = ffi::graph_list_places(&mut d);
		assert_eq!(places.len(), rows.len() * 3);
		let at = |i: usize| &places[i * 3..i * 3 + 3];
		let width_of = |name: &str| at(rows.iter().position(|r| r.name == name).unwrap())[0];
		assert_eq!(width_of("cpu"), 40);
		if tukzmon_core::Graph::memory(tukzmon_core::config::Look::default()).is_some() {
			assert_eq!(width_of("mem"), 100);
		}
		let widest = (0..rows.len()).map(|i| at(i)[0]).max().unwrap();
		for (i, row) in rows.iter().enumerate() {
			assert_eq!(at(i)[1], at(0)[1], "{}", row.name);
			assert_eq!(at(i)[2], at(0)[2], "{}", row.name);
			assert!(at(i)[2] >= at(i)[1] + widest, "{places:?}");
		}
		// past the window's room it's squeezed, not cut
		d.set("cpu_width", &Value::Float(1000.0));
		let places = ffi::graph_list_places(&mut d);
		let cpu = rows.iter().position(|r| r.name == "cpu").unwrap();
		assert_eq!(places[cpu * 3], 512);
	}

	// Test ID: defma
	#[test]
	#[cfg_attr(miri, ignore = "it builds the Qt window, and Miri can't call C++")]
	fn a_row_is_greyed_while_its_switch_is_off() {
		let _qt = qt_turn();
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_greyed_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path).unwrap();
		let shown: Vec<String> = dialog_rows(&d)
			.into_iter()
			.filter(|r| r.control != ffi::ControlKind::Heading)
			.map(|r| r.id)
			.collect();
		let gates: Vec<String> = d
			.spec()
			.settings
			.iter()
			.filter_map(|s| s.gate.clone())
			.collect();
		let want = |d: &Dialog| -> Vec<String> {
			shown
				.iter()
				.filter(|id| !d.state(id).counts)
				.cloned()
				.collect()
		};
		// Default has the disk graph and both overrides on, and network off
		assert_eq!(want(&d), ["net_split"]);
		assert_eq!(ffi::greyed_rows(&mut d), want(&d));
		for g in &gates {
			d.set(g, &Value::Bool(false));
		}
		let off = want(&d);
		assert!(off.iter().any(|o| o == "max_days"));
		// a hidden graph's width and time axis can still be set
		for id in ["spikes", "cpu_width", "mem_time"] {
			assert!(!off.iter().any(|o| o == id), "{id}");
		}
		assert_eq!(ffi::greyed_rows(&mut d), off);
	}

	// Test ID: dee9q
	#[test]
	fn one_theme_on_top_recolors_every_sub_tab() {
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path).unwrap();
		let tabs = dialog_tabs(&d);
		let rows = dialog_rows(&d);
		let choosers: Vec<&ffi::RowInfo> = rows.iter().filter(|r| r.chooser).collect();
		assert_eq!(choosers.len(), 1);
		assert_eq!(choosers[0].id, "theme");
		// qt.cpp keeps a tab's own rows, the buttons with them, over its sub-tabs
		let top = choosers[0].tab;
		assert_eq!(tabs[top as usize].parent, -1);
		let subs: Vec<i32> = (0..tabs.len())
			.filter(|&i| tabs[i].parent == top)
			.map(|i| i as i32)
			.collect();
		let names: Vec<&str> = subs
			.iter()
			.map(|&t| tabs[t as usize].label.as_str())
			.collect();
		for want in ["Colors", "Outline"] {
			assert!(names.contains(&want), "{names:?}");
		}
		let tab_of = |id: &str| rows.iter().find(|r| r.id == id).unwrap().tab;
		// owned, since the picks below borrow d mutably
		let group = d.spec().group(THEME).unwrap().clone();
		for r in rows
			.iter()
			.filter(|r| subs.contains(&r.tab) && r.control == ffi::ControlKind::Color)
		{
			assert!(group.members.contains(&r.id), "{}", r.id);
		}
		for id in &group.members {
			assert!(subs.contains(&tab_of(id)), "{id}");
		}
		let before: Vec<String> = group.members.iter().map(|id| d.state(id).text).collect();
		let mut moved: Vec<i32> = Vec::new();
		for p in &group.presets {
			assert!(d.set_text("theme", &p.key), "{}", p.key);
			for (id, was) in group.members.iter().zip(&before) {
				let want = p.values.get(id);
				let want = want.unwrap_or_else(|| panic!("{} has no {id}", p.key));
				let now = d.state(id).text;
				assert_eq!(
					now,
					tukzmon_core::config::theme_text(want),
					"{} {id}",
					p.key
				);
				if now != *was {
					moved.push(tab_of(id));
				}
			}
		}
		for &t in &subs {
			assert!(moved.contains(&t), "{}", tabs[t as usize].label);
		}
	}

	// Test ID: defwx
	#[test]
	#[cfg_attr(miri, ignore = "it builds the Qt window, and Miri can't call C++")]
	fn sub_tabs_sit_in_the_tab_list_under_their_parent() {
		let _qt = qt_turn();
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_tabs_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path).unwrap();
		let tabs = dialog_tabs(&d);
		let rows = dialog_rows(&d);
		let shown = ffi::tabs_shown(&mut d);
		let lines: Vec<&str> = shown.iter().map(|t| t.label.trim_start()).collect();
		let want: Vec<&str> = tabs.iter().map(|t| t.label.as_str()).collect();
		assert_eq!(lines, want);
		let indent = |i: usize| shown[i].label.len() - shown[i].label.trim_start().len();
		let top = rows.iter().find(|r| r.chooser).unwrap().tab;
		let subs: Vec<usize> = (0..tabs.len()).filter(|&i| tabs[i].parent == top).collect();
		assert!(subs.len() > 1, "{want:?}");
		// Themes on its own shows its first tab
		assert_eq!(shown[top as usize].rows, shown[subs[0]].rows);
		for &s in &subs {
			assert!(
				s > top as usize && indent(s) > indent(top as usize),
				"{}",
				shown[s].label
			);
			let on = &shown[s].rows;
			// the theme's own row stays on top of every sub-tab
			assert!(on.iter().any(|id| id == "theme"), "{}", tabs[s].label);
			let tab_of = |id: &str| rows.iter().find(|r| r.id == id).unwrap().tab;
			assert!(
				on.iter().any(|id| tab_of(id) == s as i32),
				"{}",
				tabs[s].label
			);
			for id in on {
				let t = tab_of(id) as usize;
				assert!(t == s || t == top as usize, "{} shows {id}", tabs[s].label);
			}
		}
	}

	// Test ID: dejmn
	#[test]
	#[cfg_attr(miri, ignore = "it builds the Qt window, and Miri can't call C++")]
	fn themes_and_the_sub_tab_it_shows_are_lit_together() {
		let _qt = qt_turn();
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_tabs_lit_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path).unwrap();
		let tabs = dialog_tabs(&d);
		let shown = ffi::tabs_shown(&mut d);
		let at = |name: &str| tabs.iter().position(|t| t.label == name).unwrap();
		let dim = |name: &str| shown[at(name)].dim.clone();
		// Themes picked lights the first sub-tab, which it shows, dimmer
		assert_eq!(dim("Themes"), ["Colors"]);
		// a sub-tab picked keeps Themes lit
		assert_eq!(dim("Colors"), ["Themes"]);
		assert_eq!(dim("Outline"), ["Themes"]);
		// a tab with none under it lights nothing else
		for tab in ["General", "Graphs"] {
			assert!(dim(tab).is_empty(), "{tab}: {:?}", dim(tab));
		}
	}

	// Test ID: dekn9
	#[test]
	#[cfg_attr(miri, ignore = "it builds the Qt window, and Miri can't call C++")]
	fn the_graph_list_is_a_tab_of_its_own() {
		let _qt = qt_turn();
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_graphs_tab_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path).unwrap();
		let tabs = dialog_tabs(&d);
		let rows = dialog_rows(&d);
		let shown = ffi::tabs_shown(&mut d);
		let on = |name: &str| {
			let at = tabs.iter().position(|t| t.label == name).unwrap();
			shown[at].rows.clone()
		};
		let listed: Vec<String> = rows
			.iter()
			.filter(|r| r.in_list && r.id != "graph_order")
			.map(|r| r.id.clone())
			.collect();
		assert!(listed.iter().any(|id| id == "cpu_width"), "{listed:?}");
		assert_eq!(on("Graphs"), listed);
		for (t, tab) in shown.iter().enumerate() {
			if tabs[t].label != "Graphs" {
				for id in &tab.rows {
					assert!(!listed.contains(id), "{} shows {id}", tabs[t].label);
				}
			}
		}
		// what every graph shares stays on General
		for id in [
			"width",
			"time_axis",
			"sample_ms",
			"border",
			"disk_drives",
			"keep_history",
		] {
			assert!(on("General").iter().any(|r| r == id), "{id}");
		}
	}

	// Test ID: dekna
	#[test]
	#[cfg_attr(miri, ignore = "it builds the Qt window, and Miri can't call C++")]
	fn the_part_that_scrolls_is_a_shade_off_the_window() {
		let _qt = qt_turn();
		// WCAG's relative luminance, from "#rrggbb"
		let lum = |hex: &str| {
			let v = u32::from_str_radix(&hex[1..], 16).unwrap();
			[16, 8, 0]
				.iter()
				.zip([0.2126, 0.7152, 0.0722])
				.map(|(shift, k)| {
					let c = f64::from((v >> shift) & 0xff) / 255.0;
					let lin = if c <= 0.04045 {
						c / 12.92
					} else {
						((c + 0.055) / 1.055).powf(2.4)
					};
					k * lin
				})
				.sum::<f64>()
		};
		for dark in [false, true] {
			let path = std::env::temp_dir().join(format!(
				"test_tukzmon-settings_{}_shades_never_written.shcl",
				std::process::id()
			));
			let mut d = Dialog::open(path).unwrap();
			let got = ffi::page_shades(&mut d, dark);
			let window = got[0].as_str();
			assert_eq!(got.len(), dialog_tabs(&d).len() + 1, "{got:?}");
			for line in &got[1..] {
				let v: Vec<&str> = line.split(' ').collect();
				let (scroll, rest) = (v[v.len() - 2], v[v.len() - 1]);
				// the tab list, the theme's Name row and the buttons keep the window's
				assert_eq!(rest, window, "dark: {dark}, {line}");
				let (a, b) = (lum(scroll), lum(rest));
				let ratio = (a.max(b) + 0.05) / (a.min(b) + 0.05);
				assert!(
					(1.05..1.3).contains(&ratio),
					"dark: {dark}, {line}, contrast {ratio:.3}"
				);
				// toward the text, so darker in a light theme
				assert_eq!(a > b, dark, "{line}");
			}
		}
	}

	// Test ID: dejad
	#[test]
	#[cfg_attr(miri, ignore = "it builds the Qt window, and Miri can't call C++")]
	fn a_moved_graph_moves_its_section_and_the_picked_tab_stays_picked() {
		let _qt = qt_turn();
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_tab_move_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path).unwrap();
		let tabs = dialog_tabs(&d);
		let last = graph_rows(&d).len() - 1;
		let shown = ffi::tabs_after_move(&mut d, "Colors", last, 0);
		assert_eq!(shown[0].label.trim_start(), "Colors");
		let lines: Vec<&str> = shown[1..].iter().map(|t| t.label.trim_start()).collect();
		let want: Vec<&str> = tabs.iter().map(|t| t.label.as_str()).collect();
		assert_eq!(lines, want);
		let at = |name: &str| lines.iter().position(|l| *l == name).unwrap();
		assert_eq!(shown[0].rows, shown[1 + at("Colors")].rows);
		// Themes on its own shows the first tab under it
		assert_eq!(shown[1 + at("Themes")].rows, shown[1 + at("Colors")].rows);
	}

	// Test ID: dejmb
	#[test]
	#[cfg_attr(miri, ignore = "it builds the Qt window, and Miri can't call C++")]
	fn the_sections_follow_a_move_with_the_background_on_the_right() {
		let _qt = qt_turn();
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_sections_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path).unwrap();
		let last = graph_rows(&d).len() - 1;
		let placed = ffi::sections_after_move(&mut d, last, 0);
		let names: Vec<&str> = placed
			.iter()
			.map(|p| p.split(' ').next().unwrap())
			.collect();
		let want = d.section_order();
		assert_eq!(names, want);
		assert_eq!(names[0], "battery");
		for p in &placed {
			let v: Vec<i64> = p.split(' ').skip(1).map(|n| n.parse().unwrap()).collect();
			// what's beside starts right of where the main colors end
			assert!(v[0] > 0 && v[1] > v[0], "{p}");
		}
	}

	// Test ID: defwy
	#[test]
	#[cfg_attr(miri, ignore = "it builds the Qt window, and Miri can't call C++")]
	fn the_buttons_are_cancel_apply_and_ok() {
		let _qt = qt_turn();
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_buttons_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path).unwrap();
		assert_eq!(
			ffi::bottom_buttons(&mut d),
			["Reset history", "Cancel", "&Apply", "OK"]
		);
	}

	// Test ID: dej8s
	#[test]
	#[cfg_attr(miri, ignore = "it builds the Qt window, and Miri can't call C++")]
	fn apply_turns_on_while_a_box_is_typed_in() {
		let _qt = qt_turn();
		for number in [false, true] {
			let path = std::env::temp_dir().join(format!(
				"test_tukzmon-settings_{}_typing_never_written.shcl",
				std::process::id()
			));
			let mut d = Dialog::open(path).unwrap();
			assert_eq!(
				ffi::apply_while_typing(&mut d, number),
				[false, true, true],
				"number box: {number}"
			);
		}
	}

	// Test ID: dej8t
	#[test]
	#[cfg_attr(miri, ignore = "it builds the Qt window, and Miri can't call C++")]
	fn an_empty_checkbox_stands_out_in_a_light_or_dark_theme() {
		let _qt = qt_turn();
		// WCAG's least contrast for the edge of a control
		for dark in [false, true] {
			let c = ffi::empty_box_contrast(dark);
			assert!(c >= 3.0, "dark: {dark}, contrast {c:.2}");
		}
	}

	// Test ID: denaq
	#[test]
	#[cfg_attr(miri, ignore = "it builds the Qt window, and Miri can't call C++")]
	fn a_refresh_restyles_only_the_chips_whose_color_changed() {
		let _qt = qt_turn();
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_chips_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path).unwrap();
		assert_eq!(ffi::chip_restyles(&mut d, "#123456"), [0, 1]);
	}

	fn median_of(mut v: Vec<f64>) -> f64 {
		v.sort_by(f64::total_cmp);
		v[v.len() / 2]
	}

	// Test ID: denam
	#[test]
	#[ignore = "a timing, run by hand with --ignored --nocapture"]
	#[cfg_attr(miri, ignore = "it builds the Qt window, and Miri can't call C++")]
	fn time_a_refresh_and_a_slider_step() {
		let _qt = qt_turn();
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_timing_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path).unwrap();
		let times = 50;
		let t = ffi::refresh_micros(&mut d, times);
		let rows = dialog_rows(&d);
		let start = std::time::Instant::now();
		for _ in 0..times {
			for r in &rows {
				std::hint::black_box(row_state(&d, &r.id));
			}
		}
		let states = start.elapsed().as_secs_f64() * 1e6 / times as f64;
		println!(
			"refresh {:.0} us, slider step {:.0} us, row_state for {} rows {states:.0} us (medians of {times})",
			median_of(t[..times].to_vec()),
			median_of(t[times..].to_vec()),
			rows.len()
		);
	}

	// x, y, width and height
	type Rect = [i32; 4];

	// Opens the window on one offscreen screen of that size, with a frame
	// round the window or none, and gives the screen less its panels, the
	// window with its frame, and each tab's label, view and content.
	fn fit_on(w: u32, h: u32, frame: bool) -> (Rect, Rect, Vec<(String, i32, i32)>) {
		let dir = std::env::temp_dir();
		let screens = dir.join(format!(
			"test_tukzmon-settings_{}_fit_{w}x{h}.json",
			std::process::id()
		));
		std::fs::write(
			&screens,
			format!(
				r#"{{"windowFrameMargins": {frame}, "screens": [{{"name": "fit", "x": 0, "y": 0, "width": {w}, "height": {h}, "logicalDpi": 96, "logicalBaseDpi": 96, "dpr": 1}}]}}"#
			),
		)
		.unwrap();
		let mut d = Dialog::open(dir.join(format!(
			"test_tukzmon-settings_{}_fit_never_written.shcl",
			std::process::id()
		)))
		.unwrap();
		let got = ffi::window_fit(&mut d, screens.to_str().unwrap());
		std::fs::remove_file(&screens).unwrap();
		let rect = |line: &str, what: &str| -> Rect {
			let v: Vec<&str> = line.split(' ').collect();
			assert_eq!(v[0], what, "{got:?}");
			std::array::from_fn(|i| v[i + 1].parse().unwrap())
		};
		let tabs = got[2..]
			.iter()
			.map(|line| {
				let v: Vec<&str> = line.rsplitn(3, ' ').collect();
				(
					v[2].to_string(),
					v[1].parse().unwrap(),
					v[0].parse().unwrap(),
				)
			})
			.collect();
		(rect(&got[0], "room"), rect(&got[1], "frame"), tabs)
	}

	// Test ID: deknz
	#[test]
	#[cfg_attr(miri, ignore = "it builds the Qt window, and Miri can't call C++")]
	fn the_window_and_its_frame_stay_on_a_short_screen() {
		let _qt = qt_turn();
		for (w, h) in [(1280, 600), (1024, 500)] {
			let (room, frame, _) = fit_on(w, h, true);
			let [rx, ry, rw, rh] = room;
			let [fx, fy, fw, fh] = frame;
			assert!(fx >= rx && fy >= ry, "{w}x{h}: {frame:?} in {room:?}");
			assert!(
				fx + fw <= rx + rw && fy + fh <= ry + rh,
				"{w}x{h}: {frame:?} in {room:?}"
			);
			// and no shorter than it has to be
			assert_eq!(fh, rh, "{w}x{h}: {frame:?} in {room:?}");
		}
	}

	// Test ID: demvm
	#[test]
	#[cfg_attr(miri, ignore = "it builds the Qt window, and Miri can't call C++")]
	fn ctrl_tab_steps_through_every_page_colors_too() {
		let _qt = qt_turn();
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_ctrl_tab_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path).unwrap();
		// Themes only shows Colors, so it's no stop of its own
		let ahead = ["General", "Graphs", "Colors", "Outline", "General"];
		let back = ["General", "Outline", "Colors", "Graphs", "General"];
		for from_page in [false, true] {
			let got = ffi::tabs_by_key(&mut d, false, from_page, false);
			assert_eq!(got, ahead, "from the page: {from_page}");
			let got = ffi::tabs_by_key(&mut d, true, from_page, false);
			assert_eq!(got, back, "from the page: {from_page}");
		}
	}

	// Test ID: den56
	#[test]
	#[cfg_attr(miri, ignore = "it builds the Qt window, and Miri can't call C++")]
	fn ctrl_page_down_and_up_step_like_ctrl_tab() {
		let _qt = qt_turn();
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_ctrl_page_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path).unwrap();
		let ahead = ["General", "Graphs", "Colors", "Outline", "General"];
		let back = ["General", "Outline", "Colors", "Graphs", "General"];
		for from_page in [false, true] {
			let got = ffi::tabs_by_key(&mut d, false, from_page, true);
			assert_eq!(got, ahead, "from the page: {from_page}");
			let got = ffi::tabs_by_key(&mut d, true, from_page, true);
			assert_eq!(got, back, "from the page: {from_page}");
		}
	}

	// Test ID: demvn
	#[test]
	#[cfg_attr(miri, ignore = "it builds the Qt window, and Miri can't call C++")]
	fn the_colors_under_the_mode_start_level_with_background() {
		let _qt = qt_turn();
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_section_tops_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path).unwrap();
		let modes: Vec<String> = dialog_rows(&d)
			.into_iter()
			.filter(|r| !r.section.is_empty() && r.id.ends_with("_mode"))
			.map(|r| r.id)
			.collect();
		assert!(modes.len() > 5, "{modes:?}");
		for mode in ["detailed", "gradient"] {
			for id in &modes {
				assert!(d.set(id, &Value::Text(mode.to_string())), "{id}");
			}
			let tops = ffi::section_tops(&mut d);
			assert_eq!(tops.len(), modes.len(), "{tops:?}");
			for t in &tops {
				let [graph, left, right] = t.split(' ').collect::<Vec<_>>()[..] else {
					panic!("{t}");
				};
				let (left, right): (i32, i32) = (left.parse().unwrap(), right.parse().unwrap());
				// under the mode row, not beside it
				assert!(left > 0, "{mode} {graph}: {tops:?}");
				assert_eq!(left, right, "{mode} {graph}: {tops:?}");
			}
		}
	}

	// Test ID: dekp0
	#[test]
	#[cfg_attr(miri, ignore = "it builds the Qt window, and Miri can't call C++")]
	fn the_colors_tab_scrolls_in_the_height_the_other_tabs_set() {
		let _qt = qt_turn();
		let (room, frame, tabs) = fit_on(1600, 2000, false);
		assert!(frame[3] < room[3] / 2, "{frame:?} in {room:?}");
		let colors = tabs.iter().find(|t| t.0 == "Colors").unwrap();
		assert!(colors.2 > colors.1, "{tabs:?}");
		let others: Vec<_> = tabs.iter().filter(|t| t.0 != "Colors").collect();
		assert!(others.len() >= 3, "{tabs:?}");
		for t in &others {
			assert!(t.1 >= t.2, "{t:?} scrolls: {tabs:?}");
		}
		// the tallest of them has no room to spare
		let slack = others.iter().map(|t| t.1 - t.2).min().unwrap();
		assert!(slack <= 4, "{tabs:?}");
	}
}
