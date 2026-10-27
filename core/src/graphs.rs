// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! Every graph one panel item shows, side by side, or stacked on a vertical
//! panel, with a little room across the panel and twice that between them.

use std::io;
use std::path::Path;

use crate::battery::BatteryReader;
use crate::config::Look;
use crate::cpu::CpuReader;
use crate::keep;
use crate::mem::{MemoryReader, SwapReader};
use crate::{Graph, Source};

pub struct Graphs {
	pub cpu: Graph,
	/// None when hidden, or on a box where memory can't be read.
	pub mem: Option<Graph<MemoryReader, 3>>,
	/// None when hidden, or on a box with no swap.
	pub swap: Option<Graph<SwapReader, 1>>,
	/// None when hidden, or on a box with no battery in.
	pub battery: Option<Graph<BatteryReader, 3>>,
	/// Top to bottom, for a vertical panel.
	pub stacked: bool,
	px: Vec<u32>,
}

/// A graph's place in the area: x, y, width, height.
type Tile = (usize, usize, usize, usize);

/// Logical pixels left clear across the panel, above and below the graphs on
/// a horizontal one, or either side of them on a vertical one.
pub const EDGE: u32 = 2;

/// Logical pixels left clear between 2 graphs.
pub const GAP: u32 = 2 * EDGE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Which {
	Cpu,
	Mem,
	Swap,
	Battery,
}

impl Which {
	/// The order with nothing saved. Graphs a saved order leaves out go
	/// last, in this order.
	const ALL: [Which; 4] = [Which::Cpu, Which::Mem, Which::Swap, Which::Battery];

	fn name(self) -> &'static str {
		match self {
			Which::Cpu => CpuReader::SECTION,
			Which::Mem => MemoryReader::SECTION,
			Which::Swap => SwapReader::SECTION,
			Which::Battery => BatteryReader::SECTION,
		}
	}

	fn wanted(self, look: &Look) -> bool {
		match self {
			Which::Cpu => look.show_cpu,
			Which::Mem => look.show_memory,
			Which::Swap => look.show_swap,
			Which::Battery => look.show_battery,
		}
	}

	/// Every graph but CPU, which is always there.
	fn optional() -> impl Iterator<Item = Which> {
		Which::ALL.into_iter().filter(|&w| w != Which::Cpu)
	}

	fn own_width(self, look: &Look) -> u32 {
		match self {
			Which::Cpu => look.cpu_width,
			Which::Mem => look.mem_width,
			Which::Swap => look.swap_width,
			Which::Battery => look.battery_width,
		}
	}
}

