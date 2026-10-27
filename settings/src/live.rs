// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

// The graph list's live graphs. The window reads them itself from when it
// opens, and draws them with its own look, Apply or not.

use tukzmon_core::config::Look;
use tukzmon_core::cpu::CpuReader;
use tukzmon_core::mem::MemoryReader;
use tukzmon_core::{Graph, Source};

pub struct Live {
	cpu: Graph,
	/// None on a box where memory can't be read.
	mem: Option<Graph<MemoryReader, 3>>,
}

impl Live {
	pub fn new(look: Look) -> Live {
		Live {
			mem: Graph::memory(look.clone()),
			cpu: Graph::new(look),
		}
	}

	pub fn set_look(&mut self, look: &Look) {
		if let Some(m) = &mut self.mem {
			m.look = look.clone();
		}
		self.cpu.look = look.clone();
	}

	/// A hidden graph is read too, since its row still draws it. The first
	/// call only primes them.
	pub fn sample(&mut self) {
		self.cpu.sample();
		if let Some(m) = &mut self.mem {
			m.sample();
		}
	}

	pub fn sample_ms(&self) -> u32 {
		self.cpu.look.sample_ms
	}

	/// The graph's width in the panel, in logical pixels. 0 for a name that
	/// isn't drawn here.
	pub fn width(&self, name: &str) -> u32 {
		let l = &self.cpu.look;
		match name {
			CpuReader::SECTION => l.width_or(l.cpu_width),
			MemoryReader::SECTION if self.mem.is_some() => l.width_or(l.mem_width),
			_ => 0,
		}
	}

	/// Cairo ARGB32 pixels, `w` by `h`, or none for a name that isn't drawn.
	pub fn render(&mut self, name: &str, w: usize, h: usize, scale: usize) -> &[u32] {
		match (name, &mut self.mem) {
			(CpuReader::SECTION, _) => self.cpu.render(w, h, scale),
			(MemoryReader::SECTION, Some(m)) => m.render(w, h, scale),
			_ => &[],
		}
	}

	pub fn easing(&self, name: &str) -> bool {
		match (name, &self.mem) {
			(CpuReader::SECTION, _) => self.cpu.easing(),
			(MemoryReader::SECTION, Some(m)) => m.easing(),
			_ => false,
		}
	}

	pub fn tip(&self, name: &str) -> String {
		match (name, &self.mem) {
			(CpuReader::SECTION, _) => self.cpu.tooltip(),
			(MemoryReader::SECTION, Some(m)) => m.tooltip(),
			_ => String::new(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::dialog::Dialog;
	use knobs::Value;

	// Test ID: defmj
	#[test]
	fn each_graph_draws_at_its_width_with_the_windows_look() {
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_live_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path);
		let mut live = Live::new(d.look());
		live.sample();
		std::thread::sleep(std::time::Duration::from_millis(30));
		live.sample();
		assert_eq!(live.width("cpu"), 128);
		assert_eq!(live.width("nope"), 0);
		assert!(live.render("nope", 10, 10, 1).is_empty());
		assert!(live.tip("cpu").starts_with("CPU "), "{}", live.tip("cpu"));

		// not applied, and still drawn that way
		d.set("cpu_width", &Value::Float(40.0));
		d.set("show_cpu", &Value::Bool(false));
		live.set_look(&d.look());
		assert_eq!(live.width("cpu"), 40);
		let before = live.render("cpu", 40, 24, 1).to_vec();
		assert_eq!(before.len(), 40 * 24);
		assert!(before.iter().all(|p| p >> 24 == 0xff));
		d.set_text("theme", "fire");
		live.set_look(&d.look());
		assert_ne!(live.render("cpu", 40, 24, 1), before.as_slice());
		if live.mem.is_some() {
			d.set("mem_width", &Value::Float(100.0));
			live.set_look(&d.look());
			assert_eq!(live.width("mem"), 100);
			assert_eq!(live.render("mem", 100, 24, 1).len(), 100 * 24);
			assert!(
				live.tip("mem").starts_with("Memory "),
				"{}",
				live.tip("mem")
			);
		}
	}

	// Test ID: defps
	#[test]
	fn a_time_axis_picked_in_the_window_eases_into_its_row() {
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_live_ease_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path);
		let mut live = Live::new(d.look());
		live.sample();
		std::thread::sleep(std::time::Duration::from_millis(30));
		live.sample();
		live.render("cpu", 40, 24, 1);
		assert!(!live.easing("cpu"));
		d.set_text("cpu_time", "linear");
		live.set_look(&d.look());
		live.render("cpu", 40, 24, 1);
		assert!(live.easing("cpu"));
		assert!(!live.easing("nope"));
	}
}
