// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

// The graph list's live graphs. The window reads them itself from when it
// opens, and draws them with its own look, Apply or not.

use tukzmon_core::battery::BatteryReader;
use tukzmon_core::config::Look;
use tukzmon_core::cpu::CpuReader;
use tukzmon_core::disk::DiskReader;
use tukzmon_core::graphs::{self, Drawn};
use tukzmon_core::load::LoadReader;
use tukzmon_core::mem::{MemoryReader, SwapReader};
use tukzmon_core::net::NetReader;
use tukzmon_core::render::Overlay;
use tukzmon_core::temp::TempReader;
use tukzmon_core::zswap::ZswapReader;
use tukzmon_core::{Graph, Source};

pub struct Live {
	cpu: Graph,
	/// None on a box where memory can't be read.
	mem: Option<Graph<MemoryReader, 3>>,
	net: Option<Graph<NetReader, 3>>,
	/// None on a box with no swap.
	swap: Option<Graph<SwapReader, 1>>,
	/// None on a box with neither zswap on nor zram set up.
	zswap: Option<Graph<ZswapReader, 1>>,
	load: Option<Graph<LoadReader, 1>>,
	/// None on a box with no disk.
	disk: Option<Graph<DiskReader, 2>>,
	/// None on a box with no CPU sensor it knows.
	temp: Option<Graph<TempReader, 2>>,
	/// None on a box with no battery in.
	battery: Option<Graph<BatteryReader, 3>>,
}

impl Live {
	pub fn new(look: Look) -> Live {
		Live {
			mem: Graph::memory(look.clone()),
			net: Graph::net(look.clone()),
			swap: Graph::swap(look.clone()),
			zswap: Graph::zswap(look.clone()),
			load: Graph::load_avg(look.clone()),
			disk: Graph::disk(look.clone()),
			temp: Graph::temp(look.clone()),
			battery: Graph::battery(look.clone()),
			cpu: Graph::new(look),
		}
	}

	fn get(&self, name: &str) -> Option<&dyn Drawn> {
		match name {
			CpuReader::SECTION => Some(&self.cpu),
			MemoryReader::SECTION => self.mem.as_ref().map(|g| g as &dyn Drawn),
			NetReader::SECTION => self.net.as_ref().map(|g| g as &dyn Drawn),
			SwapReader::SECTION => self.swap.as_ref().map(|g| g as &dyn Drawn),
			ZswapReader::SECTION => self.zswap.as_ref().map(|g| g as &dyn Drawn),
			LoadReader::SECTION => self.load.as_ref().map(|g| g as &dyn Drawn),
			DiskReader::SECTION => self.disk.as_ref().map(|g| g as &dyn Drawn),
			TempReader::SECTION => self.temp.as_ref().map(|g| g as &dyn Drawn),
			BatteryReader::SECTION => self.battery.as_ref().map(|g| g as &dyn Drawn),
			_ => None,
		}
	}

	fn get_mut(&mut self, name: &str) -> Option<&mut dyn Drawn> {
		match name {
			CpuReader::SECTION => Some(&mut self.cpu),
			MemoryReader::SECTION => self.mem.as_mut().map(|g| g as &mut dyn Drawn),
			NetReader::SECTION => self.net.as_mut().map(|g| g as &mut dyn Drawn),
			SwapReader::SECTION => self.swap.as_mut().map(|g| g as &mut dyn Drawn),
			ZswapReader::SECTION => self.zswap.as_mut().map(|g| g as &mut dyn Drawn),
			LoadReader::SECTION => self.load.as_mut().map(|g| g as &mut dyn Drawn),
			DiskReader::SECTION => self.disk.as_mut().map(|g| g as &mut dyn Drawn),
			TempReader::SECTION => self.temp.as_mut().map(|g| g as &mut dyn Drawn),
			BatteryReader::SECTION => self.battery.as_mut().map(|g| g as &mut dyn Drawn),
			_ => None,
		}
	}

	fn all_mut(&mut self) -> impl Iterator<Item = (&'static str, &mut dyn Drawn)> {
		let optional = [
			(
				MemoryReader::SECTION,
				self.mem.as_mut().map(|g| g as &mut dyn Drawn),
			),
			(
				NetReader::SECTION,
				self.net.as_mut().map(|g| g as &mut dyn Drawn),
			),
			(
				SwapReader::SECTION,
				self.swap.as_mut().map(|g| g as &mut dyn Drawn),
			),
			(
				ZswapReader::SECTION,
				self.zswap.as_mut().map(|g| g as &mut dyn Drawn),
			),
			(
				LoadReader::SECTION,
				self.load.as_mut().map(|g| g as &mut dyn Drawn),
			),
			(
				DiskReader::SECTION,
				self.disk.as_mut().map(|g| g as &mut dyn Drawn),
			),
			(
				TempReader::SECTION,
				self.temp.as_mut().map(|g| g as &mut dyn Drawn),
			),
			(
				BatteryReader::SECTION,
				self.battery.as_mut().map(|g| g as &mut dyn Drawn),
			),
		];
		std::iter::once((CpuReader::SECTION, &mut self.cpu as &mut dyn Drawn))
			.chain(optional.into_iter().filter_map(|(n, g)| Some((n, g?))))
	}

