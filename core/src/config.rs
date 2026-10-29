// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! Settings, as knobs reads them from `settings.shcl` and the config file.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use knobs::{Model, NoEnv, Spec};

use crate::axis::{Curve, Timing};
use crate::disk::Drives;
use crate::render::Paint;
use crate::when::Said;

pub const SPEC_TEXT: &str = include_str!("settings.shcl");

/// The parsed spec. A test proves it parses, so a bad one never gets built.
pub fn spec() -> Arc<Spec> {
	static SPEC: OnceLock<Arc<Spec>> = OnceLock::new();
	SPEC.get_or_init(|| {
		#[allow(clippy::expect_used, reason = "compiled in, and parse_spec checks it")]
		Arc::new(Spec::parse(SPEC_TEXT).expect("settings.shcl"))
	})
	.clone()
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
	pub fn parse(s: &str) -> Option<Mode> {
		match s.to_ascii_lowercase().as_str() {
			"detailed" => Some(Mode::Detailed),
			"gradient" => Some(Mode::Gradient),
			_ => None,
		}
	}
}

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
	pub key: String,
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
		_ => v.show(),
	}
}

// a theme member's text as the spec stores it, or None if it won't take it
fn theme_typed(spec: &Spec, id: &str, raw: &str) -> Option<knobs::Value> {
	let s = spec.get(id)?;
	let v = match s.kind {
		knobs::Kind::Int | knobs::Kind::Float => knobs::Value::Float(raw.trim().parse().ok()?),
		knobs::Kind::Bool => match raw.trim() {
			"true" | "on" => knobs::Value::Bool(true),
			"false" | "off" => knobs::Value::Bool(false),
			_ => return None,
		},
		_ => knobs::Value::Text(raw.to_string()),
	};
	spec.valid(s, &v).ok()
}

fn theme_value(spec: &Spec, id: &str, raw: &str) -> Option<String> {
	theme_typed(spec, id, raw).map(|v| theme_text(&v))
}

pub const THEMES: &str = "themes";

fn builtin_key(key: &str) -> bool {
	key.eq_ignore_ascii_case(knobs::CUSTOM)
		|| spec()
			.group(THEME)
			.is_some_and(|g| g.presets.iter().any(|p| p.key.eq_ignore_ascii_case(key)))
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
				.unwrap_or_else(|| key.clone());
			SavedTheme { key, name, colors }
		})
		.collect()
}

/// Lines under `themes` that [`saved_themes`] can't read, so a save would
/// drop or change them.
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
	let mut s = (*base).clone();
	if let Some(g) = s.groups.iter_mut().find(|g| g.id == THEME) {
		for t in themes {
			g.presets.push(knobs::Preset {
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
				_ => {
					let _ = doc.set_string(&at(id), raw);
				}
			}
		}
	}
	doc.to_canonical()
}

/// A key for a new theme from its name, not one that's taken.
pub fn theme_key(name: &str, themes: &[SavedTheme]) -> String {
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
	let taken = |k: &str| builtin_key(k) || themes.iter().any(|t| t.key.eq_ignore_ascii_case(k));
	if !taken(&slug) {
		return slug;
	}
	// n themes can take at most n of these
	(2..=themes.len() + 2)
		.map(|n| format!("{slug}_{n}"))
		.find(|k| !taken(k))
		.unwrap_or(slug)
}

// The Outline sub-tab's border and background ids, each with one graph's.
fn outline_pairs() -> impl Iterator<Item = (&'static str, String)> {
	["border_color", "back_top", "back_bottom"]
		.into_iter()
		.flat_map(|shared| {
			LEADS
				.iter()
				.map(move |(g, _)| (shared, format!("{g}_{shared}")))
		})
}

fn shared_of(id: &str) -> Option<&'static str> {
	outline_pairs()
		.find(|(_, own)| own == id)
		.map(|(shared, _)| shared)
}

// Each graph got its own border and background after the shared ones, so a
// file that set only those keeps its look.
fn outlines_follow_shared(values: &mut BTreeMap<String, knobs::Value>) {
	for (shared, own) in outline_pairs() {
		if values.contains_key(&own) {
			continue;
		}
		if let Some(v) = values.get(shared).cloned() {
			values.insert(own, v);
		}
	}
}

/// A parsed config file as a model on `spec`, with [`unthemed_is_custom`]
/// applied.
pub fn model_on(spec: Arc<Spec>, doc: &shcl::Document) -> Model {
	let mut m = Model::load_docs(spec, doc, &shcl::Document::new());
	outlines_follow_shared(&mut m.values.own);
	outlines_follow_shared(&mut m.values.changes);
	unthemed_is_custom(&mut m);
	m
}

/// A config file's text as a model, saved themes and all.
pub fn model(text: &str) -> Model {
	let doc = shcl::Document::parse(text);
	model_on(spec_with(&saved_themes(&doc)), &doc)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
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
	pub border: Rgb,
	pub top: Rgb,
	pub bottom: Rgb,
}

