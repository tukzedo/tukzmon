// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! What the settings window shows and does, with no Qt in it. The window
//! asks this for every row's state after each change.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use knobs::{Control, Kind, Model, NoEnv, Setting, Spec, Value};
use tukzmon_core::config;

const HEADER: &str = "## tukzmon settings. Only values changed from their defaults are listed.\n";

pub struct RowState {
	pub text: String,
	pub on: bool,
	pub number: f64,
	/// Slider position, 0 to 1.
	pub slider: f64,
	pub can_reset: bool,
	pub counts: bool,
	pub tip: String,
	/// What a dropdown shows, with " *" on a theme changed on top of.
	pub choice: String,
}

pub struct Dialog {
	path: PathBuf,
	spec: Arc<Spec>,
	model: Model,
	/// As the file was when it was last read or written.
	saved: Model,
	/// Settings changed in the window, so a save writes only those.
	touched: Vec<String>,
	/// Sliders that reach less far than their number box takes, as copies of
	/// the setting over the slider's own range.
	short_sliders: Vec<Setting>,
}

fn read(path: &Path) -> Result<String, String> {
	match std::fs::read_to_string(path) {
		Ok(t) => Ok(t),
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
		Err(e) => Err(format!("{} can't be read: {e}", path.display())),
	}
}

/// Lines a save would lose: ones that don't parse, and settings nobody here
/// knows about, such as a hand edit with a typo in the name.
fn would_lose(spec: &Spec, text: &str) -> Vec<String> {
	let doc = shcl::Document::parse(text);
	let mut out: Vec<String> = doc
		.diagnostics()
		.iter()
		.filter(|d| d.severity == shcl::Severity::Error)
		.map(|d| format!("line {}: {}", d.line, d.message))
		.collect();
	for p in doc.paths() {
		let known = p == "kept"
			|| p.starts_with("kept.")
			|| spec.settings.iter().any(|s| {
				s.control != Control::Heading
					&& (s.path == p
						|| s.path
							.strip_prefix(p.as_str())
							.is_some_and(|r| r.starts_with('.')))
			});
		if !known {
			out.push(format!(
				"line {}: {p} isn't a tukzmon setting",
				doc.line(&p)
			));
		}
	}
	out
}

impl Dialog {
	pub fn open(path: PathBuf) -> Dialog {
		let spec = config::spec();
		let text = read(&path).unwrap_or_default();
		let model = config::model(&text);
		let short_sliders = spec
			.settings
			.iter()
			.filter_map(|s| {
				let (min, max) = config::slider_range(&s.id)?;
				Some(Setting {
					min,
					max,
					..s.clone()
				})
			})
			.collect();
		Dialog {
			path,
			spec,
			saved: model.clone(),
			model,
			touched: Vec::new(),
			short_sliders,
		}
	}

	/// The setting as its slider sees it.
	pub fn slider(&self, id: &str) -> Option<&Setting> {
		self.short_sliders
			.iter()
			.find(|s| s.id == id)
			.or_else(|| self.spec.get(id))
	}

	pub fn path(&self) -> &Path {
		&self.path
	}

	pub fn spec(&self) -> &Spec {
		&self.spec
	}

	pub fn dirty(&self) -> bool {
		self.model != self.saved
	}

	pub fn state(&self, id: &str) -> RowState {
		let v = self.model.value(id, &NoEnv);
		let number = v.as_f64();
		let mut tip = self.model.tip(id, &NoEnv);
		if let (Some(short), Some(s)) = (
			self.short_sliders.iter().find(|s| s.id == id),
			self.spec.get(id),
		) {
			if !tip.is_empty() {
				tip.push_str("\n\n");
			}
			let _ = write!(
				tip,
				"The slider goes from {} to {}. Type in the box for anything from {} to {}.",
				short.min, short.max, s.min, s.max
			);
		}
		RowState {
			text: match &v {
				Value::Text(t) => t.clone(),
				other => other.show(),
			},
			on: v.as_bool(),
			number,
			slider: self.slider(id).map_or(0.0, |s| s.to_t(number)),
			can_reset: self.model.can_reset(id, &NoEnv),
			counts: self.model.counts(id, &NoEnv),
			tip,
			choice: match self.spec.get(id) {
				Some(s) if s.control == Control::Dropdown => self.model.shown_choice(id, &NoEnv),
				_ => String::new(),
			},
		}
	}

	fn touch(&mut self, id: &str) {
		if !self.touched.iter().any(|t| t == id) {
			self.touched.push(id.to_string());
		}
	}

	/// False when the value doesn't fit the setting, and nothing changes.
	pub fn set(&mut self, id: &str, v: &Value) -> bool {
		let Some(s) = self.spec.get(id) else {
			return false;
		};
		if self.spec.valid(s, v).is_err() {
			return false;
		}
		self.model.set(id, v, &NoEnv);
		self.touch(id);
		true
	}