	pub fn set_look(&mut self, look: &Look) {
		for (_, g) in self.all_mut() {
			g.set_look(look);
		}
	}

	pub fn clear(&mut self) {
		for (_, g) in self.all_mut() {
			g.clear();
		}
	}

	/// A hidden graph is read too, since its row still draws it. The first
	/// call only primes them.
	pub fn sample(&mut self) {
		for (_, g) in self.all_mut() {
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
			NetReader::SECTION => l.net_width,
			SwapReader::SECTION => l.swap_width,
			ZswapReader::SECTION => l.zswap_width,
			LoadReader::SECTION => l.load_width,
			DiskReader::SECTION => l.disk_width,
			TempReader::SECTION => l.temp_width,
			BatteryReader::SECTION => l.battery_width,
			_ => return 0,
		};
		if self.get(name).is_some() {
			l.share(own)
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

	pub fn overlay(&self, name: &str, w: usize, h: usize, scale: usize) -> Option<Overlay> {
		self.get(name)?.overlay(w, h, scale)
	}

	pub fn easing(&self, name: &str) -> bool {
		self.get(name).is_some_and(Drawn::easing)
	}

	pub fn tip(&self, name: &str) -> String {
		self.get(name).map(Drawn::tooltip).unwrap_or_default()
	}

	/// Puts the hover line on the named graph at device column `x` of its
	/// `w`, as the panel does, or with None takes it off every graph. True
	/// when that changed what's drawn.
	pub fn hover(&mut self, name: &str, x: Option<usize>, w: usize) -> bool {
		let hit = x.zip(w.checked_sub(1)).map(|(x, last)| (name, x.min(last)));
		graphs::point(self.all_mut().map(|(n, g)| (n as &str, g)), hit)
	}

	/// The panel's tip for the named graph's hover line, drawn `w` by `h`, or
	/// its latest read with no line on it.
	pub fn hover_tip(&self, name: &str, w: usize, h: usize, scale: usize) -> String {
		match self.get(name).and_then(|g| Some((g, g.hover()?))) {
			Some((g, x)) => g.hover_tip(x, w, h, scale),
			None => self.tip(name),
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
		// at 100 px, a width in percent is the same number
		d.set("width", &Value::Float(100.0));
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

	// Test ID: defwm
	#[test]
	fn the_hover_line_follows_the_pointer_in_its_row_as_in_the_panel() {
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_live_hover_never_written.shcl",
			std::process::id()
		));
		let d = Dialog::open(path);
		let mut look = d.look();
		look.hover = tukzmon_core::config::Rgb(1, 2, 3);
		let mut live = Live::new(look);
		live.sample();
		std::thread::sleep(std::time::Duration::from_millis(30));
		live.sample();
		let (w, h) = (40, 24);
		// the columns drawn in the hover color, with rows `w` wide
		let lit = |px: &[u32], w: usize| -> Vec<usize> {
			let mut cols: Vec<usize> = (0..px.len())
				.filter(|&i| px[i] == 0xff01_0203)
				.map(|i| i % w)
				.collect();
			cols.sort_unstable();
			cols.dedup();
			cols
		};
		assert!(lit(live.render("cpu", w, h, 1), w).is_empty());
		assert!(live.tip("cpu").starts_with("CPU "));
		assert_eq!(live.hover_tip("cpu", w, h, 1), live.tip("cpu"));

		assert!(live.hover("cpu", Some(17), w));
		assert!(!live.hover("cpu", Some(17), w), "the same column again");
		assert_eq!(lit(live.render("cpu", w, h, 1), w), [17]);
		let tip = live.hover_tip("cpu", w, h, 1);
		let mut lines = tip.lines();
		let when = lines.next().unwrap_or_default();
		assert!(when == "just now" || when.ends_with(" ago"), "{tip}");
		let what = lines.next().unwrap_or_default();
		assert!(what.starts_with("CPU ") || what == "No data", "{tip}");

		// past the right edge it stays on the graph's last column
		assert!(live.hover("cpu", Some(500), w));
		assert_eq!(live.cpu.hover, Some(w - 1));
		if live.mem.is_some() {
			assert!(live.hover("mem", Some(5), 100));
			assert_eq!(live.cpu.hover, None, "one row has the line at a time");
			assert!(lit(live.render("cpu", w, h, 1), w).is_empty());
			assert_eq!(lit(live.render("mem", 100, h, 1), 100), [5]);
		}
		assert!(live.hover("", None, 0));
		assert!(!live.hover("", None, 0));
		assert!(live.all_mut().all(|(_, g)| g.hover().is_none()));
		assert!(lit(live.render("cpu", w, h, 1), w).is_empty());
		assert!(!live.hover("nope", Some(3), w));
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
		// on by default, at a quarter of Width
		assert!(d.look().show_swap);
		assert_eq!(live.width("swap"), 32);
		// at 100 px, a width in percent is the same number
		d.set("width", &Value::Float(100.0));
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

	// Test ID: dehy5
	#[test]
	fn the_zswap_row_is_there_only_with_zswap_or_zram() {
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_live_zswap_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path);
		let mut live = Live::new(d.look());
		let Some(_) = &live.zswap else {
			assert_eq!(live.width("zswap"), 0);
			assert!(live.render("zswap", 10, 10, 1).is_empty());
			assert_eq!(live.tip("zswap"), "");
			return;
		};
		// at 100 px, a width in percent is the same number
		d.set("width", &Value::Float(100.0));
		d.set("zswap_width", &Value::Float(50.0));
		live.set_look(&d.look());
		assert_eq!(live.width("zswap"), 50);
		assert_eq!(live.render("zswap", 50, 24, 1).len(), 50 * 24);
		let tip = live.tip("zswap");
		assert!(tip.starts_with("Zswap") || tip.starts_with("Zram"), "{tip}");
	}

	// Test ID: dej08
	#[test]
	fn the_load_and_disk_rows_draw_at_their_own_width() {
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_live_load_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path);
		let mut live = Live::new(d.look());
		assert!(live.load.is_some(), "every Linux box has /proc/loadavg");
		live.sample();
		std::thread::sleep(std::time::Duration::from_millis(30));
		live.sample();
		assert!(!d.look().show_load && !d.look().show_disk);
		// at 100 px, a width in percent is the same number
		d.set("width", &Value::Float(100.0));
		d.set("load_width", &Value::Float(60.0));
		d.set("disk_width", &Value::Float(50.0));
		live.set_look(&d.look());
		assert_eq!(live.width("load"), 60);
		assert_eq!(live.render("load", 60, 24, 1).len(), 60 * 24);
		let tip = live.tip("load");
		assert!(tip.starts_with("Load"), "{tip}");
		// a container has no disk, so no disk row
		if live.disk.is_some() {
			assert_eq!(live.width("disk"), 50);
			assert_eq!(live.render("disk", 50, 24, 1).len(), 50 * 24);
			assert!(live.tip("disk").starts_with("Disk"));
		}
	}

	// Test ID: dej0w
	#[test]
	fn the_temp_row_draws_at_its_own_width() {
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_live_temp_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path);
		let mut live = Live::new(d.look());
		assert!(!d.look().show_temp);
		// at 100 px, a width in percent is the same number
		d.set("width", &Value::Float(100.0));
		d.set("temp_width", &Value::Float(40.0));
		live.set_look(&d.look());
		assert_eq!(live.width("temp"), 40);
		// the box, or a container, may have no CPU sensor
		if live.temp.is_some() {
			live.sample();
			assert_eq!(live.render("temp", 40, 24, 1).len(), 40 * 24);
			let tip = live.tip("temp");
			assert!(tip.starts_with("Temperature"), "{tip}");
		}
	}

	// Test ID: dehzq
	#[test]
	fn the_net_row_draws_at_its_own_width() {
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_live_net_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path);
		let mut live = Live::new(d.look());
		assert!(live.net.is_some(), "every Linux box has /proc/net/dev");
		live.sample();
		std::thread::sleep(std::time::Duration::from_millis(30));
		live.sample();
		assert!(!d.look().show_net);
		// at 100 px, a width in percent is the same number
		d.set("width", &Value::Float(100.0));
		d.set("net_width", &Value::Float(60.0));
		live.set_look(&d.look());
		assert_eq!(live.width("net"), 60);
		assert_eq!(live.render("net", 60, 24, 1).len(), 60 * 24);
		let tip = live.tip("net");
		assert!(tip.starts_with("Network"), "{tip}");
	}
}
