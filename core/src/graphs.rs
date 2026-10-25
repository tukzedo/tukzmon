// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! Every graph one panel item shows, side by side, or stacked on a vertical
//! panel. No gap between them, the way multiload-ng's spacing of 0 has it.

use std::io;
use std::path::Path;

use crate::Graph;
use crate::config::Look;
use crate::keep;
use crate::mem::MemoryReader;

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

	pub fn count(&self) -> usize {
		1 + usize::from(self.mem.is_some())
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
		res
	}

	pub fn sample(&mut self) {
		self.cpu.sample();
		if let Some(m) = &mut self.mem {
			m.sample();
		}
	}

	pub fn trim(&mut self) {
		self.cpu.trim();
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

	fn tiles(&self, w: usize, h: usize) -> impl Iterator<Item = Tile> {
		let (n, stacked) = (self.count(), self.stacked);
		(0..n).map(move |i| {
			let cut = |len: usize| (i * len / n, (i + 1) * len / n);
			if stacked {
				let (a, b) = cut(h);
				(0, a, w, b - a)
			} else {
				let (a, b) = cut(w);
				(a, 0, b - a, h)
			}
		})
	}

	// which graph holds device pixel (x, y), and where it is in it
	fn find(&self, x: usize, y: usize, w: usize, h: usize) -> Option<(usize, Tile)> {
		self.tiles(w, h)
			.enumerate()
			.find(|&(_, (tx, ty, tw, th))| x < tx + tw && y < ty + th)
	}

	/// Puts the hover line under device pixel (x, y), or takes it away. True
	/// when that changed what's drawn.
	pub fn hover(&mut self, at: Option<(usize, usize)>, w: usize, h: usize) -> bool {
		let hit = at.and_then(|(x, y)| Some((x, self.find(x, y, w, h)?)));
		let want = |i: usize| hit.and_then(|(x, (j, (tx, ..)))| (i == j).then(|| x - tx));
		let mut changed = false;
		let cpu = want(0);
		if self.cpu.hover != cpu {
			self.cpu.hover = cpu;
			changed = true;
		}
		let mem = want(1);
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
			(Some((1, (tx, _, tw, th))), Some(m)) => m.hover_tip(x - tx, tw, th, scale),
			(Some((_, (tx, _, tw, th))), _) => self.cpu.hover_tip(x - tx, tw, th, scale),
			(None, _) => self.tooltip(),
		}
	}

	/// The latest reads of every graph.
	pub fn tooltip(&self) -> String {
		let mut tip = self.cpu.tooltip();
		if let Some(m) = &self.mem {
			tip.push_str("\n\n");
			tip.push_str(&m.tooltip());
		}
		tip
	}

	/// Every graph as cairo ARGB32 pixels, `w` by `h`.
	pub fn render(&mut self, w: usize, h: usize, scale: usize) -> &[u32] {
		let mut px = std::mem::take(&mut self.px);
		px.clear();
		px.resize(w * h, 0);
		let tiles: Vec<Tile> = self.tiles(w, h).collect();
		for (i, &(tx, ty, tw, th)) in tiles.iter().enumerate() {
			if tw == 0 || th == 0 {
				continue;
			}
			let one = match (i, &mut self.mem) {
				(1, Some(m)) => m.render(tw, th, scale),
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
}