/// What [`Graphs`] needs of a graph, whatever it reads, so each one isn't
/// handled by hand everywhere.
pub trait Drawn {
	fn sample(&mut self);
	fn trim(&mut self);
	fn clear(&mut self);
	fn section(&self) -> (&'static str, Vec<u8>);
	fn adopt(&mut self, sections: &[(String, Vec<u8>)]) -> bool;
	fn hover(&self) -> Option<usize>;
	fn set_hover(&mut self, x: Option<usize>);
	fn easing(&self) -> bool;
	fn hover_tip(&self, x: usize, w: usize, h: usize, scale: usize) -> String;
	fn tooltip(&self) -> String;
	fn render(&mut self, w: usize, h: usize, scale: usize) -> &[u32];
	fn set_look(&mut self, look: &Look);
}

impl<S: Source<N>, const N: usize> Drawn for Graph<S, N> {
	fn sample(&mut self) {
		Graph::sample(self);
	}
	fn trim(&mut self) {
		Graph::trim(self);
	}
	fn clear(&mut self) {
		Graph::clear(self);
	}
	fn section(&self) -> (&'static str, Vec<u8>) {
		Graph::section(self)
	}
	fn adopt(&mut self, sections: &[(String, Vec<u8>)]) -> bool {
		Graph::adopt(self, sections)
	}
	fn hover(&self) -> Option<usize> {
		self.hover
	}
	fn set_hover(&mut self, x: Option<usize>) {
		self.hover = x;
	}
	fn easing(&self) -> bool {
		Graph::easing(self)
	}
	fn hover_tip(&self, x: usize, w: usize, h: usize, scale: usize) -> String {
		Graph::hover_tip(self, x, w, h, scale)
	}
	fn tooltip(&self) -> String {
		Graph::tooltip(self)
	}
	fn render(&mut self, w: usize, h: usize, scale: usize) -> &[u32] {
		Graph::render(self, w, h, scale)
	}
	fn set_look(&mut self, look: &Look) {
		look.clone_into(&mut self.look);
	}
}

// the saved order, unknown names skipped, then any it doesn't name
fn order_of(saved: &[String]) -> [Which; Which::ALL.len()] {
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

/// Puts the hover line on the graph `hit` names, at its device column, and
/// takes it off every other one. True when that changed what's drawn. The
/// settings window's graph list goes through this too.
pub fn point<'a, K: PartialEq + Copy>(
	all: impl IntoIterator<Item = (K, &'a mut dyn Drawn)>,
	hit: Option<(K, usize)>,
) -> bool {
	let mut changed = false;
	for (k, g) in all {
		let want = hit.and_then(|(j, x)| (j == k).then_some(x));
		if g.hover() != want {
			g.set_hover(want);
			changed = true;
		}
	}
	changed
}

// drops a graph turned off and makes one turned on. True when one was made.
fn switch<G>(slot: &mut Option<G>, show: bool, make: impl FnOnce() -> Option<G>) -> bool {
	if !show {
		*slot = None;
		return false;
	}
	if slot.is_some() {
		return false;
	}
	*slot = make();
	slot.is_some()
}

impl Graphs {
	pub fn new(look: Look) -> Graphs {
		let mem = if look.show_memory {
			Graph::memory(look.clone())
		} else {
			None
		};
		let swap = if look.show_swap {
			Graph::swap(look.clone())
		} else {
			None
		};
		let battery = if look.show_battery {
			Graph::battery(look.clone())
		} else {
			None
		};
		Graphs {
			cpu: Graph::new(look),
			mem,
			swap,
			battery,
			stacked: false,
			px: Vec::new(),
		}
	}

	pub fn look(&self) -> &Look {
		&self.cpu.look
	}

	// CPU always has one, shown or not
	fn slot(&self, which: Which) -> Option<&dyn Drawn> {
		match which {
			Which::Cpu => Some(&self.cpu),
			Which::Mem => self.mem.as_ref().map(|g| g as &dyn Drawn),
			Which::Swap => self.swap.as_ref().map(|g| g as &dyn Drawn),
			Which::Battery => self.battery.as_ref().map(|g| g as &dyn Drawn),
		}
	}

	fn slot_mut(&mut self, which: Which) -> Option<&mut dyn Drawn> {
		match which {
			Which::Cpu => Some(&mut self.cpu),
			Which::Mem => self.mem.as_mut().map(|g| g as &mut dyn Drawn),
			Which::Swap => self.swap.as_mut().map(|g| g as &mut dyn Drawn),
			Which::Battery => self.battery.as_mut().map(|g| g as &mut dyn Drawn),
		}
	}

	/// CPU is drawn whenever no other graph is, so the panel item never
	/// shrinks to nothing and its menu stays in reach.
	pub fn cpu_shown(&self) -> bool {
		self.cpu.look.show_cpu || Which::optional().all(|w| self.slot(w).is_none())
	}

	fn is_shown(&self, which: Which) -> bool {
		match which {
			Which::Cpu => self.cpu_shown(),
			_ => self.slot(which).is_some(),
		}
	}

	fn order(&self) -> [Which; Which::ALL.len()] {
		order_of(&self.cpu.look.order)
	}

	/// In the order drawn.
	fn shown(&self) -> impl Iterator<Item = Which> + '_ {
		self.order().into_iter().filter(|&w| self.is_shown(w))
	}

	pub fn count(&self) -> usize {
		self.shown().count()
	}

	fn width_of(&self, which: Which) -> u32 {
		let l = &self.cpu.look;
		l.width_or(which.own_width(l))
	}

	/// Each shown graph's width in logical pixels, in the order drawn.
	pub fn widths(&self) -> impl Iterator<Item = u32> + '_ {
		self.shown().map(|w| self.width_of(w))
	}

	/// The size to ask a panel of `size` logical pixels for. Side by side it's
	/// the widths' sum and the gaps by the panel's height. Stacked, it's the
	/// panel's width, and half that for each graph's height, plus the gaps.
	pub fn wanted(&self, size: u32) -> (u32, u32) {
		let n = u32::try_from(self.count()).unwrap_or(u32::MAX);
		let gaps = GAP.saturating_mul(n.saturating_sub(1));
		if self.stacked {
			(
				size,
				(size / 2).max(16).saturating_mul(n).saturating_add(gaps),
			)
		} else {
			(self.widths().sum::<u32>().saturating_add(gaps), size)
		}
	}

