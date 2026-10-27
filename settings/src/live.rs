// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

// The graph list's live graphs. The window reads them itself from when it
// opens, and draws them with its own look, Apply or not.

use tukzmon_core::battery::BatteryReader;
use tukzmon_core::config::Look;
use tukzmon_core::cpu::CpuReader;
use tukzmon_core::graphs::Drawn;
use tukzmon_core::mem::{MemoryReader, SwapReader};
use tukzmon_core::{Graph, Source};

pub struct Live {
	cpu: Graph,
	/// None on a box where memory can't be read.
	mem: Option<Graph<MemoryReader, 3>>,
	/// None on a box with no swap.
	swap: Option<Graph<SwapReader, 1>>,
	/// None on a box with no battery in.
	battery: Option<Graph<BatteryReader, 3>>,
}

impl Live {
	pub fn new(look: Look) -> Live {
		Live {
			mem: Graph::memory(look.clone()),
			swap: Graph::swap(look.clone()),
			battery: Graph::battery(look.clone()),
			cpu: Graph::new(look),
		}
	}

	fn get(&self, name: &str) -> Option<&dyn Drawn> {
		match name {
			CpuReader::SECTION => Some(&self.cpu),
			MemoryReader::SECTION => self.mem.as_ref().map(|g| g as &dyn Drawn),
			SwapReader::SECTION => self.swap.as_ref().map(|g| g as &dyn Drawn),
			BatteryReader::SECTION => self.battery.as_ref().map(|g| g as &dyn Drawn),
			_ => None,
		}
	}

	fn get_mut(&mut self, name: &str) -> Option<&mut dyn Drawn> {
		match name {
			CpuReader::SECTION => Some(&mut self.cpu),
			MemoryReader::SECTION => self.mem.as_mut().map(|g| g as &mut dyn Drawn),
			SwapReader::SECTION => self.swap.as_mut().map(|g| g as &mut dyn Drawn),
			BatteryReader::SECTION => self.battery.as_mut().map(|g| g as &mut dyn Drawn),
			_ => None,
		}
	}

	fn all_mut(&mut self) -> impl Iterator<Item = &mut dyn Drawn> {
		let optional = [
			self.mem.as_mut().map(|g| g as &mut dyn Drawn),
			self.swap.as_mut().map(|g| g as &mut dyn Drawn),
			self.battery.as_mut().map(|g| g as &mut dyn Drawn),
		];
		std::iter::once(&mut self.cpu as &mut dyn Drawn).chain(optional.into_iter().flatten())
	}

	pub fn set_look(&mut self, look: &Look) {
		for g in self.all_mut() {
			g.set_look(look);
		}
	}

	/// A hidden graph is read too, since its row still draws it. The first
	/// call only primes them.
	pub fn sample(&mut self) {
		for g in self.all_mut() {
			g.sample();
		}
	}

	pub fn sample_ms(&self) -> u32 {
		self.cpu.look.sample_ms
	}

	/// The graph's width in the panel, in logical pixels. 0 for a name that
	/// isn't drawn here.
	pub fn width(&self, name: &str) -> u32 {
		let l = &self.cpu.look;
		let own = match name {
			CpuReader::SECTION => l.cpu_width,
			MemoryReader::SECTION => l.mem_width,
			SwapReader::SECTION => l.swap_width,
			BatteryReader::SECTION => l.battery_width,
			_ => return 0,
		};
		if self.get(name).is_some() {
			l.width_or(own)
		} else {
			0
		}
	}

	/// Cairo ARGB32 pixels, `w` by `h`, or none for a name that isn't drawn.
	pub fn render(&mut self, name: &str, w: usize, h: usize, scale: usize) -> &[u32] {
		match self.get_mut(name) {
			Some(g) => g.render(w, h, scale),
			None => &[],
		}
	}

	pub fn easing(&self, name: &str) -> bool {
		self.get(name).is_some_and(Drawn::easing)
	}

	pub fn tip(&self, name: &str) -> String {
		self.get(name).map(Drawn::tooltip).unwrap_or_default()
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

	// Test ID: defq8
	#[test]
	fn the_swap_row_draws_at_its_own_width() {
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_live_swap_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path);
		let mut live = Live::new(d.look());
		if live.swap.is_none() {
			assert_eq!(live.width("swap"), 0);
			return;
		}
		live.sample();
		std::thread::sleep(std::time::Duration::from_millis(30));
		live.sample();
		// off by default, and its row still draws it
		assert!(!d.look().show_swap);
		assert_eq!(live.width("swap"), 128);
		d.set("swap_width", &Value::Float(60.0));
		live.set_look(&d.look());
		assert_eq!(live.width("swap"), 60);
		assert_eq!(live.render("swap", 60, 24, 1).len(), 60 * 24);
		assert!(
			live.tip("swap").starts_with("Swap "),
			"{}",
			live.tip("swap")
		);
	}

	// Test ID: defqe
	#[test]
	fn the_battery_row_is_there_only_with_a_battery_in() {
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_live_battery_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path);
		let mut live = Live::new(d.look());
		let Some(_) = &live.battery else {
			assert_eq!(live.width("battery"), 0);
			assert!(live.render("battery", 10, 10, 1).is_empty());
			assert_eq!(live.tip("battery"), "");
			return;
		};
		d.set("battery_width", &Value::Float(50.0));
		live.set_look(&d.look());
		assert_eq!(live.width("battery"), 50);
		assert_eq!(live.render("battery", 50, 24, 1).len(), 50 * 24);
		assert!(
			live.tip("battery").starts_with("Battery"),
			"{}",
			live.tip("battery")
		);
	}
}
