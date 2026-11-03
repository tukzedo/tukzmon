// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! What the settings window shows and does, with no Qt in it. The window
//! asks this for every row's state after each change.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use knobs::{Control, Kind, Model, NoEnv, Setting, Spec, Value};
use tukzmon_core::config::{self, Rgb, SavedTheme, THEME};
use tukzmon_core::keep;

use crate::pick::{self, Field, Hsv};

const HEADER: &str = "## tukzmon settings. Only values changed from their defaults are listed.\n";

/// One row as the window shows it now.
#[allow(
	clippy::struct_excessive_bools,
	reason = "a snapshot of one row for the window, and each flag stands alone"
)]
#[derive(Debug)]
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
	/// False while its `shown_when` dropdown is on another key.
	pub shown: bool,
}

/// A row of the graph list on the Graphs tab: a graph's history name, and the
/// ids of the settings it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphRow {
	pub name: String,
	pub show: String,
	pub width: String,
	pub sample: String,
	pub time: String,
	pub vertical: String,
}

/// The setting the graph list's grippers write.
pub const ORDER: &str = "graph_order";

/// Which theme buttons do anything right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ThemeButtons {
	pub save: bool,
	pub rename: bool,
	pub delete: bool,
}

/// The settings window's state: the file, what's changed in it, and the
/// saved themes.
#[derive(Debug)]
pub struct Dialog {
	path: PathBuf,
	spec: Arc<Spec>,
	model: Model,
	/// As the file was when it was last read or written.
	saved: Model,
	/// Sliders that reach less far than their number box takes, as copies of
	/// the setting over the slider's own range.
	short_sliders: Vec<Setting>,
	themes: Vec<SavedTheme>,
	/// As the file had them when it was last read or written.
	saved_themes: Vec<SavedTheme>,
	picker: Option<Picker>,
	/// Where the panel keeps history, for Reset history.
	history_dir: PathBuf,
}

/// An open color picker. Its row follows every change, and Cancel puts back
/// all that was stored when it opened, since a color change also marks the
/// theme changed.
#[derive(Debug)]
struct Picker {
	id: String,
	hsv: Hsv,
	values: knobs::Values,
}

fn read(path: &Path) -> Result<String, String> {
	match std::fs::read_to_string(path) {
		Ok(t) => Ok(t),
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
		Err(e) => Err(format!("{} can't be read: {e}", path.display())),
	}
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
		// the window keeps its own list too
		match out.iter_mut().find(|o| o.key == t.key) {
			Some(o) => *o = t.clone(),
			None => out.push(t.clone()),
		}
	}
	out
}

// Each entry the window changed since `was` goes onto `to`, set or taken out,
// and the rest of `to` stays as the file has it now.
fn carry<V: Clone + PartialEq>(
	was: &BTreeMap<String, V>,
	now: &BTreeMap<String, V>,
	to: &mut BTreeMap<String, V>,
) {
	for key in was.keys().chain(now.keys()) {
		match now.get(key) {
			v if v == was.get(key) => {}
			Some(v) => {
				to.insert(key.clone(), v.clone());
			}
			None => {
				to.remove(key);
			}
		}
	}
}

impl Dialog {
	/// The window's state for the file at `path`. A missing file opens at the
	/// defaults. One that can't be read is an error, so a save can't write the
	/// defaults over it.
	pub fn open(path: PathBuf) -> Result<Dialog, String> {
		let text = read(&path)?;
		let doc = shcl::Document::parse(&text);
		let themes = config::saved_themes(&doc);
		let spec = config::spec_with(&themes);
		let model = config::model_on(Arc::clone(&spec), &doc);
		let short_sliders = spec
			.settings
			.iter()
			.filter_map(|s| {
				let (min, max) = config::slider_range(&s.id)?;
				// the same setting, but over the slider's range
				Some(Setting {
					min,
					max,
					..s.clone()
				})
			})
			.collect();
		Ok(Dialog {
			path,
			spec: Arc::clone(&spec),
			// the saved copies tell what this window changed
			saved: model.clone(),
			model,
			short_sliders,
			saved_themes: themes.clone(),
			themes,
			picker: None,
			history_dir: keep::data_dir(),
		})
	}

	/// The setting as its slider sees it.
	pub fn slider(&self, id: &str) -> Option<&Setting> {
		self.short_sliders
			.iter()
			.find(|s| s.id == id)
			.or_else(|| self.spec.get(id))
	}

	/// The config file it reads and writes.
	pub fn path(&self) -> &Path {
		&self.path
	}

	/// The settings spec, with the saved themes in its theme list.
	pub fn spec(&self) -> &Spec {
		&self.spec
	}

	/// The graphs' look as the window has it now, applied or not.
	pub fn look(&self) -> config::Look {
		config::Look::from_model(&self.model)
	}

	/// True when a setting or a saved theme differs from the file.
	pub fn dirty(&self) -> bool {
		self.model != self.saved || self.themes != self.saved_themes
	}

	/// Whether Reset history deletes the saved history too: Keep across
	/// restarts is on, as shown or as the panel has it.
	pub fn reset_deletes_saved(&self) -> bool {
		self.look().keep || config::Look::from_model(&self.saved).keep
	}

	/// Starts every graph in every panel item over now. See
	/// [`Self::reset_deletes_saved`] for when the saved history goes too.
	///
	/// # Errors
	///
	/// Why the panel wasn't told.
	pub fn reset_history(&self) -> Result<(), String> {
		keep::reset(&self.history_dir, self.reset_deletes_saved()).map_err(|e| {
			format!(
				"History in {} wasn't reset: {e}",
				self.history_dir.display()
			)
		})
	}

	// the saved theme in force, if it's one
	fn on_saved(&self) -> Option<usize> {
		let key = self.model.chosen(THEME, &NoEnv);
		self.themes.iter().position(|t| t.key == key)
	}

