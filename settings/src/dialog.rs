// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! What the settings window shows and does, with no Qt in it. The window
//! asks this for every row's state after each change.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use knobs::{Control, Kind, Model, NoEnv, Setting, Spec, Value};
use tukzmon_core::config::{self, Rgb, SavedTheme, THEME};

use crate::pick::{self, Field, Hsv};

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
	/// A dropdown's keys and labels, which change as themes are saved.
	pub options: Vec<(String, String)>,
}

/// Which theme buttons do anything right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ThemeButtons {
	pub save: bool,
	pub rename: bool,
	pub delete: bool,
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
	themes: Vec<SavedTheme>,
	/// As the file had them when it was last read or written.
	saved_themes: Vec<SavedTheme>,
	picker: Option<Picker>,
}

/// An open color picker. Its row follows every change, and Cancel puts back
/// all that was stored when it opened, since a color change also marks the
/// theme changed.
struct Picker {
	id: String,
	hsv: Hsv,
	values: knobs::Values,
	touched: Vec<String>,
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
			|| p == config::THEMES
			|| p.starts_with("themes.")
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
	out.extend(config::theme_problems(&doc));
	out
}

// Saved themes as the file has them, with what this window changed since
// `was` laid over them. One changed here is written whole.
fn merge_themes(file: Vec<SavedTheme>, was: &[SavedTheme], now: &[SavedTheme]) -> Vec<SavedTheme> {
	let mut out: Vec<SavedTheme> = file
		.into_iter()
		.filter(|t| now.iter().any(|n| n.key == t.key) || !was.iter().any(|w| w.key == t.key))
		.collect();
	for t in now {
		if was.contains(t) {
			continue;
		}
		match out.iter_mut().find(|o| o.key == t.key) {
			Some(o) => *o = t.clone(),
			None => out.push(t.clone()),
		}
	}
	out
}

impl Dialog {
	pub fn open(path: PathBuf) -> Dialog {
		let text = read(&path).unwrap_or_default();
		let doc = shcl::Document::parse(&text);
		let themes = config::saved_themes(&doc);
		let spec = config::spec_with(&themes);
		let model = config::model_on(Arc::clone(&spec), &doc);
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
			spec: Arc::clone(&spec),
			saved: model.clone(),
			model,
			touched: Vec::new(),
			short_sliders,
			saved_themes: themes.clone(),
			themes,
			picker: None,
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
		self.model != self.saved || self.themes != self.saved_themes
	}

	// the saved theme in force, if it's one
	fn on_saved(&self) -> Option<usize> {
		let key = self.model.chosen(THEME, &NoEnv);
		self.themes.iter().position(|t| t.key == key)
	}

	pub fn theme_buttons(&self) -> ThemeButtons {
		if self.on_saved().is_none() {
			return ThemeButtons::default();
		}
		ThemeButtons {
			save: !self.model.changed(THEME, &NoEnv).is_empty(),
			rename: true,
			delete: true,
		}
	}

	/// The saved theme in force, or "".
	pub fn theme_name(&self) -> String {
		self.on_saved()
			.map(|i| self.themes[i].name.clone())
			.unwrap_or_default()
	}

	fn shown_colors(&self) -> std::collections::BTreeMap<String, String> {
		self.spec
			.group(THEME)
			.map(|g| {
				g.members
					.iter()
					.map(|id| {
						let v = self.model.value(id, &NoEnv);
						(id.clone(), v.as_text().to_string())
					})
					.collect()
			})
			.unwrap_or_default()
	}

	// After the themes change, the dropdown and every pick go by the new list.
	fn respec(&mut self) {
		self.spec = config::spec_with(&self.themes);
		self.model.spec = Arc::clone(&self.spec);
	}

	// the colors are the theme's own now, so nothing is changed on top of it
	fn drop_theme_changes(&mut self) {
		let members = self
			.spec
			.group(THEME)
			.map(|g| g.members.clone())
			.unwrap_or_default();
		for id in &members {
			self.model.values.changes.remove(id);
			self.touch(id);
		}
		self.model.values.changed_on.remove(THEME);
		self.touch(THEME);
	}

