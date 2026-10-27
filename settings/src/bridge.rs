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

	/// A line of the window's tab list, as drawn, and the rows it shows.
	struct TabShown {
		label: String,
		rows: Vec<String>,
	}

	struct RowInfo {
		id: String,
		label: String,
		tab: i32,
		control: String,
		whole: bool,
		min: f64,
		max: f64,
		options: Vec<String>,
		option_labels: Vec<String>,
		indent: i32,
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

	/// The open picker's color, and what each value box shows for it.
	struct PickState {
		h: f64,
		s: f64,
		v: f64,
		hex: String,
		labels: Vec<String>,
		fields: Vec<String>,
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

		type Live;
		fn live_new(d: &Dialog) -> Box<Live>;
		fn live_look(l: &mut Live, d: &Dialog);
		/// Reads every graph, and gives the ms to the next read.
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
		#[allow(dead_code, reason = "only the tests ask")]
		fn greyed_rows(d: &mut Dialog) -> Vec<String>;
		/// Each graph list row's graph width and x, and its width box's x.
		#[allow(dead_code, reason = "only the tests ask")]
		fn graph_list_places(d: &mut Dialog) -> Vec<i32>;
		#[allow(dead_code, reason = "only the tests ask")]
		fn tabs_shown(d: &mut Dialog) -> Vec<TabShown>;
		#[allow(dead_code, reason = "only the tests ask")]
		fn bottom_buttons(d: &mut Dialog) -> Vec<String>;
	}
}

fn dialog_title(d: &Dialog) -> String {
	format!("tukzmon settings - {}", d.path().display())
}

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

fn dialog_rows(d: &Dialog) -> Vec<ffi::RowInfo> {
	d.spec()
		.settings
		.iter()
		.filter(|s| s.control != Control::None)
		.map(|s| ffi::RowInfo {
			id: s.id.clone(),
			label: s.label.clone(),
			tab: s.tab as i32,
			control: if s.control == Control::Dropdown
				&& tukzmon_core::config::shown_as_radio(&s.id)
			{
				"radio".into()
			} else {
				format!("{:?}", s.control).to_lowercase()
			},
			whole: s.kind == knobs::Kind::Int,
			min: s.min,
			max: s.max,
			options: s.options.iter().map(|(k, _)| k.clone()).collect(),
			option_labels: s.options.iter().map(|(_, l)| l.clone()).collect(),
			indent: s.indent as i32,
			chooser: d.spec().chooser_of(&s.id).is_some(),
			in_list: d.in_graph_list(&s.id),
		})
		.collect()
}

fn row_state(d: &Dialog, id: &str) -> ffi::RowState {
	let s = d.state(id);
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
		option_labels: s.options.iter().map(|(_, l)| l.clone()).collect(),
		options: s.options.into_iter().map(|(k, _)| k).collect(),
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
		fields: Field::ALL.iter().map(|f| f.text(hsv)).collect(),
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
			time: r.time,
		})
		.collect()
}

fn move_graph(d: &mut Dialog, from: usize, to: usize) -> bool {
	d.move_graph(from, to)
}

fn live_new(d: &Dialog) -> Box<Live> {
	Box::new(Live::new(d.look()))
}

fn live_look(l: &mut Live, d: &Dialog) {
	l.set_look(&d.look());
}

fn live_sample(l: &mut Live) -> u32 {
	l.sample();
	l.sample_ms()
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
/// Shorter lines, adn the breaks already there, are kept as they are, so a
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
		let d = Dialog::open(path);
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

	// Test ID: defm1
	#[test]
	fn the_graph_list_sits_under_graphs_and_takes_its_rows() {
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_list_never_written.shcl",
			std::process::id()
		));
		let d = Dialog::open(path);
		let rows = dialog_rows(&d);
		let listed: Vec<&str> = rows
			.iter()
			.filter(|r| r.in_list)
			.map(|r| r.id.as_str())
			.collect();
		let mut want: Vec<String> = graph_rows(&d)
			.into_iter()
			.flat_map(|g| [g.show, g.width, g.time])
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
		let mut d = Dialog::open(path);
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
		d.set("cpu_width", &Value::Float(4096.0));
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
		let mut d = Dialog::open(path);
		let shown: Vec<String> = dialog_rows(&d)
			.into_iter()
			.filter(|r| !matches!(r.control.as_str(), "heading" | "none"))
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
		assert_eq!(
			want(&d),
			["swap_width", "swap_time", "battery_width", "battery_time"]
		);
		assert_eq!(ffi::greyed_rows(&mut d), want(&d));
		for g in &gates {
			d.set(g, &Value::Bool(false));
		}
		let off = want(&d);
		for id in ["log_curve", "spikes", "max_days", "cpu_width", "mem_time"] {
			assert!(off.iter().any(|o| o == id), "{id}");
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
		let mut d = Dialog::open(path);
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
		for want in ["CPU", "Memory", "Background"] {
			assert!(names.contains(&want), "{names:?}");
		}
		let tab_of = |id: &str| rows.iter().find(|r| r.id == id).unwrap().tab;
		// owned, since the picks below borrow d mutably
		let group = d.spec().group(THEME).unwrap().clone();
		for r in rows
			.iter()
			.filter(|r| subs.contains(&r.tab) && r.control == "color")
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
				assert_eq!(now, want.as_text(), "{} {id}", p.key);
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
		let mut d = Dialog::open(path);
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

	// Test ID: defwy
	#[test]
	#[cfg_attr(miri, ignore = "it builds the Qt window, and Miri can't call C++")]
	fn the_buttons_are_cancel_apply_and_ok() {
		let _qt = qt_turn();
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_buttons_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path);
		assert_eq!(ffi::bottom_buttons(&mut d), ["Cancel", "&Apply", "OK"]);
	}
}