	/// Which theme buttons do anything now. None do off a saved theme.
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
						(id.clone(), config::theme_text(&v))
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
			.map_or(&[][..], |g| g.members.as_slice());
		for id in members {
			self.model.values.changes.remove(id);
		}
		self.model.values.changed_on.remove(THEME);
	}

	// A theme made here since the last save, whose key or name another window
	// has saved meanwhile, moves to a free one, so neither is lost
	// (2026103115231908).
	fn make_room(&mut self, file: &[SavedTheme]) {
		// the ones this window knew are in self.themes, or deleted here
		let mut others: Vec<SavedTheme> = file
			.iter()
			.filter(|f| !self.saved_themes.iter().any(|s| s.key == f.key))
			.cloned()
			.collect();
		if others.is_empty() {
			return;
		}
		let mut moved = false;
		for i in 0..self.themes.len() {
			let t = &self.themes[i];
			if self.saved_themes.iter().any(|s| s.key == t.key) {
				continue;
			}
			let key_taken = others.iter().any(|o| o.key.eq_ignore_ascii_case(&t.key));
			let name_taken = |n: &str| {
				others.iter().any(|o| o.name.eq_ignore_ascii_case(n))
					|| self
						.themes
						.iter()
						.enumerate()
						.any(|(j, o)| j != i && o.name.eq_ignore_ascii_case(n))
			};
			if !key_taken && !name_taken(&t.name) {
				continue;
			}
			let name = if name_taken(&t.name) {
				// n names can take at most n of these
				(2..=others.len() + self.themes.len() + 2)
					.map(|n| format!("{} {n}", t.name))
					.find(|n| !name_taken(n))
					.unwrap_or_else(|| t.name.clone())
			} else {
				t.name.clone()
			};
			let old = t.key.clone();
			let key = if key_taken {
				let all: Vec<SavedTheme> = others.iter().chain(&self.themes).cloned().collect();
				config::theme_key(&name, &all)
			} else {
				old.clone()
			};
			others.push(SavedTheme {
				key: key.clone(),
				name: name.clone(),
				colors: BTreeMap::new(),
			});
			self.themes[i].name = name;
			self.themes[i].key.clone_from(&key);
			let v = &mut self.model.values;
			for store in [&mut v.own, &mut v.state] {
				if let Some(Value::Text(k)) = store.get_mut(THEME) {
					if *k == old {
						k.clone_from(&key);
					}
				}
			}
			if v.changed_on.get(THEME) == Some(&old) {
				v.changed_on.insert(THEME.to_string(), key);
			}
			moved = true;
		}
		if moved {
			self.respec();
		}
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
		// the dropdown picks it by key, once it's in the list
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
	}

	/// Random colors for every graph, as changes on the theme in force, so
	/// Save as keeps them and picking a theme drops them.
	pub fn randomize(&mut self) {
		self.randomize_from(crate::dice::fresh_seed());
	}

	/// [`Self::randomize`], from `seed`, so a test gets the same colors each time.
	pub fn randomize_from(&mut self, seed: u64) {
		let rolled = crate::dice::theme(seed);
		// a set turns its gate on, and a roll is colors, not the overrides
		let gates: Vec<(String, Value)> = rolled
			.iter()
			// owned, since the sets below take self
			.filter_map(|(id, _)| self.spec.get(id)?.gate.clone())
			.map(|g| {
				let was = self.model.value(&g, &NoEnv);
				(g, was)
			})
			.collect();
		for (id, v) in rolled {
			self.set(id, &Value::Text(v));
		}
		for (g, was) in gates {
			if self.model.value(&g, &NoEnv) != was {
				self.set(&g, &was);
			}
		}
	}

	/// The row for setting `id` as it should look now.
	pub fn state(&self, id: &str) -> RowState {
		let v = self.model.value(id, &NoEnv);
		let number = v.as_f64();
		let mut tip = self.model.tip(id, &NoEnv);
		// each lookup is a scan of every setting, and this runs per row per refresh
		let setting = self.spec.get(id);
		let short = self.short_sliders.iter().find(|s| s.id == id);
		if let (Some(short), Some(s)) = (short, setting) {
			if !tip.is_empty() {
				tip.push_str("\n\n");
			}
			let _ = write!(
				tip,
				"The slider goes from {} to {}. Type in the box for anything from {} to {}.",
				short.min, short.max, s.min, s.max
			);
		}
		let on = v.as_bool();
		RowState {
			text: match v {
				Value::Text(t) => t,
				other => other.show(),
			},
			on,
			number,
			slider: short.or(setting).map_or(0.0, |s| s.to_t(number)),
			can_reset: self.model.can_reset(id, &NoEnv),
			counts: self.model.counts(id, &NoEnv),
			tip,
			choice: match setting {
				Some(s) if s.control == Control::Dropdown => self.model.shown_choice(id, &NoEnv),
				_ => String::new(),
			},
			options: setting.map(|s| s.options.clone()).unwrap_or_default(),
			shown: config::shown_when(id)
				.is_none_or(|(on, key)| self.model.value(on, &NoEnv).as_text() == key),
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

	/// Puts setting `id` back to its default.
	pub fn reset(&mut self, id: &str) {
		if self.spec.get(id).is_some() {
			self.model.reset(id, &NoEnv);
		}
	}

	fn id_at(&self, path: &str) -> Option<&str> {
		self.spec
			.settings
			.iter()
			.find(|s| s.path == path)
			.map(|s| s.id.as_str())
	}

	fn saved_order(&self) -> Vec<String> {
		config::graph_order(self.model.value(ORDER, &NoEnv).as_text())
	}

	/// The graph list, first drawn first. A graph is in it once the spec has
	/// its `<name>.show`, `.width`, `.sample`, `.time` and `.vertical` rows,
	/// so a new graph brings its own row.
	pub fn graph_rows(&self) -> Vec<GraphRow> {
		tukzmon_core::graphs::draw_order(&self.saved_order())
			.into_iter()
			.filter_map(|name| {
				let id = |what: &str| self.id_at(&format!("{name}.{what}")).map(str::to_string);
				Some(GraphRow {
					name: name.to_string(),
					show: id("show")?,
					width: id("width")?,
					sample: id("sample")?,
					time: id("time")?,
					vertical: id("vertical")?,
				})
			})
			.collect()
	}

	/// Per spec row, the graph whose section it's in, from that section's
	/// `graph:` heading to the next one or the end of its tab, and whether it
	/// sits beside the rest, from a `beside: true` heading on.
	pub fn sections(&self) -> Vec<Option<(&'static str, bool)>> {
		let mut now: Option<(&'static str, usize, bool)> = None;
		self.spec
			.settings
			.iter()
			.map(|s| {
				if let Some(graph) = config::section_graph(&s.id) {
					now = Some((graph, s.tab, false));
				} else if now.is_some_and(|(_, tab, _)| tab != s.tab) {
					now = None;
				}
				if let Some(n) = now.as_mut() {
					n.2 |= config::beside(&s.id);
				}
				now.map(|(graph, _, beside)| (graph, beside))
			})
			.collect()
	}

	/// The graphs with a section, in the graph order, which is how they're
	/// listed on their tab.
	pub fn section_order(&self) -> Vec<String> {
		let have: Vec<&str> = self
			.sections()
			.into_iter()
			.flatten()
			.map(|(g, _)| g)
			.collect();
		self.graph_rows()
			.into_iter()
			.map(|r| r.name)
			.filter(|n| have.contains(&n.as_str()))
			.collect()
	}

	/// Drawn in the graph list rather than as a row of its own, the order
	/// included.
	pub fn in_graph_list(&self, id: &str) -> bool {
		id == ORDER
			|| self.graph_rows().iter().any(|r| {
				[&r.show, &r.width, &r.sample, &r.time, &r.vertical]
					.iter()
					.any(|s| *s == id)
			})
	}

	/// Moves the graph list's row `from` to `to`, and keeps that as the order.
	/// False when either is past the end.
	pub fn move_graph(&mut self, from: usize, to: usize) -> bool {
		let mut names: Vec<String> = self.graph_rows().into_iter().map(|r| r.name).collect();
		if from >= names.len() || to >= names.len() {
			return false;
		}
		let moved = names.remove(from);
		names.insert(to, moved);
		// a name only a newer tukzmon knows stays, after these
		let known = tukzmon_core::graphs::draw_order(&[]);
		for name in self.saved_order() {
			if !known.contains(&name.as_str()) {
				names.push(name);
			}
		}
		self.set_text(ORDER, &names.join(" "))
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
			// what Cancel puts back
			values: self.model.values.clone(),
		});
		true
	}

	/// The open picker's color, or None with no picker open.
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
		// p borrows self, and set takes it
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
		}
	}

	/// Save what was changed here onto the file as it is now, so a hand edit
	/// made while the window was open isn't lost. Lines it can't read, or for
	/// settings that are gone, are dropped (2026102913122701).
	///
	/// # Errors
	///
	/// Why nothing was written.
	pub fn apply(&mut self) -> Result<(), String> {
		let text = read(&self.path)?;
		let doc = shcl::Document::parse(&text);
		let file_themes = config::saved_themes(&doc);
		self.make_room(&file_themes);
		let themes = merge_themes(file_themes, &self.saved_themes, &self.themes);
		let spec = config::spec_with(&themes);
		let mut fresh = config::model_on(Arc::clone(&spec), &doc);
		// Store by store rather than replaying each row, since a row's set or
		// reset means something else on the file's theme than on the window's.
		let (was, now, to) = (&self.saved.values, &self.model.values, &mut fresh.values);
		carry(&was.own, &now.own, &mut to.own);
		carry(&was.changes, &now.changes, &mut to.changes);
		carry(&was.changed_on, &now.changed_on, &mut to.changed_on);
		carry(&was.state, &now.state, &mut to.state);
		config::name_the_theme(&mut fresh);
		let (body, _) = fresh.save(&NoEnv);
		// shcl writes through a link only when its file is there
		let target = config::through_links(&self.path);
		let Some(file) = target.to_str() else {
			return Err(format!("{} isn't a path shcl can write", target.display()));
		};
		if let Some(dir) = target.parent() {
			std::fs::create_dir_all(dir)
				.map_err(|e| format!("{} can't be made: {e}", dir.display()))?;
		}
		let themes_text = config::themes_text(&themes);
		shcl::write_file_atomic(file, &keep_notes(doc, &format!("{body}{themes_text}")))?;
		// as in open, kept apart to tell what changes next
		self.saved = fresh.clone();
		self.model = fresh;
		self.spec = spec;
		self.saved_themes.clone_from(&themes);
		self.themes = themes;
		Ok(())
	}
}