	fn check_name(&self, name: &str, skip: Option<usize>) -> Result<String, String> {
		let name = name.trim();
		if name.is_empty() {
			return Err("A theme needs a name.".into());
		}
		if name.chars().any(char::is_control) {
			return Err("A theme name has to fit on one line.".into());
		}
		let mine = skip.map(|i| self.themes[i].key.as_str());
		let taken = self.spec.get(THEME).is_some_and(|s| {
			s.options
				.iter()
				.any(|(k, l)| Some(k.as_str()) != mine && l.eq_ignore_ascii_case(name))
		});
		if taken {
			return Err(format!("There's already a theme called {name}."));
		}
		Ok(name.to_string())
	}

	/// Put the colors showing into the saved theme in force.
	pub fn save_theme(&mut self) {
		let Some(i) = self.on_saved() else {
			return;
		};
		self.themes[i].colors = self.shown_colors();
		self.respec();
		self.drop_theme_changes();
	}

	/// Keep the colors showing as a new theme, and pick it.
	///
	/// # Errors
	///
	/// Why the name won't do.
	pub fn save_theme_as(&mut self, name: &str) -> Result<(), String> {
		let name = self.check_name(name, None)?;
		let theme = SavedTheme {
			key: config::theme_key(&name, &self.themes),
			name,
			colors: self.shown_colors(),
		};
		let key = theme.key.clone();
		self.themes.push(theme);
		self.respec();
		self.model.set(THEME, &Value::Text(key), &NoEnv);
		self.drop_theme_changes();
		Ok(())
	}

	/// # Errors
	///
	/// Why the name won't do.
	pub fn rename_theme(&mut self, name: &str) -> Result<(), String> {
		let Some(i) = self.on_saved() else {
			return Ok(());
		};
		self.themes[i].name = self.check_name(name, Some(i))?;
		self.respec();
		Ok(())
	}