/// Each graph's section and its main series, the color it reads as in
/// Detailed. In Gradient that's its Medium. See design.md Theme.
pub const LEADS: [(&str, &str); 10] = [
	("cpu", "user"),
	("mem", "mem_user"),
	("net", "net_received"),
	("swap", "swap_used"),
	("zswap", "zswap_used"),
	("load", "load_used"),
	("disk", "disk_read"),
	("temp", "temp_normal"),
	("battery", "battery_charging"),
	("self", "self_used"),
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

#[derive(Clone, Debug, PartialEq)]
#[allow(
	clippy::struct_excessive_bools,
	reason = "each is a checkbox in settings.shcl, not a state"
)]
pub struct Look {
	pub width: u32,
	/// What a graph with no time axis of its own follows.
	pub time: Timing,
	pub sample_ms: u32,
	/// What the hover tip says about the time under the pointer.
	pub said: Said,
	/// 0 to 1.
	pub spikes: f32,
	pub border: u32,
	/// Logical pixels clear above and below each graph, and either side of it.
	/// See `graphs::Graphs::tiles` for the item's ends.
	pub margin_tb: u32,
	pub margin_lr: u32,
	/// Save history and load it back on start.
	pub keep: bool,
	pub max_days: u32,
	/// In `cpu::SERIES` order, bottom first.
	pub cpu: [Rgb; 4],
	pub cpu_mode: Mode,
	/// Low, Medium and High.
	pub cpu_gradient: [Rgb; 3],
	/// Draw the CPU graph. It's drawn anyway when no other graph is.
	pub show_cpu: bool,
	/// A percentage of `width`. See [`Look::share`].
	pub cpu_width: u32,
	/// 0 follows `sample_ms`. See [`Look::rate`].
	pub cpu_sample_ms: u32,
	/// None follows `time`. See [`Look::timing`].
	pub cpu_time: Option<Timing>,
	/// Draw the memory graph, where the box has one.
	pub show_memory: bool,
	/// A percentage of `width`. See [`Look::share`].
	pub mem_width: u32,
	pub mem_sample_ms: u32,
	pub mem_time: Option<Timing>,
	/// Draw the swap graph, where the box has swap.
	pub show_swap: bool,
	/// A percentage of `width`. See [`Look::share`].
	pub swap_width: u32,
	pub swap_sample_ms: u32,
	pub swap_time: Option<Timing>,
	/// Draw the zswap graph, where the box has zswap on or zram set up.
	pub show_zswap: bool,
	/// A percentage of `width`. See [`Look::share`].
	pub zswap_width: u32,
	pub zswap_sample_ms: u32,
	pub zswap_time: Option<Timing>,
	/// Draw the network graph.
	pub show_net: bool,
	/// A percentage of `width`. See [`Look::share`].
	pub net_width: u32,
	pub net_sample_ms: u32,
	pub net_time: Option<Timing>,
	/// Draw the load graph.
	pub show_load: bool,
	/// A percentage of `width`. See [`Look::share`].
	pub load_width: u32,
	pub load_sample_ms: u32,
	pub load_time: Option<Timing>,
	/// Draw the disk graph, where the box has a disk.
	pub show_disk: bool,
	/// A percentage of `width`. See [`Look::share`].
	pub disk_width: u32,
	pub disk_sample_ms: u32,
	pub disk_time: Option<Timing>,
	/// A log scale up the disk graph.
	pub disk_log: bool,
	/// `/proc/diskstats` names, or `all`. See [`Look::drives`].
	pub disk_drives: Vec<String>,
	/// Draw the CPU temperature graph, where the box has a sensor for it.
	pub show_temp: bool,
	/// A percentage of `width`. See [`Look::share`].
	pub temp_width: u32,
	pub temp_sample_ms: u32,
	pub temp_time: Option<Timing>,
	/// Draw the battery graph, where the box has a battery in.
	pub show_battery: bool,
	/// A percentage of `width`. See [`Look::share`].
	pub battery_width: u32,
	pub battery_sample_ms: u32,
	pub battery_time: Option<Timing>,
	/// Draw the graph of tukzmon's own CPU use.
	pub show_self: bool,
	/// A percentage of `width`. See [`Look::share`].
	pub self_width: u32,
	pub self_sample_ms: u32,
	pub self_time: Option<Timing>,
	/// Graph names as saved, first drawn first. See [`graph_order`].
	pub order: Vec<String>,
	/// In `mem::MemoryReader::SERIES` order, bottom first.
	pub mem: [Rgb; 3],
	pub mem_mode: Mode,
	pub mem_gradient: [Rgb; 3],
	pub swap: [Rgb; 1],
	pub swap_mode: Mode,
	pub swap_gradient: [Rgb; 3],
	pub zswap: [Rgb; 1],
	pub zswap_mode: Mode,
	pub zswap_gradient: [Rgb; 3],
	/// In `net::NetReader::SERIES` order.
	pub net: [Rgb; 3],
	pub net_mode: Mode,
	pub net_gradient: [Rgb; 3],
	pub load: [Rgb; 1],
	pub load_mode: Mode,
	pub load_gradient: [Rgb; 3],
	/// In `disk::DiskReader::SERIES` order.
	pub disk: [Rgb; 2],
	pub disk_mode: Mode,
	pub disk_gradient: [Rgb; 3],
	/// In `temp::TempReader::SERIES` order.
	pub temp: [Rgb; 2],
	pub temp_mode: Mode,
	pub temp_gradient: [Rgb; 3],
	/// In `battery::BatteryReader::SERIES` order.
	pub battery: [Rgb; 3],
	pub battery_mode: Mode,
	pub battery_gradient: [Rgb; 3],
	pub self_colors: [Rgb; 1],
	pub self_mode: Mode,
	pub self_gradient: [Rgb; 3],
	pub cpu_outline: Outline,
	pub mem_outline: Outline,
	pub swap_outline: Outline,
	pub zswap_outline: Outline,
	pub net_outline: Outline,
	pub load_outline: Outline,
	pub disk_outline: Outline,
	pub temp_outline: Outline,
	pub battery_outline: Outline,
	pub self_outline: Outline,
	/// The Outline sub-tab's. A graph draws with its own unless
	/// `border_all` or `back_all` is on, and then the `*_outline` are these.
	pub border_color: Rgb,
	/// No background, so the panel shows through, whatever the theme.
	pub transparent: bool,
	pub top: Rgb,
	pub bottom: Rgb,
	pub hover: Rgb,
}

impl Default for Look {
	fn default() -> Self {
		Look::from_model(&Model::new(spec()))
	}
}