/// The file as `file` has it, comments and all, with the values of `fresh`
/// in place of its own. `fresh` as it is when that wouldn't read back the same,
/// such as with a raw block, so the values always win (2026103115232235).
fn keep_notes(mut file: shcl::Document, fresh: &str) -> String {
	let plain = format!("{HEADER}{fresh}");
	let want = shcl::Document::parse(fresh);
	for path in file.paths().iter().rev() {
		if !want.exists(path) {
			file.remove(path);
		}
	}
	for path in want.paths() {
		if !want.children(&path).is_empty() {
			continue;
		}
		let now = raw_text(&want, &path);
		// an untouched line keeps its trailing comment
		if file.exists(&path) && file.children(&path).is_empty() && raw_text(&file, &path) == now {
			continue;
		}
		let set = if now.is_empty() {
			file.set_empty(&path)
		} else {
			file.set_literal(&path, &now)
		};
		if !set {
			return plain;
		}
	}
	let mut text = file.to_canonical();
	if !text.starts_with(HEADER) {
		text.insert_str(0, HEADER);
	}
	if reads_the_same(&shcl::Document::parse(&text), &want) {
		text
	} else {
		plain
	}
}

fn raw_text(doc: &shcl::Document, path: &str) -> String {
	doc.read_string(path).raw.unwrap_or_default()
}

fn reads_the_same(a: &shcl::Document, b: &shcl::Document) -> bool {
	let (mut on_a, mut on_b) = (a.paths(), b.paths());
	on_a.sort_unstable();
	on_b.sort_unstable();
	a.error_count() == b.error_count()
		&& on_a == on_b
		&& on_a
			.iter()
			.all(|p| a.count(p) == b.count(p) && raw_text(a, p) == raw_text(b, p))
}

#[cfg(test)]
mod tests {
	use super::*;
	use tukzmon_core::config::Which;

	struct Dir(PathBuf);
	impl Drop for Dir {
		fn drop(&mut self) {
			// The folder is this test's own, named by pid and test, so a test may write any name in it.
			let _ = std::fs::remove_dir_all(&self.0);
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

	// Out since 2026-10-28: Reset history acts at once rather than at Apply,
	// per backlog 2026102814061807. dejaf checks it now.
	// // Test ID: defxs
	// #[test]
	// #[cfg_attr(miri, ignore = "the save fsyncs its folder, which Miri can't")]
	// fn reset_history_waits_for_apply_and_deletes_saved_history_only_when_kept() {
	// 	let d = scratch("reset");
	// 	let data = d.0.join("data");
	// 	let hist = keep::history_file(&data, 3);
	// 	keep::write(&hist, &[("cpu", b"x")]).unwrap();
	// 	let mut dlg = Dialog::open(d.0.join("t.shcl")).unwrap();
	// 	dlg.history_dir.clone_from(&data);
	// 	dlg.reset_history();
	// 	assert!(dlg.dirty());
	// 	assert!(!keep::reset_mark(&data).exists());
	// 	dlg.apply().unwrap();
	// 	assert!(!dlg.dirty() && !dlg.reset_pending());
	// 	assert!(!hist.exists());
	// 	let mut watch = keep::ResetWatch::new(&data);
	// 	assert!(keep::reset_mark(&data).exists());
	// 	// with history not kept, only what's on screen goes
	// 	keep::write(&hist, &[("cpu", b"x")]).unwrap();
	// 	std::thread::sleep(std::time::Duration::from_millis(50));
	// 	dlg.set("keep_history", &Value::Bool(false));
	// 	dlg.reset_history();
	// 	dlg.apply().unwrap();
	// 	assert!(hist.exists());
	// 	assert!(watch.asked());
	// 	std::fs::remove_file(&hist).unwrap();
	// 	std::fs::remove_file(keep::reset_mark(&data)).unwrap();
	// 	std::fs::remove_dir(&data).unwrap();
	// }

	// Test ID: dejaf
	#[test]
	fn reset_history_goes_at_once_and_deletes_saved_history_only_when_kept() {
		let d = scratch("reset_now");
		let data = d.0.join("data");
		let hist = keep::history_file(&data, 3);
		keep::write(&hist, &[("cpu", b"x")]).unwrap();
		let mut dlg = Dialog::open(d.0.join("t.shcl")).unwrap();
		dlg.history_dir.clone_from(&data);
		assert!(dlg.reset_deletes_saved());
		dlg.reset_history().unwrap();
		assert!(!dlg.dirty());
		assert!(keep::reset_mark(&data).exists());
		assert!(!hist.exists());
		let mut watch = keep::ResetWatch::new(&data);
		// off in the window but not yet applied: the panel still keeps it
		keep::write(&hist, &[("cpu", b"x")]).unwrap();
		dlg.set("keep_history", &Value::Bool(false));
		assert!(dlg.reset_deletes_saved());
		// timestamps are coarse, so the next reset must be told apart
		std::thread::sleep(std::time::Duration::from_millis(50));
		dlg.reset_history().unwrap();
		assert!(watch.asked() && !hist.exists());
		// off and applied: only what's on screen goes
		dlg.saved = dlg.model.clone();
		assert!(!dlg.reset_deletes_saved());
		keep::write(&hist, &[("cpu", b"x")]).unwrap();
		std::thread::sleep(std::time::Duration::from_millis(50));
		dlg.reset_history().unwrap();
		assert!(watch.asked() && hist.exists());
		std::fs::remove_file(&hist).unwrap();
		std::fs::remove_file(keep::reset_mark(&data)).unwrap();
		std::fs::remove_dir(&data).unwrap();
	}

	// Test ID: debjc
	#[test]
	fn opens_on_defaults_with_no_file() {
		let d = scratch("defaults");
		let dlg = Dialog::open(d.0.join("t.shcl")).unwrap();
		assert!(!dlg.dirty());
		let s = dlg.state("user");
		assert_eq!(s.text, "#00ff80");
		assert!(!s.can_reset);
		assert_eq!(dlg.state("width").number, 100.0);
	}

	// Test ID: den9y
	#[test]
	fn a_file_that_cant_be_read_is_an_error_not_the_defaults() {
		use std::os::unix::fs::PermissionsExt as _;
		let d = scratch("unreadable");
		let path = d.0.join("t.shcl");
		// a folder can't be read as a file, even by root
		std::fs::create_dir(&path).unwrap();
		let e = Dialog::open(path.clone()).err().unwrap();
		std::fs::remove_dir(&path).unwrap();
		assert!(
			e.starts_with(&format!("{} can't be read: ", path.display())),
			"{e}"
		);
		std::fs::write(&path, "graph.width: 120\n").unwrap();
		// Miri has no chmod
		if cfg!(miri) {
			return;
		}
		std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
		// root reads it anyway, and then it isn't the case under test
		if std::fs::read(&path).is_err() {
			assert!(Dialog::open(path.clone()).is_err());
		}
		std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
		assert_eq!(
			std::fs::read_to_string(&path).unwrap(),
			"graph.width: 120\n"
		);
	}

	// Test ID: dejac
	#[test]
	fn the_graph_sections_follow_the_graph_order_and_the_tabs_stay_put() {
		let d = scratch("tab_order");
		let mut dlg = Dialog::open(d.0.join("t.shcl")).unwrap();
		let tabs: Vec<String> = dlg.spec.tabs.iter().map(|t| t.label.clone()).collect();
		assert_eq!(tabs, ["General", "Graphs", "Themes", "Colors", "Outline"]);
		// each section by its heading, as the window shows it
		let labels = |dlg: &Dialog| -> Vec<String> {
			dlg.section_order()
				.iter()
				.map(|g| {
					let head = dlg
						.spec
						.settings
						.iter()
						.find(|s| config::section_graph(&s.id) == Some(g.as_str()));
					head.unwrap_or_else(|| panic!("{g} has no heading"))
						.label
						.clone()
				})
				.collect()
		};
		// the spec lists them in the default graph order
		let graphs = [
			"CPU",
			"Memory",
			"Processes",
			"GPU",
			"GPU memory",
			"Network",
			"Disk",
			"Temperature",
			"Swap",
			"Zswap",
			"Load",
			"tukzmon",
			"Battery",
		];
		assert_eq!(labels(&dlg), graphs);
		let last = dlg.graph_rows().len() - 1;
		assert!(dlg.move_graph(last, 0));
		assert!(dlg.move_graph(2, 4));
		let moved = [
			"Battery",
			"CPU",
			"Processes",
			"GPU",
			"Memory",
			"GPU memory",
			"Network",
			"Disk",
			"Temperature",
			"Swap",
			"Zswap",
			"Load",
			"tukzmon",
		];
		assert_eq!(labels(&dlg), moved);
		assert_eq!(
			dlg.spec
				.tabs
				.iter()
				.map(|t| t.label.clone())
				.collect::<Vec<_>>(),
			tabs
		);
		dlg.reset(ORDER);
		assert_eq!(labels(&dlg), graphs);
	}

	// Test ID: dejma
	#[test]
	fn every_graphs_colors_are_its_own_section_with_the_background_beside() {
		let d = scratch("sections");
		let dlg = Dialog::open(d.0.join("t.shcl")).unwrap();
		let sections = dlg.sections();
		let names: Vec<String> = dlg.graph_rows().into_iter().map(|r| r.name).collect();
		assert_eq!(dlg.section_order(), names);
		let colors = dlg
			.spec
			.tabs
			.iter()
			.position(|t| t.label == "Colors")
			.unwrap();
		for name in &names {
			let mode = dlg.id_at(&format!("{name}.mode")).unwrap();
			let mine: Vec<(&Setting, bool)> = dlg
				.spec
				.settings
				.iter()
				.zip(&sections)
				.filter_map(|(s, at)| at.filter(|(g, _)| g == name).map(|(_, b)| (s, b)))
				.collect();
			// one run of rows, all on the one tab, and every one of them this graph's
			let first = dlg
				.spec
				.settings
				.iter()
				.position(|s| s.id == mine[0].0.id)
				.unwrap();
			for (i, (s, _)) in mine.iter().enumerate() {
				assert_eq!(dlg.spec.settings[first + i].id, s.id, "{name}");
				assert_eq!(s.tab, colors, "{}", s.id);
				assert!(
					s.path.is_empty() || s.path.starts_with(&format!("{name}.")),
					"{} in {name}",
					s.id
				);
			}
			let beside: Vec<&str> = mine
				.iter()
				.filter(|(_, b)| *b)
				.map(|(s, _)| s.path.as_str())
				.filter(|p| !p.is_empty())
				.collect();
			let want: Vec<String> = ["border", "top", "bottom"]
				.iter()
				.map(|c| format!("{name}.colors.{c}"))
				.collect();
			assert_eq!(beside, want, "{name}");
			let left: Vec<&str> = mine
				.iter()
				.filter(|(_, b)| !*b)
				.map(|(s, _)| s.id.as_str())
				.collect();
			assert!(left.contains(&mode), "{name}");
			assert!(
				left.iter().any(|id| dlg
					.spec
					.get(id)
					.is_some_and(|s| s.control == Control::Color)),
				"{name}"
			);
		}
		// no graph's rows outside its section
		for (s, at) in dlg.spec.settings.iter().zip(&sections) {
			if s.tab == colors {
				assert!(at.is_some(), "{} is in no section", s.id);
			}
		}
	}

	// Test ID: dedqr
	#[test]
	#[cfg_attr(miri, ignore = "the save fsyncs its folder, which Miri can't")]
	fn the_graph_order_is_saved_as_typed_and_reads_back() {
		let d = scratch("order");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone()).unwrap();
		for typed in ["mem cpu", "mem, cpu"] {
			assert!(dlg.set_text("graph_order", typed));
			dlg.apply().unwrap();
			assert_eq!(
				config::Look::load(&path).unwrap().order,
				["mem", "cpu"],
				"{typed}"
			);
			// and the window takes the file back without a complaint
			assert!(
				Dialog::open(path.clone()).unwrap().apply().is_ok(),
				"{typed}"
			);
		}
	}

