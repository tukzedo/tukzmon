// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! Every graph one panel item shows, side by side, or stacked on a vertical
//! panel. No gap between them, the way multiload-ng's spacing of 0 has it.

use std::io;
use std::path::Path;

use crate::config::Look;
use crate::cpu::CpuReader;
use crate::keep;
use crate::mem::MemoryReader;
use crate::{Graph, Source};

pub struct Graphs {
	pub cpu: Graph,
	/// None when hidden, or on a box where memory can't be read.
	pub mem: Option<Graph<MemoryReader, 3>>,
	/// Top to bottom, for a vertical panel.
	pub stacked: bool,
	px: Vec<u32>,
}

/// A graph's place in the area: x, y, width, height.
type Tile = (usize, usize, usize, usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Which {
	Cpu,
	Mem,
}

impl Which {
	/// The order with nothing saved. Graphs a saved order leaves out go
	/// last, in this order.
	const ALL: [Which; 2] = [Which::Cpu, Which::Mem];

	fn name(self) -> &'static str {
		match self {
			Which::Cpu => CpuReader::SECTION,
			Which::Mem => MemoryReader::SECTION,
		}
	}
}

// the saved order, unknown names skipped, then any it doesn't name
fn order_of(saved: &[String]) -> [Which; 2] {
	let mut out = Which::ALL;
	let mut n = 0;
	let named = saved
		.iter()
		.filter_map(|name| Which::ALL.into_iter().find(|w| w.name() == name.as_str()));
	for w in named.chain(Which::ALL) {
		if !out[..n].contains(&w) {
			out[n] = w;
			n += 1;
		}
	}
	out
}

/// Every graph's name, first drawn first, from a saved `graph.order`. The
/// settings window's graph list goes by this too, so it shows the panel's order.
pub fn draw_order(saved: &[String]) -> Vec<&'static str> {
	order_of(saved).into_iter().map(Which::name).collect()
}

impl Graphs {
	pub fn new(look: Look) -> Graphs {
		let mem = if look.show_memory {
			Graph::memory(look.clone())
		} else {
			None
		};
		Graphs {
			cpu: Graph::new(look),
			mem,
			stacked: false,
			px: Vec::new(),
		}
	}

	pub fn look(&self) -> &Look {
		&self.cpu.look
	}

	/// CPU is drawn whenever no other graph is, so the panel item never
	/// shrinks to nothing and its menu stays in reach.
	pub fn cpu_shown(&self) -> bool {
		self.cpu.look.show_cpu || self.mem.is_none()
	}

	fn order(&self) -> [Which; 2] {
		order_of(&self.cpu.look.order)
	}

