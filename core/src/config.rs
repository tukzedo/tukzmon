// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! Settings, as knobs reads them from `settings.shcl` and the config file.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use knobs::{Model, NoEnv, Spec};

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

#[derive(Clone, Debug, PartialEq)]
pub struct Look {
	pub width: u32,
	pub log_axis: bool,
	pub sample_ms: u32,
	/// 0 to 1.
	pub spikes: f32,
	pub border: u32,
	/// In `cpu::SERIES` order, bottom first.
	pub cpu: [Rgb; 4],
	pub border_color: Rgb,
	pub top: Rgb,
	pub bottom: Rgb,
}

impl Default for Look {
	fn default() -> Self {
		Look::from_model(&Model::new(spec()))
	}
}

impl Look {
	pub fn from_model(m: &Model) -> Look {
		let num = |id: &str| m.value(id, &NoEnv).as_f64();
		let color = |id: &str| Rgb::parse(m.value(id, &NoEnv).as_text()).unwrap_or(Rgb(0, 0, 0));
		Look {
			width: num("width") as u32,
			log_axis: m.value("log_axis", &NoEnv).as_bool(),
			sample_ms: num("sample_ms") as u32,
			spikes: (num("spikes") / 100.0) as f32,
			border: num("border") as u32,
			cpu: [
				color("iowait"),
				color("system"),
				color("user"),
				color("nice"),
			],
			border_color: color("border_color"),
			top: color("back_top"),
			bottom: color("back_bottom"),
		}
	}

	/// A missing or unreadable file gives the defaults, and a bad line gives
	/// that setting's default.
	pub fn load(path: &Path) -> Look {
		let text = std::fs::read_to_string(path).unwrap_or_default();
		Look::from_model(&Model::load(spec(), &text, ""))
	}
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

/// One file for every graph, for now. A file per plugin instance comes with
/// more than one kind of graph.
pub fn config_file() -> PathBuf {
	config_dir().join("tukzmon.shcl")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parse_spec() {
		if let Err(e) = Spec::parse(SPEC_TEXT) {
			panic!("{}", e.join("\n"));
		}
	}

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

	#[test]
	fn defaults_are_the_multiload_colors() {
		let l = Look::default();
		assert_eq!(l.cpu[0], Rgb(0xff, 0, 0));
		assert_eq!(l.cpu[1], Rgb(0, 0x80, 0));
		assert_eq!(l.cpu[2], Rgb(0, 0xff, 0x80));
		assert_eq!(l.cpu[3], Rgb(0xc0, 0, 0xc0));
		assert_eq!(l.border_color, Rgb(0x40, 0x80, 0x6a));
		assert_eq!(l.top, Rgb(0x2a, 0x54, 0x46));
		assert_eq!(l.bottom, Rgb(0, 0, 0));
		assert_eq!((l.width, l.sample_ms, l.border), (128, 500, 1));
		assert!(l.log_axis);
		assert_eq!(l.spikes, 0.5);
	}

	#[test]
	fn a_file_overrides_and_bad_lines_fall_back() {
		let text = "graph:\n\twidth: 200\n\tsample_ms: 99999\ncpu:\n\tcolors:\n\t\tuser: \"#123456\"\n\t\tnice: red\n";
		let l = Look::from_model(&Model::load(spec(), text, ""));
		assert_eq!(l.width, 200);
		assert_eq!(l.sample_ms, 500);
		assert_eq!(l.cpu[2], Rgb(0x12, 0x34, 0x56));
		assert_eq!(l.cpu[3], Rgb(0xc0, 0, 0xc0));
	}

	#[test]
	fn missing_file_is_defaults() {
		assert_eq!(
			Look::load(Path::new("/nonexistent/tukzmon.shcl")),
			Look::default()
		);
	}

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