impl Look {
	/// The model as read from a file, so [`unthemed_is_custom`] applies.
	pub fn from_model(m: &Model) -> Look {
		let custom;
		let m = if unthemed(m) {
			custom = {
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
		// the Outline sub-tab's go over every graph's own when ticked
		let outline = |g: &str| {
			let pick = |all: bool, shared: &str| {
				color(&if all {
					shared.to_string()
				} else {
					format!("{g}_{shared}")
				})
			};
			Outline {
				border: pick(border_all, "border_color"),
				top: pick(back_all, "back_top"),
				bottom: pick(back_all, "back_bottom"),
			}
		};
		Look {
			width: num("width") as u32,
			time: Timing::parse(m.value("time_axis", &NoEnv).as_text())
				.unwrap_or(Timing::Log(Curve::Smooth)),
			sample_ms: num("sample_ms") as u32,
			said: Said::parse(m.value("hover_time", &NoEnv).as_text()).unwrap_or_default(),
			spikes: (num("spikes") / 100.0) as f32,
			border: num("border") as u32,
			margin_tb: num("margin_tb") as u32,
			margin_lr: num("margin_lr") as u32,
			keep: m.value("keep_history", &NoEnv).as_bool(),
			max_days: num("max_days") as u32,
			cpu: [
				color("iowait"),
				color("system"),
				color("user"),
				color("nice"),
			],
			cpu_mode: Mode::parse(m.value("cpu_mode", &NoEnv).as_text()).unwrap_or_default(),
			cpu_gradient: [color("cpu_low"), color("cpu_medium"), color("cpu_high")],
			show_cpu: m.value("show_cpu", &NoEnv).as_bool(),
			cpu_width: num("cpu_width") as u32,
			cpu_sample_ms: num("cpu_sample_ms") as u32,
			cpu_time: Timing::parse(m.value("cpu_time", &NoEnv).as_text()),
			show_memory: m.value("show_memory", &NoEnv).as_bool(),
			mem_width: num("mem_width") as u32,
			mem_sample_ms: num("mem_sample_ms") as u32,
			mem_time: Timing::parse(m.value("mem_time", &NoEnv).as_text()),
			show_swap: m.value("show_swap", &NoEnv).as_bool(),
			swap_width: num("swap_width") as u32,
			swap_sample_ms: num("swap_sample_ms") as u32,
			swap_time: Timing::parse(m.value("swap_time", &NoEnv).as_text()),
			show_zswap: m.value("show_zswap", &NoEnv).as_bool(),
			zswap_width: num("zswap_width") as u32,
			zswap_sample_ms: num("zswap_sample_ms") as u32,
			zswap_time: Timing::parse(m.value("zswap_time", &NoEnv).as_text()),
			show_net: m.value("show_net", &NoEnv).as_bool(),
			net_width: num("net_width") as u32,
			net_sample_ms: num("net_sample_ms") as u32,
			net_time: Timing::parse(m.value("net_time", &NoEnv).as_text()),
			show_load: m.value("show_load", &NoEnv).as_bool(),
			load_width: num("load_width") as u32,
			load_sample_ms: num("load_sample_ms") as u32,
			load_time: Timing::parse(m.value("load_time", &NoEnv).as_text()),
			show_disk: m.value("show_disk", &NoEnv).as_bool(),
			disk_width: num("disk_width") as u32,
			disk_sample_ms: num("disk_sample_ms") as u32,
			disk_time: Timing::parse(m.value("disk_time", &NoEnv).as_text()),
			disk_log: m.value("disk_log", &NoEnv).as_bool(),
			disk_drives: graph_order(m.value("disk_drives", &NoEnv).as_text()),
			show_temp: m.value("show_temp", &NoEnv).as_bool(),
			temp_width: num("temp_width") as u32,
			temp_sample_ms: num("temp_sample_ms") as u32,
			temp_time: Timing::parse(m.value("temp_time", &NoEnv).as_text()),
			show_battery: m.value("show_battery", &NoEnv).as_bool(),
			battery_width: num("battery_width") as u32,
			battery_sample_ms: num("battery_sample_ms") as u32,
			battery_time: Timing::parse(m.value("battery_time", &NoEnv).as_text()),
			show_self: m.value("show_self", &NoEnv).as_bool(),
			self_width: num("self_width") as u32,
			self_sample_ms: num("self_sample_ms") as u32,
			self_time: Timing::parse(m.value("self_time", &NoEnv).as_text()),
			order: graph_order(m.value("graph_order", &NoEnv).as_text()),
			mem: [color("mem_user"), color("mem_buffers"), color("mem_cache")],
			mem_mode: Mode::parse(m.value("mem_mode", &NoEnv).as_text()).unwrap_or_default(),
			mem_gradient: [color("mem_low"), color("mem_medium"), color("mem_high")],
			swap: [color("swap_used")],
			swap_mode: Mode::parse(m.value("swap_mode", &NoEnv).as_text()).unwrap_or_default(),
			swap_gradient: [color("swap_low"), color("swap_medium"), color("swap_high")],
			zswap: [color("zswap_used")],
			zswap_mode: Mode::parse(m.value("zswap_mode", &NoEnv).as_text()).unwrap_or_default(),
			zswap_gradient: [
				color("zswap_low"),
				color("zswap_medium"),
				color("zswap_high"),
			],
			net: [color("net_local"), color("net_sent"), color("net_received")],
			net_mode: Mode::parse(m.value("net_mode", &NoEnv).as_text()).unwrap_or_default(),
			net_gradient: [color("net_low"), color("net_medium"), color("net_high")],
			load: [color("load_used")],
			load_mode: Mode::parse(m.value("load_mode", &NoEnv).as_text()).unwrap_or_default(),
			load_gradient: [color("load_low"), color("load_medium"), color("load_high")],
			disk: [color("disk_write"), color("disk_read")],
			disk_mode: Mode::parse(m.value("disk_mode", &NoEnv).as_text()).unwrap_or_default(),
			disk_gradient: [color("disk_low"), color("disk_medium"), color("disk_high")],
			temp: [color("temp_normal"), color("temp_critical")],
			temp_mode: Mode::parse(m.value("temp_mode", &NoEnv).as_text()).unwrap_or_default(),
			temp_gradient: [color("temp_low"), color("temp_medium"), color("temp_high")],
			battery: [
				color("battery_charging"),
				color("battery_discharging"),
				color("battery_critical"),
			],
			battery_mode: Mode::parse(m.value("battery_mode", &NoEnv).as_text())
				.unwrap_or_default(),
			battery_gradient: [
				color("battery_low"),
				color("battery_medium"),
				color("battery_high"),
			],
			self_colors: [color("self_used")],
			self_mode: Mode::parse(m.value("self_mode", &NoEnv).as_text()).unwrap_or_default(),
			self_gradient: [color("self_low"), color("self_medium"), color("self_high")],
			cpu_outline: outline("cpu"),
			mem_outline: outline("mem"),
			swap_outline: outline("swap"),
			zswap_outline: outline("zswap"),
			net_outline: outline("net"),
			load_outline: outline("load"),
			disk_outline: outline("disk"),
			temp_outline: outline("temp"),
			battery_outline: outline("battery"),
			self_outline: outline("self"),
			border_color: color("border_color"),
			transparent: m.value("transparent", &NoEnv).as_bool(),
			top: color("back_top"),
			bottom: color("back_bottom"),
			hover: color("hover_color"),
		}
	}

	pub fn cpu_paint(&self) -> Paint<'_, 4> {
		match self.cpu_mode {
			Mode::Detailed => Paint::Stack(&self.cpu),
			Mode::Gradient => Paint::Gradient(self.cpu_gradient),
		}
	}

	pub fn mem_paint(&self) -> Paint<'_, 3> {
		match self.mem_mode {
			Mode::Detailed => Paint::Stack(&self.mem),
			Mode::Gradient => Paint::Gradient(self.mem_gradient),
		}
	}

