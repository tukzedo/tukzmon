// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

// The graph list's live graphs. The window reads them itself from when it
// opens, and draws them with its own look, Apply or not.

use tukzmon_core::config::{Look, Which};
use tukzmon_core::graphs::{self, Drawn, Slots};

pub struct Live {
	/// Every graph this box can read. The own graph is the settings window's,
	/// not the panel's.
	slots: Slots,
}

impl Live {
	pub fn new(look: Look) -> Live {
		Live {
			slots: Slots::new(look, |_, _| true),
		}
	}

	// None for a name that isn't a graph's, too
	fn get(&self, name: &str) -> Option<&dyn Drawn> {
		self.slots.get(Which::named(name)?)
	}

	fn get_mut(&mut self, name: &str) -> Option<&mut dyn Drawn> {
		self.slots.get_mut(Which::named(name)?)
	}

	fn all_mut(&mut self) -> impl Iterator<Item = (Which, &mut dyn Drawn)> {
		self.slots.iter_mut()
	}

	/// Each row keeps the graph it has, and one this box couldn't read
	/// stays out.
	pub fn set_look(&mut self, look: Look) {
		self.slots.set_look(look);
	}

	pub fn clear(&mut self) {
		for (_, g) in self.all_mut() {
			g.clear();
		}
	}

	/// Every graph read now, for tests that can't wait for a beat. The first
	/// call only primes them.
	#[cfg(test)]
	pub fn sample(&mut self) {
		for (_, g) in self.all_mut() {
			g.sample();
		}
	}

	/// Reads each graph whose beat has come, and gives the ms to the next
	/// one's, as the panel does. A hidden graph is read too, since its row
	/// still draws it. See [`Graph::tick`].
	pub fn tick(&mut self) -> u32 {
		graphs::tick(self.all_mut().map(|(_, g)| g), tukzmon_core::clock_ms())
	}

