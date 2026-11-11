// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! Settings, as knobs reads them from `settings.shcl` and the config file.

use std::collections::BTreeMap;
use std::ops::{Index, IndexMut};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use knobs::{Model, NoEnv, Spec};

use crate::axis::{Curve, Timing};
use crate::disk::Drives;
use crate::remote::Session;
use crate::render::{Paint, Vertical};
use crate::when::Said;

/// The settings spec, in shcl.
pub const SPEC_TEXT: &str = include_str!("settings.shcl");

/// The parsed spec. A test proves it parses, so a bad one never gets built.
pub fn spec() -> Arc<Spec> {
	static SPEC: OnceLock<Arc<Spec>> = OnceLock::new();
	Arc::clone(SPEC.get_or_init(|| {
		#[allow(clippy::expect_used, reason = "compiled in, and parse_spec checks it")]
		Arc::new(Spec::parse(SPEC_TEXT).expect("settings.shcl"))
	}))
}

fn slider_ranges() -> &'static [(String, f64, f64)] {
	static RANGES: OnceLock<Vec<(String, f64, f64)>> = OnceLock::new();
	RANGES.get_or_init(|| {
		let doc = shcl::Document::parse(SPEC_TEXT);
		doc.children("settings")
			.into_iter()
			.filter_map(|id| {
				let at = format!("settings.{id}.slider_range");
				let v = doc.get_float_array(&at).or_else(|_| {
					doc.get_int_array(&at)
						.map(|v| v.into_iter().map(|i| i as f64).collect())
				});
				match v.ok()?.as_slice() {
					[lo, hi] => Some((id, *lo, *hi)),
					_ => None,
				}
			})
			.collect()
	})
}

/// How far a slider reaches, where `slider_range` in settings.shcl makes it
/// shorter than the range the number box beside it takes. knobs has no such
/// key, so it's read here.
pub fn slider_range(id: &str) -> Option<(f64, f64)> {
	slider_ranges()
		.iter()
		.find(|(k, ..)| k == id)
		.map(|&(_, lo, hi)| (lo, hi))
}

fn row_texts(key: &str) -> Vec<(String, String)> {
	let doc = shcl::Document::parse(SPEC_TEXT);
	doc.children("settings")
		.into_iter()
		.filter_map(|id| {
			let v = doc.get_string(&format!("settings.{id}.{key}")).ok()?;
			Some((id, v))
		})
		.collect()
}

/// Rows with `shown_as: radio`, whose dropdown is drawn as radio buttons.
pub fn shown_as_radio(id: &str) -> bool {
	static RADIOS: OnceLock<Vec<(String, String)>> = OnceLock::new();
	RADIOS
		.get_or_init(|| row_texts("shown_as"))
		.iter()
		.any(|(k, v)| k == id && v == "radio")
}

/// The graph whose section a heading starts, from `graph: name`.
pub fn section_graph(id: &str) -> Option<&'static str> {
	static GRAPHS: OnceLock<Vec<(String, String)>> = OnceLock::new();
	GRAPHS
		.get_or_init(|| row_texts("graph"))
		.iter()
		.find(|(k, _)| k == id)
		.map(|(_, g)| g.as_str())
}

fn rows_with(key: &str) -> Vec<String> {
	let doc = shcl::Document::parse(SPEC_TEXT);
	doc.children("settings")
		.into_iter()
		.filter(|row| doc.get_bool_or(&format!("settings.{row}.{key}"), false))
		.collect()
}

/// Headings with `beside: true`, which sit with the rest of their section to
/// the right of what's above them.
pub fn beside(id: &str) -> bool {
	static BESIDE: OnceLock<Vec<String>> = OnceLock::new();
	BESIDE
		.get_or_init(|| rows_with("beside"))
		.iter()
		.any(|row| row == id)
}

/// Rows with `same_line: true`, which sit to the right of the row above them,
/// on its line.
pub fn same_line(id: &str) -> bool {
	static SAME: OnceLock<Vec<String>> = OnceLock::new();
	SAME.get_or_init(|| rows_with("same_line"))
		.iter()
		.any(|row| row == id)
}

/// The dropdown and key a row is shown for, from `shown_when: "id = key"`.
/// knobs' gates only take a checkbox, so this is read here.
pub fn shown_when(id: &str) -> Option<(&'static str, &'static str)> {
	static WHEN: OnceLock<Vec<(String, String, String)>> = OnceLock::new();
	WHEN.get_or_init(|| {
		row_texts("shown_when")
			.into_iter()
			.filter_map(|(row, v)| {
				let (on, key) = v.split_once('=')?;
				Some((row, on.trim().to_string(), key.trim().to_string()))
			})
			.collect()
	})
	.iter()
	.find(|(row, ..)| row == id)
	.map(|(_, on, key)| (on.as_str(), key.as_str()))
}

// knobs drops a key it doesn't know with no word, so a typo such as `gaet:`
// is caught here instead.
const KNOBS_ROW_KEYS: &[&str] = &[
	"auto",
	"control",
	"default",
	"detent_labels",
	"detents",
	"gate",
	"group",
	"indent",
	"label",
	"option_labels",
	"options",
	"path",
	"range",
	"rule",
	"scale",
	"store",
	"tab",
	"tip",
	"type",
	"whole",
];
const OUR_ROW_KEYS: &[&str] = &[
	"beside",
	"graph",
	"same_line",
	"shown_as",
	"shown_when",
	"slider_range",
];
const KNOBS_GROUP_KEYS: &[&str] = &["chooser", "custom", "from_program", "noun", "presets"];
const KNOBS_PRESET_WORDS: &[&str] = &["label", "temporary"];
const TOP_KEYS: &[&str] = &["groups", "settings", "tabs"];

/// Every key in a spec that neither knobs nor tukzmon reads, as its path.
pub fn unknown_spec_keys(text: &str) -> Vec<String> {
	let doc = shcl::Document::parse(text);
	let mut odd = Vec::new();
	let mut check = |at: String, known: &[&[&str]]| {
		for key in doc.children(&at) {
			if !known.iter().any(|k| k.contains(&key.as_str())) {
				odd.push(if at.is_empty() {
					key
				} else {
					format!("{at}.{key}")
				});
			}
		}
	};
	check(String::new(), &[TOP_KEYS]);
	for row in doc.children("settings") {
		check(format!("settings.{row}"), &[KNOBS_ROW_KEYS, OUR_ROW_KEYS]);
	}
	let rows = doc.children("settings");
	let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
	for group in doc.children("groups") {
		check(format!("groups.{group}"), &[KNOBS_GROUP_KEYS]);
		for preset in doc.children(&format!("groups.{group}.presets")) {
			check(
				format!("groups.{group}.presets.{preset}"),
				&[KNOBS_PRESET_WORDS, &rows],
			);
		}
	}
	odd
}

/// How a graph's columns are colored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
	/// A color per series, stacked.
	#[default]
	Detailed,
	/// Low, Medium and High, blended by height.
	Gradient,
}

impl Mode {
	/// From the settings file's value, in any case.
	pub fn parse(s: &str) -> Option<Mode> {
		[("detailed", Mode::Detailed), ("gradient", Mode::Gradient)]
			.into_iter()
			.find_map(|(key, m)| s.eq_ignore_ascii_case(key).then_some(m))
	}
}

/// How the network graph tells the LAN from the internet. Off draws local,
/// sent and received. See design.md, Data sources on Linux.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Split {
	/// No split.
	#[default]
	Off,
	/// Polls every TCP socket, with no root. UDP, QUIC among it, counts as
	/// internet.
	Sockets,
}

impl Split {
	/// From the settings file's value, in any case.
	pub fn parse(s: &str) -> Option<Split> {
		[("off", Split::Off), ("sockets", Split::Sockets)]
			.into_iter()
			.find_map(|(key, v)| s.eq_ignore_ascii_case(key).then_some(v))
	}
}

/// Whether graphs slide between samples. `animate` in settings.shcl, and
/// design.md Drawing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Motion {
	/// On with 8 or more CPUs, since it draws up to 60 times a second.
	#[default]
	Auto,
	/// Always slides.
	On,
	/// A whole step at each sample.
	Off,
}

impl Motion {
	/// From the settings file's value, in any case.
	pub fn parse(s: &str) -> Option<Motion> {
		[
			("auto", Motion::Auto),
			("on", Motion::On),
			("off", Motion::Off),
		]
		.into_iter()
		.find_map(|(key, v)| s.eq_ignore_ascii_case(key).then_some(v))
	}

	/// True when graphs slide on a box with `cpus` CPUs.
	pub fn slides(self, cpus: f32) -> bool {
		match self {
			Motion::Auto => cpus >= 8.0,
			Motion::On => true,
			Motion::Off => false,
		}
	}
}

/// No graph is read more often than this while [`Look::calmed`].
pub const CALM_MS: u32 = 1000;

/// True for a setting [`Look::calmed`] stands in for: Sample ms default,
/// each graph's sample %, and Smooth scrolling. The settings window greys
/// them and says why.
pub fn calmed_id(id: &str) -> bool {
	id == "sample_ms" || id == "animate" || Which::ALL.iter().any(|w| w.ids().sample == id)
}

/// The setting that turns on [`Look::calmed`].
pub const CALM_REMOTE: &str = "calm_remote";

/// The group every color is in, and the id of the dropdown that picks one.
pub const THEME: &str = "theme";

fn unthemed(m: &Model) -> bool {
	m.spec.group(THEME).is_some_and(|g| {
		!m.values.own.contains_key(&g.chooser)
			&& g.members.iter().any(|id| m.values.own.contains_key(id))
	})
}

/// Colors in a file with no theme line are from before themes, or a hand
/// edit, so they read as Custom rather than hide under Default.
/// [`name_the_theme`] keeps that from misreading a file saved here.
pub fn unthemed_is_custom(m: &mut Model) {
	if unthemed(m) {
		m.set(THEME, &knobs::Value::Text(knobs::CUSTOM.into()), &NoEnv);
	}
}

/// Before a save: knobs leaves out a theme pick that is the default, but
/// still writes the Custom colors, which would read back as Custom.
pub fn name_the_theme(m: &mut Model) {
	if unthemed(m) {
		let on = m.stored_choice(THEME, &NoEnv);
		m.values.own.insert(THEME.into(), knobs::Value::Text(on));
	}
}

/// A theme saved in the window, kept whole in the config file under
/// `themes.<key>`, with every color named by its setting id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedTheme {
	/// Its key under `themes.`.
	pub key: String,
	/// The name shown in the window.
	pub name: String,
	/// Setting id to `#rrggbb`, one per theme color, plus each graph's mode,
	/// the margins and the 2 override checkboxes, as text.
	pub colors: BTreeMap<String, String>,
}

/// A theme member's value as [`SavedTheme`] keeps it. The margins are
/// numbers, the override checkboxes `on` or `off` as the window shows them,
/// and the rest text.
pub fn theme_text(v: &knobs::Value) -> String {
	match v {
		knobs::Value::Text(t) => t.clone(),
		knobs::Value::Bool(_) | knobs::Value::Int(_) | knobs::Value::Float(_) => v.show(),
	}
}

// a theme member's text as the spec stores it, or None if it won't take it
fn theme_typed(spec: &Spec, id: &str, raw: &str) -> Option<knobs::Value> {
	let s = spec.get(id)?;
	let v = match s.kind {
		// knobs' range check passes NaN (2026103115231954)
		knobs::Kind::Int | knobs::Kind::Float => {
			knobs::Value::Float(raw.trim().parse().ok().filter(|x: &f64| x.is_finite())?)
		}
		knobs::Kind::Bool => match raw.trim() {
			"true" | "on" => knobs::Value::Bool(true),
			"false" | "off" => knobs::Value::Bool(false),
			_ => return None,
		},
		knobs::Kind::Text | knobs::Kind::Color | knobs::Kind::Choice => {
			knobs::Value::Text(raw.to_string())
		}
	};
	spec.valid(s, &v).ok()
}

fn theme_value(spec: &Spec, id: &str, raw: &str) -> Option<String> {
	theme_typed(spec, id, raw).map(|v| theme_text(&v))
}

/// The config file's section for saved themes.
pub const THEMES: &str = "themes";

fn builtin_key(key: &str) -> bool {
	key.eq_ignore_ascii_case(knobs::CUSTOM)
		|| spec().group(THEME).is_some_and(|g| {
			g.presets
				.listed()
				.iter()
				.any(|p| p.key.eq_ignore_ascii_case(key))
		})
}

/// The saved themes in a config file, in file order. A value that's missing
/// or unreadable is Default's, and a theme with a built-in's key is left out.
pub fn saved_themes(doc: &shcl::Document) -> Vec<SavedTheme> {
	let spec = spec();
	let Some(g) = spec.group(THEME) else {
		return Vec::new();
	};
	doc.children(THEMES)
		.into_iter()
		.filter(|key| !builtin_key(key))
		.map(|key| {
			let at = |k: &str| format!("{THEMES}.{key}.{k}");
			let colors = g
				.members
				.iter()
				// a saved theme owns its ids, apart from the spec's
				.map(|id| {
					let read = |id: &str| {
						doc.get_string(&at(id))
							.ok()
							.and_then(|raw| theme_value(&spec, id, &raw))
					};
					let v = read(id)
						.or_else(|| shared_of(id).and_then(read))
						.or_else(|| spec.get(id).map(|s| theme_text(&s.default)))
						.unwrap_or_default();
					(id.clone(), v)
				})
				.collect();
			let name = doc
				.get_string(&at("name"))
				.ok()
				.map(|n| n.trim().to_string())
				.filter(|n| !n.is_empty())
				// with no name of its own it goes by its key, a copy since both are kept
				.unwrap_or_else(|| key.clone());
			SavedTheme { key, name, colors }
		})
		.collect()
}