	// Test ID: defkz
	#[test]
	fn the_graph_list_has_a_row_per_graph_in_the_saved_order() {
		let d = scratch("list");
		let mut dlg = Dialog::open(d.0.join("t.shcl")).unwrap();
		let names = |dlg: &Dialog| -> Vec<String> {
			dlg.graph_rows().into_iter().map(|r| r.name).collect()
		};
		assert_eq!(
			dlg.graph_rows()[1],
			GraphRow {
				name: "mem".into(),
				show: "show_memory".into(),
				width: "mem_width".into(),
				sample: "mem_sample".into(),
				time: "mem_time".into(),
				vertical: "mem_vertical".into(),
			}
		);
		// every graph the panel draws, so a new one can't be left off the list
		let default = tukzmon_core::config::Look::default().order;
		assert_eq!(names(&dlg), tukzmon_core::graphs::draw_order(&default));
		assert_eq!(
			names(&dlg).len(),
			tukzmon_core::graphs::draw_order(&[]).len()
		);
		// the rest follow in the order the graphs were added
		assert!(dlg.set_text(ORDER, "mem, cpu"));
		assert_eq!(
			names(&dlg),
			[
				"mem", "cpu", "gpu", "vram", "net", "swap", "zswap", "load", "disk", "temp",
				"battery", "self", "procs"
			]
		);
		for id in [
			"show_cpu",
			"cpu_width",
			"cpu_time",
			"cpu_vertical",
			"show_memory",
			"mem_width",
			"mem_time",
			ORDER,
		] {
			assert!(dlg.in_graph_list(id), "{id}");
		}
		for id in ["width", "time_axis", "vertical_axis", "graphshead", "theme"] {
			assert!(!dlg.in_graph_list(id), "{id}");
		}
		// a hidden graph's width and time axis can still be set
		assert!(dlg.set(&dlg.graph_rows()[0].show.clone(), &Value::Bool(false)));
		assert!(dlg.state("mem_width").counts && dlg.state("mem_time").counts);
		assert!(dlg.state("cpu_width").counts);
	}

	// Test ID: defm0
	#[test]
	#[cfg_attr(miri, ignore = "the save fsyncs its folder, which Miri can't")]
	fn dragging_a_graph_saves_the_order_and_keeps_names_it_doesnt_know() {
		let d = scratch("drag");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert!(!dlg.move_graph(0, 13));
		assert!(!dlg.dirty());
		assert!(dlg.move_graph(0, 1));
		assert_eq!(
			dlg.state(ORDER).text,
			"mem cpu procs gpu vram net disk temp swap zswap load self battery"
		);
		dlg.apply().unwrap();
		assert_eq!(
			config::Look::load(&path).unwrap().order,
			[
				"mem", "cpu", "procs", "gpu", "vram", "net", "disk", "temp", "swap", "zswap",
				"load", "self", "battery"
			]
		);
		assert!(dlg.move_graph(1, 0));
		assert!(!dlg.state(ORDER).can_reset);
		assert!(dlg.set_text(ORDER, "nope mem cpu"));
		assert!(dlg.move_graph(0, 1));
		assert_eq!(
			dlg.state(ORDER).text,
			"cpu mem gpu vram net swap zswap load disk temp battery self procs nope"
		);
	}