	/// Typed text, read the way the setting takes it.
	pub fn set_text(&mut self, id: &str, text: &str) -> bool {
		let Some(s) = self.spec.get(id) else {
			return false;
		};
		let t = text.trim();
		let v = match s.kind {
			Kind::Bool => match t {
				"true" | "on" => Value::Bool(true),
				"false" | "off" => Value::Bool(false),
				_ => return false,
			},
			Kind::Int | Kind::Float => match t.parse::<f64>() {
				Ok(n) if n.is_finite() => Value::Float(n),
				_ => return false,
			},
			Kind::Color if !t.starts_with('#') => Value::Text(format!("#{t}")),
			_ => Value::Text(t.to_string()),
		};
		self.set(id, &v)
	}

	/// A slider dragged to `t`, 0 to 1, pulled onto a nearby detent.
	pub fn set_slider(&mut self, id: &str, t: f64) {
		let Some(s) = self.slider(id) else {
			return;
		};
		let v = s.from_t(s.snap_t(t, 0.015));
		self.set(id, &Value::Float(v));
	}

	pub fn reset(&mut self, id: &str) {
		if self.spec.get(id).is_some() {
			self.model.reset(id, &NoEnv);
			self.touch(id);
		}
	}

	/// Save what was changed here onto the file as it is now, so a hand edit
	/// made while the window was open isn't lost.
	///
	/// # Errors
	///
	/// Why nothing was written.
	pub fn apply(&mut self) -> Result<(), String> {
		let text = read(&self.path)?;
		let lose = would_lose(&self.spec, &text);
		if !lose.is_empty() {
			return Err(format!(
				"{} has lines a save would drop, so it wasn't changed. Fix or remove them first.\n\n{}",
				self.path.display(),
				lose.join("\n")
			));
		}
		let mut fresh = config::model(&text);
		for id in &self.touched {
			if self.model.can_reset(id, &NoEnv) {
				fresh.set(id, &self.model.value(id, &NoEnv), &NoEnv);
			} else {
				fresh.reset(id, &NoEnv);
			}
		}
		config::name_the_theme(&mut fresh);
		let (body, _) = fresh.save(&NoEnv);
		let Some(file) = self.path.to_str() else {
			return Err(format!(
				"{} isn't a path shcl can write",
				self.path.display()
			));
		};
		if let Some(dir) = self.path.parent() {
			std::fs::create_dir_all(dir)
				.map_err(|e| format!("{} can't be made: {e}", dir.display()))?;
		}
		shcl::write_file_atomic(file, &format!("{HEADER}{body}"))?;
		self.saved = fresh.clone();
		self.model = fresh;
		self.touched.clear();
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	struct Dir(PathBuf);
	impl Drop for Dir {
		fn drop(&mut self) {
			let _ = std::fs::remove_file(self.0.join("t.shcl"));
			let _ = std::fs::remove_dir(&self.0);
		}
	}

	fn scratch(name: &str) -> Dir {
		let base = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_{name}",
			std::process::id()
		));
		std::fs::create_dir_all(&base).unwrap();
		Dir(base)
	}

	#[test]
	fn opens_on_defaults_with_no_file() {
		let d = scratch("defaults");
		let dlg = Dialog::open(d.0.join("t.shcl"));
		assert!(!dlg.dirty());
		let s = dlg.state("user");
		assert_eq!(s.text, "#00ff80");
		assert!(!s.can_reset);
		assert_eq!(dlg.state("width").number, 128.0);
	}

	#[test]
	fn apply_writes_only_what_changed() {
		let d = scratch("apply");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone());
		assert!(dlg.set_text("user", "123abc"));
		assert!(dlg.dirty());
		dlg.apply().unwrap();
		assert!(!dlg.dirty());
		let text = std::fs::read_to_string(&path).unwrap();
		assert!(text.starts_with("##"));
		assert!(text.contains("#123abc"));
		assert!(!text.contains("width"));
		assert_eq!(
			config::Look::load(&path).cpu[2],
			config::Rgb(0x12, 0x3a, 0xbc)
		);
	}

	#[test]
	fn a_hand_edit_made_while_open_survives() {
		let d = scratch("hand");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone());
		std::fs::write(&path, "graph:\n\twidth: 300\n").unwrap();
		assert!(dlg.set_text("border", "2"));
		dlg.apply().unwrap();
		let l = config::Look::load(&path);
		assert_eq!((l.width, l.border), (300, 2));
		// and a reset in the window takes the line back out
		dlg.reset("border");
		dlg.apply().unwrap();
		assert_eq!(config::Look::load(&path).border, 1);
	}

	#[test]
	fn refuses_to_drop_lines_it_cant_place() {
		let d = scratch("lose");
		let path = d.0.join("t.shcl");
		std::fs::write(&path, "graph:\n\twidht: 300\n").unwrap();
		let mut dlg = Dialog::open(path.clone());
		dlg.set_text("border", "2");
		let e = dlg.apply().unwrap_err();
		assert!(e.contains("graph.widht"), "{e}");
		assert_eq!(
			std::fs::read_to_string(&path).unwrap(),
			"graph:\n\twidht: 300\n"
		);
	}

	#[test]
	fn bad_input_changes_nothing() {
		let d = scratch("bad");
		let mut dlg = Dialog::open(d.0.join("t.shcl"));
		assert!(!dlg.set_text("width", "huge"));
		assert!(!dlg.set_text("width", "5000"));
		assert!(!dlg.set_text("user", "#12"));
		assert!(!dlg.set_text("nope", "1"));
		assert!(!dlg.dirty());
	}

	#[test]
	fn the_box_takes_more_than_the_slider_reaches() {
		let d = scratch("reach");
		let mut dlg = Dialog::open(d.0.join("t.shcl"));
		assert!(dlg.set_text("width", "2000"));
		let s = dlg.state("width");
		assert_eq!(s.number, 2000.0);
		assert_eq!(s.slider, 1.0);
		assert!(s.tip.contains("16 to 4096"), "{}", s.tip);
		dlg.set_slider("width", 1.0);
		assert_eq!(dlg.state("width").number, 1024.0);
		dlg.set_slider("width", 0.0);
		assert_eq!(dlg.state("width").number, 24.0);
		assert!(dlg.set_text("width", "16"));
		assert!(dlg.set_text("sample_ms", "30000"));
		assert!(dlg.set_text("border", "10"));
		assert!(!dlg.set_text("border", "17"));
		assert!(!dlg.set_text("spikes", "101"));
		assert!(!dlg.state("spikes").tip.contains("The slider goes"));
	}

	#[test]
	fn sliders_snap_to_detents_and_gates_turn_on() {
		let d = scratch("slider");
		let mut dlg = Dialog::open(d.0.join("t.shcl"));
		let s = dlg.slider("width").unwrap().clone();
		dlg.set_slider("width", s.to_t(130.0));
		assert_eq!(dlg.state("width").number, 128.0);
		assert!(dlg.set_text("log_axis", "off"));
		assert!(!dlg.state("spikes").counts);
		// changing a row under a switch turns the switch back on
		assert!(dlg.set_text("spikes", "20"));
		assert!(dlg.state("log_axis").on);
	}

	#[test]
	fn a_theme_and_the_changes_on_it_are_saved() {
		let d = scratch("theme");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone());
		assert_eq!(dlg.state("theme").choice, "Default");
		assert!(dlg.state("width").choice.is_empty());
		assert!(dlg.set_text("theme", "blue_standard"));
		assert_eq!(dlg.state("user").text, "#0080ff");
		assert!(dlg.set_text("user", "#111111"));
		assert_eq!(dlg.state("theme").choice, "Blue standard *");
		dlg.apply().unwrap();
		let dlg = Dialog::open(path.clone());
		assert_eq!(dlg.state("theme").choice, "Blue standard *");
		assert_eq!(dlg.state("user").text, "#111111");
		assert_eq!(dlg.state("nice").text, "#20405f");
	}

	#[test]
	fn custom_then_the_revert_arrow_is_default_again() {
		let d = scratch("custom");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone());
		assert!(dlg.set_text("theme", "custom"));
		assert!(dlg.set_text("user", "#123456"));
		dlg.apply().unwrap();
		assert_eq!(
			config::Look::load(&path).cpu[2],
			config::Rgb(0x12, 0x34, 0x56)
		);
		dlg.reset("theme");
		dlg.apply().unwrap();
		let dlg = Dialog::open(path.clone());
		assert_eq!(dlg.state("theme").choice, "Default");
		assert_eq!(dlg.state("user").text, "#00ff80");
		assert!(!dlg.dirty());
	}

	#[test]
	fn the_log_curve_sits_under_the_log_switch() {
		let d = scratch("curve");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone());
		assert_eq!(dlg.state("log_curve").text, "smooth");
		assert!(dlg.set_text("log_axis", "off"));
		assert!(!dlg.state("log_curve").counts);
		assert!(!dlg.set_text("log_curve", "squiggly"));
		assert!(dlg.set_text("log_curve", "fixed_b"));
		assert!(dlg.state("log_axis").on);
		dlg.apply().unwrap();
		let text = std::fs::read_to_string(&path).unwrap();
		assert!(text.contains("log_curve: fixed_b"), "{text}");
	}
}