/// Lines under `themes` that [`saved_themes`] can't read, so a save would
/// drop or change them.
#[cfg(test)]
pub fn theme_problems(doc: &shcl::Document) -> Vec<String> {
	let spec = spec();
	let members = spec.group(THEME).map_or(&[][..], |g| g.members.as_slice());
	let mut out = Vec::new();
	for key in doc.children(THEMES) {
		let at = format!("{THEMES}.{key}");
		if builtin_key(&key) {
			out.push(format!(
				"line {}: {at} has the name of a built-in theme",
				doc.line(&at)
			));
			continue;
		}
		for field in doc.children(&at) {
			let path = format!("{at}.{field}");
			let fine = if field == "name" {
				doc.get_string(&path).is_ok()
			} else {
				members.contains(&field)
					&& doc
						.get_string(&path)
						.is_ok_and(|raw| theme_value(&spec, &field, &raw).is_some())
			};
			if !fine {
				out.push(format!(
					"line {}: {path} isn't a theme value I can read",
					doc.line(&path)
				));
			}
		}
	}
	out
}

/// The spec with saved themes as presets after the built-in ones, so the
/// Theme dropdown lists them and a pick of one sets its colors.
pub fn spec_with(themes: &[SavedTheme]) -> Arc<Spec> {
	let base = spec();
	if themes.is_empty() {
		return base;
	}
	// a copy, since the one spec() hands out is shared
	let mut s = (*base).clone();
	if let Some(knobs::Presets::Listed(list)) = s
		.groups
		.iter_mut()
		.find(|g| g.id == THEME)
		.map(|g| &mut g.presets)
	{
		for t in themes {
			list.push(knobs::Preset {
				key: t.key.clone(),
				label: t.name.clone(),
				temporary: false,
				values: t
					.colors
					.iter()
					.filter_map(|(id, raw)| Some((id.clone(), theme_typed(&base, id, raw)?)))
					.collect(),
			});
		}
	}
	// the presets are whole and their colors checked, so this can't fail
	s.finish().map_or(base, Arc::new)
}

/// The themes as config file lines, to go after what knobs writes.
pub fn themes_text(themes: &[SavedTheme]) -> String {
	if themes.is_empty() {
		return String::new();
	}
	let mut doc = shcl::Document::new();
	for t in themes {
		let at = |k: &str| format!("{THEMES}.{}.{k}", t.key);
		// keys are from theme_key or a file that parsed, so the paths are good
		let _ = doc.set_string(&at("name"), &t.name);
		for (id, raw) in &t.colors {
			match theme_typed(&spec(), id, raw) {
				Some(knobs::Value::Int(n)) => {
					let _ = doc.set_int(&at(id), n);
				}
				Some(knobs::Value::Bool(b)) => {
					let _ = doc.set_bool(&at(id), b);
				}
				// the rest keep the text they came as, refused ones too
				Some(knobs::Value::Float(_) | knobs::Value::Text(_)) | None => {
					let _ = doc.set_string(&at(id), raw);
				}
			}
		}
	}
	doc.to_canonical()
}

/// A key for a new theme from its name, not a built-in's nor one of `keys`.
pub fn theme_key<'a, I>(name: &str, keys: I) -> String
where
	I: IntoIterator<Item = &'a str>,
	I::IntoIter: Clone,
{
	let keys = keys.into_iter();
	let mut slug = String::new();
	for c in name.chars() {
		if c.is_ascii_alphanumeric() {
			slug.push(c.to_ascii_lowercase());
		} else if !slug.is_empty() && !slug.ends_with('_') {
			slug.push('_');
		}
	}
	let slug = slug.trim_end_matches('_');
	let slug = if slug.is_empty() || slug.starts_with(|c: char| c.is_ascii_digit()) {
		format!("theme_{slug}").trim_end_matches('_').to_string()
	} else {
		slug.to_string()
	};
	let taken = |k: &str| builtin_key(k) || keys.clone().any(|t| t.eq_ignore_ascii_case(k));
	if !taken(&slug) {
		return slug;
	}
	// n themes can take at most n of these
	(2..=keys.clone().count() + 2)
		.map(|n| format!("{slug}_{n}"))
		.find(|k| !taken(k))
		.unwrap_or(slug)
}

// The Outline sub-tab's border and background ids. Each graph's own is
// `<graph>_<id>`.
const SHARED_OUTLINE: [&str; 3] = ["border_color", "back_top", "back_bottom"];

// The Outline sub-tab's border and background ids, each with one graph's.
fn outline_pairs() -> impl Iterator<Item = (&'static str, String)> {
	SHARED_OUTLINE.into_iter().flat_map(|shared| {
		LEADS
			.iter()
			.map(move |(g, _)| (shared, format!("{g}_{shared}")))
	})
}

fn shared_of(id: &str) -> Option<&'static str> {
	SHARED_OUTLINE.into_iter().find(|shared| {
		id.strip_suffix(shared)
			.and_then(|g| g.strip_suffix('_'))
			.is_some_and(|g| LEADS.iter().any(|(k, _)| *k == g))
	})
}

// Where the Outline sub-tab's colors were kept before each graph had its own.
const OLD_SHARED: [(&str, &str); 3] = [
	("graph.colors.border", "border_color"),
	("graph.colors.top", "back_top"),
	("graph.colors.bottom", "back_bottom"),
];

// Old paths moved on load: see shared_colors_upgraded and the disk graph's
// log scale, which became its Vertical scale.
fn upgraded(doc: &shcl::Document) -> Option<shcl::Document> {
	let mut out = shared_colors_upgraded(doc);
	if doc.exists(OLD_DISK_LOG) {
		let out = out.get_or_insert_with(|| doc.clone());
		if doc.get_bool(OLD_DISK_LOG) == Ok(true) && !out.exists("disk.vertical") {
			// a fixed path and value, which always take
			let _ = out.set_string("disk.vertical", "log10");
		}
		out.remove(OLD_DISK_LOG);
	}
	out
}

const OLD_DISK_LOG: &str = "disk.log";

// A file from before each graph had its own border and background gives every
// graph the shared ones, so it keeps its look. The old paths are how it's told
// apart: a save leaves out a graph's colors when they're its theme's, so a
// missing one says nothing. 2026102820512294.
fn shared_colors_upgraded(doc: &shcl::Document) -> Option<shcl::Document> {
	let bases = [String::new(), format!("kept.changes.{THEME}.values.")];
	let old_at = |base: &String| OLD_SHARED.map(|(old, shared)| (format!("{base}{old}"), shared));
	if !bases.iter().flat_map(old_at).any(|(at, _)| doc.exists(&at)) {
		return None;
	}
	let spec = spec();
	let mut out = doc.clone();
	let olds = bases
		.iter()
		.flat_map(|base| old_at(base).map(move |(at, shared)| (base, at, shared)));
	for (base, at, shared) in olds {
		let Ok(v) = doc.get_string(&at) else {
			continue;
		};
		let ids = std::iter::once(shared.to_string()).chain(
			outline_pairs()
				.filter(|(s, _)| *s == shared)
				.map(|(_, own)| own),
		);
		let paths = ids.filter_map(|id| Some(format!("{base}{}", spec.get(&id)?.path)));
		for path in paths {
			// a spec path, and text the file held already, so it takes
			if !out.exists(&path) {
				let _ = out.set_string(&path, &v);
			}
		}
		out.remove(&at);
	}
	Some(out)
}

/// A parsed config file as a model on `spec`, with [`unthemed_is_custom`]
/// applied.
pub fn model_on(spec: Arc<Spec>, doc: &shcl::Document) -> Model {
	let up = upgraded(doc);
	let mut m = Model::load_docs(spec, up.as_ref().unwrap_or(doc), &shcl::Document::new());
	unthemed_is_custom(&mut m);
	m
}

/// A config file's text as a model, saved themes and all.
pub fn model(text: &str) -> Model {
	let doc = shcl::Document::parse(text);
	model_on(spec_with(&saved_themes(&doc)), &doc)
}

/// A color, red, green and blue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
	/// From `#rrggbb`, or None.
	pub fn parse(hex: &str) -> Option<Rgb> {
		let h = hex.strip_prefix('#')?;
		if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
			return None;
		}
		let n = u32::from_str_radix(h, 16).ok()?;
		Some(Rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
	}

	/// Opaque, as cairo's ARGB32 wants it.
	pub fn argb(self) -> u32 {
		0xff00_0000 | (self.0 as u32) << 16 | (self.1 as u32) << 8 | self.2 as u32
	}
}

/// A graph's frame, and the background its plot blends from top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outline {
	/// The frame around the graph.
	pub border: Rgb,
	/// The background at the top.
	pub top: Rgb,
	/// The background at the bottom.
	pub bottom: Rgb,
}

/// Each graph's section and its main series, the color it reads as in
/// Detailed. In Gradient that's its Medium. See design.md Theme.
pub const LEADS: [(&str, &str); 13] = [
	("cpu", "user"),
	("mem", "mem_user"),
	("gpu", "gpu_busy"),
	("vram", "vram_used"),
	("net", "net_received"),
	("swap", "swap_used"),
	("zswap", "zswap_used"),
	("load", "load_used"),
	("disk", "disk_read"),
	("temp", "temp_normal"),
	("battery", "battery_charging"),
	("self", "self_used"),
	("procs", "procs_me"),
];

/// I/O wait and system, which every lead keeps clear of.
pub const ALARMS: [&str; 2] = ["iowait", "system"];

/// A theme's leads, in `LEADS` order, each in the mode the theme draws its
/// graph in. `get` gives a theme member's value by its setting id.
pub fn leads<'a>(get: impl Fn(&str) -> Option<&'a str>) -> Vec<Rgb> {
	LEADS
		.iter()
		.filter_map(|(g, main)| {
			let gradient = get(&format!("{g}_mode")).and_then(Mode::parse) == Some(Mode::Gradient);
			let id = if gradient {
				format!("{g}_medium")
			} else {
				(*main).to_string()
			};
			get(&id).and_then(Rgb::parse)
		})
		.collect()
}

/// Every graph there is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
	/// Processor use.
	Cpu,
	/// Memory use.
	Mem,
	/// GPU use.
	Gpu,
	/// GPU memory use.
	Vram,
	/// Network traffic.
	Net,
	/// Swap use.
	Swap,
	/// zswap's pool.
	Zswap,
	/// Load average.
	Load,
	/// Disk traffic.
	Disk,
	/// Temperatures.
	Temp,
	/// Battery charge.
	Battery,
	/// tukzmon's own CPU and memory.
	Own,
	/// Process counts.
	Procs,
}

impl Which {
	/// The order with nothing saved. Graphs a saved order leaves out go
	/// last, in this order. Declaration order too, so a `Which` is its own
	/// index in [`Look::graphs`].
	pub const ALL: [Which; 13] = [
		Which::Cpu,
		Which::Mem,
		Which::Gpu,
		Which::Vram,
		Which::Net,
		Which::Swap,
		Which::Zswap,
		Which::Load,
		Which::Disk,
		Which::Temp,
		Which::Battery,
		Which::Own,
		Which::Procs,
	];

	/// Its history section and its name in `graph.order`, which its setting
	/// ids start with too, such as `mem_width`.
	pub const fn key(self) -> &'static str {
		match self {
			Which::Cpu => "cpu",
			Which::Mem => "mem",
			Which::Gpu => "gpu",
			Which::Vram => "vram",
			Which::Net => "net",
			Which::Swap => "swap",
			Which::Zswap => "zswap",
			Which::Load => "load",
			Which::Disk => "disk",
			Which::Temp => "temp",
			Which::Battery => "battery",
			Which::Own => "self",
			Which::Procs => "procs",
		}
	}

	/// The graph whose key is `key`, or None.
	pub fn named(key: &str) -> Option<Which> {
		Which::ALL.into_iter().find(|w| w.key() == key)
	}

	/// Its Detailed colors' setting ids, in its reader's `SERIES` order,
	/// bottom first.
	pub const fn color_ids(self) -> &'static [&'static str] {
		match self {
			Which::Cpu => &["iowait", "system", "user", "nice"],
			Which::Mem => &["mem_user", "mem_buffers", "mem_cache"],
			Which::Gpu => &["gpu_busy"],
			Which::Vram => &["vram_used"],
			Which::Net => &[
				"net_local",
				"net_lan_sent",
				"net_lan_received",
				"net_sent",
				"net_received",
			],
			Which::Swap => &["swap_used"],
			Which::Zswap => &["zswap_used"],
			Which::Load => &["load_used"],
			Which::Disk => &["disk_write", "disk_read"],
			Which::Temp => &["temp_normal", "temp_critical"],
			Which::Battery => &[
				"battery_charging",
				"battery_discharging",
				"battery_critical",
			],
			Which::Own => &["self_used"],
			Which::Procs => &["procs_system", "procs_users", "procs_me"],
		}
	}

	fn show_id(self) -> String {
		// saved as that before the others got short keys
		if self == Which::Mem {
			"show_memory".to_string()
		} else {
			format!("show_{}", self.key())
		}
	}

	// Made once, since a Look reads them on every refresh.
	fn ids(self) -> &'static GraphIds {
		static IDS: OnceLock<[GraphIds; Which::ALL.len()]> = OnceLock::new();
		&IDS.get_or_init(|| Which::ALL.map(GraphIds::of))[self as usize]
	}
}

// A graph's setting ids, but its Detailed colors, which are `color_ids`.
#[derive(Debug)]
struct GraphIds {
	show: String,
	width: String,
	sample: String,
	time: String,
	vertical: String,
	mode: String,
	gradient: [String; 3],
	// Border, Top and Bottom, in SHARED_OUTLINE order.
	outline: [String; 3],
}

impl GraphIds {
	fn of(w: Which) -> GraphIds {
		let at = |s: &str| format!("{}_{s}", w.key());
		GraphIds {
			show: w.show_id(),
			width: at("width"),
			sample: at("sample"),
			time: at("time"),
			vertical: at("vertical"),
			mode: at("mode"),
			gradient: ["low", "medium", "high"].map(at),
			outline: SHARED_OUTLINE.map(at),
		}
	}
}