	/// Delete the saved theme in force, which leaves Default picked.
	pub fn delete_theme(&mut self) {
		let Some(i) = self.on_saved() else {
			return;
		};
		self.themes.remove(i);
		self.model.reset(THEME, &NoEnv);
		self.respec();
		self.touch(THEME);
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
			options: self
				.spec
				.get(id)
				.map(|s| s.options.clone())
				.unwrap_or_default(),
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

	/// Opens the picker on a color row. False for any other row.
	pub fn pick_open(&mut self, id: &str) -> bool {
		if self.spec.get(id).is_none_or(|s| s.kind != Kind::Color) {
			return false;
		}
		let now = Rgb::parse(self.model.value(id, &NoEnv).as_text()).unwrap_or(Rgb(0, 0, 0));
		self.picker = Some(Picker {
			id: id.to_string(),
			hsv: pick::from_rgb(now, Hsv::default()),
			values: self.model.values.clone(),
			touched: self.touched.clone(),
		});
		true
	}

	pub fn pick_hsv(&self) -> Option<Hsv> {
		self.picker.as_ref().map(|p| p.hsv)
	}

	/// The one place the picker's color changes, and its row with it.
	pub fn pick_set(&mut self, hsv: Hsv) {
		let Some(p) = self.picker.as_mut() else {
			return;
		};
		p.hsv = Hsv {
			h: hsv.h.rem_euclid(1.0),
			s: hsv.s.clamp(0.0, 1.0),
			v: hsv.v.clamp(0.0, 1.0),
		};
		let (id, text) = (p.id.clone(), pick::hex(pick::to_rgb(p.hsv)));
		self.set(&id, &Value::Text(text));
	}

	/// A value typed into one of the picker's boxes. False while it says
	/// nothing yet, and nothing changes.
	pub fn pick_type(&mut self, field: Field, typed: &str) -> bool {
		let Some(hsv) = self.pick_hsv() else {
			return false;
		};
		let Some(hsv) = field.apply(hsv, typed) else {
			return false;
		};
		self.pick_set(hsv);
		true
	}

	/// OK keeps the color, and Cancel puts back what was there before.
	pub fn pick_close(&mut self, keep: bool) {
		let Some(p) = self.picker.take() else {
			return;
		};
		if !keep {
			self.model.values = p.values;
			self.touched = p.touched;
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
		let lose = would_lose(&config::spec(), &text);
		if !lose.is_empty() {
			return Err(format!(
				"{} has lines a save would drop, so it wasn't changed. Fix or remove them first.\n\n{}",
				self.path.display(),
				lose.join("\n")
			));
		}
		let doc = shcl::Document::parse(&text);
		let themes = merge_themes(config::saved_themes(&doc), &self.saved_themes, &self.themes);
		let spec = config::spec_with(&themes);
		let mut fresh = config::model_on(Arc::clone(&spec), &doc);
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
		let themes_text = config::themes_text(&themes);
		shcl::write_file_atomic(file, &format!("{HEADER}{body}{themes_text}"))?;
		self.saved = fresh.clone();
		self.model = fresh;
		self.spec = spec;
		self.saved_themes = themes.clone();
		self.themes = themes;
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
	fn save_as_keeps_the_colors_whole_and_picks_the_new_theme() {
		let d = scratch("save_as");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone());
		assert_eq!(dlg.theme_buttons(), ThemeButtons::default());
		assert!(dlg.set_text("theme", "blue_standard"));
		assert!(dlg.set_text("user", "#111111"));
		assert!(dlg.save_theme_as("  ").is_err());
		assert!(dlg.save_theme_as("blue Standard").is_err());
		assert!(dlg.save_theme_as("Custom").is_err());
		dlg.save_theme_as(" Night ").unwrap();
		assert_eq!(dlg.state("theme").choice, "Night");
		assert_eq!(dlg.state("user").text, "#111111");
		assert_eq!(dlg.state("nice").text, "#20405f");
		assert!(!dlg.state("user").can_reset);
		assert_eq!(
			dlg.state("theme")
				.options
				.iter()
				.map(|(_, l)| l.as_str())
				.nth(5),
			Some("Night")
		);
		let b = dlg.theme_buttons();
		assert!(!b.save && b.rename && b.delete);
		assert!(dlg.save_theme_as("night").is_err());
		dlg.apply().unwrap();
		let text = std::fs::read_to_string(&path).unwrap();
		assert!(text.contains("name: night"), "{text}");
		assert_eq!(text.matches("\"#").count(), 8, "{text}");
		assert!(!text.contains("kept"), "{text}");
		let dlg = Dialog::open(path.clone());
		assert_eq!(dlg.state("theme").choice, "Night");
		assert!(!dlg.dirty());
		assert_eq!(
			config::Look::load(&path).cpu[2],
			config::Rgb(0x11, 0x11, 0x11)
		);
	}

	#[test]
	fn save_puts_the_changes_into_the_theme() {
		let d = scratch("save");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone());
		dlg.save_theme_as("Mine").unwrap();
		dlg.apply().unwrap();
		assert!(dlg.set_text("user", "#222222"));
		assert_eq!(dlg.state("theme").choice, "Mine *");
		assert!(dlg.theme_buttons().save);
		dlg.save_theme();
		assert_eq!(dlg.state("theme").choice, "Mine");
		assert!(!dlg.theme_buttons().save);
		assert!(dlg.dirty());
		dlg.apply().unwrap();
		let text = std::fs::read_to_string(&path).unwrap();
		assert!(text.contains("#222222") && !text.contains("kept"), "{text}");
		// the theme picked again gives the saved color
		let mut dlg = Dialog::open(path.clone());
		assert!(dlg.set_text("theme", "default"));
		assert!(dlg.set_text("theme", "mine"));
		assert_eq!(dlg.state("user").text, "#222222");
	}

	#[test]
	fn rename_and_delete() {
		let d = scratch("rename");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone());
		dlg.rename_theme("Nope").unwrap();
		assert!(!dlg.dirty());
		dlg.save_theme_as("Mine").unwrap();
		dlg.save_theme_as("Other").unwrap();
		assert!(dlg.rename_theme("mine").is_err());
		dlg.rename_theme("OTHER").unwrap();
		assert_eq!(dlg.theme_name(), "OTHER");
		dlg.apply().unwrap();
		let text = std::fs::read_to_string(&path).unwrap();
		assert!(
			text.contains("name: OTHER") && text.contains("name: other"),
			"{text}"
		);
		dlg.delete_theme();
		assert_eq!(dlg.state("theme").choice, "Default");
		dlg.apply().unwrap();
		let text = std::fs::read_to_string(&path).unwrap();
		assert!(text.contains("mine") && !text.contains("other"), "{text}");
		let dlg = Dialog::open(path.clone());
		assert_eq!(dlg.state("theme").choice, "Default");
		assert_eq!(dlg.state("theme").options.len(), 7);
	}