	/// True when `look` would hide a graph that's drawn now, so what it read
	/// should be saved first.
	pub fn drops(&self, look: &Look) -> bool {
		Which::optional().any(|w| self.slot(w).is_some() && !w.wanted(look))
	}

	/// Takes a new look, adding or dropping graphs to match. One added loads
	/// its history from `history`, when that's given.
	///
	/// # Errors
	///
	/// As [`keep::read`], but not for a file that isn't there. The look is
	/// taken anyway.
	pub fn set_look(&mut self, look: Look, history: Option<&Path>) -> io::Result<()> {
		let mut added = Vec::new();
		if switch(&mut self.mem, look.show_memory, || {
			Graph::memory(look.clone())
		}) {
			added.push(Which::Mem);
		}
		if switch(&mut self.swap, look.show_swap, || Graph::swap(look.clone())) {
			added.push(Which::Swap);
		}
		if switch(&mut self.battery, look.show_battery, || {
			Graph::battery(look.clone())
		}) {
			added.push(Which::Battery);
		}
		let mut res = Ok(());
		if let Some(path) = history.filter(|_| !added.is_empty()) {
			match keep::read(path) {
				Ok(sections) => {
					for &w in &added {
						if let Some(g) = self.slot_mut(w) {
							g.adopt(&sections);
						}
					}
				}
				Err(e) if e.kind() == io::ErrorKind::NotFound => {}
				Err(e) => res = Err(e),
			}
		}
		for w in Which::optional() {
			if let Some(g) = self.slot_mut(w) {
				g.set_look(&look);
			}
		}
		self.cpu.look = look;
		if !self.cpu_shown() {
			self.cpu.hover = None;
		}
		res
	}

	/// A hidden graph isn't read, so it shows a gap when it's back.
	pub fn sample(&mut self) {
		for w in Which::ALL {
			if self.is_shown(w)
				&& let Some(g) = self.slot_mut(w)
			{
				g.sample();
			}
		}
	}

	pub fn trim(&mut self) {
		for w in Which::ALL {
			if self.is_shown(w)
				&& let Some(g) = self.slot_mut(w)
			{
				g.trim();
			}
		}
	}

	/// Starts every graph over. With `saved`, that file goes too, a hidden
	/// graph's history and all.
	///
	/// # Errors
	///
	/// As [`keep::remove`]. The graphs start over anyway.
	pub fn reset(&mut self, saved: Option<&Path>) -> io::Result<()> {
		for w in Which::ALL {
			if let Some(g) = self.slot_mut(w) {
				g.clear();
			}
		}
		saved.map_or(Ok(()), keep::remove)
	}

	/// Every graph's history, plus any section in the file that none of them
	/// owns, such as a hidden graph's, so showing it again brings it back.
	///
	/// # Errors
	///
	/// As [`keep::write`].
	pub fn save(&self, path: &Path) -> io::Result<()> {
		let ours: Vec<(&str, Vec<u8>)> = Which::ALL
			.into_iter()
			.filter_map(|w| self.slot(w).map(Drawn::section))
			.collect();
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
		let mut any = false;
		for w in Which::ALL {
			if let Some(g) = self.slot_mut(w) {
				any |= g.adopt(&sections);
			}
		}
		if any {
			Ok(())
		} else {
			Err(io::Error::new(io::ErrorKind::InvalidData, "no history"))
		}
	}

