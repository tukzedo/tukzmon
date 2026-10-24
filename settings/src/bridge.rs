// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

// The calls between the Rust model and the Qt window in qt.cpp.

use knobs::{Control, Value};

use crate::dialog::Dialog;
use crate::pick::{Field, Hsv};

#[cxx::bridge(namespace = "tukzmon")]
pub mod ffi {
	struct TabInfo {
		label: String,
		parent: i32,
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
	}

	unsafe extern "C++" {
		include!("tukzmon-settings/src/qt.h");
		fn run_dialog(d: &mut Dialog, snapshot: &str) -> i32;
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

pub fn run(d: &mut Dialog, snapshot: &str) -> i32 {
	ffi::run_dialog(d, snapshot)
}