/// The most series a graph stacks, the network's.
pub const MOST_SERIES: usize = 5;

/// One graph's settings in a [`Look`].
#[derive(Clone, Debug, PartialEq)]
pub struct Each {
	/// Draw it, where the box has it. CPU is drawn anyway when no other
	/// graph is.
	pub show: bool,
	/// A percentage of `width`. See [`Look::share`].
	pub width: u32,
	/// A percentage of `sample_ms`. See [`Look::rate`].
	pub sample: u32,
	/// None follows `time`. See [`Look::timing`].
	pub time: Option<Timing>,
	/// None follows `vertical`. See [`Look::scaling`].
	pub vertical: Option<Vertical>,
	/// In [`Which::color_ids`] order, and black past its series count.
	pub colors: [Rgb; MOST_SERIES],
	/// Detailed or Gradient.
	pub mode: Mode,
	/// Low, Medium and High.
	pub gradient: [Rgb; 3],
	/// Its frame and background, unless the Outline sub-tab overrides it.
	pub outline: Outline,
}

/// Every setting a panel draws with, as read from the config file.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
	/// Each graph's width in logical pixels, before its own share.
	pub width: u32,
	/// What a graph with no time axis of its own follows.
	pub time: Timing,
	/// What a graph with no vertical scale of its own follows.
	pub vertical: Vertical,
	/// Milliseconds between reads, before each graph's own rate.
	pub sample_ms: u32,
	/// What the hover tip says about the time under the pointer.
	pub said: Said,
	/// Whether graphs slide between samples.
	pub motion: Motion,
	/// Draw less often in a remote desktop session. See [`Look::calmed`].
	pub calm_remote: bool,
	/// Where this runs. Not in the file, since it's found at run time, by
	/// [`crate::remote::session`].
	pub session: Session,
	/// 0 to 1.
	pub spikes: f32,
	/// The frame's width in logical pixels.
	pub border: u32,
	/// Logical pixels clear above and below each graph, and either side of it.
	/// See `graphs::Graphs::tiles` for the item's ends.
	pub margin_tb: u32,
	/// Logical pixels clear either side of each graph.
	pub margin_lr: u32,
	/// Save history and load it back on start.
	pub keep: bool,
	/// History older than this many seconds is dropped. Max age's days, hours
	/// and minutes added up, and never under a minute.
	pub max_age: u32,
	/// Each graph's own, by [`Which`]: `look[Which::Mem].width`.
	pub graphs: [Each; Which::ALL.len()],
	/// How the network graph tells the LAN from the internet.
	pub net_split: Split,
	/// `/proc/diskstats` names, or `all`. See [`Look::drives`].
	pub disk_drives: Vec<String>,
	/// Graph names as saved, first drawn first. See [`graph_order`].
	pub order: Vec<String>,
	/// The Outline sub-tab's. A graph draws with its own unless
	/// `border_all` or `back_all` is on, and then each one's `outline` is these.
	pub border_color: Rgb,
	/// No background, so the panel shows through, whatever the theme.
	pub transparent: bool,
	/// The background at the top, for every graph, when `back_all` is on.
	pub top: Rgb,
	/// The background at the bottom, for every graph, when `back_all` is on.
	pub bottom: Rgb,
	/// The hover line.
	pub hover: Rgb,
}

impl Default for Look {
	fn default() -> Self {
		Look::from_model(&Model::new(spec()))
	}
}

impl Index<Which> for Look {
	type Output = Each;
	fn index(&self, w: Which) -> &Each {
		&self.graphs[w as usize]
	}
}

impl IndexMut<Which> for Look {
	fn index_mut(&mut self, w: Which) -> &mut Each {
		&mut self.graphs[w as usize]
	}
}

impl Look {
	/// The model as read from a file, so [`unthemed_is_custom`] applies.
	pub fn from_model(m: &Model) -> Look {
		let custom;
		let m = if unthemed(m) {
			custom = {
				// the caller's model stays as read
				let mut c = m.clone();
				unthemed_is_custom(&mut c);
				c
			};
			&custom
		} else {
			m
		};
		let num = |id: &str| m.value(id, &NoEnv).as_f64();
		let color = |id: &str| Rgb::parse(m.value(id, &NoEnv).as_text()).unwrap_or(Rgb(0, 0, 0));
		let on = |id: &str| m.value(id, &NoEnv).as_bool();
		let (border_all, back_all) = (on("border_all"), on("back_all"));
		let each = |w: Which| {
			let ids = w.ids();
			let mut colors = [Rgb(0, 0, 0); MOST_SERIES];
			for (c, id) in colors.iter_mut().zip(w.color_ids()) {
				*c = color(id);
			}
			// the Outline sub-tab's go over every graph's own when ticked
			let pick = |all: bool, i: usize| {
				color(if all {
					SHARED_OUTLINE[i]
				} else {
					&ids.outline[i]
				})
			};
			Each {
				show: on(&ids.show),
				width: num(&ids.width) as u32,
				sample: num(&ids.sample) as u32,
				time: Timing::parse(m.value(&ids.time, &NoEnv).as_text()),
				vertical: Vertical::parse(m.value(&ids.vertical, &NoEnv).as_text()),
				colors,
				mode: Mode::parse(m.value(&ids.mode, &NoEnv).as_text()).unwrap_or_default(),
				gradient: ids.gradient.each_ref().map(|id| color(id)),
				outline: Outline {
					border: pick(border_all, 0),
					top: pick(back_all, 1),
					bottom: pick(back_all, 2),
				},
			}
		};
		Look {
			width: num("width") as u32,
			time: Timing::parse(m.value("time_axis", &NoEnv).as_text())
				.unwrap_or(Timing::Log(Curve::Smooth)),
			vertical: Vertical::parse(m.value("vertical_axis", &NoEnv).as_text())
				.unwrap_or_default(),
			sample_ms: num("sample_ms") as u32,
			said: Said::parse(m.value("hover_time", &NoEnv).as_text()).unwrap_or_default(),
			motion: Motion::parse(m.value("animate", &NoEnv).as_text()).unwrap_or_default(),
			calm_remote: on("calm_remote"),
			session: Session::Local,
			spikes: (num("spikes") / 100.0) as f32,
			border: num("border") as u32,
			margin_tb: num("margin_tb") as u32,
			margin_lr: num("margin_lr") as u32,
			keep: m.value("keep_history", &NoEnv).as_bool(),
			// 36500 days and change is still well inside a u32 of seconds
			max_age: (num("max_days") * 86400.0
				+ num("max_hours") * 3600.0
				+ num("max_minutes") * 60.0)
				.max(60.0) as u32,
			graphs: Which::ALL.map(each),
			net_split: Split::parse(m.value("net_split", &NoEnv).as_text()).unwrap_or_default(),
			disk_drives: graph_order(m.value("disk_drives", &NoEnv).as_text()),
			order: graph_order(m.value("graph_order", &NoEnv).as_text()),
			border_color: color("border_color"),
			transparent: m.value("transparent", &NoEnv).as_bool(),
			top: color("back_top"),
			bottom: color("back_bottom"),
			hover: color("hover_color"),
		}
	}

	/// The look with graph `w`'s settings changed by `f`.
	#[cfg(test)]
	pub(crate) fn with(mut self, w: Which, f: impl FnOnce(&mut Each)) -> Look {
		f(&mut self[w]);
		self
	}

	/// How graph `w` colors its `N` series.
	pub fn paint<const N: usize>(&self, w: Which) -> Paint<'_, N> {
		const { assert!(N <= MOST_SERIES) };
		let g = &self[w];
		match g.mode {
			#[allow(clippy::expect_used, reason = "N <= MOST_SERIES, asserted at build")]
			Mode::Detailed => Paint::Stack(g.colors.first_chunk().expect("N <= MOST_SERIES")),
			Mode::Gradient => Paint::Gradient(g.gradient),
		}
	}

	/// The drives the disk graph counts.
	pub fn drives(&self) -> Drives {
		if self.disk_drives.is_empty() || self.disk_drives.iter().any(|d| d == "all") {
			Drives::All
		} else {
			// the reader keeps its own, and follows a new look by comparing
			Drives::Only(self.disk_drives.clone())
		}
	}

	/// A graph's own pick, or else Scale default under Time axis.
	pub fn timing(&self, own: Option<Timing>) -> Timing {
		own.unwrap_or(self.time)
	}

	/// A graph's own vertical scale, or else Scale default under Vertical axis.
	pub fn scaling(&self, own: Option<Vertical>) -> Vertical {
		own.unwrap_or(self.vertical)
	}

	/// A graph's sample rate in ms from its percentage of Sample ms default,
	/// rounded, and never under [`crate::FASTEST_MS`]. While [`Look::calmed`],
	/// twice that, and never under [`CALM_MS`].
	pub fn rate(&self, percent: u32) -> u32 {
		let ms = (u64::from(self.sample_ms) * u64::from(percent) + 50) / 100;
		let ms = u32::try_from(ms).unwrap_or(u32::MAX).max(crate::FASTEST_MS);
		if self.calmed() {
			ms.saturating_mul(2).max(CALM_MS)
		} else {
			ms
		}
	}

	/// True in a remote desktop session with Reduce motion on, so every graph
	/// reads half as often and none slides.
	pub fn calmed(&self) -> bool {
		self.calm_remote && self.session == Session::Remote
	}

	/// True when graphs slide on a box with `cpus` CPUs.
	pub fn slides(&self, cpus: f32) -> bool {
		!self.calmed() && self.motion.slides(cpus)
	}

	/// A graph's width from its percentage of Width px default, rounded, and never 0.
	pub fn share(&self, percent: u32) -> u32 {
		let px = (u64::from(self.width) * u64::from(percent) + 50) / 100;
		u32::try_from(px).unwrap_or(u32::MAX).max(1)
	}

	/// A missing file gives the defaults, and a bad line gives that setting's
	/// default.
	///
	/// # Errors
	///
	/// A file that's there but can't be read, or isn't UTF-8, so the caller
	/// can keep what it has.
	pub fn load(path: &Path) -> std::io::Result<Look> {
		let text = match std::fs::read_to_string(path) {
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
			r => r?,
		};
		Ok(Look::from_model(&model(&text)))
	}
}

/// The names in a `graph.order` line, lower case, each once. Commas work as
/// well as spaces, and so does a list, which shcl hands back as its text.
pub fn graph_order(text: &str) -> Vec<String> {
	let mut names: Vec<String> = Vec::new();
	for word in text.split(|c: char| c == ',' || c.is_whitespace()) {
		let name = word
			.trim_matches(|c| matches!(c, '"' | '\'' | '[' | ']'))
			.to_ascii_lowercase();
		if !name.is_empty() && !names.contains(&name) {
			names.push(name);
		}
	}
	names
}

/// `$XDG_CONFIG_HOME/tukzmon`, else `~/.config/tukzmon`.
pub fn config_dir() -> PathBuf {
	let base = std::env::var_os("XDG_CONFIG_HOME")
		.filter(|v| !v.is_empty())
		.map(PathBuf::from)
		.or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
		.unwrap_or_else(|| PathBuf::from("."));
	base.join("tukzmon")
}

/// The shared file: what tukzmon-settings edits with no `--config`, and what
/// a new panel item starts from. See [`instance_file`].
pub fn config_file() -> PathBuf {
	shared_file(&config_dir())
}

/// The shared file in `dir`, as [`config_file`] is in the config folder.
pub fn shared_file(dir: &Path) -> PathBuf {
	dir.join("tukzmon.shcl")
}

/// One panel item's own settings file in `dir`, by its unique id, named like
/// its history file.
pub fn instance_file(dir: &Path, id: i32) -> PathBuf {
	dir.join(format!("plugin-{id}.shcl"))
}

/// Where a write to `path` goes: the file its links end at, even one that
/// isn't there yet, so a save keeps the link rather than putting a file over it.
pub fn through_links(path: &Path) -> PathBuf {
	let mut at = path.to_path_buf();
	// the kernel's own limit; a loop fails at the read before this matters
	for _ in 0..40 {
		let Ok(to) = std::fs::read_link(&at) else {
			break;
		};
		at = match at.parent() {
			Some(dir) => dir.join(to),
			None => to,
		};
	}
	at
}

/// Gives `own` a copy of `shared` when it has no file yet, so an item set up
/// before each one had its own keeps its look, and a new one starts from it.
/// With neither, nothing is written, and the item has the defaults.
///
/// # Errors
///
/// Whatever reading `shared` or writing `own` hit.
pub fn start_from(own: &Path, shared: &Path) -> std::io::Result<()> {
	let own = &through_links(own);
	if own.exists() {
		return Ok(());
	}
	let text = match std::fs::read(shared) {
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
		r => r?,
	};
	// renamed in whole, so a crash can't leave half a file that then wins
	let tmp = own.with_extension("shcl.tmp");
	std::fs::write(&tmp, text)
		.and_then(|()| std::fs::rename(&tmp, own))
		.inspect_err(|_| {
			// the write's error is the one to give
			let _ = std::fs::remove_file(&tmp);
		})
}

/// The mark beside an [`instance_file`] that says its item made its panel
/// file. See [`claim`].
pub fn mark_of(own: &Path) -> PathBuf {
	own.with_extension("mark")
}