	// Test ID: debjd
	#[test]
	#[cfg_attr(miri, ignore = "the save fsyncs its folder, which Miri can't")]
	fn apply_writes_only_what_changed() {
		let d = scratch("apply");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert!(dlg.set_text("user", "123abc"));
		assert!(dlg.dirty());
		dlg.apply().unwrap();
		assert!(!dlg.dirty());
		let text = std::fs::read_to_string(&path).unwrap();
		assert!(text.starts_with("##"));
		assert!(text.contains("#123abc"));
		assert!(!text.contains("width"));
		assert_eq!(
			config::Look::load(&path).unwrap()[Which::Cpu].colors[2],
			config::Rgb(0x12, 0x3a, 0xbc)
		);
	}

	// Test ID: deqw1
	#[cfg(unix)]
	#[test]
	#[cfg_attr(miri, ignore = "the save fsyncs its folder, which Miri can't")]
	fn apply_writes_through_a_link_to_a_file_not_there_yet() {
		use std::os::unix::fs::symlink;
		let d = scratch("dangling");
		std::fs::create_dir_all(d.0.join("real")).unwrap();
		// a chain, with a relative and an absolute hop
		let path = d.0.join("t.shcl");
		symlink("mid.shcl", &path).unwrap();
		symlink(d.0.join("real/t.shcl"), d.0.join("mid.shcl")).unwrap();
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert!(dlg.set_text("user", "123abc"));
		dlg.apply().unwrap();
		for link in [&path, &d.0.join("mid.shcl")] {
			assert!(
				std::fs::symlink_metadata(link).unwrap().is_symlink(),
				"{}",
				link.display()
			);
		}
		let text = std::fs::read_to_string(d.0.join("real/t.shcl")).unwrap();
		assert!(text.contains("#123abc"));
		// and once it's there, the next save still goes through
		assert!(dlg.set_text("user", "456def"));
		dlg.apply().unwrap();
		assert!(std::fs::symlink_metadata(&path).unwrap().is_symlink());
		assert!(
			std::fs::read_to_string(d.0.join("real/t.shcl"))
				.unwrap()
				.contains("#456def")
		);
	}

	// Test ID: debje
	#[test]
	#[cfg_attr(miri, ignore = "the save fsyncs its folder, which Miri can't")]
	fn a_hand_edit_made_while_open_survives() {
		let d = scratch("hand");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone()).unwrap();
		std::fs::write(&path, "graph:\n\twidth: 300\n").unwrap();
		assert!(dlg.set_text("border", "2"));
		dlg.apply().unwrap();
		let l = config::Look::load(&path).unwrap();
		assert_eq!((l.width, l.border), (300, 2));
		// and a reset in the window takes the line back out
		dlg.reset("border");
		dlg.apply().unwrap();
		assert_eq!(config::Look::load(&path).unwrap().border, 1);
	}

	// Out since 2026-10-29: a save drops what it can't place rather than
	// refusing, per backlog 2026102913122701. dekm0 checks it now.
	// // Test ID: debjf
	// #[test]
	// fn refuses_to_drop_lines_it_cant_place() {
	// 	let d = scratch("lose");
	// 	let path = d.0.join("t.shcl");
	// 	std::fs::write(&path, "graph:\n\twidht: 300\n").unwrap();
	// 	let mut dlg = Dialog::open(path.clone()).unwrap();
	// 	dlg.set_text("border", "2");
	// 	let e = dlg.apply().unwrap_err();
	// 	assert!(e.contains("graph.widht"), "{e}");
	// 	assert_eq!(
	// 		std::fs::read_to_string(&path).unwrap(),
	// 		"graph:\n\twidht: 300\n"
	// 	);
	// }

	// Test ID: dekm0
	#[test]
	#[cfg_attr(miri, ignore = "the save fsyncs its folder, which Miri can't")]
	fn an_old_file_with_lines_it_cant_place_saves_without_them() {
		let d = scratch("old");
		let path = d.0.join("t.shcl");
		let old = "graph:\n\twidth: 300\n\twidht: 120\ncpu:\n\tsample_ms: 500\n\t: 7\n\
			themes:\n\tmine:\n\t\tname: Mine\n\t\tuser: blue\n\t\tzswap_ratio: \"#123456\"\n\t\tcpu_low: \"#010203\"\n";
		let doc = shcl::Document::parse(old);
		assert!(
			doc.diagnostics()
				.iter()
				.any(|d| d.severity == shcl::Severity::Error),
			"wants a line that doesn't parse"
		);
		std::fs::write(&path, old).unwrap();
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert!(dlg.set_text("border", "2"));
		dlg.apply().unwrap();
		let text = std::fs::read_to_string(&path).unwrap();
		for gone in ["widht", "sample_ms", ": 7", "blue", "zswap_ratio"] {
			assert!(!text.contains(gone), "{gone} in {text}");
		}
		assert!(text.contains("mine:") && text.contains("#010203"), "{text}");
		let l = config::Look::load(&path).unwrap();
		assert_eq!((l.width, l.border), (300, 2));
		assert!(!dlg.dirty());
		// the file it wrote is one it reads back the same
		let back = Dialog::open(path.clone()).unwrap();
		assert_eq!(back.look(), dlg.look());
	}

	// Test ID: der5j
	#[test]
	#[cfg_attr(miri, ignore = "the save fsyncs its folder, which Miri can't")]
	fn apply_keeps_the_comments_a_hand_edit_put_there() {
		let d = scratch("notes");
		let path = d.0.join("t.shcl");
		let mine =
			"# mine\ngraph:\n\twidth: 300  # why\n\tborder: 5  # thin\n\twidht: 120  # typo\n";
		std::fs::write(&path, mine).unwrap();
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert!(dlg.set_text("border", "2"));
		dlg.apply().unwrap();
		let text = std::fs::read_to_string(&path).unwrap();
		assert!(text.starts_with(HEADER), "{text}");
		for kept in ["# mine\n", "width: 300  # why\n"] {
			assert!(text.contains(kept), "{kept:?} in {text}");
		}
		for gone in ["widht", "typo", "border: 5"] {
			assert!(!text.contains(gone), "{gone} in {text}");
		}
		let l = config::Look::load(&path).unwrap();
		assert_eq!((l.width, l.border), (300, 2));
		// a second save keeps them too, and the header only once
		assert!(dlg.set_text("border", "3"));
		dlg.apply().unwrap();
		let again = std::fs::read_to_string(&path).unwrap();
		assert!(
			again.contains("# mine\n") && again.contains("# why"),
			"{again}"
		);
		assert_eq!(again.matches(HEADER).count(), 1, "{again}");
	}

	// Test ID: debjg
	#[test]
	fn bad_input_changes_nothing() {
		let d = scratch("bad");
		let mut dlg = Dialog::open(d.0.join("t.shcl")).unwrap();
		assert!(!dlg.set_text("width", "huge"));
		assert!(!dlg.set_text("width", "5000"));
		assert!(!dlg.set_text("user", "#12"));
		assert!(!dlg.set_text("nope", "1"));
		assert!(!dlg.dirty());
	}