	pub fn swap_paint(&self) -> Paint<'_, 1> {
		match self.swap_mode {
			Mode::Detailed => Paint::Stack(&self.swap),
			Mode::Gradient => Paint::Gradient(self.swap_gradient),
		}
	}

	pub fn zswap_paint(&self) -> Paint<'_, 1> {
		match self.zswap_mode {
			Mode::Detailed => Paint::Stack(&self.zswap),
			Mode::Gradient => Paint::Gradient(self.zswap_gradient),
		}
	}

	pub fn net_paint(&self) -> Paint<'_, 3> {
		match self.net_mode {
			Mode::Detailed => Paint::Stack(&self.net),
			Mode::Gradient => Paint::Gradient(self.net_gradient),
		}
	}

	pub fn load_paint(&self) -> Paint<'_, 1> {
		match self.load_mode {
			Mode::Detailed => Paint::Stack(&self.load),
			Mode::Gradient => Paint::Gradient(self.load_gradient),
		}
	}

	pub fn disk_paint(&self) -> Paint<'_, 2> {
		match self.disk_mode {
			Mode::Detailed => Paint::Stack(&self.disk),
			Mode::Gradient => Paint::Gradient(self.disk_gradient),
		}
	}

	/// The drives the disk graph counts.
	pub fn drives(&self) -> Drives {
		if self.disk_drives.is_empty() || self.disk_drives.iter().any(|d| d == "all") {
			Drives::All
		} else {
			Drives::Only(self.disk_drives.clone())
		}
	}

	pub fn temp_paint(&self) -> Paint<'_, 2> {
		match self.temp_mode {
			Mode::Detailed => Paint::Stack(&self.temp),
			Mode::Gradient => Paint::Gradient(self.temp_gradient),
		}
	}

	pub fn battery_paint(&self) -> Paint<'_, 3> {
		match self.battery_mode {
			Mode::Detailed => Paint::Stack(&self.battery),
			Mode::Gradient => Paint::Gradient(self.battery_gradient),
		}
	}

	pub fn self_paint(&self) -> Paint<'_, 1> {
		match self.self_mode {
			Mode::Detailed => Paint::Stack(&self.self_colors),
			Mode::Gradient => Paint::Gradient(self.self_gradient),
		}
	}

	/// A graph's own pick, or else Time axis default.
	pub fn timing(&self, own: Option<Timing>) -> Timing {
		own.unwrap_or(self.time)
	}

	/// A graph's own sample rate in ms, or else Sample ms default, and never
	/// under [`crate::FASTEST_MS`].
	pub fn rate(&self, own: u32) -> u32 {
		if own == 0 { self.sample_ms } else { own }.max(crate::FASTEST_MS)
	}

	/// A graph's width from its percentage of Width, rounded, and never 0.
	pub fn share(&self, percent: u32) -> u32 {
		let px = (u64::from(self.width) * u64::from(percent) + 50) / 100;
		u32::try_from(px).unwrap_or(u32::MAX).max(1)
	}

	/// A missing or unreadable file gives the defaults, and a bad line gives
	/// that setting's default.
	pub fn load(path: &Path) -> Look {
		let text = std::fs::read_to_string(path).unwrap_or_default();
		Look::from_model(&model(&text))
	}
}