/// Clears an earlier item's `own` settings file and `history` when the panel
/// has made a new item on the same id, then marks the id for the next start.
///
/// The panel can drop an item without telling the plugin, but it deletes its
/// `rc` file for an id when it makes a new item on it. So an id with our mark
/// and no `rc` belongs to a new item. With neither, the id is new, or its item
/// ran a build from before the marks, and its files stay. See `design.md`
/// Settings window.
///
/// # Errors
///
/// Whatever checking, deleting, or writing hit. Nothing is marked then.
pub fn claim(rc: &Path, own: &Path, history: &Path) -> std::io::Result<()> {
	let mark = mark_of(own);
	if !rc.try_exists()? {
		if mark.try_exists()? {
			for f in [own, history, &mark] {
				crate::keep::remove(f)?;
			}
		}
		// the panel reads nothing from it
		std::fs::write(
			rc,
			format!(
				"# tukzmon keeps this item's settings in {}\n",
				own.display()
			),
		)?;
	}
	// only once the rc is there, or the next start would take this id for new
	if !mark.try_exists()? {
		if let Some(dir) = mark.parent() {
			std::fs::create_dir_all(dir)?;
		}
		std::fs::write(&mark, "")?;
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	// Test ID: debhg
	#[test]
	fn parse_spec() {
		if let Err(e) = Spec::parse(SPEC_TEXT) {
			panic!("{}", e.join("\n"));
		}
	}

	// Test ID: der4w
	#[test]
	fn spec_keys_all_read() {
		assert_eq!(unknown_spec_keys(SPEC_TEXT), Vec::<String>::new());
		let typo = SPEC_TEXT.replacen("\t\tgate:", "\t\tgaet:", 1);
		assert_ne!(typo, SPEC_TEXT);
		let odd = unknown_spec_keys(&typo);
		assert_eq!(odd.len(), 1, "{odd:?}");
		let parts: Vec<&str> = odd[0].split('.').collect();
		assert_eq!((parts.len(), parts[0], parts[2]), (3, "settings", "gaet"));
	}

	// Test ID: decb4
	#[test]
	fn slider_inside_range() {
		let doc = shcl::Document::parse(SPEC_TEXT);
		let named: Vec<String> = doc
			.paths()
			.into_iter()
			.filter(|p| p.ends_with(".slider_range"))
			.collect();
		assert!(!named.is_empty());
		assert_eq!(named.len(), slider_ranges().len(), "one didn't parse");
		let spec = spec();
		for (id, lo, hi) in slider_ranges() {
			let s = spec.get(id).unwrap();
			assert_eq!(s.control, knobs::Control::Slider, "{id}");
			assert!(s.min <= *lo && lo < hi && *hi <= s.max, "{id}");
		}
		assert_eq!(slider_range("width"), Some((24.0, 1024.0)));
		assert_eq!(slider_range("spikes"), None);
	}

	// Test ID: debhh
	#[test]
	fn no_comma_splits() {
		// a bare value with a comma in it reads as a list, which shows up quoted
		for s in &spec().settings {
			assert!(
				!s.label.contains('"') && !s.tip.contains('"'),
				"{}: quote its label or tip",
				s.id
			);
		}
	}

	// Test ID: debhj
	#[test]
	fn multiload_default_colors() {
		let l = Look::default();
		assert_eq!(l[Which::Cpu].colors[0], Rgb(0xff, 0, 0));
		assert_eq!(l[Which::Cpu].colors[1], Rgb(0x69, 0x3d, 0x91));
		assert_eq!(l[Which::Cpu].colors[2], Rgb(0, 0xff, 0x80));
		assert_eq!(l[Which::Cpu].colors[3], Rgb(0x20, 0x5f, 0x40));
		assert_eq!(l.border_color, Rgb(0x59, 0x59, 0x59));
		assert_eq!(l.top, Rgb(0x31, 0x31, 0x31));
		assert_eq!(l.bottom, Rgb(0, 0, 0));
		assert_eq!((l.width, l.sample_ms, l.border), (100, 1000, 1));
		assert_eq!(l.time, Timing::Log(Curve::Smooth));
		assert_eq!(l.spikes, 0.75);
		assert_eq!(l.hover, Rgb(0, 0x0f, 0xff));
		assert_eq!(l.said, Said::Ago);
	}

	// Test ID: df3e3
	#[test]
	fn smooth_auto_by_cpus() {
		let read = |text: &str| Look::from_model(&Model::load(spec(), text, "")).motion;
		assert_eq!(Look::default().motion, Motion::Auto);
		assert_eq!(read("graph:\n\tanimate: off\n"), Motion::Off);
		assert_eq!(read("graph:\n\tanimate: On\n"), Motion::On);
		assert_eq!(read("graph:\n\tanimate: sideways\n"), Motion::Auto);
		assert!(Motion::Auto.slides(8.0) && !Motion::Auto.slides(7.0));
		assert!(Motion::On.slides(1.0) && !Motion::Off.slides(64.0));
	}

	// Test ID: df3ng
	#[test]
	fn remote_halves_reads_no_slide() {
		let read = |text: &str, remote: bool| Look {
			session: if remote {
				Session::Remote
			} else {
				Session::Local
			},
			..Look::from_model(&Model::load(spec(), text, ""))
		};
		let local = read("graph:\n\tanimate: on\n", false);
		assert!(local.calm_remote && !local.calmed() && local.slides(1.0));
		assert_eq!((local.rate(50), local.rate(300)), (500, 3000));
		let remote = read("graph:\n\tanimate: on\n", true);
		assert!(remote.calmed() && !remote.slides(64.0));
		assert_eq!(
			(remote.rate(50), remote.rate(100), remote.rate(300)),
			(1000, 2000, 6000)
		);
		assert_eq!(remote.rate(1), CALM_MS);
		let off = read("graph:\n\tanimate: on\n\tcalm_remote: false\n", true);
		assert!(!off.calmed() && off.slides(1.0));
		assert_eq!(off.rate(50), 500);
		for id in ["sample_ms", "animate", "cpu_sample", "net_sample"] {
			assert!(calmed_id(id), "{id}");
		}
		assert!(!calmed_id(CALM_REMOTE) && !calmed_id("cpu_width"));
	}

	// Test ID: dencn
	#[test]
	fn graph_settings_own_record() {
		let spec = spec();
		let mut m = Model::new(spec.clone());
		for (i, w) in Which::ALL.into_iter().enumerate() {
			assert_eq!(w as usize, i, "{w:?}");
			assert_eq!(Which::named(w.key()), Some(w));
			let k = w.key();
			let shared = ["width", "sample", "time", "mode", "low", "medium", "high"]
				.map(|s| format!("{k}_{s}"))
				.into_iter()
				.chain(["border_color", "back_top", "back_bottom"].map(|s| format!("{k}_{s}")))
				.chain([w.show_id()]);
			for id in shared.chain(w.color_ids().iter().map(ToString::to_string)) {
				assert!(spec.get(&id).is_some(), "{w:?}: no {id}");
			}
			let n = i as i64;
			m.set(&format!("{k}_width"), &knobs::Value::Int(10 + n), &NoEnv);
			m.set(&format!("{k}_sample"), &knobs::Value::Int(30 + n), &NoEnv);
			m.set(&w.show_id(), &knobs::Value::Bool(i % 2 == 0), &NoEnv);
			for (j, id) in w.color_ids().iter().enumerate() {
				let c = format!("#{i:02x}{j:02x}01");
				m.set(id, &knobs::Value::Text(c), &NoEnv);
			}
		}
		let l = Look::from_model(&m);
		for (i, w) in Which::ALL.into_iter().enumerate() {
			let g = &l[w];
			assert_eq!((g.width, g.sample), (10 + i as u32, 30 + i as u32), "{w:?}");
			assert_eq!(g.show, i % 2 == 0, "{w:?}");
			for (j, c) in g.colors.iter().enumerate() {
				let want = if j < w.color_ids().len() {
					Rgb(i as u8, j as u8, 1)
				} else {
					Rgb(0, 0, 0)
				};
				assert_eq!(*c, want, "{w:?} color {j}");
			}
		}
	}

	// Test ID: deccf
	#[test]
	fn default_theme_is_row_defaults() {
		let spec = spec();
		let g = spec.group(THEME).unwrap();
		assert_eq!(g.members.len(), 127);
		let default = g.preset("default").unwrap();
		assert_eq!(default.label, "Default");
		for id in &g.members {
			let s = spec.get(id).unwrap();
			// colors, a graph's mode, the margins and the overrides
			let control = if id.ends_with("_mode") {
				knobs::Control::Dropdown
			} else if id.starts_with("margin_") {
				knobs::Control::Slider
			} else if id.ends_with("_all") {
				knobs::Control::Checkbox
			} else {
				knobs::Control::Color
			};
			assert_eq!(s.control, control, "{id}");
			assert_eq!(default.values.get(id), Some(&s.default), "{id}");
		}
		// every color row is in it
		for s in &spec.settings {
			if s.control == knobs::Control::Color {
				assert!(g.members.contains(&s.id), "{}", s.id);
			}
		}
	}

	// Test ID: dejbq
	#[test]
	fn default_theme_keeps_picks() {
		// b23's changes on Default as of 2026-10-28, 2026-10-31 for
		// temperature and processes, and 2026-11-09 for the rest, kept as
		// they were
		let picked = [
			("system", "#693d91"),
			("net_local", "#ffed00"),
			("net_sent", "#ff7777"),
			("net_received", "#6ad4ff"),
			("net_lan_sent", "#ff6bf1"),
			("net_lan_received", "#84ff7b"),
			("swap_mode", "gradient"),
			("swap_used", "#a45f00"),
			("zswap_mode", "gradient"),
			("zswap_used", "#4a8e54"),
			("load_mode", "gradient"),
			("load_used", "#924444"),
			("load_low", "#553030"),
			("load_medium", "#924444"),
			("load_high", "#eb4a4a"),
			("disk_write", "#c60000"),
			("disk_read", "#6bb3ff"),
			("temp_mode", "gradient"),
			("temp_low", "#60dae2"),
			("temp_medium", "#d9da62"),
			("temp_high", "#ff0000"),
			("procs_system", "#707070"),
			("procs_users", "#6bc9ff"),
			("procs_me", "#fff02a"),
			("battery_discharging", "#c46e43"),
			("border_color", "#595959"),
			("back_top", "#313131"),
			("hover_color", "#000fff"),
		];
		let spec = spec();
		let default = spec.group(THEME).unwrap().preset("default").unwrap();
		let v = |id: &str| default.values[id].as_text().to_string();
		for (id, want) in picked {
			assert_eq!(v(id), want, "{id}");
		}
		// a graph drawn as a gradient runs through the color picked for it
		for (lead, medium) in [
			("swap_used", "swap_medium"),
			("zswap_used", "zswap_medium"),
			("temp_normal", "temp_medium"),
			("procs_me", "procs_medium"),
			("procs_system", "procs_low"),
		] {
			assert_eq!(v(lead), v(medium), "{lead}");
		}
	}

	// Test ID: deccg
	#[test]
	fn theme_covers_colors() {
		let l = Look::from_model(&model("theme:\n\tname: pastel\n"));
		assert_eq!(l[Which::Cpu].colors[2], Rgb(0xa1, 0xc8, 0xf0));
		assert_eq!(l.bottom, Rgb(0x0f, 0x12, 0x14));
		assert_eq!(l.width, 100);
		assert_eq!(
			Look::from_model(&model("theme:\n\tname: plaid\n")),
			Look::default()
		);
	}

	// Test ID: decdy
	#[test]
	fn transparent_outlives_themes() {
		assert!(!Look::default().transparent);
		assert_eq!(
			spec().get("transparent").and_then(|s| s.group.as_deref()),
			None
		);
		let on = "graph:\n\ttransparent: true\n";
		for theme in ["theme:\n\tname: fire\n", "theme:\n\tname: plaid\n", ""] {
			let l = Look::from_model(&model(&format!("{theme}{on}")));
			assert!(l.transparent, "{theme:?}");
		}
		// Custom too
		let l = Look::from_model(&model(&format!(
			"{on}graph:\n\tcolors:\n\t\ttop: \"#123456\"\n"
		)));
		assert!(l.transparent);
		assert_eq!(l.top, Rgb(0x12, 0x34, 0x56));
	}

	// Test ID: dejjt
	#[test]
	fn theme_margins_reach_look() {
		let s = spec();
		for id in ["margin_tb", "margin_lr"] {
			assert_eq!(s.get(id).and_then(|r| r.group.as_deref()), Some("theme"));
		}
		for p in s.group("theme").unwrap().presets.listed() {
			for id in ["margin_tb", "margin_lr"] {
				assert_eq!(p.values[id].as_f64(), 2.0, "{} {id}", p.key);
			}
		}
		let l = Look::default();
		assert_eq!((l.margin_tb, l.margin_lr), (2, 2));
		let set = "graph:\n\tmargins:\n\t\ttop_bottom: 0\n\t\tleft_right: 7\n";
		let l = Look::from_model(&model(set));
		assert_eq!((l.margin_tb, l.margin_lr), (0, 7));
		let l = Look::from_model(&model("theme:\n\tname: fire\n"));
		assert_eq!((l.margin_tb, l.margin_lr), (2, 2));
		// a theme saved before the margins gets Default's, and is still listed
		let old = "theme:\n\tname: old\nthemes:\n\told:\n\t\tname: Old\n\t\tuser: \"#123456\"\n";
		let t = saved_themes(&shcl::Document::parse(old));
		assert_eq!(t[0].colors["margin_lr"], "2");
		assert!(spec_with(&t).group(THEME).unwrap().preset("old").is_some());
		let l = Look::from_model(&model(old));
		assert_eq!(
			(l.margin_lr, l[Which::Cpu].colors[2]),
			(2, Rgb(0x12, 0x34, 0x56))
		);
	}

	// Test ID: dejky
	#[test]
	fn outline_colors_override_all() {
		let s = spec();
		let g = s.group(THEME).unwrap();
		for id in ["border_all", "back_all"] {
			assert!(g.members.iter().any(|m| m == id), "{id}");
			assert_eq!(s.get(id).unwrap().default, knobs::Value::Bool(true));
		}
		assert_eq!(
			s.get("border_color").unwrap().gate.as_deref(),
			Some("border_all")
		);
		assert_eq!(s.get("back_top").unwrap().gate.as_deref(), Some("back_all"));
		assert_eq!(
			s.get("back_bottom").unwrap().gate.as_deref(),
			Some("back_all")
		);
		let mine = "cpu:\n\tcolors:\n\t\tborder: \"#aa0000\"\n\t\ttop: \"#00aa00\"\n\t\tbottom: \"#0000aa\"\n\
			graph:\n\toverrides:\n\t\tcolors:\n\t\t\tborder: \"#111111\"\n\t\t\ttop: \"#222222\"\n\t\t\tbottom: \"#333333\"\n";
		let own = Outline {
			border: Rgb(0xaa, 0, 0),
			top: Rgb(0, 0xaa, 0),
			bottom: Rgb(0, 0, 0xaa),
		};
		let shared = Outline {
			border: Rgb(0x11, 0x11, 0x11),
			top: Rgb(0x22, 0x22, 0x22),
			bottom: Rgb(0x33, 0x33, 0x33),
		};
		let grey = Look::default()[Which::Mem].outline;
		let ticked = |border: bool, back: bool| {
			let text = format!("{mine}\t\tborder: {border}\n\t\tbackground: {back}\n");
			Look::from_model(&model(&text))
		};
		let l = ticked(false, false);
		assert_eq!((l[Which::Cpu].outline, l[Which::Mem].outline), (own, grey));
		let l = ticked(true, false);
		assert_eq!(
			l[Which::Cpu].outline,
			Outline {
				border: shared.border,
				..own
			}
		);
		let l = ticked(false, true);
		assert_eq!(
			l[Which::Cpu].outline,
			Outline {
				border: own.border,
				..shared
			}
		);
		let l = ticked(true, true);
		for o in [
			l[Which::Cpu].outline,
			l[Which::Mem].outline,
			l[Which::Own].outline,
			l[Which::Battery].outline,
		] {
			assert_eq!(o, shared);
		}
		// a saved theme keeps them, and one saved before them has Default's
		let mut t = saved_themes(&shcl::Document::parse("themes:\n\tx:\n\t\tname: X\n"));
		assert_eq!(t[0].colors["back_all"], "on");
		t[0].colors.insert("back_all".into(), "off".into());
		let text = themes_text(&t);
		assert!(text.contains("back_all: false\n"), "{text}");
		let doc = shcl::Document::parse(&text);
		assert!(theme_problems(&doc).is_empty());
		assert_eq!(saved_themes(&doc), t);
		let m = Model::load_docs(
			spec_with(&t),
			&shcl::Document::parse("theme:\n\tname: x\n"),
			&shcl::Document::new(),
		);
		assert!(!m.value("back_all", &NoEnv).as_bool());
		assert!(m.value("border_all", &NoEnv).as_bool());
	}

	// Test ID: dejkg
	#[test]
	fn outline_per_graph_per_theme() {
		let s = spec();
		let g = s.group(THEME).unwrap();
		let graphs = LEADS.map(|(graph, _)| graph);
		for graph in graphs {
			for part in ["border_color", "back_top", "back_bottom"] {
				let id = format!("{graph}_{part}");
				assert!(g.members.contains(&id), "{id}");
				let shared = &s.get(part).unwrap().default;
				assert_eq!(&s.get(&id).unwrap().default, shared, "{id}");
				for p in g.presets.listed() {
					assert_eq!(p.values[&id], p.values[part], "{} {id}", p.key);
				}
			}
		}
		let outlines = |l: &Look| {
			[
				l[Which::Cpu].outline,
				l[Which::Mem].outline,
				l[Which::Gpu].outline,
				l[Which::Vram].outline,
				l[Which::Net].outline,
				l[Which::Swap].outline,
				l[Which::Zswap].outline,
				l[Which::Load].outline,
				l[Which::Disk].outline,
				l[Which::Temp].outline,
				l[Which::Battery].outline,
				l[Which::Own].outline,
				l[Which::Procs].outline,
			]
		};
		let d = Look::default();
		let grey = Outline {
			border: Rgb(0x59, 0x59, 0x59),
			top: Rgb(0x31, 0x31, 0x31),
			bottom: Rgb(0, 0, 0),
		};
		assert_eq!(outlines(&d), [grey; 13]);
		// with the Outline sub-tab's overrides off, as Default has them on
		let own = "graph:\n\toverrides:\n\t\tborder: false\n\t\tbackground: false\n";
		// each graph reads its own, and no other graph's, here as Custom
		let l = Look::from_model(&model(&format!(
			"{own}mem:\n\tcolors:\n\t\ttop: \"#123456\"\nself:\n\tcolors:\n\t\tborder: \"#abcdef\"\n\t\tbottom: \"#010203\"\n"
		)));
		let mut want = [grey; 13];
		want[1].top = Rgb(0x12, 0x34, 0x56);
		want[11].border = Rgb(0xab, 0xcd, 0xef);
		want[11].bottom = Rgb(1, 2, 3);
		assert_eq!(outlines(&l), want);
		let fire = g.preset("fire").unwrap();
		let l = Look::from_model(&model("theme:\n\tname: fire\n"));
		assert_eq!(
			l[Which::Load].outline.top,
			Rgb::parse(fire.values["load_back_top"].as_text()).unwrap()
		);
		// the Outline sub-tab's draw no graph, but a file from before each
		// graph had its own keeps its look
		let shared = "graph:\n\tcolors:\n\t\ttop: \"#123456\"\n";
		let l = Look::from_model(&model(&format!(
			"{shared}\toverrides:\n\t\tborder: false\n\t\tbackground: false\nmem:\n\tcolors:\n\t\ttop: \"#654321\"\n"
		)));
		assert_eq!(l.top, Rgb(0x12, 0x34, 0x56));
		let mut want = [grey; 13].map(|o| Outline {
			top: Rgb(0x12, 0x34, 0x56),
			..o
		});
		want[1].top = Rgb(0x65, 0x43, 0x21);
		assert_eq!(outlines(&l), want);
		let kept = "theme:\n\tname: fire\nkept:\n\tchanges:\n\t\ttheme:\n\t\t\tpreset: fire\n\t\t\tvalues:\n\t\t\t\tgraph:\n\t\t\t\t\tcolors:\n\t\t\t\t\t\tborder: \"#123456\"\n";
		let l = Look::from_model(&model(kept));
		assert_eq!(l[Which::Disk].outline.border, Rgb(0x12, 0x34, 0x56));
		let old =
			"theme:\n\tname: old\nthemes:\n\told:\n\t\tname: Old\n\t\tback_bottom: \"#123456\"\n";
		let t = saved_themes(&shcl::Document::parse(old));
		assert_eq!(t[0].colors["temp_back_bottom"], "#123456");
		assert_eq!(t[0].colors["temp_back_top"], "#313131");
		let l = Look::from_model(&model(old));
		assert_eq!(l[Which::Battery].outline.bottom, Rgb(0x12, 0x34, 0x56));
	}

	// Test ID: decch
	#[test]
	fn unthemed_colors_are_custom() {
		let text = "cpu:\n\tcolors:\n\t\tuser: \"#123456\"\n";
		let m = model(text);
		assert_eq!(m.chosen(THEME, &NoEnv), knobs::CUSTOM);
		assert_eq!(
			Look::from_model(&m)[Which::Cpu].colors[2],
			Rgb(0x12, 0x34, 0x56)
		);
		// the rest of Custom starts as Default
		assert_eq!(
			Look::from_model(&m)[Which::Cpu].colors[3],
			Rgb(0x20, 0x5f, 0x40)
		);
		// with a theme named, they're its Custom set and wait
		let text = format!("theme:\n\tname: default\n{text}");
		let l = Look::from_model(&model(&text));
		assert_eq!(l[Which::Cpu].colors[2], Rgb(0, 0xff, 0x80));
	}

	// Test ID: deccj
	#[test]
	fn theme_edit_sticks() {
		let mut m = model("theme:\n\tname: primary\n");
		m.set("user", &knobs::Value::Text("#abcdef".into()), &NoEnv);
		assert_eq!(m.shown_choice(THEME, &NoEnv), "Primary *");
		name_the_theme(&mut m);
		let back = model(&m.save(&NoEnv).0);
		assert_eq!(back.shown_choice(THEME, &NoEnv), "Primary *");
		let l = Look::from_model(&back);
		assert_eq!(l[Which::Cpu].colors[2], Rgb(0xab, 0xcd, 0xef));
		assert_eq!(l[Which::Cpu].colors[3], Rgb(0x20, 0x5f, 0x20));
	}

	// Test ID: decck
	#[test]
	fn custom_to_default_sticks() {
		let mut m = model("");
		let text = |v: &str| knobs::Value::Text(v.into());
		m.set(THEME, &text(knobs::CUSTOM), &NoEnv);
		m.set("user", &text("#123456"), &NoEnv);
		// the revert arrow, which is also how a save takes back a pick of the
		// default; the Custom set is still written, so the theme has to be too
		m.reset(THEME, &NoEnv);
		assert_eq!(m.chosen(THEME, &NoEnv), "default");
		name_the_theme(&mut m);
		let saved = m.save(&NoEnv).0;
		assert!(saved.contains("#123456"), "{saved}");
		let back = model(&saved);
		assert_eq!(back.chosen(THEME, &NoEnv), "default");
		assert_eq!(
			Look::from_model(&back)[Which::Cpu].colors[2],
			Rgb(0, 0xff, 0x80)
		);
		// and nothing to name when there's no Custom set
		let mut plain = model("");
		name_the_theme(&mut plain);
		assert_eq!(plain.save(&NoEnv).0.trim(), "");
	}

	// Test ID: decct
	#[test]
	fn saved_theme_listed_last() {
		let text = "theme:\n\tname: mine\nthemes:\n\tmine:\n\t\tname: \"My green\"\n\t\tuser: \"#11AA22\"\n\t\tnice: nope\n";
		let m = model(text);
		let options = &m.spec.get(THEME).unwrap().options;
		let keys: Vec<&str> = options.iter().map(|(k, _)| k.as_str()).collect();
		assert_eq!(
			keys,
			[
				"default",
				"primary",
				"pastel",
				"neon",
				"earth",
				"fire",
				"terminal",
				"vintage",
				"frost",
				"mine",
				knobs::CUSTOM
			]
		);
		assert_eq!(m.shown_choice(THEME, &NoEnv), "My green");
		let l = Look::from_model(&m);
		assert_eq!(l[Which::Cpu].colors[2], Rgb(0x11, 0xaa, 0x22));
		// a missing or bad color is Default's
		assert_eq!(l[Which::Cpu].colors[3], Rgb(0x20, 0x5f, 0x40));
		let doc = shcl::Document::parse(text);
		let p = theme_problems(&doc);
		assert_eq!(p.len(), 1, "{p:?}");
		assert!(p[0].contains("themes.mine.nice"), "{p:?}");
		// the plugin reads it the same way
		let dir = std::env::temp_dir().join(format!("test_tukzmon_{}_saved", std::process::id()));
		std::fs::create_dir_all(&dir).unwrap();
		let path = dir.join("t.shcl");
		std::fs::write(&path, text).unwrap();
		let loaded = Look::load(&path).unwrap();
		std::fs::remove_file(&path).unwrap();
		std::fs::remove_dir(&dir).unwrap();
		assert_eq!(loaded, l);
	}

	// Test ID: decdg
	#[test]
	fn fire_gradient_rest_detailed() {
		let l = Look::from_model(&model("theme:\n\tname: fire\n"));
		assert_eq!(l[Which::Cpu].mode, Mode::Gradient);
		assert_eq!(
			l[Which::Cpu].gradient,
			[
				Rgb(0x8b, 0x1a, 0),
				Rgb(0xff, 0xf4, 0xc8),
				Rgb(0x7f, 0xd4, 0xff)
			]
		);
		assert_eq!(
			l.paint::<4>(Which::Cpu),
			crate::render::Paint::Gradient(l[Which::Cpu].gradient)
		);
		let spec = spec();
		for p in spec.group(THEME).unwrap().presets.listed() {
			let want = if p.key == "fire" {
				"gradient"
			} else {
				"detailed"
			};
			assert_eq!(p.values["cpu_mode"].as_text(), want, "{}", p.key);
		}
		let d = Look::default();
		assert_eq!(d[Which::Cpu].mode, Mode::Detailed);
		assert_eq!(
			d.paint::<4>(Which::Cpu),
			crate::render::Paint::Stack(d[Which::Cpu].colors.first_chunk().unwrap())
		);
		// a mode set by hand is a change on the theme, like a color
		let mut m = model("theme:\n\tname: neon\n");
		m.set("cpu_mode", &knobs::Value::Text("Gradient".into()), &NoEnv);
		assert_eq!(m.shown_choice(THEME, &NoEnv), "Neon *");
		name_the_theme(&mut m);
		let l = Look::from_model(&model(&m.save(&NoEnv).0));
		assert_eq!(l[Which::Cpu].mode, Mode::Gradient);
		assert_eq!(l[Which::Cpu].gradient[1], Rgb(0x39, 0xff, 0x14));
		assert_eq!(Mode::parse("plaid"), None);
	}

	// Test ID: dedpz
	#[test]
	fn memory_colors_in_themes() {
		let d = Look::default();
		assert_eq!(
			d[Which::Mem].colors[..3],
			[
				Rgb(0xff, 0xff, 0xb8),
				Rgb(0x80, 0x40, 0x80),
				Rgb(0x40, 0x80, 0x60)
			]
		);
		assert!(d[Which::Mem].show);
		assert_eq!(d[Which::Mem].mode, Mode::Detailed);
		assert_eq!(
			d.paint::<3>(Which::Mem),
			crate::render::Paint::Stack(d[Which::Mem].colors.first_chunk().unwrap())
		);
		let spec = spec();
		for p in spec.group(THEME).unwrap().presets.listed() {
			let v = |id: &str| p.values[id].as_text().to_string();
			assert_eq!(v("mem_mode"), v("cpu_mode"), "{}", p.key);
			assert_ne!(v("mem_buffers"), v("mem_cache"), "{}", p.key);
			assert_ne!(v("mem_user"), v("mem_buffers"), "{}", p.key);
		}
		let fire = Look::from_model(&model("theme:\n\tname: fire\n"));
		assert_eq!(
			fire.paint::<3>(Which::Mem),
			crate::render::Paint::Gradient(fire[Which::Mem].gradient)
		);
		assert_ne!(fire[Which::Mem].gradient, fire[Which::Cpu].gradient);
		assert_eq!(shown_when("mem_buffers"), Some(("mem_mode", "detailed")));
		assert_eq!(shown_when("mem_high"), Some(("mem_mode", "gradient")));
		assert!(shown_as_radio("mem_mode"));
		// a file can hide it, and the colors are its own
		let l = Look::from_model(&model(
			"mem:\n\tshow: false\n\tcolors:\n\t\tcache: \"#010203\"\n",
		));
		assert!(!l[Which::Mem].show);
		assert_eq!(l[Which::Mem].colors[2], Rgb(1, 2, 3));
		assert_eq!(l[Which::Cpu].colors, d[Which::Cpu].colors);
	}

	// Test ID: defx7
	#[test]
	fn theme_names_not_colors() {
		let colors = [
			"red", "orange", "yellow", "green", "blue", "purple", "violet", "pink", "cyan",
			"magenta", "teal", "brown", "gray", "grey",
		];
		let spec = spec();
		let presets = spec.group(THEME).unwrap().presets.listed();
		assert!(presets.len() > 2);
		for p in presets {
			let label = p.label.to_lowercase();
			assert!(!colors.iter().any(|c| label.contains(c)), "{label}");
			let rgb = |id: &str| Rgb::parse(p.values[id].as_text()).unwrap();
			let ramp = |g: &str| ["low", "medium", "high"].map(|s| rgb(&format!("{g}_{s}")));
			let ramps = [
				"cpu", "mem", "gpu", "vram", "swap", "zswap", "net", "load", "disk", "temp",
				"battery", "self", "procs",
			]
			.map(ramp);
			// how far apart the leads look is dejbr's, by CIEDE2000, not hue angle
			if ramps.iter().all(|r| *r == ramps[0]) {
				// one color, like dejbw's
				continue;
			}
			for a in 0..ramps.len() {
				for b in a + 1..ramps.len() {
					assert_ne!(ramps[a], ramps[b], "{} {a} {b}", p.key);
					assert_ne!(ramps[a][1], ramps[b][1], "{} {a} {b}", p.key);
				}
			}
		}
	}

	// Test ID: dejbr
	#[test]
	fn lead_colors_distinct() {
		use crate::color::{APART, apart};
		let spec = spec();
		for p in spec.group(THEME).unwrap().presets.listed() {
			let get = |id: &str| p.values.get(id).map(knobs::Value::as_text);
			let leads = leads(get);
			assert_eq!(leads.len(), LEADS.len(), "{}", p.key);
			// both picked by hand on b23, and in Default what was picked wins
			let let_off: &[(&str, &str)] = if p.key == "default" {
				&[("temp", "procs")]
			} else {
				&[]
			};
			// a one-color theme leads every graph alike, on purpose (dejbw)
			let one_color = leads.iter().all(|c| *c == leads[0]);
			for a in 0..leads.len() {
				for b in a + 1..leads.len() {
					if one_color || let_off.contains(&(LEADS[a].0, LEADS[b].0)) {
						continue;
					}
					let d = apart(leads[a], leads[b]);
					assert!(
						d >= APART,
						"{}: {} and {} are {d:.1} apart",
						p.key,
						LEADS[a].0,
						LEADS[b].0
					);
				}
			}
			for alarm in ALARMS {
				let c = Rgb::parse(get(alarm).unwrap()).unwrap();
				for (lead, (g, _)) in leads.iter().zip(LEADS) {
					let d = apart(*lead, c);
					assert!(d >= APART, "{}: {g} is {d:.1} from {alarm}", p.key);
				}
			}
		}
		// a gradient's lead is its Medium
		let one = |mode: &'static str| {
			move |id: &str| match id {
				"cpu_mode" => Some(mode),
				"cpu_medium" => Some("#010203"),
				"user" => Some("#040506"),
				_ => Some("#000000"),
			}
		};
		assert_eq!(leads(one("gradient"))[0], Rgb(1, 2, 3));
		assert_eq!(leads(one("detailed"))[0], Rgb(4, 5, 6));
	}

	// Test ID: dejbw
	#[test]
	fn mono_themes_keep_alarms() {
		use crate::color::{APART, apart, closest};
		let spec = spec();
		let g = spec.group(THEME).unwrap();
		let mut one_color = vec![];
		for p in g.presets.listed() {
			let get = |id: &str| p.values.get(id).map(knobs::Value::as_text);
			let leads = leads(get);
			if leads.iter().any(|c| *c != leads[0]) {
				continue;
			}
			one_color.push(p.key.as_str());
			let rgb = |id: &str| Rgb::parse(get(id).unwrap()).unwrap();
			let alarms = ALARMS.map(rgb);
			assert!(apart(alarms[0], alarms[1]) >= APART, "{}", p.key);
			for alarm in alarms {
				assert!(apart(leads[0], alarm) >= APART, "{}", p.key);
			}
			// each graph's own colors read apart, and every color shows on the top
			for (graph, ids) in [
				("cpu", &["iowait", "system", "user", "nice"][..]),
				("mem", &["mem_user", "mem_buffers", "mem_cache"]),
				(
					"net",
					&[
						"net_local",
						"net_lan_sent",
						"net_lan_received",
						"net_sent",
						"net_received",
					],
				),
				("disk", &["disk_write", "disk_read"]),
				("temp", &["temp_normal", "temp_critical"]),
				(
					"battery",
					&[
						"battery_charging",
						"battery_discharging",
						"battery_critical",
					],
				),
			] {
				let colors: Vec<Rgb> = ids.iter().map(|id| rgb(id)).collect();
				let (d, a, b) = closest(&colors).unwrap();
				assert!(d >= APART, "{}: {} {} {d:.1}", p.key, ids[a], ids[b]);
				let top = rgb(&format!("{graph}_back_top"));
				for c in colors {
					assert!(apart(c, top) >= APART, "{} {c:?}", p.key);
				}
			}
		}
		assert_eq!(one_color, ["terminal", "vintage", "frost"]);
	}

	// Test ID: dedqf
	#[test]
	fn switch_and_width_rows() {
		let d = Look::default();
		assert!(d[Which::Cpu].show && d[Which::Mem].show && d[Which::Swap].show);
		assert_eq!(
			(
				d[Which::Cpu].width,
				d[Which::Mem].width,
				d[Which::Net].width
			),
			(100, 100, 50)
		);
		let l = Look::from_model(&model(
			"graph:\n\twidth: 90\ncpu:\n\tshow: false\n\twidth: 64\nmem:\n\twidth: 1000\n",
		));
		assert_eq!(
			(
				l[Which::Cpu].show,
				l[Which::Cpu].width,
				l[Which::Mem].width,
				l.width
			),
			(false, 64, 1000, 90)
		);
		// a percentage of Width px default, rounded, and never 0
		assert_eq!((l.share(64), l.share(1000), l.share(50)), (58, 900, 45));
		assert_eq!(
			Look {
				width: 16,
				..l.clone()
			}
			.share(1),
			1
		);
		assert!(l[Which::Mem].show);
		let spec = spec();
		let gate = |id: &str| {
			spec.settings
				.iter()
				.find(|s| s.id == id)
				.unwrap()
				.gate
				.clone()
		};
		// a hidden graph's width can be set before it's turned on
		assert_eq!(gate("cpu_width"), None);
		assert_eq!(gate("mem_width"), None);
	}

	// Test ID: deja1
	#[test]
	fn sample_rate_old_files() {
		let d = Look::default();
		assert_eq!((d[Which::Cpu].sample, d[Which::Battery].sample), (50, 100));
		assert_eq!(d.rate(d[Which::Cpu].sample), 500);
		let l = Look::from_model(&model(
			"graph:\n\tsample_ms: 400\ncpu:\n\tsample: 250\nmem:\n\tsample: 5\nswap:\n\tsample: 33\n",
		));
		assert_eq!((l[Which::Cpu].sample, l[Which::Mem].sample), (250, 5));
		// a percentage of Sample ms default, rounded, and never under 50 ms
		assert_eq!(l.rate(l[Which::Cpu].sample), 1000);
		assert_eq!(l.rate(l[Which::Mem].sample), crate::FASTEST_MS);
		assert_eq!(l.rate(l[Which::Swap].sample), 132);
		// a file from before has none, and Sample ms default rules
		assert_eq!(l.rate(l[Which::Net].sample), 400);
		// nor does the ms a graph had before it took a percentage
		let old = Look::from_model(&model("cpu:\n\tsample_ms: 250\n"));
		assert_eq!(old.rate(old[Which::Cpu].sample), 500);
		let spec = spec();
		for g in [
			"cpu", "mem", "gpu", "vram", "net", "swap", "zswap", "load", "disk", "temp", "battery",
			"self", "procs",
		] {
			let id = format!("{g}_sample");
			let Some(s) = spec.get(&id) else {
				panic!("no {id} in settings.shcl");
			};
			// set while its graph is off, and 100 is Sample ms default
			assert_eq!(s.gate, None);
			let want = match g {
				"cpu" | "mem" => 50.0,
				"swap" | "zswap" | "load" => 200.0,
				"self" => 400.0,
				"procs" => 500.0,
				_ => 100.0,
			};
			assert_eq!(s.default.as_f64(), want, "{id}");
			assert_eq!((s.min, s.max), (1.0, 1000.0));
			assert_eq!(s.path, format!("{g}.sample"));
		}
	}

	// Test ID: deea4
	#[test]
	fn time_axis_old_files() {
		let d = Look::default();
		assert_eq!((d[Which::Cpu].time, d[Which::Mem].time), (None, None));
		let l = Look::from_model(&model("cpu:\n\ttime: linear\nmem:\n\ttime: Fixed_B\n"));
		assert_eq!(l[Which::Cpu].time, Some(Timing::Linear));
		assert_eq!(l[Which::Mem].time, Some(Timing::Log(Curve::FixedB)));
		// a file from before has neither, and Scale default rules
		let old = Look::from_model(&model("graph:\n\ttime: linear\n"));
		assert_eq!(old.timing(old[Which::Cpu].time), Timing::Linear);
		let old = Look::from_model(&model("graph:\n\ttime: fixed_a\n"));
		assert_eq!(old.timing(old[Which::Mem].time), Timing::Log(Curve::FixedA));
		assert_eq!(
			Look::from_model(&model("cpu:\n\ttime: squiggly\n"))[Which::Cpu].time,
			None
		);
		let spec = spec();
		for id in ["cpu_time", "mem_time"] {
			let Some(s) = spec.get(id) else {
				panic!("no {id} in settings.shcl");
			};
			// set while its graph is off
			assert_eq!(s.gate, None);
			assert_eq!(s.default.as_text(), "same");
			let picks: Vec<_> = s.options.iter().map(|(k, _)| Timing::parse(k)).collect();
			assert_eq!(picks[0], None, "{id}: same comes first");
			assert_eq!(picks.len(), 5, "{id}: linear and every curve");
			for (i, p) in picks.iter().enumerate().skip(1) {
				assert!(
					p.is_some() && !picks[i + 1..].contains(p),
					"{id}: {:?}",
					s.options[i]
				);
			}
		}
	}

	// Test ID: dej09
	#[test]
	fn disk_all_or_named() {
		assert_eq!(Look::default().drives(), Drives::All);
		assert_eq!(Look::default()[Which::Disk].vertical, None);
		for text in ["disk:\n\tdrives: all\n", "disk:\n\tdrives: \"\"\n"] {
			assert_eq!(
				Look::from_model(&model(text)).drives(),
				Drives::All,
				"{text}"
			);
		}
		assert_eq!(
			Look::from_model(&model("disk:\n\tdrives: [sda, nvme0n1]\n\tlog: true\n")).drives(),
			Drives::Only(vec!["sda".into(), "nvme0n1".into()])
		);
	}

	// Test ID: der3n
	#[test]
	fn old_disk_log_is_log10() {
		let disk = |text: &str| Look::from_model(&model(text))[Which::Disk].vertical;
		assert_eq!(disk("disk:\n\tlog: true\n"), Some(Vertical::Log10));
		assert_eq!(disk("disk:\n\tlog: false\n"), None);
		assert_eq!(
			disk("disk:\n\tlog: true\n\tvertical: squared\n"),
			Some(Vertical::Squared)
		);
		let up = upgraded(&shcl::Document::parse("disk:\n\tlog: true\n")).unwrap();
		assert!(!up.exists("disk.log"));
		assert_eq!(up.get_string("disk.vertical").unwrap(), "log10");
		assert!(upgraded(&shcl::Document::parse("disk:\n\tdrives: sda\n")).is_none());
		// the other graphs and the default stay as they were
		let l = Look::from_model(&model("disk:\n\tlog: true\n"));
		assert_eq!(l.vertical, Vertical::Linear);
		assert_eq!(l[Which::Net].vertical, None);
	}

	// Test ID: df3rj
	#[test]
	fn graph_list_b23_defaults() {
		// b23's widths and vertical scales as of 2026-11-09, but with tukzmon's own graph
		// linear rather than log10, and the user's picks of 500% and 400% for processes and tukzmon
		let l = Look::default();
		let want = [
			(Which::Cpu, 100, 50, None),
			(Which::Mem, 100, 50, None),
			(Which::Procs, 50, 500, Some(Vertical::Squared)),
			(Which::Gpu, 50, 100, None),
			(Which::Vram, 50, 100, None),
			(Which::Net, 50, 100, None),
			(Which::Disk, 50, 100, None),
			(Which::Temp, 50, 100, None),
			(Which::Swap, 25, 200, None),
			(Which::Zswap, 25, 200, None),
			(Which::Load, 25, 200, None),
			(Which::Own, 12, 400, None),
			(Which::Battery, 100, 100, None),
		];
		for (w, width, sample, vertical) in want {
			let g = &l[w];
			assert_eq!(
				(g.width, g.sample, g.vertical),
				(width, sample, vertical),
				"{w:?}"
			);
		}
		assert_eq!(l.scaling(l[Which::Own].vertical), Vertical::Linear);
	}

	// Test ID: der3p
	#[test]
	fn vertical_default_overridable() {
		let l = Look::default();
		assert_eq!(l.vertical, Vertical::Linear);
		// the process count sits near its top, where squared spreads it out
		assert_eq!(l[Which::Procs].vertical, Some(Vertical::Squared));
		assert!(
			Which::ALL
				.into_iter()
				.filter(|&w| w != Which::Procs)
				.all(|w| l[w].vertical.is_none())
		);
		let l = Look::from_model(&model(
			"graph:\n\tvertical: squared\ncpu:\n\tvertical: log10\nmem:\n\tvertical: linear\n",
		));
		assert_eq!(l.scaling(l[Which::Cpu].vertical), Vertical::Log10);
		assert_eq!(l.scaling(l[Which::Mem].vertical), Vertical::Linear);
		assert_eq!(l.scaling(l[Which::Disk].vertical), Vertical::Squared);
		let wobbly = Look::from_model(&model("graph:\n\tvertical: wobbly\n"));
		assert_eq!(wobbly.vertical, Vertical::Linear);
	}

	// Test ID: demw1
	#[test]
	fn lan_split_off_by_default() {
		assert_eq!(Look::default().net_split, Split::Off);
		let l = Look::from_model(&model("net:\n\tsplit: sockets\n"));
		assert_eq!(l.net_split, Split::Sockets);
		let l = Look::from_model(&model("net:\n\tsplit: nope\n"));
		assert_eq!(l.net_split, Split::Off);
	}

	// Test ID: dedqq
	#[test]
	fn order_spaces_or_commas() {
		assert_eq!(
			Look::default().order,
			[
				"cpu", "mem", "procs", "gpu", "vram", "net", "disk", "temp", "swap", "zswap",
				"load", "self", "battery"
			]
		);
		for text in [
			"graph:\n\torder: mem cpu\n",
			"graph:\n\torder: mem,cpu\n",
			"graph:\n\torder: \"MEM  cpu\"\n",
			"graph:\n\torder: [mem, mem, cpu]\n",
			"graph:\n\torder: [mem, cpu]\n",
		] {
			assert_eq!(
				Look::from_model(&model(text)).order,
				["mem", "cpu"],
				"{text}"
			);
		}
		assert_eq!(graph_order("net, \"mem\" 'cpu'"), ["net", "mem", "cpu"]);
		assert!(graph_order(" , ").is_empty());
	}

	// Test ID: decdh
	#[test]
	fn mode_rows_shown() {
		assert!(shown_as_radio("cpu_mode"));
		assert!(!shown_as_radio("theme"));
		assert_eq!(shown_when("user"), Some(("cpu_mode", "detailed")));
		assert_eq!(shown_when("cpu_low"), Some(("cpu_mode", "gradient")));
		assert_eq!(shown_when("back_top"), None);
		// every shown_when names a dropdown and one of its keys
		let spec = spec();
		let named = shcl::Document::parse(SPEC_TEXT)
			.paths()
			.into_iter()
			.filter(|p| p.ends_with(".shown_when"))
			.count();
		let mut found = 0;
		for s in &spec.settings {
			let Some((on, key)) = shown_when(&s.id) else {
				continue;
			};
			found += 1;
			let d = spec.get(on).unwrap();
			assert_eq!(d.control, knobs::Control::Dropdown, "{}", s.id);
			assert!(d.options.iter().any(|(k, _)| k == key), "{}", s.id);
		}
		assert_eq!(found, named, "one didn't parse");
		// a gradient color is in every theme, like the others
		let g = spec.group(THEME).unwrap();
		for id in ["cpu_mode", "cpu_low", "cpu_medium", "cpu_high"] {
			assert!(g.members.iter().any(|m| m == id), "{id}");
		}
	}

	// Test ID: decdj
	#[test]
	fn theme_keeps_mode() {
		let text = "theme:\n\tname: mine\nthemes:\n\tmine:\n\t\tname: Mine\n\t\tcpu_mode: GRADIENT\n\t\tcpu_low: \"#010203\"\n";
		let doc = shcl::Document::parse(text);
		let t = saved_themes(&doc);
		assert_eq!(t[0].colors["cpu_mode"], "gradient");
		assert!(theme_problems(&doc).is_empty());
		let l = Look::from_model(&model(text));
		assert_eq!(l[Which::Cpu].mode, Mode::Gradient);
		assert_eq!(l[Which::Cpu].gradient[0], Rgb(1, 2, 3));
		assert_eq!(saved_themes(&shcl::Document::parse(&themes_text(&t))), t);
		// a mode it can't read is a problem, and reads as Default's
		let bad = text.replace("GRADIENT", "plaid");
		let doc = shcl::Document::parse(&bad);
		assert_eq!(saved_themes(&doc)[0].colors["cpu_mode"], "detailed");
		let p = theme_problems(&doc);
		assert_eq!(p.len(), 1, "{p:?}");
		assert!(p[0].contains("themes.mine.cpu_mode"), "{p:?}");
		// and a color where the mode goes
		let doc = shcl::Document::parse(&text.replace("GRADIENT", "\"#123456\""));
		assert_eq!(theme_problems(&doc).len(), 1);
	}

	// Test ID: deccv
	#[test]
	fn theme_refuses_built_in_key() {
		let text = "themes:\n\tdefault:\n\t\tuser: \"#123456\"\n\tCustom:\n\t\tuser: \"#123456\"\n";
		let doc = shcl::Document::parse(text);
		assert!(saved_themes(&doc).is_empty());
		assert_eq!(theme_problems(&doc).len(), 2);
		assert_eq!(Look::from_model(&model(text)), Look::default());
	}

	// Test ID: deqvz
	#[test]
	fn theme_nonfinite_number_junk() {
		for raw in ["nan", "NaN", "-nan", "inf", "-infinity", "1e999", "junk"] {
			let text =
				format!("theme:\n\tname: x\nthemes:\n\tx:\n\t\tname: X\n\t\tmargin_tb: {raw}\n");
			let t = saved_themes(&shcl::Document::parse(&text));
			assert_eq!(t[0].colors["margin_tb"], "2", "{raw}");
			let l = Look::from_model(&model(&text));
			assert_eq!(l.margin_tb, 2, "{raw}");
		}
	}

	// Test ID: deccw
	#[test]
	fn themes_round_trip() {
		let mut t = saved_themes(&shcl::Document::parse(
			"themes:\n\tx:\n\t\tname: 'Say \"hi\"'\n",
		));
		assert_eq!(t[0].name, "Say \"hi\"");
		assert_eq!(t[0].colors.len(), 127);
		t.push(SavedTheme {
			key: theme_key("Default", t.iter().map(|t| t.key.as_str())),
			name: "Default".into(),
			colors: t[0].colors.clone(),
		});
		assert_eq!(t[1].key, "default_2");
		let back = saved_themes(&shcl::Document::parse(&themes_text(&t)));
		assert_eq!(back, t);
		assert_eq!(themes_text(&[]), "");
	}

	// Test ID: deccx
	#[test]
	fn plain_free_theme_keys() {
		assert_eq!(theme_key("My Green!", []), "my_green");
		assert_eq!(theme_key("  -- ", []), "theme");
		assert_eq!(theme_key("1984", []), "theme_1984");
		assert_eq!(theme_key("Gr\u{fc}n", []), "gr_n");
		assert_eq!(theme_key("custom", []), "custom_2");
		assert_eq!(theme_key("Earth", []), "earth_2");
	}

	// Test ID: decc5
	#[test]
	fn hover_from_file() {
		let text = "graph:\n\thover_time: clock\n\tcolors:\n\t\thover: \"#ffffff\"\n";
		let l = Look::from_model(&Model::load(spec(), text, ""));
		assert_eq!(l.said, Said::Clock);
		assert_eq!(l.hover, Rgb(0xff, 0xff, 0xff));
		let l = Look::from_model(&Model::load(spec(), "graph:\n\thover_time: never\n", ""));
		assert_eq!(l.said, Said::Ago);
	}

	// Test ID: decbx
	#[test]
	fn axis_default_from_file() {
		let pick = |v: &str| {
			let text = format!("graph:\n\ttime: {v}\n");
			Look::from_model(&Model::load(spec(), &text, "")).time
		};
		assert_eq!(pick("fixed_a"), Timing::Log(Curve::FixedA));
		assert_eq!(pick("Fixed_B"), Timing::Log(Curve::FixedB));
		assert_eq!(pick("linear"), Timing::Linear);
		assert_eq!(pick("wobbly"), Timing::Log(Curve::Smooth));
		// the tip's paragraph break is a real one
		let tip = spec()
			.get("time_axis")
			.map(|s| s.tip.clone())
			.unwrap_or_default();
		assert!(tip.contains("\n\nLog A") && !tip.contains("\\n"), "{tip}");
	}

	// Test ID: decbf
	#[test]
	fn four_weeks_by_default() {
		let l = Look::default();
		assert!(l.keep);
		assert_eq!(l.max_age, 28 * 86400);
		let text = "history:\n\tkeep: false\n\tmax_days: 36501\n";
		let l = Look::from_model(&Model::load(spec(), text, ""));
		assert!(!l.keep);
		assert_eq!(l.max_age, 28 * 86400);
	}

	// Test ID: df51d
	#[test]
	fn max_age_sums_parts() {
		let age = |text: &str| Look::from_model(&Model::load(spec(), text, "")).max_age;
		// a file from before hours and minutes reads as it did
		assert_eq!(age("history:\n\tmax_days: 3\n"), 3 * 86400);
		assert_eq!(
			age("history:\n\tmax_days: 1\n\tmax_hours: 2\n\tmax_minutes: 30\n"),
			86400 + 2 * 3600 + 30 * 60
		);
		assert_eq!(age("history:\n\tmax_days: 0\n\tmax_hours: 6\n"), 6 * 3600);
		assert_eq!(age("history:\n\tmax_days: 0\n"), 60);
		assert_eq!(
			age("history:\n\tmax_days: 36500\n\tmax_hours: 23\n\tmax_minutes: 59\n"),
			36500 * 86400 + 86340
		);
		// past a day's hours or an hour's minutes is a bad line
		assert_eq!(
			age("history:\n\tmax_hours: 24\n\tmax_minutes: 60\n"),
			28 * 86400
		);
	}

	// Test ID: debhk
	#[test]
	fn overrides_and_bad_lines() {
		let text = "graph:\n\twidth: 200\n\tsample_ms: 99999\ncpu:\n\tcolors:\n\t\tuser: \"#123456\"\n\t\tnice: red\n";
		let l = Look::from_model(&Model::load(spec(), text, ""));
		assert_eq!(l.width, 200);
		assert_eq!(l.sample_ms, 1000);
		assert_eq!(l[Which::Cpu].colors[2], Rgb(0x12, 0x34, 0x56));
		assert_eq!(l[Which::Cpu].colors[3], Rgb(0x20, 0x5f, 0x40));
	}

	fn scratch(name: &str) -> PathBuf {
		let dir =
			std::env::temp_dir().join(format!("test_tukzmon_config_{}_{name}", std::process::id()));
		let _ = std::fs::remove_dir_all(&dir);
		std::fs::create_dir_all(&dir).unwrap();
		dir
	}

	// Out since 2026-11-07: nothing reads or rewrites a file by shcl 2 rules, per
	// backlog 2026110709261106. df03n checks a file with no Format line reads by
	// shcl 3 rules.
	// // Test ID: deyr0
	// #[test]
	// fn shcl2_reads_as_before() {
	// 	let old = "graph:\n\torder: mem, cpu\n\ndisk:\n\tdrives: sda, nvme0n1\n\n\
	// 		themes:\n\tx:\n\t\tname: \"Say \\\"hi\\\"\"\n";
	// 	let l = Look::from_model(&model(old));
	// 	assert_eq!(l.order, ["mem", "cpu"]);
	// 	assert_eq!(
	// 		l.drives(),
	// 		Drives::Only(vec!["sda".into(), "nvme0n1".into()])
	// 	);
	// 	let t = saved_themes(&shcl::Document::parse(&as_shcl3(old)));
	// 	assert_eq!(t[0].name, "Say \"hi\"");
	//
	// 	// a file that loads clean, or says it's for shcl 3, is read as written
	// 	for text in ["graph:\n\torder: mem cpu\n", "graph:\n\twidth: [1, 2]\n"] {
	// 		assert!(matches!(as_shcl3(text), Cow::Borrowed(_)), "{text}");
	// 	}
	// 	let stamped = format!("{}\ngraph:\n\torder: mem, cpu\n", shcl::FORMAT_LINE);
	// 	assert!(matches!(as_shcl3(&stamped), Cow::Borrowed(_)));
	// }

	// // Test ID: deyr1
	// #[test]
	// #[cfg_attr(miri, ignore = "the write fsyncs its folder, which Miri can't")]
	// fn shcl2_upgrade_once() {
	// 	let dir = scratch("upgrade");
	// 	let path = dir.join("plugin-3.shcl");
	// 	assert_eq!(upgrade_file(&path), Ok(None));
	// 	let old = "graph:\n\torder: mem, cpu\n";
	// 	std::fs::write(&path, old).unwrap();
	// 	let kept = upgrade_file(&path).unwrap().unwrap();
	// 	assert_eq!(std::fs::read_to_string(&kept).unwrap(), old);
	// 	let new = std::fs::read_to_string(&path).unwrap();
	// 	assert_eq!(
	// 		shcl::format_version(&new),
	// 		Some(shcl::FORMAT_MAJOR),
	// 		"{new}"
	// 	);
	// 	assert_eq!(Look::load(&path).unwrap().order, ["mem", "cpu"]);
	// 	assert_eq!(upgrade_file(&path), Ok(None));
	// 	assert_eq!(std::fs::read_to_string(&path).unwrap(), new);
	//
	// 	let clean = dir.join("plugin-4.shcl");
	// 	std::fs::write(&clean, "graph:\n\twidth: 77\n").unwrap();
	// 	assert_eq!(upgrade_file(&clean), Ok(None));
	// 	assert_eq!(
	// 		std::fs::read_to_string(&clean).unwrap(),
	// 		"graph:\n\twidth: 77\n"
	// 	);
	// 	let _ = std::fs::remove_dir_all(&dir);
	// }

	// Test ID: df03n
	#[test]
	fn unstamped_file_shcl3() {
		// a 2.x list is an error in 3, so its line goes
		let l = Look::from_model(&model("graph:\n\torder: mem, cpu\n"));
		assert_eq!(l.order, Look::default().order);
		let l = Look::from_model(&model("graph:\n\torder: [mem, cpu]\n"));
		assert_eq!(l.order, ["mem", "cpu"]);
	}

	// Test ID: defy3
	#[test]
	fn new_item_copies_shared() {
		let dir = scratch("start");
		let shared = dir.join("tukzmon.shcl");
		let text = "graph:\n\twidth: 77\n\ncpu:\n\tcolors:\n\t\tuser: \"#123456\"\n";
		std::fs::write(&shared, text).unwrap();
		let own = instance_file(&dir, 7);
		assert_eq!(own, dir.join("plugin-7.shcl"));
		start_from(&own, &shared).unwrap();
		assert_eq!(std::fs::read_to_string(&own).unwrap(), text);
		assert_eq!(std::fs::read_to_string(&shared).unwrap(), text);
		assert_eq!(Look::load(&own).unwrap(), Look::load(&shared).unwrap());
		assert_ne!(Look::load(&own).unwrap(), Look::default());
		assert!(!dir.join("plugin-7.shcl.tmp").exists());

		// its own file wins from then on
		std::fs::write(&own, "cpu:\n\tcolors:\n\t\tuser: \"#654321\"\n").unwrap();
		start_from(&own, &shared).unwrap();
		assert!(std::fs::read_to_string(&own).unwrap().contains("#654321"));

		// with no shared file there's nothing to copy, and it has the defaults
		let fresh = instance_file(&dir, 8);
		start_from(&fresh, &dir.join("missing.shcl")).unwrap();
		assert!(!fresh.exists());
		assert_eq!(Look::load(&fresh).unwrap(), Look::default());
		let _ = std::fs::remove_dir_all(&dir);
	}

	// Test ID: deqw2
	#[cfg(unix)]
	#[test]
	#[cfg_attr(miri, ignore = "makes symlinks")]
	fn dangling_link_filled() {
		let dir = scratch("link");
		let shared = dir.join("tukzmon.shcl");
		std::fs::write(&shared, "graph:\n\twidth: 77\n").unwrap();
		let own = instance_file(&dir, 3);
		std::os::unix::fs::symlink("elsewhere.shcl", &own).unwrap();
		assert_eq!(through_links(&own), dir.join("elsewhere.shcl"));
		assert_eq!(through_links(&shared), shared);
		start_from(&own, &shared).unwrap();
		assert!(std::fs::symlink_metadata(&own).unwrap().is_symlink());
		assert_eq!(Look::load(&dir.join("elsewhere.shcl")).unwrap().width, 77);
		let _ = std::fs::remove_dir_all(&dir);
	}

	// Test ID: defy4
	#[test]
	fn save_leaves_sibling() {
		let dir = scratch("two");
		let shared = dir.join("tukzmon.shcl");
		std::fs::write(&shared, "graph:\n\twidth: 77\n").unwrap();
		let (one, two) = (instance_file(&dir, 1), instance_file(&dir, 2));
		assert_ne!(one, two);
		start_from(&one, &shared).unwrap();
		start_from(&two, &shared).unwrap();
		std::fs::write(&one, "mem:\n\tshow: false\n").unwrap();
		assert!(!Look::load(&one).unwrap()[Which::Mem].show);
		assert!(Look::load(&two).unwrap()[Which::Mem].show);
		assert_eq!(Look::load(&two).unwrap().width, 77);
		assert_eq!(Look::load(&two).unwrap(), Look::load(&shared).unwrap());
		let _ = std::fs::remove_dir_all(&dir);
	}

	// Test ID: df36b
	#[test]
	fn reused_id_starts_fresh() {
		let dir = scratch("claim");
		let shared = dir.join("tukzmon.shcl");
		std::fs::write(&shared, "shared\n").unwrap();
		let rc = dir.join("tukzmon-4.rc");
		let own = instance_file(&dir, 4);
		let hist = dir.join("plugin-4.hist");
		std::fs::write(&own, "old look\n").unwrap();
		std::fs::write(&hist, "old history").unwrap();
		std::fs::write(mark_of(&own), "").unwrap();
		// the panel dropped the item and made a new one, deleting its rc
		claim(&rc, &own, &hist).unwrap();
		assert_eq!((own.exists(), hist.exists()), (false, false));
		assert!(rc.exists() && mark_of(&own).exists());
		start_from(&own, &shared).unwrap();
		assert_eq!(std::fs::read_to_string(&own).unwrap(), "shared\n");
		let _ = std::fs::remove_dir_all(&dir);
	}

	// Test ID: df36c
	#[test]
	fn older_items_keep_files() {
		let dir = scratch("keep");
		let rc = dir.join("tukzmon-5.rc");
		let own = instance_file(&dir, 5);
		let hist = dir.join("plugin-5.hist");
		std::fs::write(&own, "mine\n").unwrap();
		std::fs::write(&hist, "history").unwrap();
		// neither mark yet, as an older build left it
		claim(&rc, &own, &hist).unwrap();
		assert!(rc.exists() && mark_of(&own).exists());
		// and every start after
		claim(&rc, &own, &hist).unwrap();
		assert_eq!(std::fs::read_to_string(&own).unwrap(), "mine\n");
		assert_eq!(std::fs::read_to_string(&hist).unwrap(), "history");
		assert_eq!(mark_of(&own), dir.join("plugin-5.mark"));
		// an rc that can't be written leaves no mark, so the files aren't
		// taken for an earlier item's at the next start
		let blocked = dir.join("plugin-6.shcl");
		assert!(claim(&dir.join("no/tukzmon-6.rc"), &blocked, &hist).is_err());
		assert!(!mark_of(&blocked).exists());
		let _ = std::fs::remove_dir_all(&dir);
	}

	// Test ID: debhm
	#[test]
	fn missing_file_defaults() {
		assert_eq!(
			Look::load(Path::new("/nonexistent/tukzmon.shcl")).unwrap(),
			Look::default()
		);
	}

	// Test ID: dena8
	#[test]
	fn unreadable_file_errors() {
		let dir = scratch("unreadable");
		// a folder, so it can't be read as root either
		let folder = dir.join("plugin-3.shcl");
		std::fs::create_dir(&folder).unwrap();
		let not_text = dir.join("plugin-4.shcl");
		std::fs::write(&not_text, b"graph:\n\twidth: \xff\n").unwrap();
		let got = (Look::load(&folder), Look::load(&not_text));
		let _ = std::fs::remove_dir_all(&dir);
		assert!(got.0.is_err(), "{:?}", got.0.map(|l| l.width));
		assert!(got.1.is_err(), "{:?}", got.1.map(|l| l.width));
	}

	// Test ID: debhn
	#[test]
	fn rgb_parsing() {
		assert_eq!(Rgb::parse("#0a0B0c"), Some(Rgb(10, 11, 12)));
		assert_eq!(Rgb(1, 2, 3).argb(), 0xff01_0203);
		for bad in [
			"",
			"#",
			"#12345",
			"123456",
			"#12345g",
			"#1234567",
			"#1234\u{e9}",
			"#+12345",
		] {
			assert_eq!(Rgb::parse(bad), None, "{bad}");
		}
	}

	// Test ID: dezbw
	#[test]
	fn shared_of_outline_ids() {
		let listed = |id: &str| {
			outline_pairs()
				.find(|(_, own)| own == id)
				.map(|(shared, _)| shared)
		};
		let spec = spec();
		let odd = [
			"border_color",
			"_border_color",
			"x_border_color",
			"cpuback_top",
			"cpu__back_top",
			"cpu_back_top_",
			"Cpu_back_top",
		];
		let ids = spec.settings.iter().map(|s| s.id.as_str()).chain(odd);
		let mut found = 0;
		for id in ids {
			assert_eq!(shared_of(id), listed(id), "{id}");
			found += usize::from(shared_of(id).is_some());
		}
		assert_eq!(found, 3 * LEADS.len());
		for w in Which::ALL {
			for (shared, own) in SHARED_OUTLINE.iter().zip(&w.ids().outline) {
				assert_eq!(shared_of(own), Some(*shared), "{own}");
			}
		}
	}

	/// A settings file as the window would save it: some saved themes, a
	/// pick, and a few rows set off their defaults. Without its header.
	fn rolled_settings(dice: &mut crate::fuzz::Dice) -> String {
		use knobs::{Control, Value};
		let plain = saved_themes(&shcl::Document::parse("themes:\n\tx:\n\t\tname: X\n"));
		let mut themes: Vec<SavedTheme> = Vec::new();
		for _ in 0..dice.below(3) {
			let name = *dice.pick(&[
				"Mine",
				"Default",
				"night owl",
				"2nd",
				"Say \"hi\"",
				"\u{e9}t\u{e9}",
			]);
			let mut colors = plain[0].colors.clone();
			for v in colors.values_mut().filter(|v| v.starts_with('#')) {
				if dice.one_in(4) {
					*v = format!("#{:06x}", dice.next() & 0xff_ffff);
				}
			}
			let key = theme_key(name, themes.iter().map(|t| t.key.as_str()));
			themes.push(SavedTheme {
				key,
				name: name.into(),
				colors,
			});
		}
		let spec = spec_with(&themes);
		let mut m = Model::new(Arc::clone(&spec));
		for s in spec
			.settings
			.iter()
			.filter(|s| s.control != Control::Heading)
		{
			if !dice.one_in(8) {
				continue;
			}
			let span = |d: &mut crate::fuzz::Dice| {
				let (lo, hi) = (s.min.max(-1e6), s.max.min(1e6));
				lo + d.unit() * (hi - lo).max(0.0)
			};
			let v = match &s.default {
				Value::Bool(b) => Value::Bool(!b),
				Value::Int(_) => Value::Int(span(dice).round() as i64),
				Value::Float(_) => Value::Float(span(dice)),
				Value::Text(_) if !s.options.is_empty() => {
					Value::Text(dice.pick(&s.options).0.clone())
				}
				Value::Text(_) if s.control == Control::Color => {
					Value::Text(format!("#{:06x}", dice.next() & 0xff_ffff))
				}
				Value::Text(_) => {
					let words = [
						"",
						"sda",
						"mem,cpu",
						"[cpu, mem]",
						"a b",
						"x: y",
						"'q'",
						"# not",
					];
					Value::Text((*dice.pick(&words)).to_string())
				}
			};
			m.set(&s.id, &v, &NoEnv);
		}
		name_the_theme(&mut m);
		let (body, _) = m.save(&NoEnv);
		format!("{body}{}", themes_text(&themes))
	}

	/// What the window writes back for a file it read, apart from its
	/// comments and header.
	fn written(text: &str) -> String {
		let doc = shcl::Document::parse(text);
		let themes = saved_themes(&doc);
		let spec = spec_with(&themes);
		let mut m = model_on(Arc::clone(&spec), &doc);
		for s in spec
			.settings
			.iter()
			.filter(|s| s.control != knobs::Control::Heading)
		{
			let v = m.value(&s.id, &NoEnv);
			assert_eq!(
				spec.valid(s, &v),
				Ok(v.clone()),
				"{} took a value it refuses",
				s.id
			);
		}
		let _ = Look::from_model(&m);
		let _ = theme_problems(&doc);
		name_the_theme(&mut m);
		let (body, _) = m.save(&NoEnv);
		format!("{body}{}", themes_text(&themes))
	}

	/// Whether the reader took it all: whole UTF-8 with no line shcl
	/// dropped. Either way what it did take writes back the same twice.
	fn reads_settings(bytes: &[u8]) -> bool {
		let text = String::from_utf8_lossy(bytes);
		let once = written(&text);
		assert_eq!(written(&once), once, "what was read writes back the same");
		matches!(text, std::borrow::Cow::Borrowed(_))
			&& shcl::Document::parse(&text).error_count() == 0
	}

	// Test ID: df4g5
	#[test]
	#[cfg_attr(miri, ignore = "a spec parse takes about an hour under Miri")]
	fn fuzz_settings_files() {
		let header = "## tukzmon settings\n";
		let tally = crate::fuzz::run("settings file", |dice| {
			let body = rolled_settings(dice);
			let mut bytes = format!("{header}{}\n{body}", shcl::FORMAT_LINE).into_bytes();
			if dice.one_in(4) {
				assert_eq!(
					written(&String::from_utf8_lossy(&bytes)),
					body,
					"a whole file reads as written"
				);
			} else {
				crate::fuzz::mangle(dice, &mut bytes);
			}
			reads_settings(&bytes)
		});
		tally.assert_mixed();
	}

	// Test ID: df4g6
	#[test]
	#[cfg_attr(miri, ignore = "a spec parse takes about an hour under Miri")]
	fn fuzz_settings_bit_flips() {
		let real = include_bytes!("../tests/data/plugin-7.shcl");
		assert!(reads_settings(real), "the file itself reads whole");
		let tally = crate::fuzz::run("plugin-7.shcl", |dice| {
			let mut bytes = real.to_vec();
			crate::fuzz::mangle(dice, &mut bytes);
			reads_settings(&bytes)
		});
		tally.assert_mixed();
	}
}