	// Test ID: decb5
	#[test]
	fn the_box_takes_more_than_the_slider_reaches() {
		let d = scratch("reach");
		let mut dlg = Dialog::open(d.0.join("t.shcl")).unwrap();
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

	// Test ID: debjh
	#[test]
	fn sliders_snap_to_detents_and_gates_turn_on() {
		let d = scratch("slider");
		let mut dlg = Dialog::open(d.0.join("t.shcl")).unwrap();
		let s = dlg.slider("width").unwrap().clone();
		dlg.set_slider("width", s.to_t(130.0));
		assert_eq!(dlg.state("width").number, 128.0);
		assert!(dlg.set_text("keep_history", "off"));
		assert!(!dlg.state("max_days").counts);
		// changing a row under a switch turns the switch back on
		assert!(dlg.set_text("max_days", "20"));
		assert!(dlg.state("keep_history").on);
	}

	// Test ID: deccm
	#[test]
	#[cfg_attr(miri, ignore = "the save fsyncs its folder, which Miri can't")]
	fn a_theme_and_the_changes_on_it_are_saved() {
		let d = scratch("theme");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert_eq!(dlg.state("theme").choice, "Default");
		assert!(dlg.state("width").choice.is_empty());
		assert!(dlg.set_text("theme", "neon"));
		assert_eq!(dlg.state("user").text, "#39ff14");
		assert!(dlg.set_text("user", "#111111"));
		assert_eq!(dlg.state("theme").choice, "Neon *");
		dlg.apply().unwrap();
		let dlg = Dialog::open(path.clone()).unwrap();
		assert_eq!(dlg.state("theme").choice, "Neon *");
		assert_eq!(dlg.state("user").text, "#111111");
		assert_eq!(dlg.state("nice").text, "#1f6a10");
	}

	// Test ID: decdn
	#[test]
	#[cfg_attr(miri, ignore = "the save fsyncs its folder, which Miri can't")]
	fn the_mode_swaps_the_rows_and_fire_is_a_gradient() {
		let d = scratch("mode");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone()).unwrap();
		let shown = |dlg: &Dialog, id: &str| dlg.state(id).shown;
		assert_eq!(dlg.state("cpu_mode").text, "detailed");
		for id in ["cpuhead", "user", "iowait"] {
			assert!(shown(&dlg, id), "{id}");
		}
		for id in ["cpugradhead", "cpu_low", "cpu_medium", "cpu_high"] {
			assert!(!shown(&dlg, id), "{id}");
		}
		assert!(shown(&dlg, "back_top") && shown(&dlg, "theme"));
		assert!(dlg.set_text("cpu_mode", "gradient"));
		assert!(shown(&dlg, "cpu_low") && !shown(&dlg, "user"));
		assert_eq!(dlg.state("theme").choice, "Default *");
		assert!(!dlg.set_text("cpu_mode", "plaid"));
		// Fire picks the mode with its colors, and the revert arrow takes it back
		assert!(dlg.set_text("theme", "fire"));
		assert_eq!(dlg.state("theme").choice, "Fire");
		assert_eq!(dlg.state("cpu_high").text, "#7fd4ff");
		dlg.apply().unwrap();
		let l = config::Look::load(&path).unwrap();
		assert_eq!(l[Which::Cpu].mode, config::Mode::Gradient);
		assert_eq!(l[Which::Cpu].gradient[0], config::Rgb(0x8b, 0x1a, 0));
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert!(shown(&dlg, "cpu_medium"));
		// a theme saved from it keeps the mode
		dlg.save_theme_as("Embers").unwrap();
		dlg.reset("theme");
		assert!(!shown(&dlg, "cpu_medium"));
		assert!(dlg.set_text("theme", "embers"));
		assert!(shown(&dlg, "cpu_medium"));
		dlg.apply().unwrap();
		assert_eq!(
			config::Look::load(&path).unwrap()[Which::Cpu].mode,
			config::Mode::Gradient
		);
		let text = std::fs::read_to_string(&path).unwrap();
		assert!(text.contains("cpu_mode: gradient"), "{text}");
	}

	// Test ID: dece0
	#[test]
	#[cfg_attr(miri, ignore = "the save fsyncs its folder, which Miri can't")]
	fn transparent_stays_through_a_theme_pick_and_a_roll() {
		let d = scratch("clear");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert!(!dlg.state("transparent").on);
		assert!(dlg.set_text("transparent", "on"));
		assert!(dlg.set_text("theme", "fire"));
		dlg.randomize_from(7);
		assert!(dlg.set_text("theme", "pastel"));
		assert!(dlg.state("transparent").on);
		assert_eq!(dlg.state("theme").choice, "Pastel");
		dlg.apply().unwrap();
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert!(dlg.state("transparent").on);
		assert!(config::Look::load(&path).unwrap().transparent);
		// Save as doesn't take it into the theme
		dlg.save_theme_as("Clear").unwrap();
		dlg.apply().unwrap();
		let text = std::fs::read_to_string(&path).unwrap();
		let themes = &text[text.find("themes:").unwrap()..];
		assert!(!themes.contains("transparent"), "{text}");
	}