	/// In the order drawn.
	fn shown(&self) -> impl Iterator<Item = Which> + '_ {
		self.order().into_iter().filter(|w| match w {
			Which::Cpu => self.cpu_shown(),
			Which::Mem => self.mem.is_some(),
		})
	}

	pub fn count(&self) -> usize {
		self.shown().count()
	}

	fn width_of(&self, which: Which) -> u32 {
		let l = &self.cpu.look;
		l.width_or(match which {
			Which::Cpu => l.cpu_width,
			Which::Mem => l.mem_width,
		})
	}

	/// Each shown graph's width in logical pixels, in the order drawn.
	pub fn widths(&self) -> impl Iterator<Item = u32> + '_ {
		self.shown().map(|w| self.width_of(w))
	}

	/// Takes a new look, adding or dropping the memory graph to match. One
	/// added loads its history from `history`, when that's given.
	///
	/// # Errors
	///
	/// As [`keep::read`], but not for a file that isn't there. The look is
	/// taken anyway.
	pub fn set_look(&mut self, look: Look, history: Option<&Path>) -> io::Result<()> {
		let mut res = Ok(());
		if !look.show_memory {
			self.mem = None;
		} else if self.mem.is_none() {
			self.mem = Graph::memory(look.clone());
			if let (Some(m), Some(path)) = (&mut self.mem, history) {
				match keep::read(path) {
					Ok(sections) => {
						m.adopt(&sections);
					}
					Err(e) if e.kind() == io::ErrorKind::NotFound => {}
					Err(e) => res = Err(e),
				}
			}
		}
		if let Some(m) = &mut self.mem {
			m.look = look.clone();
		}
		self.cpu.look = look;
		if !self.cpu_shown() {
			self.cpu.hover = None;
		}
		res
	}

	/// A hidden graph isn't read, so it shows a gap when it's back.
	pub fn sample(&mut self) {
		if self.cpu_shown() {
			self.cpu.sample();
		}
		if let Some(m) = &mut self.mem {
			m.sample();
		}
	}

	pub fn trim(&mut self) {
		if self.cpu_shown() {
			self.cpu.trim();
		}
		if let Some(m) = &mut self.mem {
			m.trim();
		}
	}

	/// Every graph's history, plus any section in the file that none of them
	/// owns, such as a hidden graph's, so showing it again brings it back.
	///
	/// # Errors
	///
	/// As [`keep::write`].
	pub fn save(&self, path: &Path) -> io::Result<()> {
		let mut ours = vec![self.cpu.section()];
		if let Some(m) = &self.mem {
			ours.push(m.section());
		}
		let kept: Vec<(String, Vec<u8>)> = keep::read(path)
			.unwrap_or_default()
			.into_iter()
			.filter(|(name, _)| ours.iter().all(|(n, _)| n != name))
			.collect();
		let all: Vec<(&str, &[u8])> = ours
			.iter()
			.map(|(n, b)| (*n, b.as_slice()))
			.chain(kept.iter().map(|(n, b)| (n.as_str(), b.as_slice())))
			.collect();
		keep::write(path, &all)
	}

	/// # Errors
	///
	/// As [`keep::read`], or `InvalidData` when no graph's history in it
	/// reads. A graph with none in the file keeps what it has.
	pub fn load(&mut self, path: &Path) -> io::Result<()> {
		let sections = keep::read(path)?;
		let cpu = self.cpu.adopt(&sections);
		let mem = self.mem.as_mut().is_some_and(|m| m.adopt(&sections));
		if cpu || mem {
			Ok(())
		} else {
			Err(io::Error::new(io::ErrorKind::InvalidData, "no history"))
		}
	}

	// side by side, the area is shared out by width, since the panel may
	// not give exactly what was asked for. Stacked, they share it evenly.
	fn tiles(&self, w: usize, h: usize) -> impl Iterator<Item = (Which, Tile)> + '_ {
		let (n, stacked) = (self.count(), self.stacked);
		let total = self.widths().map(|x| x as usize).sum::<usize>().max(1);
		let (mut i, mut done) = (0, 0);
		self.shown().map(move |which| {
			let own = self.width_of(which) as usize;
			let tile = if stacked {
				let (a, b) = (i * h / n, (i + 1) * h / n);
				(0, a, w, b - a)
			} else {
				let (a, b) = (done * w / total, (done + own) * w / total);
				(a, 0, b - a, h)
			};
			i += 1;
			done += own;
			(which, tile)
		})
	}

	// which graph holds device pixel (x, y), and where it is in it
	fn find(&self, x: usize, y: usize, w: usize, h: usize) -> Option<(Which, Tile)> {
		self.tiles(w, h)
			.find(|&(_, (tx, ty, tw, th))| x < tx + tw && y < ty + th)
	}

	/// Puts the hover line under device pixel (x, y), or takes it away. True
	/// when that changed what's drawn.
	pub fn hover(&mut self, at: Option<(usize, usize)>, w: usize, h: usize) -> bool {
		let hit = at.and_then(|(x, y)| Some((x, self.find(x, y, w, h)?)));
		let want = |w: Which| hit.and_then(|(x, (j, (tx, ..)))| (w == j).then(|| x - tx));
		let mut changed = false;
		let cpu = want(Which::Cpu);
		if self.cpu.hover != cpu {
			self.cpu.hover = cpu;
			changed = true;
		}
		let mem = want(Which::Mem);
		if let Some(m) = &mut self.mem
			&& m.hover != mem
		{
			m.hover = mem;
			changed = true;
		}
		changed
	}

	pub fn hovered(&self) -> bool {
		self.cpu.hover.is_some() || self.mem.as_ref().is_some_and(|m| m.hover.is_some())
	}

	/// The tip for the pointer at device pixel (x, y).
	pub fn hover_tip(&self, x: usize, y: usize, w: usize, h: usize, scale: usize) -> String {
		match (self.find(x, y, w, h), &self.mem) {
			(Some((Which::Mem, (tx, _, tw, th))), Some(m)) => m.hover_tip(x - tx, tw, th, scale),
			(Some((_, (tx, _, tw, th))), _) => self.cpu.hover_tip(x - tx, tw, th, scale),
			(None, _) => self.tooltip(),
		}
	}

	/// The latest reads of every graph shown.
	pub fn tooltip(&self) -> String {
		let tips: Vec<String> = self
			.shown()
			.map(|w| match (w, &self.mem) {
				(Which::Mem, Some(m)) => m.tooltip(),
				_ => self.cpu.tooltip(),
			})
			.collect();
		tips.join("\n\n")
	}

	/// Every graph as cairo ARGB32 pixels, `w` by `h`.
	pub fn render(&mut self, w: usize, h: usize, scale: usize) -> &[u32] {
		let mut px = std::mem::take(&mut self.px);
		px.clear();
		px.resize(w * h, 0);
		let tiles: Vec<(Which, Tile)> = self.tiles(w, h).collect();
		for &(which, (tx, ty, tw, th)) in &tiles {
			if tw == 0 || th == 0 {
				continue;
			}
			let one = match (which, &mut self.mem) {
				(Which::Mem, Some(m)) => m.render(tw, th, scale),
				_ => self.cpu.render(tw, th, scale),
			};
			for (r, row) in one.chunks_exact(tw).enumerate() {
				let at = (ty + r) * w + tx;
				px[at..at + tw].copy_from_slice(row);
			}
		}
		self.px = px;
		&self.px
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn both() -> Graphs {
		let mut g = Graphs::new(Look::default());
		let now = g.cpu.now();
		g.cpu.hist.add(now - 60.0, now, &[0.0, 0.0, 0.5, 0.0]);
		let m = g.mem.as_mut().unwrap();
		m.hist.add(now - 60.0, now, &[0.25, 0.25, 0.25]);
		g
	}

	fn scratch(name: &str) -> std::path::PathBuf {
		std::env::temp_dir().join(format!(
			"test_tukzmon_graphs_{}_{name}.hist",
			std::process::id()
		))
	}

	// Test ID: dedq1
	#[test]
	fn the_memory_graph_samples_draws_and_tips() {
		let mut m = Graph::memory(Look::default()).unwrap();
		assert_eq!(m.tooltip(), "Memory");
		m.sample();
		std::thread::sleep(std::time::Duration::from_millis(20));
		m.sample();
		assert!(m.tooltip().starts_with("Memory "), "{}", m.tooltip());
		let px = m.render(64, 20, 1);
		assert!(px.iter().all(|p| p >> 24 == 0xff));
		// memory's own colors, not CPU's
		let mut m = Graph::memory(Look::default()).unwrap();
		let now = m.now();
		m.hist.add(now - 60.0, now, &[1.0, 0.0, 0.0]);
		let px = m.render(64, 20, 1);
		assert_eq!(px[10 * 64 + 60], Look::default().mem[0].argb());
		let tip = m.hover_tip(60, 64, 20, 1);
		assert!(
			tip.ends_with("\nMemory 100% in use\nBuffers 0%  Cache 0%"),
			"{tip}"
		);
	}

	// Test ID: dedq2
	#[test]
	fn graphs_sit_side_by_side_and_the_pointer_finds_its_own() {
		let mut g = both();
		assert_eq!(g.count(), 2);
		let (w, h) = (40, 10);
		let left = g.cpu.render(20, h, 1).to_vec();
		let right = g.mem.as_mut().unwrap().render(20, h, 1).to_vec();
		let px = g.render(w, h, 1).to_vec();
		for r in 0..h {
			assert_eq!(&px[r * w..r * w + 20], &left[r * 20..(r + 1) * 20], "{r}");
			assert_eq!(
				&px[r * w + 20..(r + 1) * w],
				&right[r * 20..(r + 1) * 20],
				"{r}"
			);
		}
		assert!(g.hover(Some((25, 3)), w, h));
		assert_eq!(g.cpu.hover, None);
		assert_eq!(g.mem.as_ref().unwrap().hover, Some(5));
		assert!(!g.hover(Some((25, 4)), w, h), "same column");
		assert!(g.hover_tip(25, 3, w, h, 1).contains("Memory 25% in use"));
		assert!(g.hover_tip(5, 3, w, h, 1).contains("CPU 50%"));
		assert!(g.hover(None, 0, 0));
		assert!(!g.hovered());
		// on a vertical panel the second is under the first
		g.stacked = true;
		assert!(g.hover(Some((5, 7)), 10, 10));
		assert_eq!(g.mem.as_ref().unwrap().hover, Some(5));
		assert!(g.hover(Some((5, 2)), 10, 10));
		assert_eq!(g.cpu.hover, Some(5));
		assert_eq!(g.mem.as_ref().unwrap().hover, None);
		assert_eq!(g.render(10, 10, 1).len(), 100);
		assert!(g.tooltip().starts_with("CPU\n\nMemory"));
	}

	// Test ID: dedq3
	#[test]
	fn a_save_keeps_every_graph_and_a_hidden_ones_history() {
		let path = scratch("keep");
		keep::write(&path, &[("net", b"later")]).unwrap();
		let g = both();
		g.save(&path).unwrap();
		let names: Vec<String> = keep::read(&path)
			.unwrap()
			.into_iter()
			.map(|(n, _)| n)
			.collect();
		assert_eq!(names, ["cpu", "mem", "net"]);
		let mut back = Graphs::new(Look::default());
		back.load(&path).unwrap();
		assert_eq!(back.cpu.hist.last(), g.cpu.hist.last());
		let mem = |g: &Graphs| g.mem.as_ref().unwrap().hist.last();
		assert_eq!(mem(&back), mem(&g));
		// a file from before the memory graph
		keep::write(&path, &[("cpu", &g.cpu.section().1)]).unwrap();
		let mut back = Graphs::new(Look::default());
		back.load(&path).unwrap();
		assert_eq!(back.cpu.hist.last(), g.cpu.hist.last());
		assert_eq!(mem(&back), None);
		keep::write(&path, &[("net", b"x")]).unwrap();
		assert!(back.load(&path).is_err());
		std::fs::remove_file(&path).unwrap();
	}

	// Test ID: dedq4
	#[test]
	fn hiding_memory_drops_it_and_showing_brings_its_history_back() {
		let path = scratch("show");
		let mut g = both();
		let had = g.mem.as_ref().unwrap().hist.last();
		g.save(&path).unwrap();
		let hide = Look {
			show_memory: false,
			width: 99,
			..Look::default()
		};
		g.set_look(hide.clone(), Some(&path)).unwrap();
		assert!(g.mem.is_none());
		assert_eq!(g.look().width, 99);
		// the hidden graph's history stays in the file
		g.save(&path).unwrap();
		g.set_look(Look::default(), Some(&path)).unwrap();
		assert_eq!(g.mem.as_ref().unwrap().hist.last(), had);
		assert_eq!(g.mem.as_ref().unwrap().look, Look::default());
		// with history off it starts empty
		g.set_look(hide, None).unwrap();
		g.set_look(Look::default(), None).unwrap();
		assert_eq!(g.mem.as_ref().unwrap().hist.last(), None);
		std::fs::remove_file(&path).unwrap();
		assert!(
			Graphs::new(Look {
				show_memory: false,
				..Look::default()
			})
			.mem
			.is_none()
		);
	}

	// Test ID: deea2
	#[test]
	fn each_graph_takes_its_own_time_axis_or_the_time_settings() {
		use crate::axis::{Axis, Curve, Timing};
		let mut g = both();
		let total = 60.0;
		let linear = Axis::linear(100.0 * 0.5);
		let axes = |g: &Graphs| (g.cpu.axis(100), g.mem.as_ref().unwrap().axis(100));
		assert_eq!(
			axes(&g),
			(
				Axis::log(Curve::Smooth, total),
				Axis::log(Curve::Smooth, total)
			)
		);
		g.set_look(
			Look {
				cpu_time: Some(Timing::Linear),
				mem_time: Some(Timing::Log(Curve::FixedB)),
				..Look::default()
			},
			None,
		)
		.unwrap();
		assert_eq!(axes(&g), (linear.clone(), Axis::log(Curve::FixedB, total)));
		// one with no pick of its own follows Log time axis and Log curve
		g.set_look(
			Look {
				log_axis: false,
				mem_time: Some(Timing::Log(Curve::FixedA)),
				..Look::default()
			},
			None,
		)
		.unwrap();
		assert_eq!(axes(&g), (linear, Axis::log(Curve::FixedA, total)));
		g.set_look(
			Look {
				curve: Curve::FixedB,
				..Look::default()
			},
			None,
		)
		.unwrap();
		assert_eq!(
			axes(&g),
			(
				Axis::log(Curve::FixedB, total),
				Axis::log(Curve::FixedB, total)
			)
		);
	}

	// Test ID: dedqd
	#[test]
	fn each_graph_takes_its_own_width_and_0_is_width() {
		let mut g = both();
		g.set_look(
			Look {
				width: 100,
				cpu_width: 40,
				..Look::default()
			},
			None,
		)
		.unwrap();
		assert_eq!(g.widths().collect::<Vec<_>>(), [40, 100]);
		let h = 10;
		for scale in [1, 2] {
			let (cw, mw) = (40 * scale, 100 * scale);
			let w = cw + mw;
			let left = g.cpu.render(cw, h, scale).to_vec();
			let right = g.mem.as_mut().unwrap().render(mw, h, scale).to_vec();
			let px = g.render(w, h, scale).to_vec();
			for r in 0..h {
				assert_eq!(&px[r * w..r * w + cw], &left[r * cw..(r + 1) * cw]);
				assert_eq!(&px[r * w + cw..(r + 1) * w], &right[r * mw..(r + 1) * mw]);
			}
			assert!(g.hover(Some((cw + 5, 3)), w, h));
			assert_eq!(g.mem.as_ref().unwrap().hover, Some(5));
			assert!(g.hover(Some((cw - 1, 3)), w, h));
			assert_eq!(g.cpu.hover, Some(cw - 1));
		}
		// stacked, they share the height evenly whatever their widths
		g.stacked = true;
		assert!(g.hover(Some((5, 6)), 10, 10));
		assert_eq!(g.mem.as_ref().unwrap().hover, Some(5));
	}

	// Test ID: dedqe
	#[test]
	fn cpu_can_be_hidden_but_not_with_nothing_else_shown() {
		let mut g = both();
		assert!(g.hover(Some((5, 3)), 40, 10));
		let had = g.cpu.hist.last();
		let no_cpu = Look {
			show_cpu: false,
			mem_width: 30,
			..Look::default()
		};
		g.set_look(no_cpu.clone(), None).unwrap();
		assert_eq!(g.cpu.hover, None);
		assert_eq!(g.count(), 1);
		assert_eq!(g.widths().collect::<Vec<_>>(), [30]);
		let alone = g.mem.as_mut().unwrap().render(30, 10, 1).to_vec();
		assert_eq!(g.render(30, 10, 1), alone.as_slice());
		assert!(g.hover(Some((5, 3)), 30, 10));
		assert_eq!(g.cpu.hover, None);
		assert_eq!(g.mem.as_ref().unwrap().hover, Some(5));
		assert!(g.tooltip().starts_with("Memory"), "{}", g.tooltip());
		// a hidden graph isn't read
		g.sample();
		assert_eq!(g.cpu.hist.last(), had);
		// with nothing else to show, CPU shows anyway
		let none = Look {
			show_memory: false,
			..no_cpu
		};
		g.set_look(none.clone(), None).unwrap();
		assert!(g.cpu_shown());
		assert_eq!(g.widths().collect::<Vec<_>>(), [none.width]);
		assert_eq!(g.tooltip(), "CPU");
		assert!(Graphs::new(none).cpu_shown());
	}

	// Test ID: dedqp
	#[test]
	fn the_saved_order_lays_them_out_and_one_it_leaves_out_goes_last() {
		let mut g = both();
		let order = |names: &[&str]| names.iter().map(|n| (*n).to_string()).collect();
		g.set_look(
			Look {
				cpu_width: 40,
				mem_width: 100,
				order: order(&["mem", "cpu"]),
				..Look::default()
			},
			None,
		)
		.unwrap();
		assert_eq!(g.widths().collect::<Vec<_>>(), [100, 40]);
		let (w, h) = (140, 10);
		let left = g.mem.as_mut().unwrap().render(100, h, 1).to_vec();
		let right = g.cpu.render(40, h, 1).to_vec();
		let px = g.render(w, h, 1).to_vec();
		for r in 0..h {
			assert_eq!(
				&px[r * w..r * w + 100],
				&left[r * 100..(r + 1) * 100],
				"{r}"
			);
			assert_eq!(
				&px[r * w + 100..(r + 1) * w],
				&right[r * 40..(r + 1) * 40],
				"{r}"
			);
		}
		assert!(g.hover(Some((5, 3)), w, h));
		assert_eq!(g.mem.as_ref().unwrap().hover, Some(5));
		assert_eq!(g.cpu.hover, None);
		assert!(g.hover_tip(105, 3, w, h, 1).contains("CPU 50%"));
		assert!(g.tooltip().starts_with("Memory"), "{}", g.tooltip());
		// stacked, the first is on top
		g.stacked = true;
		assert!(g.hover(Some((6, 7)), 10, 10));
		assert_eq!(g.cpu.hover, Some(6));
		assert!(g.hover(Some((6, 2)), 10, 10));
		assert_eq!(g.mem.as_ref().unwrap().hover, Some(6));
		g.stacked = false;
		// a name it doesn't know is skipped, and a graph it doesn't name goes last
		for (saved, want) in [
			(order(&["mem"]), [100, 40]),
			(order(&["net", "mem"]), [100, 40]),
			(order(&["cpu"]), [40, 100]),
			(Vec::new(), [40, 100]),
		] {
			let look = Look {
				order: saved,
				..g.look().clone()
			};
			g.set_look(look, None).unwrap();
			assert_eq!(g.widths().collect::<Vec<_>>(), want, "{:?}", g.look().order);
		}
		// a hidden graph keeps its place for when it's back
		let look = Look {
			order: order(&["mem", "cpu"]),
			show_memory: false,
			..g.look().clone()
		};
		g.set_look(look.clone(), None).unwrap();
		assert_eq!(g.widths().collect::<Vec<_>>(), [40]);
		g.set_look(
			Look {
				show_memory: true,
				..look
			},
			None,
		)
		.unwrap();
		assert_eq!(g.widths().collect::<Vec<_>>(), [100, 40]);
	}

	// Test ID: dee9p
	#[test]
	fn every_graph_takes_its_colors_from_the_one_theme() {
		let spec = crate::config::spec();
		let theme = spec.group(crate::config::THEME).unwrap();
		let top = spec.get(&theme.chooser).unwrap().tab;
		assert_eq!(spec.tabs[top].parent, None);
		let under_themes = |tab: usize| tab == top || spec.tabs[tab].parent == Some(top);
		// no graph gets a pick of its own
		for g in &spec.groups {
			let at = spec.get(&g.chooser).unwrap().tab;
			assert!(g.id == theme.id || !under_themes(at), "{}", g.id);
		}
		for w in Which::ALL {
			let mode = format!("{}_mode", w.name());
			let s = spec.get(&mode).unwrap_or_else(|| panic!("no {mode}"));
			assert_eq!(s.group.as_deref(), Some(theme.id.as_str()), "{mode}");
			assert_eq!(spec.tabs[s.tab].parent, Some(top), "{mode}");
		}
	}
}