	/// The graph's width in the panel, in logical pixels. 0 for a name that
	/// isn't drawn here.
	pub fn width(&self, name: &str) -> u32 {
		let l = self.slots.look();
		Which::named(name)
			.filter(|&w| self.slots.get(w).is_some())
			.map_or(0, |w| l.share(l[w].width))
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

	/// Puts the hover line on the named graph at device column `x` of its
	/// `w`, as the panel does, or with None takes it off every graph. True
	/// when that changed what's drawn.
	pub fn hover(&mut self, name: &str, x: Option<usize>, w: usize) -> bool {
		let hit = Which::named(name)
			.zip(x.zip(w.checked_sub(1)))
			.map(|(which, (x, last))| (which, x.min(last)));
		graphs::point(self.all_mut(), hit)
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
	use std::any::Any;
	use std::rc::Rc;
	use tukzmon_core::Graph;

	impl Live {
		fn has(&self, which: Which) -> bool {
			self.slots.get(which).is_some()
		}

		fn cpu(&self) -> &Graph {
			let g: &dyn Any = self.slots.get(Which::Cpu).expect("always there");
			g.downcast_ref().expect("CPU's own reader")
		}
	}

	// Test ID: dend2
	#[test]
	fn every_row_shares_the_one_look() {
		fn looks(l: &Live) -> Vec<&Rc<Look>> {
			Which::ALL
				.into_iter()
				.filter_map(|w| l.slots.get(w).map(Drawn::look))
				.collect()
		}
		let mut live = Live::new(Look::default());
		// net is read from /proc on any Linux box, so one is always there
		assert!(live.has(Which::Net));
		for l in looks(&live) {
			assert!(Rc::ptr_eq(l, live.slots.look()));
		}
		let was = Rc::clone(live.slots.look());
		live.set_look(Look {
			width: 77,
			..Look::default()
		});
		assert_eq!(live.slots.look().width, 77);
		for l in looks(&live) {
			assert!(Rc::ptr_eq(l, live.slots.look()));
		}
		assert_eq!(Rc::strong_count(&was), 1, "no row kept the old look");
	}

	// Test ID: dendg
	#[test]
	fn every_graph_the_panel_can_draw_has_a_row_by_its_name() {
		use tukzmon_core::graphs::Graphs;
		let mut every = Look::default();
		for w in Which::ALL {
			every[w].show = true;
		}
		let panel = Graphs::new(every.clone());
		let live = Live::new(every);
		for w in Which::ALL {
			assert_eq!(live.get(w.key()).is_some(), panel.has(w), "{w:?}");
			assert_eq!(live.width(w.key()) > 0, panel.has(w), "{w:?}");
		}
		assert!(live.get("nope").is_none());
	}

	// Test ID: defmj
	#[test]
	fn each_graph_draws_at_its_width_with_the_windows_look() {
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_live_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path).unwrap();
		let mut live = Live::new(d.look());
		live.sample();
		std::thread::sleep(std::time::Duration::from_millis(30));
		live.sample();
		assert_eq!(live.width("cpu"), 100);
		assert_eq!(live.width("nope"), 0);
		assert!(live.render("nope", 10, 10, 1).is_empty());
		assert!(live.tip("cpu").starts_with("CPU "), "{}", live.tip("cpu"));

		// not applied, and still drawn that way
		// at 100 px, a width in percent is the same number
		d.set("width", &Value::Float(100.0));
		d.set("cpu_width", &Value::Float(40.0));
		d.set("show_cpu", &Value::Bool(false));
		live.set_look(d.look());
		assert_eq!(live.width("cpu"), 40);
		let before = live.render("cpu", 40, 24, 1).to_vec();
		assert_eq!(before.len(), 40 * 24);
		assert!(before.iter().all(|p| p >> 24 == 0xff));
		d.set_text("theme", "fire");
		live.set_look(d.look());
		assert_ne!(live.render("cpu", 40, 24, 1), before.as_slice());
		if live.has(Which::Mem) {
			d.set("mem_width", &Value::Float(100.0));
			live.set_look(d.look());
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
		let d = Dialog::open(path).unwrap();
		let mut look = d.look();
		look.hover = tukzmon_core::config::Rgb(1, 2, 3);
		look.said = tukzmon_core::when::Said::Ago;
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
		assert_eq!(live.cpu().hover, Some(w - 1));
		if live.has(Which::Mem) {
			assert!(live.hover("mem", Some(5), 100));
			assert_eq!(live.cpu().hover, None, "one row has the line at a time");
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
		let mut d = Dialog::open(path).unwrap();
		let mut live = Live::new(d.look());
		live.sample();
		std::thread::sleep(std::time::Duration::from_millis(30));
		live.sample();
		live.render("cpu", 40, 24, 1);
		assert!(!live.easing("cpu"));
		d.set_text("cpu_time", "linear");
		live.set_look(d.look());
		live.render("cpu", 40, 24, 1);
		assert!(live.easing("cpu"));
		assert!(!live.easing("nope"));
	}

	// Test ID: deja2
	#[test]
	fn a_rows_sample_rate_picked_in_the_window_sets_when_it_reads() {
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_live_rate_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path).unwrap();
		d.set("cpu_sample", &Value::Float(100.0));
		let mut live = Live::new(d.look());
		// a beat due a little early counts as now, so the next is a rate on
		let within = |rate: u32| f64::from(rate) + tukzmon_core::EARLY_MS;
		assert!(f64::from(live.tick()) <= within(1000));
		// not applied, and still read that way
		d.set("sample_ms", &Value::Float(5000.0));
		live.set_look(d.look());
		assert_eq!(live.cpu().sample_ms(), 5000);
		assert!(f64::from(live.tick()) <= within(5000));
		d.set("cpu_sample", &Value::Float(2.0));
		live.set_look(d.look());
		assert_eq!(live.cpu().sample_ms(), 100);
		assert!(f64::from(live.tick()) <= within(100));
		// back to Sample ms default
		d.set("cpu_sample", &Value::Float(100.0));
		live.set_look(d.look());
		assert_eq!(live.cpu().sample_ms(), 5000);
	}

	// Test ID: defq8
	#[test]
	fn the_swap_row_draws_at_its_own_width() {
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_live_swap_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path).unwrap();
		let mut live = Live::new(d.look());
		if !live.has(Which::Swap) {
			assert_eq!(live.width("swap"), 0);
			return;
		}
		live.sample();
		std::thread::sleep(std::time::Duration::from_millis(30));
		live.sample();
		// on by default, at a quarter of Width
		assert!(d.look()[Which::Swap].show);
		assert_eq!(live.width("swap"), 25);
		// at 100 px, a width in percent is the same number
		d.set("width", &Value::Float(100.0));
		d.set("swap_width", &Value::Float(60.0));
		live.set_look(d.look());
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
		let mut d = Dialog::open(path).unwrap();
		let mut live = Live::new(d.look());
		if !live.has(Which::Battery) {
			assert_eq!(live.width("battery"), 0);
			assert!(live.render("battery", 10, 10, 1).is_empty());
			assert_eq!(live.tip("battery"), "");
			return;
		}
		d.set("battery_width", &Value::Float(50.0));
		live.set_look(d.look());
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
		let mut d = Dialog::open(path).unwrap();
		let mut live = Live::new(d.look());
		if !live.has(Which::Zswap) {
			assert_eq!(live.width("zswap"), 0);
			assert!(live.render("zswap", 10, 10, 1).is_empty());
			assert_eq!(live.tip("zswap"), "");
			return;
		}
		// at 100 px, a width in percent is the same number
		d.set("width", &Value::Float(100.0));
		d.set("zswap_width", &Value::Float(50.0));
		live.set_look(d.look());
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
		let mut d = Dialog::open(path).unwrap();
		let mut live = Live::new(d.look());
		assert!(live.has(Which::Load), "every Linux box has /proc/loadavg");
		live.sample();
		std::thread::sleep(std::time::Duration::from_millis(30));
		live.sample();
		assert!(!d.look()[Which::Load].show && d.look()[Which::Disk].show);
		// at 100 px, a width in percent is the same number
		d.set("width", &Value::Float(100.0));
		d.set("load_width", &Value::Float(60.0));
		d.set("disk_width", &Value::Float(50.0));
		live.set_look(d.look());
		assert_eq!(live.width("load"), 60);
		assert_eq!(live.render("load", 60, 24, 1).len(), 60 * 24);
		let tip = live.tip("load");
		assert!(tip.starts_with("Load"), "{tip}");
		// a container has no disk, so no disk row
		if live.has(Which::Disk) {
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
		let mut d = Dialog::open(path).unwrap();
		let mut live = Live::new(d.look());
		assert!(d.look()[Which::Temp].show);
		// at 100 px, a width in percent is the same number
		d.set("width", &Value::Float(100.0));
		d.set("temp_width", &Value::Float(40.0));
		live.set_look(d.look());
		assert_eq!(live.width("temp"), 40);
		// the box, or a container, may have no CPU sensor
		if live.has(Which::Temp) {
			live.sample();
			std::thread::sleep(std::time::Duration::from_millis(20));
			live.sample();
			assert_eq!(live.render("temp", 40, 24, 1).len(), 40 * 24);
			// the title alone is the tip before a reading
			let tip = live.tip("temp");
			assert!(tip.starts_with("Temperature "), "{tip}");
		}
	}

	// Test ID: dekmd
	#[test]
	#[cfg_attr(miri, ignore = "Miri can't load a library")]
	fn the_gpu_rows_are_there_only_with_a_gpu_to_read() {
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_live_gpu_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path).unwrap();
		let mut live = Live::new(d.look());
		assert!(!d.look()[Which::Gpu].show && d.look()[Which::Vram].show);
		d.set("width", &Value::Float(100.0));
		d.set("gpu_width", &Value::Float(30.0));
		d.set("vram_width", &Value::Float(40.0));
		live.set_look(d.look());
		// an amdgpu card can have a busy share and no memory files
		let rows = [
			("gpu", 30, "GPU", "GPU ", live.has(Which::Gpu)),
			("vram", 40, "GPU memory", " GiB", live.has(Which::Vram)),
		];
		for (name, _, _, _, there) in rows {
			if !there {
				assert_eq!(live.width(name), 0);
				assert!(live.render(name, 10, 10, 1).is_empty());
				assert_eq!(live.tip(name), "");
			}
		}
		// the first read only primes, as with every graph
		live.sample();
		for (name, _, title, _, there) in rows {
			if there {
				assert_eq!(live.tip(name), title);
			}
		}
		std::thread::sleep(std::time::Duration::from_millis(20));
		live.sample();
		for (name, width, _, says, there) in rows {
			if !there {
				continue;
			}
			assert_eq!(live.width(name), width);
			let w = width as usize;
			assert_eq!(live.render(name, w, 24, 1).len(), w * 24);
			let tip = live.tip(name);
			assert!(tip.contains(says), "{tip}");
		}
	}

	// Test ID: dejb8
	#[test]
	#[cfg_attr(miri, ignore = "Miri has no CPU time clock")]
	fn the_own_row_draws_the_settings_window_at_its_own_width() {
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_live_self_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path).unwrap();
		let mut live = Live::new(d.look());
		assert!(!d.look()[Which::Own].show);
		// at 100 px, a width in percent is the same number
		d.set("width", &Value::Float(100.0));
		d.set("self_width", &Value::Float(40.0));
		live.set_look(d.look());
		assert_eq!(live.width("self"), 40);
		live.sample();
		std::thread::sleep(std::time::Duration::from_millis(20));
		live.sample();
		assert_eq!(live.render("self", 40, 24, 1).len(), 40 * 24);
		// the test binary's name isn't the settings window's
		let tip = live.tip("self");
		assert!(
			tip.starts_with("tukzmon ") && tip.contains(" of all CPUs\n"),
			"{tip}"
		);
	}

	// Test ID: dehzq
	#[test]
	fn the_net_row_draws_at_its_own_width() {
		let path = std::env::temp_dir().join(format!(
			"test_tukzmon-settings_{}_live_net_never_written.shcl",
			std::process::id()
		));
		let mut d = Dialog::open(path).unwrap();
		let mut live = Live::new(d.look());
		assert!(live.has(Which::Net), "every Linux box has /proc/net/dev");
		live.sample();
		std::thread::sleep(std::time::Duration::from_millis(30));
		live.sample();
		assert!(!d.look()[Which::Net].show);
		// at 100 px, a width in percent is the same number
		d.set("width", &Value::Float(100.0));
		d.set("net_width", &Value::Float(60.0));
		live.set_look(d.look());
		assert_eq!(live.width("net"), 60);
		assert_eq!(live.render("net", 60, 24, 1).len(), 60 * 24);
		let tip = live.tip("net");
		assert!(tip.starts_with("Network"), "{tip}");
	}
}