	// Test ID: dejkz
	#[test]
	#[cfg_attr(miri, ignore = "the save fsyncs its folder, which Miri can't")]
	fn a_shared_color_ticks_its_override_and_a_roll_leaves_both() {
		let d = scratch("overrides");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone()).unwrap();
		// Default has both on
		assert!(dlg.state("border_color").counts && dlg.state("back_top").counts);
		assert!(dlg.set_text("border_all", "off"));
		assert!(dlg.set_text("back_all", "off"));
		assert!(!dlg.state("border_color").counts);
		assert!(!dlg.state("back_top").counts);
		dlg.randomize_from(42);
		assert_eq!(dlg.state("border_all").text, "off");
		assert_eq!(dlg.state("back_all").text, "off");
		assert_eq!(
			dlg.state("cpu_back_top").text,
			crate::dice::theme(42)
				.into_iter()
				.find(|(id, _)| *id == "cpu_back_top")
				.unwrap()
				.1
		);
		// picking a shared color is asking for it everywhere
		assert!(dlg.set_text("back_top", "#123456"));
		assert_eq!(dlg.state("back_all").text, "on");
		assert_eq!(dlg.state("border_all").text, "off");
		dlg.randomize_from(43);
		assert_eq!(dlg.state("back_all").text, "on");
		assert_eq!(dlg.state("border_all").text, "off");
		// unticked before the rolled colors, and still off once saved
		dlg.apply().unwrap();
		let l = config::Look::load(&path).unwrap();
		assert_eq!(l[Which::Mem].outline.top, l.top);
		assert_eq!(l[Which::Mem].outline.bottom, l.bottom);
		assert_ne!(l[Which::Mem].outline.border, l.border_color);
		// a theme pick takes the theme's, which is off
		assert!(dlg.set_text("theme", "fire"));
		assert_eq!(dlg.state("back_all").text, "off");
	}

	// Test ID: dejng
	#[test]
	#[cfg_attr(miri, ignore = "the save fsyncs its folder, which Miri can't")]
	fn unticked_overrides_give_every_graph_its_own_back_after_a_reload() {
		let d = scratch("untick");
		let grey = config::Look::default()[Which::Mem].outline;
		let mine = config::Rgb(0xaa, 0, 0);
		// Default keeps a change, Custom its own set
		for (n, start) in ["theme:\n\tname: default\n", ""].into_iter().enumerate() {
			let path = d.0.join(format!("t{n}.shcl"));
			std::fs::write(&path, start).unwrap();
			let mut dlg = Dialog::open(path.clone()).unwrap();
			assert!(dlg.set_text("border_color", "#123456"));
			assert!(dlg.set_text("back_top", "#654321"));
			assert!(dlg.set_text("cpu_border_color", "#aa0000"));
			dlg.apply().unwrap();
			let l = config::Look::load(&path).unwrap();
			assert_eq!(
				l[Which::Mem].outline.border,
				config::Rgb(0x12, 0x34, 0x56),
				"{n}"
			);
			assert_eq!(
				l[Which::Cpu].outline.border,
				config::Rgb(0x12, 0x34, 0x56),
				"{n}"
			);
			assert_eq!(
				l[Which::Own].outline.top,
				config::Rgb(0x65, 0x43, 0x21),
				"{n}"
			);
			assert!(dlg.set_text("border_all", "off"));
			assert!(dlg.set_text("back_all", "off"));
			dlg.apply().unwrap();
			let l = config::Look::load(&path).unwrap();
			assert_eq!(
				l[Which::Cpu].outline,
				config::Outline {
					border: mine,
					..grey
				},
				"{n}"
			);
			assert_eq!(
				[l[Which::Mem].outline, l[Which::Own].outline],
				[grey; 2],
				"{n}"
			);
			// and again through a window opened on that file
			let mut dlg = Dialog::open(path.clone()).unwrap();
			assert_eq!(dlg.state("border_color").text, "#123456", "{n}");
			assert!(dlg.set_text("border", "2"));
			dlg.apply().unwrap();
			let l = config::Look::load(&path).unwrap();
			assert_eq!(
				[l[Which::Mem].outline, l[Which::Disk].outline],
				[grey; 2],
				"{n}"
			);
			assert_eq!(l[Which::Cpu].outline.border, mine, "{n}");
		}
		// a file from before each graph had its own still saves, and keeps its look
		let path = d.0.join("old.shcl");
		std::fs::write(&path, "graph:\n\tcolors:\n\t\tborder: \"#123456\"\n").unwrap();
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert!(dlg.set_text("border", "2"));
		dlg.apply().unwrap();
		let text = std::fs::read_to_string(&path).unwrap();
		let doc = shcl::Document::parse(&text);
		assert!(!doc.exists("graph.colors.border"), "{text}");
		assert!(doc.exists("graph.overrides.colors.border"), "{text}");
		let l = config::Look::load(&path).unwrap();
		assert_eq!(l[Which::Net].outline.border, config::Rgb(0x12, 0x34, 0x56));
		assert_eq!(l.border_color, config::Rgb(0x12, 0x34, 0x56));
	}

	// Test ID: der3r
	#[test]
	fn the_old_disk_log_scale_saves_as_the_disk_graphs_own() {
		let d = scratch("disk_log");
		let path = d.0.join("old.shcl");
		std::fs::write(&path, "disk:\n\tlog: true\n").unwrap();
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert_eq!(dlg.state("disk_vertical").text, "log10");
		assert!(dlg.set_text("border", "2"));
		dlg.apply().unwrap();
		let text = std::fs::read_to_string(&path).unwrap();
		let doc = shcl::Document::parse(&text);
		assert!(!doc.exists("disk.log"), "{text}");
		assert_eq!(doc.get_string("disk.vertical").unwrap(), "log10", "{text}");
		let l = config::Look::load(&path).unwrap();
		assert_eq!(
			l[Which::Disk].vertical,
			Some(tukzmon_core::render::Vertical::Log10)
		);
	}

	// Test ID: dejjv
	#[test]
	fn the_margins_go_with_the_theme_and_a_roll_leaves_them() {
		let d = scratch("margins");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert!(dlg.set_text("theme", "neon"));
		assert!(dlg.set_text("margin_lr", "5"));
		assert_eq!(dlg.state("theme").choice, "Neon *");
		dlg.randomize_from(42);
		assert_eq!(dlg.state("margin_lr").text, "5");
		assert_eq!(dlg.state("margin_tb").text, "2");
		// a saved theme keeps it, as a number, and the panel reads it
		dlg.save_theme_as("Wide").unwrap();
		dlg.apply().unwrap();
		let text = std::fs::read_to_string(&path).unwrap();
		assert!(text.contains("margin_lr: 5\n"), "{text}");
		assert_eq!(config::Look::load(&path).unwrap().margin_lr, 5);
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert_eq!(dlg.state("theme").choice, "Wide");
		assert_eq!(dlg.state("margin_lr").text, "5");
		assert!(dlg.set_text("theme", "fire"));
		assert_eq!(dlg.state("margin_lr").text, "2");
	}

	// Test ID: decdq
	#[test]
	#[cfg_attr(miri, ignore = "the save fsyncs its folder, which Miri can't")]
	fn randomize_changes_every_color_on_the_theme_and_turns_off_the_gradient() {
		let d = scratch("dice");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert!(dlg.set_text("theme", "fire"));
		dlg.randomize_from(42);
		assert_eq!(dlg.state("theme").choice, "Fire *");
		assert_eq!(dlg.state("cpu_mode").text, "detailed");
		assert!(dlg.state("user").shown && !dlg.state("cpu_high").shown);
		let rolled = crate::dice::theme(42);
		for (id, v) in &rolled {
			assert_eq!(&dlg.state(id).text, v, "{id}");
		}
		// another roll, then Save as keeps that one whole
		let first_user = dlg.state("user").text;
		dlg.randomize_from(43);
		assert_ne!(dlg.state("user").text, first_user);
		dlg.save_theme_as("Lucky").unwrap();
		dlg.apply().unwrap();
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert_eq!(dlg.state("theme").choice, "Lucky");
		for (id, v) in crate::dice::theme(43) {
			assert_eq!(dlg.state(id).text, v, "{id}");
		}
		// picking a theme drops a roll
		dlg.randomize_from(44);
		assert!(dlg.set_text("theme", "default"));
		assert_eq!(dlg.state("user").text, "#00ff80");
		assert_eq!(dlg.state("theme").choice, "Default");
	}

	// Test ID: deccn
	#[test]
	#[cfg_attr(miri, ignore = "the save fsyncs its folder, which Miri can't")]
	fn custom_then_the_revert_arrow_is_default_again() {
		let d = scratch("custom");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert!(dlg.set_text("theme", "custom"));
		assert!(dlg.set_text("user", "#123456"));
		dlg.apply().unwrap();
		assert_eq!(
			config::Look::load(&path).unwrap()[Which::Cpu].colors[2],
			config::Rgb(0x12, 0x34, 0x56)
		);
		dlg.reset("theme");
		dlg.apply().unwrap();
		let dlg = Dialog::open(path.clone()).unwrap();
		assert_eq!(dlg.state("theme").choice, "Default");
		assert_eq!(dlg.state("user").text, "#00ff80");
		assert!(!dlg.dirty());
	}

	// Test ID: deqqp
	#[test]
	#[cfg_attr(miri, ignore = "the save fsyncs its folder, which Miri can't")]
	fn custom_keeps_its_edit_when_another_theme_is_picked_before_apply() {
		let d = scratch("custom_left");
		let path = d.0.join("t.shcl");
		let file = "theme:\n\tname: custom\ncpu:\n\tcolors:\n\t\tuser: \"#112233\"\n\t\tnice: \"#445566\"\n";
		std::fs::write(&path, file).unwrap();
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert_eq!(dlg.state("theme").choice, "Custom");
		assert!(dlg.set_text("user", "#abcdef"));
		assert!(dlg.set_text("theme", "default"));
		assert!(dlg.set_text("theme", "custom"));
		assert_eq!(dlg.state("user").text, "#abcdef", "as the window shows it");
		assert!(dlg.set_text("theme", "default"));
		dlg.apply().unwrap();
		assert_eq!(dlg.state("theme").choice, "Default");
		for dlg in [&mut dlg, &mut Dialog::open(path.clone()).unwrap()] {
			assert!(dlg.set_text("theme", "custom"));
			assert_eq!(dlg.state("user").text, "#abcdef");
			assert_eq!(dlg.state("nice").text, "#445566", "untouched");
		}
		// a roll on Custom is kept the same way
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert!(dlg.set_text("theme", "custom"));
		dlg.randomize_from(7);
		let rolled = crate::dice::theme(7);
		assert!(dlg.set_text("theme", "default"));
		dlg.apply().unwrap();
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert!(dlg.set_text("theme", "custom"));
		for (id, v) in &rolled {
			assert_eq!(&dlg.state(id).text, v, "{id}");
		}
		// and a hand edit made meanwhile to a row the window didn't change stays
		std::fs::write(&path, file).unwrap();
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert!(dlg.set_text("user", "#fedcba"));
		std::fs::write(&path, format!("{file}graph:\n\twidth: 300\n")).unwrap();
		assert!(dlg.set_text("theme", "default"));
		dlg.apply().unwrap();
		assert_eq!(config::Look::load(&path).unwrap().width, 300);
		let mut dlg = Dialog::open(path).unwrap();
		assert!(dlg.set_text("theme", "custom"));
		assert_eq!(dlg.state("user").text, "#fedcba");
	}

	// Test ID: deccy
	#[test]
	#[cfg_attr(miri, ignore = "the save fsyncs its folder, which Miri can't")]
	fn save_as_keeps_the_colors_whole_and_picks_the_new_theme() {
		let d = scratch("save_as");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert_eq!(dlg.theme_buttons(), ThemeButtons::default());
		assert!(dlg.set_text("theme", "neon"));
		assert!(dlg.set_text("user", "#111111"));
		assert!(dlg.save_theme_as("  ").is_err());
		assert!(dlg.save_theme_as("nEon").is_err());
		assert!(dlg.save_theme_as("Custom").is_err());
		dlg.save_theme_as(" Night ").unwrap();
		assert_eq!(dlg.state("theme").choice, "Night");
		assert_eq!(dlg.state("user").text, "#111111");
		assert_eq!(dlg.state("nice").text, "#1f6a10");
		assert!(!dlg.state("user").can_reset);
		assert_eq!(
			dlg.state("theme")
				.options
				.iter()
				.map(|(_, l)| l.as_str())
				.nth(9),
			Some("Night")
		);
		let b = dlg.theme_buttons();
		assert!(!b.save && b.rename && b.delete);
		assert!(dlg.save_theme_as("night").is_err());
		dlg.apply().unwrap();
		let text = std::fs::read_to_string(&path).unwrap();
		assert!(text.contains("name: night"), "{text}");
		assert_eq!(text.matches("\"#").count(), 110, "{text}");
		assert!(!text.contains("kept"), "{text}");
		let dlg = Dialog::open(path.clone()).unwrap();
		assert_eq!(dlg.state("theme").choice, "Night");
		assert!(!dlg.dirty());
		assert_eq!(
			config::Look::load(&path).unwrap()[Which::Cpu].colors[2],
			config::Rgb(0x11, 0x11, 0x11)
		);
	}

	// Test ID: deccz
	#[test]
	#[cfg_attr(miri, ignore = "the save fsyncs its folder, which Miri can't")]
	fn save_puts_the_changes_into_the_theme() {
		let d = scratch("save");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone()).unwrap();
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
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert!(dlg.set_text("theme", "default"));
		assert!(dlg.set_text("theme", "mine"));
		assert_eq!(dlg.state("user").text, "#222222");
	}

	// Test ID: decd0
	#[test]
	#[cfg_attr(miri, ignore = "the save fsyncs its folder, which Miri can't")]
	fn rename_and_delete() {
		let d = scratch("rename");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone()).unwrap();
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
		let dlg = Dialog::open(path.clone()).unwrap();
		assert_eq!(dlg.state("theme").choice, "Default");
		assert_eq!(dlg.state("theme").options.len(), 11);
	}

	// Test ID: decd1
	#[test]
	#[cfg_attr(miri, ignore = "the save fsyncs its folder, which Miri can't")]
	fn a_theme_saved_by_hand_meanwhile_survives() {
		let d = scratch("merge");
		let path = d.0.join("t.shcl");
		std::fs::write(&path, "themes:\n\tgone:\n\t\tuser: \"#010101\"\n").unwrap();
		let mut dlg = Dialog::open(path.clone()).unwrap();
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
		assert_eq!(dlg.state("theme").options.len(), 12);
	}

	// Test ID: der2h
	#[test]
	#[cfg_attr(miri, ignore = "the save fsyncs its folder, which Miri can't")]
	fn two_windows_saving_one_new_name_keep_both() {
		let d = scratch("two_windows");
		let path = d.0.join("t.shcl");
		let mut first = Dialog::open(path.clone()).unwrap();
		let mut second = Dialog::open(path.clone()).unwrap();
		first.save_theme_as("Mine").unwrap();
		first.apply().unwrap();
		let firsts = first.themes[0].clone();
		second.randomize_from(7);
		second.save_theme_as("mine").unwrap();
		second.apply().unwrap();
		let doc = shcl::Document::parse(&std::fs::read_to_string(&path).unwrap());
		let saved = config::saved_themes(&doc);
		assert_eq!(saved.len(), 2, "{saved:?}");
		assert!(saved.contains(&firsts), "{saved:?}");
		let seconds = saved.iter().find(|t| t.key != firsts.key).unwrap();
		assert_eq!(seconds.name, "mine 2");
		assert_ne!(seconds.colors, firsts.colors);
		assert_eq!(second.state("theme").choice, "mine 2");
		let reread = Dialog::open(path).unwrap();
		assert_eq!(reread.state("theme").choice, "mine 2");
	}

	// Out since 2026-10-29: a theme line it can't read is dropped rather than
	// stopping the save, per backlog 2026102913122701. dekm0 checks it now.
	// // Test ID: decd2
	// #[test]
	// fn a_theme_line_it_cant_read_stops_the_save() {
	// 	let d = scratch("bad_theme");
	// 	let path = d.0.join("t.shcl");
	// 	let text = "themes:\n\tmine:\n\t\tuser: blue\n";
	// 	std::fs::write(&path, text).unwrap();
	// 	let mut dlg = Dialog::open(path.clone()).unwrap();
	// 	assert!(dlg.set_text("border", "2"));
	// 	let e = dlg.apply().unwrap_err();
	// 	assert!(e.contains("themes.mine.user"), "{e}");
	// 	assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
	// }

	// Test ID: decbz
	#[test]
	#[cfg_attr(miri, ignore = "the save fsyncs its folder, which Miri can't")]
	fn the_time_axis_default_is_a_dropdown_with_linear_first() {
		let d = scratch("curve");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert_eq!(dlg.state("time_axis").text, "smooth");
		assert!(!dlg.set_text("time_axis", "squiggly"));
		assert!(dlg.set_text("time_axis", "linear"));
		// spike emphasis still counts, for a graph with a log axis of its own
		assert!(dlg.state("spikes").counts);
		dlg.apply().unwrap();
		let text = std::fs::read_to_string(&path).unwrap();
		assert!(text.contains("time: linear"), "{text}");
	}

	// Test ID: decd3
	#[test]
	fn the_picker_moves_its_row_and_cancel_puts_it_all_back() {
		let d = scratch("pick_cancel");
		let mut dlg = Dialog::open(d.0.join("t.shcl")).unwrap();
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
		assert_eq!(
			dlg.model.values, dlg.saved.values,
			"a save would still write the row"
		);
		// and with it closed, nothing moves the row
		dlg.pick_set(Hsv {
			s: 0.0,
			v: 1.0,
			..start
		});
		assert_eq!(dlg.state("user").text, was);
	}

	// Test ID: decd4
	#[test]
	#[cfg_attr(miri, ignore = "the save fsyncs its folder, which Miri can't")]
	fn the_picker_keeps_what_ok_was_pressed_on() {
		let d = scratch("pick_ok");
		let path = d.0.join("t.shcl");
		let mut dlg = Dialog::open(path.clone()).unwrap();
		assert!(dlg.pick_open("nice"));
		assert!(
			!dlg.pick_type(Field::Hex, "#33"),
			"half-typed changes nothing"
		);
		assert!(dlg.pick_type(Field::Hex, "#3366cc"));
		let hsv = dlg.pick_hsv().unwrap();
		assert_eq!(Field::Red.percent(hsv), Some(20));
		assert_eq!(Field::Blue.percent(hsv), Some(80));
		assert!(dlg.pick_type(Field::Red, "100"));
		assert_eq!(dlg.state("nice").text, "#ff66cc");
		dlg.pick_close(true);
		assert!(dlg.pick_hsv().is_none());
		assert_eq!(dlg.state("nice").text, "#ff66cc");
		dlg.apply().unwrap();
		let dlg = Dialog::open(path).unwrap();
		assert_eq!(dlg.state("nice").text, "#ff66cc");
		assert_eq!(dlg.state("theme").choice, "Default *");
	}
}