	// side by side, what the gaps leave is shared out by width, since the
	// panel may not give exactly what was asked for. Stacked, they share it
	// evenly. A panel too thin for the edges keeps most of itself for graphs.
	fn tiles(&self, w: usize, h: usize, scale: usize) -> impl Iterator<Item = (Which, Tile)> + '_ {
		let (n, stacked) = (self.count(), self.stacked);
		let total = self.widths().map(|x| x as usize).sum::<usize>().max(1);
		let (along, across) = if stacked { (h, w) } else { (w, h) };
		let gap = GAP as usize * scale;
		let edge = (EDGE as usize * scale).min(across / 4);
		let left = along.saturating_sub(gap * n.saturating_sub(1));
		let (mut i, mut done) = (0, 0);
		self.shown().map(move |which| {
			let own = self.width_of(which) as usize;
			let (a, b) = if stacked {
				(i * left / n, (i + 1) * left / n)
			} else {
				(done * left / total, (done + own) * left / total)
			};
			let (at, long, thick) = (a + i * gap, b - a, across - 2 * edge);
			let tile = if stacked {
				(edge, at, thick, long)
			} else {
				(at, edge, long, thick)
			};
			i += 1;
			done += own;
			(which, tile)
		})
	}

	// which graph holds device pixel (x, y), and the column in it. The edges
	// count as the graph beside them, so a pointer pushed to the screen's edge
	// still finds it. A gap between 2 finds neither.
	fn find(
		&self,
		x: usize,
		y: usize,
		w: usize,
		h: usize,
		scale: usize,
	) -> Option<(Which, Tile, usize)> {
		let stacked = self.stacked;
		self.tiles(w, h, scale)
			.find(|&(_, (tx, ty, tw, th))| {
				let (from, long, at) = if stacked { (ty, th, y) } else { (tx, tw, x) };
				tw > 0 && th > 0 && (from..from + long).contains(&at)
			})
			.map(|(which, t)| (which, t, x.clamp(t.0, t.0 + t.2 - 1) - t.0))
	}

	fn all_mut(&mut self) -> impl Iterator<Item = (Which, &mut dyn Drawn)> {
		let optional = [
			(Which::Mem, self.mem.as_mut().map(|g| g as &mut dyn Drawn)),
			(Which::Swap, self.swap.as_mut().map(|g| g as &mut dyn Drawn)),
			(
				Which::Battery,
				self.battery.as_mut().map(|g| g as &mut dyn Drawn),
			),
		];
		std::iter::once((Which::Cpu, &mut self.cpu as &mut dyn Drawn))
			.chain(optional.into_iter().filter_map(|(w, g)| Some((w, g?))))
	}

	/// Puts the hover line under device pixel (x, y), or takes it away. True
	/// when that changed what's drawn.
	pub fn hover(&mut self, at: Option<(usize, usize)>, w: usize, h: usize, scale: usize) -> bool {
		let hit = at
			.and_then(|(x, y)| self.find(x, y, w, h, scale))
			.map(|(which, _, cx)| (which, cx));
		point(self.all_mut(), hit)
	}

	/// True while a shown graph's axis is easing in.
	pub fn easing(&self) -> bool {
		self.shown()
			.any(|w| self.slot(w).is_some_and(Drawn::easing))
	}

	pub fn hovered(&self) -> bool {
		Which::ALL
			.into_iter()
			.any(|w| self.slot(w).is_some_and(|g| g.hover().is_some()))
	}

	/// The tip for the pointer at device pixel (x, y).
	pub fn hover_tip(&self, x: usize, y: usize, w: usize, h: usize, scale: usize) -> String {
		self.find(x, y, w, h, scale)
			.and_then(|(which, (_, _, tw, th), cx)| {
				Some(self.slot(which)?.hover_tip(cx, tw, th, scale))
			})
			.unwrap_or_else(|| self.tooltip())
	}

	/// The latest reads of every graph shown.
	pub fn tooltip(&self) -> String {
		let tips: Vec<String> = self
			.shown()
			.filter_map(|w| self.slot(w).map(Drawn::tooltip))
			.collect();
		tips.join("\n\n")
	}

	/// Every graph as cairo ARGB32 pixels, `w` by `h`.
	pub fn render(&mut self, w: usize, h: usize, scale: usize) -> &[u32] {
		let mut px = std::mem::take(&mut self.px);
		px.clear();
		px.resize(w * h, 0);
		let tiles: Vec<(Which, Tile)> = self.tiles(w, h, scale).collect();
		for &(which, (tx, ty, tw, th)) in &tiles {
			if tw == 0 || th == 0 {
				continue;
			}
			let Some(g) = self.slot_mut(which) else {
				continue;
			};
			let one = g.render(tw, th, scale);
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

	const E: usize = EDGE as usize;
	const G: usize = GAP as usize;

	// one graph's own pixels sit at its tile in the whole area's
	fn sits(px: &[u32], w: usize, one: &[u32], (x, y, tw, th): Tile) {
		for r in 0..th {
			let at = (y + r) * w + x;
			assert_eq!(
				&px[at..at + tw],
				&one[r * tw..(r + 1) * tw],
				"row {r} of the tile at {x},{y}"
			);
		}
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
		let (w, h) = (40 + G, 10 + 2 * E);
		let left = g.cpu.render(20, 10, 1).to_vec();
		let right = g.mem.as_mut().unwrap().render(20, 10, 1).to_vec();
		let px = g.render(w, h, 1).to_vec();
		sits(&px, w, &left, (0, E, 20, 10));
		sits(&px, w, &right, (20 + G, E, 20, 10));
		assert!(g.hover(Some((25 + G, 3)), w, h, 1));
		assert_eq!(g.cpu.hover, None);
		assert_eq!(g.mem.as_ref().unwrap().hover, Some(5));
		assert!(!g.hover(Some((25 + G, 4)), w, h, 1), "same column");
		assert!(
			g.hover_tip(25 + G, 3, w, h, 1)
				.contains("Memory 25% in use")
		);
		assert!(g.hover_tip(5, 3, w, h, 1).contains("CPU 50%"));
		assert!(g.hover(None, 0, 0, 1));
		assert!(!g.hovered());
		// on a vertical panel the second is under the first
		g.stacked = true;
		assert!(g.hover(Some((5 + E, 17)), 14, 24, 1));
		assert_eq!(g.mem.as_ref().unwrap().hover, Some(5));
		assert!(g.hover(Some((5 + E, 2)), 14, 24, 1));
		assert_eq!(g.cpu.hover, Some(5));
		assert_eq!(g.mem.as_ref().unwrap().hover, None);
		assert_eq!(g.render(14, 24, 1).len(), 14 * 24);
		assert!(g.tooltip().starts_with("CPU\n\nMemory"));
	}

	// Test ID: defw2
	#[test]
	fn the_edges_and_gaps_stay_clear_and_an_edge_still_finds_its_graph() {
		let mut g = both();
		for (stacked, w, h) in [(false, 40 + G, 10 + 2 * E), (true, 10 + 2 * E, 40 + G)] {
			g.stacked = stacked;
			let px = g.render(w, h, 1).to_vec();
			let clear = |x: usize, y: usize| px[y * w + x] == 0;
			let (along, across) = if stacked { (h, w) } else { (w, h) };
			let at = |a: usize, c: usize| if stacked { (c, a) } else { (a, c) };
			for a in 0..along {
				for c in (0..E).chain(across - E..across) {
					let (x, y) = at(a, c);
					assert!(clear(x, y), "edge {x},{y} stacked {stacked}");
				}
			}
			for a in 20..20 + G {
				for c in 0..across {
					let (x, y) = at(a, c);
					assert!(clear(x, y), "gap {x},{y} stacked {stacked}");
				}
			}
			let (x, y) = at(5, E);
			assert_ne!(px[y * w + x], 0, "stacked {stacked}");
			// the screen's edge, past the graph's, still finds the graph
			let (x, y) = at(25 + G, 0);
			assert!(g.hover(Some((x, y)), w, h, 1));
			assert!(g.mem.as_ref().unwrap().hover.is_some());
			let (x, y) = at(5, across - 1);
			assert!(g.hover(Some((x, y)), w, h, 1));
			assert!(g.cpu.hover.is_some());
			// a gap finds neither, and its tip is every graph's latest
			let (x, y) = at(21, E + 1);
			assert!(g.hover(Some((x, y)), w, h, 1));
			assert!(!g.hovered());
			assert_eq!(g.hover_tip(x, y, w, h, 1), g.tooltip());
		}
		// at 2x the edges and gaps are twice as many device pixels
		g.stacked = false;
		let (w, h) = (2 * (40 + G), 2 * (10 + 2 * E));
		let px = g.render(w, h, 2).to_vec();
		assert_eq!(px[(2 * E - 1) * w + 5], 0);
		assert_ne!(px[2 * E * w + 5], 0);
		assert_eq!(px[(2 * E + 1) * w + 2 * 20 + 2 * G - 1], 0);
		assert_ne!(px[(2 * E + 1) * w + 2 * (20 + G)], 0);
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

	// Test ID: defxr
	#[test]
	fn a_reset_starts_every_graph_over_and_drops_the_file_only_when_kept() {
		let path = scratch("reset");
		keep::write(&path, &[("net", b"later")]).unwrap();
		let mut g = both();
		g.save(&path).unwrap();
		g.reset(None).unwrap();
		assert_eq!(g.cpu.hist.last(), None);
		assert_eq!(g.mem.as_ref().unwrap().hist.last(), None);
		assert!(path.exists());
		let mut g = both();
		g.reset(Some(&path)).unwrap();
		assert_eq!(g.cpu.hist.last(), None);
		assert!(!path.exists());
		// and it goes on from there
		g.cpu.sample();
		std::thread::sleep(std::time::Duration::from_millis(20));
		g.cpu.sample();
		assert!(g.cpu.hist.last().is_some());
		assert!(g.reset(Some(&path)).is_ok());
	}

	// Test ID: defv4
	#[test]
	fn a_file_the_5_tier_plugin_wrote_still_loads() {
		// saved at a panel restart by the plugin built from 6753a2b's parent,
		// before the 50 ms tier
		let old = include_bytes!("../tests/data/plugin-5-tiers.hist");
		let path = scratch("five");
		std::fs::write(&path, old).unwrap();
		let sections = keep::read(&path).unwrap();
		let ends = |name: &str| {
			let body = &sections.iter().find(|(n, _)| n == name).unwrap().1;
			let f = |at: usize| f64::from_le_bytes(body[at..at + 8].try_into().unwrap());
			assert_eq!(u32::from_le_bytes(body[16..20].try_into().unwrap()), 5);
			(Some(f(0)), Some(f(8)))
		};
		let (cpu, mem) = (ends("cpu"), ends("mem"));
		// so it never ages out
		let look = Look {
			max_days: 36500,
			..Look::default()
		};
		let mut g = Graphs::new(look.clone());
		g.load(&path).unwrap();
		let got = |g: &Graphs| {
			let m = &g.mem.as_ref().unwrap().hist;
			(
				(g.cpu.hist.first(), g.cpu.hist.last()),
				(m.first(), m.last()),
			)
		};
		assert_eq!(got(&g), (cpu, mem));
		let (a, b) = (cpu.0.unwrap(), cpu.1.unwrap());
		assert!(g.cpu.hist.span(a, b).covered > 0.9);
		// and the next restart reads what this one saves
		g.save(&path).unwrap();
		let tiers = (crate::history::TIERS.len() as u32).to_le_bytes();
		assert!(
			keep::read(&path)
				.unwrap()
				.iter()
				.all(|(_, body)| body[16..20] == tiers)
		);
		let mut back = Graphs::new(look);
		back.load(&path).unwrap();
		assert_eq!(got(&back), (cpu, mem));
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
		let th = 10;
		for scale in [1, 2] {
			let (cw, mw, gap, edge) = (40 * scale, 100 * scale, G * scale, E * scale);
			let (w, h) = (cw + gap + mw, th + 2 * edge);
			let left = g.cpu.render(cw, th, scale).to_vec();
			let right = g.mem.as_mut().unwrap().render(mw, th, scale).to_vec();
			let px = g.render(w, h, scale).to_vec();
			sits(&px, w, &left, (0, edge, cw, th));
			sits(&px, w, &right, (cw + gap, edge, mw, th));
			assert!(g.hover(Some((cw + gap + 5, 3)), w, h, scale));
			assert_eq!(g.mem.as_ref().unwrap().hover, Some(5));
			assert!(g.hover(Some((cw - 1, 3)), w, h, scale));
			assert_eq!(g.cpu.hover, Some(cw - 1));
		}
		// stacked, they share the height evenly whatever their widths
		g.stacked = true;
		assert!(g.hover(Some((5 + E, 17)), 14, 24, 1));
		assert_eq!(g.mem.as_ref().unwrap().hover, Some(5));
	}

	// Test ID: defvs
	#[test]
	fn the_ask_follows_the_panels_size_and_orientation() {
		let mut g = both();
		g.set_look(
			Look {
				mem_width: 30,
				..Look::default()
			},
			None,
		)
		.unwrap();
		let cpu = g.width_of(Which::Cpu);
		assert_eq!(g.wanted(26), (cpu + 30 + GAP, 26));
		assert_eq!(g.wanted(60), (cpu + 30 + GAP, 60));
		// a vertical panel ignores the widths, and has a floor
		g.stacked = true;
		assert_eq!(g.wanted(48), (48, 48 + GAP));
		assert_eq!(g.wanted(20), (20, 32 + GAP));
		g.set_look(
			Look {
				show_memory: false,
				..Look::default()
			},
			None,
		)
		.unwrap();
		assert_eq!(g.wanted(48), (48, 24));
	}

	// Test ID: dedqe
	#[test]
	fn cpu_can_be_hidden_but_not_with_nothing_else_shown() {
		let mut g = both();
		assert!(g.hover(Some((5, 3)), 40, 10, 1));
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
		let px = g.render(30, 10 + 2 * E, 1).to_vec();
		sits(&px, 30, &alone, (0, E, 30, 10));
		assert!(g.hover(Some((5, 3)), 30, 10, 1));
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
		let (w, h) = (140 + G, 10 + 2 * E);
		let left = g.mem.as_mut().unwrap().render(100, 10, 1).to_vec();
		let right = g.cpu.render(40, 10, 1).to_vec();
		let px = g.render(w, h, 1).to_vec();
		sits(&px, w, &left, (0, E, 100, 10));
		sits(&px, w, &right, (100 + G, E, 40, 10));
		assert!(g.hover(Some((5, 3)), w, h, 1));
		assert_eq!(g.mem.as_ref().unwrap().hover, Some(5));
		assert_eq!(g.cpu.hover, None);
		assert!(g.hover_tip(105 + G, 3, w, h, 1).contains("CPU 50%"));
		assert!(g.tooltip().starts_with("Memory"), "{}", g.tooltip());
		// stacked, the first is on top
		g.stacked = true;
		assert!(g.hover(Some((6 + E, 17)), 14, 24, 1));
		assert_eq!(g.cpu.hover, Some(6));
		assert!(g.hover(Some((6 + E, 2)), 14, 24, 1));
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

	// all 3, with swap made by hand so a box with no swap runs it too
	fn three() -> Graphs {
		let mut g = both();
		let look = Look {
			show_swap: true,
			..Look::default()
		};
		g.swap = Some(Graph::with(look.clone(), SwapReader::default()));
		g.set_look(look, None).unwrap();
		let now = g.cpu.now();
		g.swap.as_mut().unwrap().hist.add(now - 60.0, now, &[1.0]);
		g
	}

	// Test ID: defq5
	#[test]
	fn swap_is_off_by_default_and_draws_its_own_color_when_on() {
		assert!(!Look::default().show_swap);
		assert!(Graphs::new(Look::default()).swap.is_none());
		let mut g = three();
		assert_eq!(g.count(), 3);
		let s = g.swap.as_mut().unwrap();
		let px = s.render(64, 20, 1);
		assert_eq!(px[10 * 64 + 60], Look::default().swap[0].argb());
		let tip = s.hover_tip(60, 64, 20, 1);
		assert!(tip.ends_with("\nSwap 100% used"), "{tip}");
		// Fire draws it as a gradient
		let fire = Look::from_model(&crate::config::model("theme:\n\tname: fire\n"));
		assert_eq!(
			fire.swap_paint(),
			crate::render::Paint::Gradient(fire.swap_gradient)
		);
		if let Some(mut live) = Graph::swap(Look::default()) {
			live.sample();
			std::thread::sleep(std::time::Duration::from_millis(20));
			live.sample();
			assert!(live.tooltip().starts_with("Swap "), "{}", live.tooltip());
		}
	}

	// Test ID: defq6
	#[test]
	fn three_sit_side_by_side_and_the_pointer_finds_swap() {
		let mut g = three();
		let (w, h) = (60 + 2 * G, 10 + 2 * E);
		let tiles: Vec<Vec<u32>> = vec![
			g.cpu.render(20, 10, 1).to_vec(),
			g.mem.as_mut().unwrap().render(20, 10, 1).to_vec(),
			g.swap.as_mut().unwrap().render(20, 10, 1).to_vec(),
		];
		let px = g.render(w, h, 1).to_vec();
		for (i, t) in tiles.iter().enumerate() {
			sits(&px, w, t, (i * (20 + G), E, 20, 10));
		}
		assert!(g.hover(Some((45 + 2 * G, 3)), w, h, 1));
		assert_eq!(g.swap.as_ref().unwrap().hover, Some(5));
		assert_eq!((g.cpu.hover, g.mem.as_ref().unwrap().hover), (None, None));
		assert!(g.hovered());
		assert!(
			g.hover_tip(45 + 2 * G, 3, w, h, 1)
				.contains("Swap 100% used")
		);
		// nothing read yet, so each says its title
		assert_eq!(g.tooltip(), "CPU\n\nMemory\n\nSwap");
		// first in a saved order
		let look = Look {
			order: vec!["swap".into()],
			swap_width: 30,
			..g.look().clone()
		};
		g.set_look(look, None).unwrap();
		let w0 = g.look().width;
		assert_eq!(g.widths().collect::<Vec<_>>(), [30, w0, w0]);
		assert!(g.hover(Some((7, 3)), 30 + 2 * w0 as usize + 2 * G, h, 1));
		assert_eq!(g.swap.as_ref().unwrap().hover, Some(7));
		// swap alone hides CPU, as memory alone does
		let alone = Look {
			show_cpu: false,
			show_memory: false,
			..g.look().clone()
		};
		g.set_look(alone, None).unwrap();
		assert!(!g.cpu_shown());
		assert_eq!(g.widths().collect::<Vec<_>>(), [30]);
	}

	// Test ID: defq7
	#[test]
	fn hiding_swap_keeps_its_history_for_when_its_back() {
		let path = scratch("swap");
		let mut g = three();
		let had = g.swap.as_ref().unwrap().hist.last();
		g.save(&path).unwrap();
		let names: Vec<String> = keep::read(&path)
			.unwrap()
			.into_iter()
			.map(|(n, _)| n)
			.collect();
		assert_eq!(names, ["cpu", "mem", "swap"]);
		let on = g.look().clone();
		let off = Look {
			show_swap: false,
			..on.clone()
		};
		assert!(g.drops(&off));
		assert!(!g.drops(&on));
		g.set_look(off, Some(&path)).unwrap();
		assert!(g.swap.is_none());
		g.save(&path).unwrap();
		let mut back = Graphs::new(Look::default());
		back.load(&path).unwrap();
		assert!(back.swap.is_none(), "off by default");
		// a box with no swap can't bring it back, so the rest is for one with it
		if <SwapReader as crate::collect::Collector<1>>::probe().is_some() {
			g.set_look(on, Some(&path)).unwrap();
			assert_eq!(g.swap.as_ref().unwrap().hist.last(), had);
		}
		std::fs::remove_file(&path).unwrap();
	}

	// Test ID: defqc
	#[test]
	fn the_battery_graph_colors_the_charge_by_state() {
		assert!(!Look::default().show_battery);
		assert!(Graphs::new(Look::default()).battery.is_none());
		let look = Look {
			show_battery: true,
			..Look::default()
		};
		let mut b = Graph::with(look.clone(), BatteryReader::default());
		let now = b.now();
		b.hist.add(now - 60.0, now, &[0.0, 0.8, 0.0]);
		let px = b.render(64, 20, 1);
		assert_eq!(px[10 * 64 + 60], look.battery[1].argb());
		let tip = b.hover_tip(60, 64, 20, 1);
		assert!(tip.ends_with("\nBattery 80%, on battery"), "{tip}");
		for (v, want) in [
			([1.0, 0.0, 0.0], "Battery 100%, plugged in"),
			([0.0, 0.0, 0.03], "Battery 3%, critical"),
			([0.0, 0.0, 0.0], "Battery 0%, plugged in"),
		] {
			assert_eq!(<BatteryReader as Source<3>>::text(&v), want);
		}
		// one with none in reads as a gap
		b.sample();
		b.sample();
		assert_eq!(b.tooltip(), "Battery");
		let fire = Look::from_model(&crate::config::model("theme:\n\tname: fire\n"));
		assert_eq!(
			fire.battery_paint(),
			crate::render::Paint::Gradient(fire.battery_gradient)
		);
	}

	// Test ID: defqd
	#[test]
	fn four_sit_side_by_side_and_battery_keeps_its_history_hidden() {
		let path = scratch("battery");
		let mut g = three();
		let look = Look {
			show_battery: true,
			..g.look().clone()
		};
		g.battery = Some(Graph::with(look.clone(), BatteryReader::default()));
		g.set_look(look.clone(), None).unwrap();
		let now = g.cpu.now();
		let b = g.battery.as_mut().unwrap();
		b.hist.add(now - 60.0, now, &[0.5, 0.0, 0.0]);
		let had = b.hist.last();
		assert_eq!(g.count(), 4);
		let (w, h) = (80 + 3 * G, 10 + 2 * E);
		let right = g.battery.as_mut().unwrap().render(20, 10, 1).to_vec();
		let px = g.render(w, h, 1).to_vec();
		sits(&px, w, &right, (60 + 3 * G, E, 20, 10));
		assert!(g.hover(Some((65 + 3 * G, 3)), w, h, 1));
		assert_eq!(g.battery.as_ref().unwrap().hover, Some(5));
		assert!(
			g.hover_tip(65 + 3 * G, 3, w, h, 1)
				.contains("Battery 50%, plugged in")
		);
		g.save(&path).unwrap();
		let names: Vec<String> = keep::read(&path)
			.unwrap()
			.into_iter()
			.map(|(n, _)| n)
			.collect();
		assert_eq!(names, ["cpu", "mem", "swap", "battery"]);
		let off = Look {
			show_battery: false,
			..look
		};
		assert!(g.drops(&off));
		g.set_look(off, Some(&path)).unwrap();
		assert!(g.battery.is_none());
		assert_eq!(g.count(), 3);
		g.save(&path).unwrap();
		let back = keep::read(&path).unwrap();
		let mut again = Graph::with(Look::default(), BatteryReader::default());
		assert!(again.adopt(&back));
		assert_eq!(again.hist.last(), had);
		std::fs::remove_file(&path).unwrap();
	}
}