	#[test]
	fn a_theme_saved_by_hand_meanwhile_survives() {
		let d = scratch("merge");
		let path = d.0.join("t.shcl");
		std::fs::write(&path, "themes:\n\tgone:\n\t\tuser: \"#010101\"\n").unwrap();
		let mut dlg = Dialog::open(path.clone());
		assert!(dlg.set_text("theme", "gone"));
		dlg.delete_theme();
		dlg.save_theme_as("Mine").unwrap();
		std::fs::write(
			&path,
			"themes:\n\tgone:\n\t\tuser: \"#010101\"\n\thand:\n\t\tuser: \"#020202\"\n",
		)
		.unwrap();
		dlg.apply().unwrap();
		let text = std::fs::read_to_string(&path).unwrap();
		assert!(
			!text.contains("gone") && text.contains("#020202") && text.contains("mine:"),
			"{text}"
		);
		assert_eq!(dlg.state("theme").options.len(), 8);
	}

	#[test]
	fn a_theme_line_it_cant_read_stops_the_save() {
		let d = scratch("bad_theme");
		let path = d.0.join("t.shcl");
		let text = "themes:\n\tmine:\n\t\tuser: blue\n";
		std::fs::write(&path, text).unwrap();
		let mut dlg = Dialog::open(path.clone());
		assert!(dlg.set_text("border", "2"));
		let e = dlg.apply().unwrap_err();
		assert!(e.contains("themes.mine.user"), "{e}");
		assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
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

	#[test]
	fn the_picker_moves_its_row_and_cancel_puts_it_all_back() {
		let d = scratch("pick_cancel");
		let mut dlg = Dialog::open(d.0.join("t.shcl"));
		let was = dlg.state("user").text;
		assert!(!dlg.pick_open("width"), "not a color row");
		assert!(dlg.pick_open("user"));
		let start = dlg.pick_hsv().unwrap();
		assert_eq!(pick::hex(pick::to_rgb(start)), was);
		// the square's top left is white at any hue
		dlg.pick_set(Hsv {
			s: 0.0,
			v: 1.0,
			..start
		});
		assert_eq!(dlg.state("user").text, "#ffffff", "the row follows at once");
		assert_eq!(dlg.state("theme").choice, "Default *");
		assert!(dlg.dirty());
		dlg.pick_close(false);
		assert_eq!(dlg.state("user").text, was);
		assert_eq!(dlg.state("theme").choice, "Default");
		assert!(!dlg.dirty());
		assert!(dlg.touched.is_empty(), "a save would still write the row");
		// and with it closed, nothing moves the row
		dlg.pick_set(Hsv {
			s: 0.0,
			v: 1.0,
			..start
		});
		assert_eq!(dlg.state("user").text, was);
	}

	#[test]
	fn the_picker_keeps_what_ok_was_pressed_on() {
		let d = scratch("pick_ok");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone());
		assert!(dlg.pick_open("nice"));
		assert!(
			!dlg.pick_type(Field::Hex, "#33"),
			"half-typed changes nothing"
		);
		assert!(dlg.pick_type(Field::Hex, "#3366cc"));
		let hsv = dlg.pick_hsv().unwrap();
		assert_eq!(Field::Red.text(hsv), "20");
		assert_eq!(Field::Blue.text(hsv), "80");
		assert!(dlg.pick_type(Field::Red, "100"));
		assert_eq!(dlg.state("nice").text, "#ff66cc");
		dlg.pick_close(true);
		assert!(dlg.pick_hsv().is_none());
		assert_eq!(dlg.state("nice").text, "#ff66cc");
		dlg.apply().unwrap();
		let dlg = Dialog::open(path);
		assert_eq!(dlg.state("nice").text, "#ff66cc");
		assert_eq!(dlg.state("theme").choice, "Default *");
	}
}