/// The names in a `graph.order` line, lower case, each once. Commas work as
/// well as spaces, since shcl reads `cpu, mem` as a list and hands it back
/// with quotes.
pub fn graph_order(text: &str) -> Vec<String> {
	let mut names: Vec<String> = Vec::new();
	for word in text.split(|c: char| c == ',' || c.is_whitespace()) {
		let name = word
			.trim_matches(|c| c == '"' || c == '\'')
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
	config_dir().join("tukzmon.shcl")
}

/// One panel item's own settings file in `dir`, by its unique id, named like
/// its history file.
pub fn instance_file(dir: &Path, id: i32) -> PathBuf {
	dir.join(format!("plugin-{id}.shcl"))
}

/// Gives `own` a copy of `shared` when it has no file yet, so an item set up
/// before each one had its own keeps its look, and a new one starts from it.
/// With neither, nothing is written, and the item has the defaults.
///
/// # Errors
///
/// Whatever reading `shared` or writing `own` hit.
pub fn start_from(own: &Path, shared: &Path) -> std::io::Result<()> {
	if own.exists() {
		return Ok(());
	}
	let text = match std::fs::read(shared) {
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
		r => r?,
	};
	// renamed in whole, so a crash can't leave half a file that then wins
	let tmp = own.with_extension("shcl.tmp");
	std::fs::write(&tmp, text)?;
	std::fs::rename(&tmp, own)
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

	// Test ID: decb4
	#[test]
	fn slider_ranges_sit_inside_their_range() {
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
	fn no_text_was_split_on_a_comma() {
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
	fn defaults_are_the_multiload_colors() {
		let l = Look::default();
		assert_eq!(l.cpu[0], Rgb(0xff, 0, 0));
		assert_eq!(l.cpu[1], Rgb(0xfa, 0x72, 0xf0));
		assert_eq!(l.cpu[2], Rgb(0, 0xff, 0x80));
		assert_eq!(l.cpu[3], Rgb(0x20, 0x5f, 0x40));
		assert_eq!(l.border_color, Rgb(0x8b, 0x8b, 0x8b));
		assert_eq!(l.top, Rgb(0x31, 0x31, 0x31));
		assert_eq!(l.bottom, Rgb(0, 0, 0));
		assert_eq!((l.width, l.sample_ms, l.border), (128, 500, 1));
		assert_eq!(l.time, Timing::Log(Curve::Smooth));
		assert_eq!(l.spikes, 0.5);
		assert_eq!(l.hover, Rgb(0, 0, 0));
		assert_eq!(l.said, Said::Ago);
	}

	// Test ID: deccf
	#[test]
	fn the_default_theme_is_the_row_defaults() {
		let spec = spec();
		let g = spec.group(THEME).unwrap();
		assert_eq!(g.members.len(), 99);
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
	fn the_default_theme_keeps_the_colors_picked_on_it() {
		// b23's changes on Default as of 2026-10-28, kept as they were
		let picked = [
			("system", "#fa72f0"),
			("net_local", "#ffed00"),
			("net_sent", "#ff7777"),
			("net_received", "#6ad4ff"),
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
			("temp_low", "#4eb6bd"),
			("temp_medium", "#feff40"),
			("temp_high", "#ff0000"),
			("battery_discharging", "#c46e43"),
			("border_color", "#8b8b8b"),
			("back_top", "#313131"),
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
		] {
			assert_eq!(v(lead), v(medium), "{lead}");
		}
	}

	// Test ID: deccg
	#[test]
	fn a_theme_sets_every_color() {
		let l = Look::from_model(&model("theme:\n\tname: pastel\n"));
		assert_eq!(l.cpu[2], Rgb(0xa1, 0xc8, 0xf0));
		assert_eq!(l.bottom, Rgb(0x0f, 0x12, 0x14));
		assert_eq!(l.width, 128);
		assert_eq!(
			Look::from_model(&model("theme:\n\tname: plaid\n")),
			Look::default()
		);
	}

	// Test ID: decdy
	#[test]
	fn transparent_is_off_and_outlives_any_theme() {
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
	fn margins_are_the_themes_and_reach_the_look() {
		let s = spec();
		for id in ["margin_tb", "margin_lr"] {
			assert_eq!(s.get(id).and_then(|r| r.group.as_deref()), Some("theme"));
		}
		for p in &s.group("theme").unwrap().presets {
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
		assert_eq!((l.margin_lr, l.cpu[2]), (2, Rgb(0x12, 0x34, 0x56)));
	}

	// Test ID: dejky
	#[test]
	fn the_outline_tabs_colors_go_over_every_graph_when_ticked() {
		let s = spec();
		let g = s.group(THEME).unwrap();
		for id in ["border_all", "back_all"] {
			assert!(g.members.iter().any(|m| m == id), "{id}");
			assert_eq!(s.get(id).unwrap().default, knobs::Value::Bool(false));
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
			graph:\n\tcolors:\n\t\tborder: \"#111111\"\n\t\ttop: \"#222222\"\n\t\tbottom: \"#333333\"\n";
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
		let l = Look::from_model(&model(mine));
		assert_eq!((l.cpu_outline, l.mem_outline), (own, shared));
		let ticked = |border: bool, back: bool| {
			let text =
				format!("{mine}\toverrides:\n\t\tborder: {border}\n\t\tbackground: {back}\n");
			Look::from_model(&model(&text))
		};
		let l = ticked(true, false);
		assert_eq!(
			l.cpu_outline,
			Outline {
				border: shared.border,
				..own
			}
		);
		let l = ticked(false, true);
		assert_eq!(
			l.cpu_outline,
			Outline {
				border: own.border,
				..shared
			}
		);
		let l = ticked(true, true);
		for o in [
			l.cpu_outline,
			l.mem_outline,
			l.self_outline,
			l.battery_outline,
		] {
			assert_eq!(o, shared);
		}
		// a saved theme keeps them, and one saved before them has them off
		let mut t = saved_themes(&shcl::Document::parse("themes:\n\tx:\n\t\tname: X\n"));
		assert_eq!(t[0].colors["back_all"], "off");
		t[0].colors.insert("back_all".into(), "on".into());
		let text = themes_text(&t);
		assert!(text.contains("back_all: true\n"), "{text}");
		let doc = shcl::Document::parse(&text);
		assert!(theme_problems(&doc).is_empty());
		assert_eq!(saved_themes(&doc), t);
		let m = Model::load_docs(
			spec_with(&t),
			&shcl::Document::parse("theme:\n\tname: x\n"),
			&shcl::Document::new(),
		);
		assert!(m.value("back_all", &NoEnv).as_bool());
		assert!(!m.value("border_all", &NoEnv).as_bool());
	}

	// Test ID: dejkg
	#[test]
	fn each_graph_has_its_own_outline_in_every_theme() {
		let s = spec();
		let g = s.group(THEME).unwrap();
		let graphs = LEADS.map(|(graph, _)| graph);
		for graph in graphs {
			for part in ["border_color", "back_top", "back_bottom"] {
				let id = format!("{graph}_{part}");
				assert!(g.members.contains(&id), "{id}");
				let shared = &s.get(part).unwrap().default;
				assert_eq!(&s.get(&id).unwrap().default, shared, "{id}");
				for p in &g.presets {
					assert_eq!(p.values[&id], p.values[part], "{} {id}", p.key);
				}
			}
		}
		let outlines = |l: &Look| {
			[
				l.cpu_outline,
				l.mem_outline,
				l.net_outline,
				l.swap_outline,
				l.zswap_outline,
				l.load_outline,
				l.disk_outline,
				l.temp_outline,
				l.battery_outline,
				l.self_outline,
			]
		};
		let d = Look::default();
		let grey = Outline {
			border: Rgb(0x8b, 0x8b, 0x8b),
			top: Rgb(0x31, 0x31, 0x31),
			bottom: Rgb(0, 0, 0),
		};
		assert_eq!(outlines(&d), [grey; 10]);
		// each graph reads its own, and no other graph's, here as Custom
		let l = Look::from_model(&model(
			"mem:\n\tcolors:\n\t\ttop: \"#123456\"\nself:\n\tcolors:\n\t\tborder: \"#abcdef\"\n\t\tbottom: \"#010203\"\n",
		));
		let mut want = [grey; 10];
		want[1].top = Rgb(0x12, 0x34, 0x56);
		want[9].border = Rgb(0xab, 0xcd, 0xef);
		want[9].bottom = Rgb(1, 2, 3);
		assert_eq!(outlines(&l), want);
		let fire = g.preset("fire").unwrap();
		let l = Look::from_model(&model("theme:\n\tname: fire\n"));
		assert_eq!(
			l.load_outline.top,
			Rgb::parse(fire.values["load_back_top"].as_text()).unwrap()
		);
		// the Outline sub-tab's draw no graph, but a file from before each
		// graph had its own keeps its look
		let shared = "graph:\n\tcolors:\n\t\ttop: \"#123456\"\n";
		let l = Look::from_model(&model(&format!(
			"{shared}mem:\n\tcolors:\n\t\ttop: \"#654321\"\n"
		)));
		assert_eq!(l.top, Rgb(0x12, 0x34, 0x56));
		let mut want = [grey; 10].map(|o| Outline {
			top: Rgb(0x12, 0x34, 0x56),
			..o
		});
		want[1].top = Rgb(0x65, 0x43, 0x21);
		assert_eq!(outlines(&l), want);
		let kept = "theme:\n\tname: fire\nkept:\n\tchanges:\n\t\ttheme:\n\t\t\tpreset: fire\n\t\t\tvalues:\n\t\t\t\tgraph:\n\t\t\t\t\tcolors:\n\t\t\t\t\t\tborder: \"#123456\"\n";
		let l = Look::from_model(&model(kept));
		assert_eq!(l.disk_outline.border, Rgb(0x12, 0x34, 0x56));
		let old =
			"theme:\n\tname: old\nthemes:\n\told:\n\t\tname: Old\n\t\tback_bottom: \"#123456\"\n";
		let t = saved_themes(&shcl::Document::parse(old));
		assert_eq!(t[0].colors["temp_back_bottom"], "#123456");
		assert_eq!(t[0].colors["temp_back_top"], "#313131");
		let l = Look::from_model(&model(old));
		assert_eq!(l.battery_outline.bottom, Rgb(0x12, 0x34, 0x56));
	}

	// Test ID: decch
	#[test]
	fn colors_with_no_theme_line_are_custom() {
		let text = "cpu:\n\tcolors:\n\t\tuser: \"#123456\"\n";
		let m = model(text);
		assert_eq!(m.chosen(THEME, &NoEnv), knobs::CUSTOM);
		assert_eq!(Look::from_model(&m).cpu[2], Rgb(0x12, 0x34, 0x56));
		// the rest of Custom starts as Default
		assert_eq!(Look::from_model(&m).cpu[3], Rgb(0x20, 0x5f, 0x40));
		// with a theme named, they're its Custom set and wait
		let text = format!("theme:\n\tname: default\n{text}");
		let l = Look::from_model(&model(&text));
		assert_eq!(l.cpu[2], Rgb(0, 0xff, 0x80));
	}

	// Test ID: deccj
	#[test]
	fn a_change_on_a_theme_is_kept_with_it() {
		let mut m = model("theme:\n\tname: primary\n");
		m.set("user", &knobs::Value::Text("#abcdef".into()), &NoEnv);
		assert_eq!(m.shown_choice(THEME, &NoEnv), "Primary *");
		name_the_theme(&mut m);
		let back = model(&m.save(&NoEnv).0);
		assert_eq!(back.shown_choice(THEME, &NoEnv), "Primary *");
		let l = Look::from_model(&back);
		assert_eq!(l.cpu[2], Rgb(0xab, 0xcd, 0xef));
		assert_eq!(l.cpu[3], Rgb(0x20, 0x5f, 0x20));
	}

	// Test ID: decck
	#[test]
	fn back_to_default_from_custom_stays_default() {
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
		assert_eq!(Look::from_model(&back).cpu[2], Rgb(0, 0xff, 0x80));
		// and nothing to name when there's no Custom set
		let mut plain = model("");
		name_the_theme(&mut plain);
		assert_eq!(plain.save(&NoEnv).0.trim(), "");
	}

	// Test ID: decct
	#[test]
	fn a_saved_theme_is_listed_after_the_built_ins_and_sets_the_look() {
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
		assert_eq!(l.cpu[2], Rgb(0x11, 0xaa, 0x22));
		// a missing or bad color is Default's
		assert_eq!(l.cpu[3], Rgb(0x20, 0x5f, 0x40));
		let doc = shcl::Document::parse(text);
		let p = theme_problems(&doc);
		assert_eq!(p.len(), 1, "{p:?}");
		assert!(p[0].contains("themes.mine.nice"), "{p:?}");
		// the plugin reads it the same way
		let dir = std::env::temp_dir().join(format!("test_tukzmon_{}_saved", std::process::id()));
		std::fs::create_dir_all(&dir).unwrap();
		let path = dir.join("t.shcl");
		std::fs::write(&path, text).unwrap();
		let loaded = Look::load(&path);
		std::fs::remove_file(&path).unwrap();
		std::fs::remove_dir(&dir).unwrap();
		assert_eq!(loaded, l);
	}

	// Test ID: decdg
	#[test]
	fn fire_is_a_gradient_and_the_rest_are_detailed() {
		let l = Look::from_model(&model("theme:\n\tname: fire\n"));
		assert_eq!(l.cpu_mode, Mode::Gradient);
		assert_eq!(
			l.cpu_gradient,
			[
				Rgb(0x8b, 0x1a, 0),
				Rgb(0xff, 0xf4, 0xc8),
				Rgb(0x7f, 0xd4, 0xff)
			]
		);
		assert_eq!(
			l.cpu_paint(),
			crate::render::Paint::Gradient(l.cpu_gradient)
		);
		let spec = spec();
		for p in &spec.group(THEME).unwrap().presets {
			let want = if p.key == "fire" {
				"gradient"
			} else {
				"detailed"
			};
			assert_eq!(p.values["cpu_mode"].as_text(), want, "{}", p.key);
		}
		let d = Look::default();
		assert_eq!(d.cpu_mode, Mode::Detailed);
		assert_eq!(d.cpu_paint(), crate::render::Paint::Stack(&d.cpu));
		// a mode set by hand is a change on the theme, like a color
		let mut m = model("theme:\n\tname: neon\n");
		m.set("cpu_mode", &knobs::Value::Text("Gradient".into()), &NoEnv);
		assert_eq!(m.shown_choice(THEME, &NoEnv), "Neon *");
		name_the_theme(&mut m);
		let l = Look::from_model(&model(&m.save(&NoEnv).0));
		assert_eq!(l.cpu_mode, Mode::Gradient);
		assert_eq!(l.cpu_gradient[1], Rgb(0x39, 0xff, 0x14));
		assert_eq!(Mode::parse("plaid"), None);
	}

	// Test ID: dedpz
	#[test]
	fn memory_colors_are_multiload_ngs_and_every_theme_has_them() {
		let d = Look::default();
		assert_eq!(
			d.mem,
			[
				Rgb(0xff, 0xff, 0xa3),
				Rgb(0x80, 0x40, 0x80),
				Rgb(0x40, 0x80, 0x60)
			]
		);
		assert!(d.show_memory);
		assert_eq!(d.mem_mode, Mode::Detailed);
		assert_eq!(d.mem_paint(), crate::render::Paint::Stack(&d.mem));
		let spec = spec();
		for p in &spec.group(THEME).unwrap().presets {
			let v = |id: &str| p.values[id].as_text().to_string();
			assert_eq!(v("mem_mode"), v("cpu_mode"), "{}", p.key);
			assert_ne!(v("mem_buffers"), v("mem_cache"), "{}", p.key);
			assert_ne!(v("mem_user"), v("mem_buffers"), "{}", p.key);
		}
		let fire = Look::from_model(&model("theme:\n\tname: fire\n"));
		assert_eq!(
			fire.mem_paint(),
			crate::render::Paint::Gradient(fire.mem_gradient)
		);
		assert_ne!(fire.mem_gradient, fire.cpu_gradient);
		assert_eq!(shown_when("mem_buffers"), Some(("mem_mode", "detailed")));
		assert_eq!(shown_when("mem_high"), Some(("mem_mode", "gradient")));
		assert!(shown_as_radio("mem_mode"));
		// a file can hide it, and the colors are its own
		let l = Look::from_model(&model(
			"mem:\n\tshow: false\n\tcolors:\n\t\tcache: \"#010203\"\n",
		));
		assert!(!l.show_memory);
		assert_eq!(l.mem[2], Rgb(1, 2, 3));
		assert_eq!(l.cpu, d.cpu);
	}

	// Test ID: defx7
	#[test]
	fn no_theme_is_named_for_a_color_and_each_graph_has_its_own() {
		let colors = [
			"red", "orange", "yellow", "green", "blue", "purple", "violet", "pink", "cyan",
			"magenta", "teal", "brown", "gray", "grey",
		];
		let spec = spec();
		let presets = &spec.group(THEME).unwrap().presets;
		assert!(presets.len() > 2);
		for p in presets {
			let label = p.label.to_lowercase();
			assert!(!colors.iter().any(|c| label.contains(c)), "{label}");
			let rgb = |id: &str| Rgb::parse(p.values[id].as_text()).unwrap();
			let ramp = |g: &str| ["low", "medium", "high"].map(|s| rgb(&format!("{g}_{s}")));
			let ramps = [
				"cpu", "mem", "swap", "zswap", "net", "load", "disk", "temp", "battery", "self",
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
	fn every_graph_leads_with_a_color_that_reads_apart() {
		use crate::color::{APART, apart, closest};
		let spec = spec();
		for p in &spec.group(THEME).unwrap().presets {
			let get = |id: &str| p.values.get(id).map(knobs::Value::as_text);
			let leads = leads(get);
			assert_eq!(leads.len(), LEADS.len(), "{}", p.key);
			let (d, a, b) = closest(&leads).unwrap();
			// a one-color theme leads every graph alike, on purpose (dejbw)
			assert!(
				d >= APART || leads.iter().all(|c| *c == leads[0]),
				"{}: {} and {} are {d:.1} apart",
				p.key,
				LEADS[a].0,
				LEADS[b].0
			);
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
	fn the_one_color_themes_lead_alike_and_keep_the_alarms() {
		use crate::color::{APART, apart, closest};
		let spec = spec();
		let g = spec.group(THEME).unwrap();
		let mut one_color = vec![];
		for p in &g.presets {
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
				("net", &["net_local", "net_sent", "net_received"]),
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
	fn each_graph_has_a_switch_and_a_width() {
		let d = Look::default();
		assert!(d.show_cpu && d.show_memory && d.show_swap);
		assert_eq!((d.cpu_width, d.mem_width, d.net_width), (100, 100, 50));
		let l = Look::from_model(&model(
			"graph:\n\twidth: 90\ncpu:\n\tshow: false\n\twidth: 64\nmem:\n\twidth: 1000\n",
		));
		assert_eq!(
			(l.show_cpu, l.cpu_width, l.mem_width, l.width),
			(false, 64, 1000, 90)
		);
		// a percentage of Width, rounded, and never 0
		assert_eq!((l.share(64), l.share(1000), l.share(50)), (58, 900, 45));
		assert_eq!(Look { width: 16, ..l }.share(1), 1);
		assert!(l.show_memory);
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
	fn each_graph_has_a_sample_rate_and_old_files_keep_the_default() {
		let d = Look::default();
		assert_eq!((d.cpu_sample_ms, d.battery_sample_ms), (0, 0));
		assert_eq!(d.rate(d.cpu_sample_ms), 500);
		let l = Look::from_model(&model(
			"graph:\n\tsample_ms: 1000\ncpu:\n\tsample_ms: 250\nmem:\n\tsample_ms: 20\n",
		));
		assert_eq!((l.cpu_sample_ms, l.mem_sample_ms), (250, 20));
		assert_eq!(l.rate(l.cpu_sample_ms), 250);
		assert_eq!(l.rate(l.mem_sample_ms), crate::FASTEST_MS);
		// a file from before has none, and Sample ms default rules
		assert_eq!(l.rate(l.net_sample_ms), 1000);
		let spec = spec();
		for g in [
			"cpu", "mem", "net", "swap", "zswap", "load", "disk", "temp", "battery", "self",
		] {
			let id = format!("{g}_sample_ms");
			let Some(s) = spec.get(&id) else {
				panic!("no {id} in settings.shcl");
			};
			// set while its graph is off, and 0 is Default
			assert_eq!(s.gate, None);
			assert_eq!(s.default.as_f64(), 0.0);
			assert_eq!((s.min, s.max), (0.0, 60000.0));
			assert_eq!(s.path, format!("{g}.sample_ms"));
		}
	}

	// Test ID: deea4
	#[test]
	fn each_graph_has_a_time_axis_and_old_files_keep_theirs() {
		let d = Look::default();
		assert_eq!((d.cpu_time, d.mem_time), (None, None));
		let l = Look::from_model(&model("cpu:\n\ttime: linear\nmem:\n\ttime: Fixed_B\n"));
		assert_eq!(l.cpu_time, Some(Timing::Linear));
		assert_eq!(l.mem_time, Some(Timing::Log(Curve::FixedB)));
		// a file from before has neither, and Time axis default rules
		let old = Look::from_model(&model("graph:\n\ttime: linear\n"));
		assert_eq!(old.timing(old.cpu_time), Timing::Linear);
		let old = Look::from_model(&model("graph:\n\ttime: fixed_a\n"));
		assert_eq!(old.timing(old.mem_time), Timing::Log(Curve::FixedA));
		assert_eq!(
			Look::from_model(&model("cpu:\n\ttime: squiggly\n")).cpu_time,
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
	fn the_disk_graph_counts_every_disk_or_the_ones_named() {
		assert_eq!(Look::default().drives(), Drives::All);
		assert!(!Look::default().disk_log);
		for text in ["disk:\n\tdrives: all\n", "disk:\n\tdrives: \"\"\n"] {
			assert_eq!(
				Look::from_model(&model(text)).drives(),
				Drives::All,
				"{text}"
			);
		}
		assert_eq!(
			Look::from_model(&model("disk:\n\tdrives: sda, nvme0n1\n\tlog: true\n")).drives(),
			Drives::Only(vec!["sda".into(), "nvme0n1".into()])
		);
	}

	// Test ID: dedqq
	#[test]
	fn the_graph_order_reads_with_spaces_or_commas() {
		assert_eq!(
			Look::default().order,
			[
				"cpu", "mem", "net", "disk", "temp", "swap", "zswap", "load", "battery", "self"
			]
		);
		for text in [
			"graph:\n\torder: mem cpu\n",
			"graph:\n\torder: mem, cpu\n",
			"graph:\n\torder: \"MEM  cpu\"\n",
			"graph:\n\torder: mem, mem, cpu\n",
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
	fn the_mode_shows_only_its_own_rows() {
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
	fn a_saved_theme_keeps_its_mode() {
		let text = "theme:\n\tname: mine\nthemes:\n\tmine:\n\t\tname: Mine\n\t\tcpu_mode: GRADIENT\n\t\tcpu_low: \"#010203\"\n";
		let doc = shcl::Document::parse(text);
		let t = saved_themes(&doc);
		assert_eq!(t[0].colors["cpu_mode"], "gradient");
		assert!(theme_problems(&doc).is_empty());
		let l = Look::from_model(&model(text));
		assert_eq!(l.cpu_mode, Mode::Gradient);
		assert_eq!(l.cpu_gradient[0], Rgb(1, 2, 3));
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
	fn a_saved_theme_cant_take_a_built_in_key() {
		let text = "themes:\n\tdefault:\n\t\tuser: \"#123456\"\n\tCustom:\n\t\tuser: \"#123456\"\n";
		let doc = shcl::Document::parse(text);
		assert!(saved_themes(&doc).is_empty());
		assert_eq!(theme_problems(&doc).len(), 2);
		assert_eq!(Look::from_model(&model(text)), Look::default());
	}

	// Test ID: deccw
	#[test]
	fn saved_themes_write_whole_and_read_back() {
		let mut t = saved_themes(&shcl::Document::parse(
			"themes:\n\tx:\n\t\tname: \"Say \\\"hi\\\"\"\n",
		));
		assert_eq!(t[0].name, "Say \"hi\"");
		assert_eq!(t[0].colors.len(), 99);
		t.push(SavedTheme {
			key: theme_key("Default", &t),
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
	fn theme_keys_are_plain_and_free() {
		assert_eq!(theme_key("My Green!", &[]), "my_green");
		assert_eq!(theme_key("  -- ", &[]), "theme");
		assert_eq!(theme_key("1984", &[]), "theme_1984");
		assert_eq!(theme_key("Gr\u{fc}n", &[]), "gr_n");
		assert_eq!(theme_key("custom", &[]), "custom_2");
		assert_eq!(theme_key("Earth", &[]), "earth_2");
	}

	// Test ID: decc5
	#[test]
	fn the_hover_settings_come_from_the_file() {
		let text = "graph:\n\thover_time: clock\n\tcolors:\n\t\thover: \"#ffffff\"\n";
		let l = Look::from_model(&Model::load(spec(), text, ""));
		assert_eq!(l.said, Said::Clock);
		assert_eq!(l.hover, Rgb(0xff, 0xff, 0xff));
		let l = Look::from_model(&Model::load(spec(), "graph:\n\thover_time: never\n", ""));
		assert_eq!(l.said, Said::Ago);
	}

	// Test ID: decbx
	#[test]
	fn the_time_axis_default_comes_from_the_file() {
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
	fn history_is_kept_a_year_by_default() {
		let l = Look::default();
		assert!(l.keep);
		assert_eq!(l.max_days, 365);
		let text = "history:\n\tkeep: false\n\tmax_days: 0\n";
		let l = Look::from_model(&Model::load(spec(), text, ""));
		assert!(!l.keep);
		assert_eq!(l.max_days, 365);
	}

	// Test ID: debhk
	#[test]
	fn a_file_overrides_and_bad_lines_fall_back() {
		let text = "graph:\n\twidth: 200\n\tsample_ms: 99999\ncpu:\n\tcolors:\n\t\tuser: \"#123456\"\n\t\tnice: red\n";
		let l = Look::from_model(&Model::load(spec(), text, ""));
		assert_eq!(l.width, 200);
		assert_eq!(l.sample_ms, 500);
		assert_eq!(l.cpu[2], Rgb(0x12, 0x34, 0x56));
		assert_eq!(l.cpu[3], Rgb(0x20, 0x5f, 0x40));
	}

	fn scratch(name: &str) -> PathBuf {
		let dir =
			std::env::temp_dir().join(format!("test_tukzmon_config_{}_{name}", std::process::id()));
		let _ = std::fs::remove_dir_all(&dir);
		std::fs::create_dir_all(&dir).unwrap();
		dir
	}

	// Test ID: defy3
	#[test]
	fn an_item_with_no_file_starts_from_the_shared_one_and_leaves_it() {
		let dir = scratch("start");
		let shared = dir.join("tukzmon.shcl");
		let text = "graph:\n\twidth: 77\n\ncpu:\n\tcolors:\n\t\tuser: \"#123456\"\n";
		std::fs::write(&shared, text).unwrap();
		let own = instance_file(&dir, 7);
		assert_eq!(own, dir.join("plugin-7.shcl"));
		start_from(&own, &shared).unwrap();
		assert_eq!(std::fs::read_to_string(&own).unwrap(), text);
		assert_eq!(std::fs::read_to_string(&shared).unwrap(), text);
		assert_eq!(Look::load(&own), Look::load(&shared));
		assert_ne!(Look::load(&own), Look::default());
		assert!(!dir.join("plugin-7.shcl.tmp").exists());

		// its own file wins from then on
		std::fs::write(&own, "cpu:\n\tcolors:\n\t\tuser: \"#654321\"\n").unwrap();
		start_from(&own, &shared).unwrap();
		assert!(std::fs::read_to_string(&own).unwrap().contains("#654321"));

		// with no shared file there's nothing to copy, and it has the defaults
		let fresh = instance_file(&dir, 8);
		start_from(&fresh, &dir.join("missing.shcl")).unwrap();
		assert!(!fresh.exists());
		assert_eq!(Look::load(&fresh), Look::default());
		let _ = std::fs::remove_dir_all(&dir);
	}

	// Test ID: defy4
	#[test]
	fn a_save_for_one_item_leaves_the_other_alone() {
		let dir = scratch("two");
		let shared = dir.join("tukzmon.shcl");
		std::fs::write(&shared, "graph:\n\twidth: 77\n").unwrap();
		let (one, two) = (instance_file(&dir, 1), instance_file(&dir, 2));
		assert_ne!(one, two);
		start_from(&one, &shared).unwrap();
		start_from(&two, &shared).unwrap();
		std::fs::write(&one, "mem:\n\tshow: false\n").unwrap();
		assert!(!Look::load(&one).show_memory);
		assert!(Look::load(&two).show_memory);
		assert_eq!(Look::load(&two).width, 77);
		assert_eq!(Look::load(&two), Look::load(&shared));
		let _ = std::fs::remove_dir_all(&dir);
	}

	// Test ID: debhm
	#[test]
	fn missing_file_is_defaults() {
		assert_eq!(
			Look::load(Path::new("/nonexistent/tukzmon.shcl")),
			Look::default()
		);
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
}
