// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! The part of tukzmon every panel shares. No GUI toolkit in here.

pub mod axis;
pub mod collect;
pub mod config;
pub mod cpu;
pub mod disk;
pub mod graphs;
pub mod history;
pub mod keep;
pub mod load;
pub mod mem;
pub mod net;
pub mod render;
pub mod temp;
pub mod when;
pub mod zswap;

use std::io;
use std::path::Path;
use std::time::{Instant, SystemTime};

use axis::{Axis, Timing};
use collect::Collector;
use config::Look;
use cpu::CpuReader;
use history::History;
use mem::MemoryReader;
use render::{Frame, Paint};
use when::Said;

/// A collector as one graph draws and names it.
pub trait Source<const N: usize>: Collector<N> {
	/// The graph's section in the history file.
	const SECTION: &'static str;
	/// The tip before there's anything read.
	const TITLE: &'static str;
	fn text(v: &[f32; N]) -> String;
	fn paint(look: &Look) -> Paint<'_, N>;
	fn timing(look: &Look) -> Timing;
}

fn pc(x: f32) -> i32 {
	(x * 100.0).round() as i32
}

impl Source<4> for CpuReader {
	const SECTION: &'static str = "cpu";
	const TITLE: &'static str = "CPU";

	fn text(v: &[f32; 4]) -> String {
		let busy: f32 = v.iter().sum();
		format!(
			"CPU {}%\nUser {}%  System {}%\nNice {}%  I/O wait {}%",
			pc(busy),
			pc(v[2]),
			pc(v[1]),
			pc(v[3]),
			pc(v[0])
		)
	}

	fn paint(look: &Look) -> Paint<'_, 4> {
		look.cpu_paint()
	}

	fn timing(look: &Look) -> Timing {
		look.timing(look.cpu_time)
	}
}

impl Source<3> for MemoryReader {
	const SECTION: &'static str = "mem";
	const TITLE: &'static str = "Memory";

	fn text(v: &[f32; 3]) -> String {
		format!(
			"Memory {}% in use\nBuffers {}%  Cache {}%",
			pc(v[0]),
			pc(v[1]),
			pc(v[2])
		)
	}

	fn paint(look: &Look) -> Paint<'_, 3> {
		look.mem_paint()
	}

	fn timing(look: &Look) -> Timing {
		look.timing(look.mem_time)
	}
}

/// One graph: reads, history, and drawing. CPU unless said otherwise.
pub struct Graph<S: Source<N> = CpuReader, const N: usize = 4> {
	pub look: Look,
	hist: History<N>,
	src: S,
	clock: Instant,
	/// Unix time at `clock`'s start, so saved history lines up across runs.
	epoch: f64,
	read_at: Option<f64>,
	latest: Option<[f32; N]>,
	px: Vec<u32>,
	/// Where the pointer is, in device pixels from the left, for the hover line.
	pub hover: Option<usize>,
}

impl Graph {
	pub fn new(look: Look) -> Graph {
		Graph::with(look, CpuReader::default())
	}
}

impl Graph<MemoryReader, 3> {
	/// None where memory can't be read, so the graph is left out.
	pub fn memory(look: Look) -> Option<Self> {
		MemoryReader::probe().map(|src| Graph::with(look, src))
	}
}

impl<S: Source<N>, const N: usize> Graph<S, N> {
	fn with(look: Look, src: S) -> Self {
		Graph {
			look,
			hist: History::default(),
			src,
			clock: Instant::now(),
			epoch: SystemTime::now()
				.duration_since(SystemTime::UNIX_EPOCH)
				.map_or(0.0, |d| d.as_secs_f64()),
			read_at: None,
			latest: None,
			px: Vec::new(),
			hover: None,
		}
	}

	fn now(&self) -> f64 {
		self.epoch + self.clock.elapsed().as_secs_f64()
	}

	/// Moves the clock on, so the soak test can run days in seconds.
	#[doc(hidden)]
	pub fn skip(&mut self, secs: f64) {
		self.epoch += secs;
	}

	/// Drops history older than the max age.
	pub fn trim(&mut self) {
		let max_age = f64::from(self.look.max_days) * 86400.0;
		self.hist.trim(self.now() - max_age);
	}

	/// Swaps in the saved history, if there is one that reads. The time since
	/// it was saved is a gap.
	///
	/// # Errors
	///
	/// As [`keep::read`], or `InvalidData` with no history of this graph's
	/// that reads. The history in memory is left as it was.
	pub fn load(&mut self, path: &Path) -> io::Result<()> {
		if self.adopt(&keep::read(path)?) {
			Ok(())
		} else {
			Err(io::Error::new(
				io::ErrorKind::InvalidData,
				format!("no {} history", S::SECTION),
			))
		}
	}

	/// [`Self::load`] from a file's sections, read once for every graph in
	/// it. False, with nothing changed, when none of them is this graph's.
	pub fn adopt(&mut self, sections: &[(String, Vec<u8>)]) -> bool {
		let Some(saved) = sections
			.iter()
			.find(|(name, _)| name == S::SECTION)
			.and_then(|(_, body)| History::decode(body))
		else {
			return false;
		};
		// a clock set back mustn't hide new samples behind old ones
		if let Some(last) = saved.last() {
			self.epoch += (last - self.now()).max(0.0);
		}
		self.hist = saved;
		self.read_at = None;
		self.trim();
		true
	}

	/// The graph's history file section, for [`keep::write`].
	pub fn section(&self) -> (&'static str, Vec<u8>) {
		let mut body = Vec::new();
		self.hist.encode(&mut body);
		(S::SECTION, body)
	}

	/// # Errors
	///
	/// As [`keep::write`].
	pub fn save(&self, path: &Path) -> io::Result<()> {
		let (name, body) = self.section();
		keep::write(path, &[(name, &body)])
	}

	/// Read the source and keep it. The first call only primes it.
	pub fn sample(&mut self) {
		let now = self.now();
		let shares = self.src.read();
		if let (Some(v), Some(was)) = (shares, self.read_at) {
			self.hist.add(was, now, &v);
			self.latest = Some(v);
		}
		self.read_at = Some(now);
	}

	pub fn axis(&self, columns: usize) -> Axis {
		match S::timing(&self.look) {
			Timing::Log(curve) => {
				let total = match (self.hist.first(), self.hist.last()) {
					(Some(a), Some(b)) => b - a,
					_ => 0.0,
				};
				Axis::log(curve, total)
			}
			Timing::Linear => Axis::linear(columns as f64 * self.look.sample_ms as f64 / 1000.0),
		}
	}

	fn columns(&self, w: usize, scale: usize) -> usize {
		w.saturating_sub(2 * self.look.border as usize * scale)
	}

	// as Frame::draw clamps it
	fn border_px(&self, w: usize, h: usize, scale: usize) -> usize {
		(self.look.border as usize * scale).min(w / 2).min(h / 2)
	}

	/// The time under the hover line at device column `x`, oldest end first, as
	/// `render` at that size draws it. None with no history yet.
	pub fn span_at(&self, x: usize, w: usize, h: usize, scale: usize) -> Option<(f64, f64)> {
		let now = self.hist.last()?;
		let bw = self.border_px(w, h, scale);
		if w <= 2 * bw || h <= 2 * bw {
			return None;
		}
		let pw = w - 2 * bw;
		let thick = scale.clamp(1, pw);
		let x0 = x.clamp(bw, bw + pw - thick) - bw;
		let axis = self.axis(self.columns(w, scale));
		let u = |cx: usize| (pw - cx) as f64 / pw as f64;
		Some((now - axis.age(u(x0)), now - axis.age(u(x0 + thick))))
	}

	/// The tip for the pointer at device column `x`: when that is, and what
	/// the source read then.
	pub fn hover_tip(&self, x: usize, w: usize, h: usize, scale: usize) -> String {
		let Some((a, b)) = self.span_at(x, w, h, scale) else {
			return self.tooltip();
		};
		let t = f64::midpoint(a, b);
		let now = self.now().max(t);
		let when = match self.look.said {
			Said::Ago => when::ago(now - t),
			Said::Clock => when::clock(t, now).unwrap_or_else(|| when::ago(now - t)),
		};
		let span = self.hist.span(a, b);
		if span.covered < render::MIN_COVER {
			return format!("{when}\nNo data");
		}
		format!("{when}\n{}", S::text(&span.mean))
	}

	/// The graph as cairo ARGB32 pixels, `w` by `h`.
	pub fn render(&mut self, w: usize, h: usize, scale: usize) -> &[u32] {
		let columns = self.columns(w, scale);
		let axis = self.axis(columns);
		let mut px = std::mem::take(&mut self.px);
		Frame {
			hist: &self.hist,
			axis: &axis,
			now: self.hist.last().unwrap_or(0.0),
			look: &self.look,
			paint: S::paint(&self.look),
		}
		.draw(&mut px, w, h, scale);
		if let Some(x) = self.hover {
			let bw = self.border_px(w, h, scale);
			render::line(&mut px, w, h, bw, x, scale, self.look.hover);
		}
		self.px = px;
		&self.px
	}

	/// The latest read, as the panel's tooltip.
	pub fn tooltip(&self) -> String {
		let Some(v) = self.latest else {
			return S::TITLE.into();
		};
		S::text(&v)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	// Test ID: debj4
	#[test]
	fn samples_and_draws() {
		let mut g = Graph::new(Look::default());
		assert_eq!(g.tooltip(), "CPU");
		g.sample();
		std::thread::sleep(std::time::Duration::from_millis(40));
		g.sample();
		let px = g.render(128, 26, 1);
		assert_eq!(px.len(), 128 * 26);
		assert!(px.iter().all(|p| p >> 24 == 0xff));
		assert!(g.tooltip().starts_with("CPU "));
	}

	// Test ID: debj5
	#[test]
	fn linear_axis_follows_the_sample_rate() {
		let g = Graph::new(Look {
			log_axis: false,
			sample_ms: 250,
			..Look::default()
		});
		assert_eq!(g.axis(100), Axis::linear(25.0));
	}

	// Test ID: decby
	#[test]
	fn log_axis_takes_the_chosen_curve() {
		let mut g = Graph::new(Look::default());
		g.hist.add(0.0, 1.0, &[0.1; 4]);
		g.hist.add(7200.0, 7201.0, &[0.1; 4]);
		assert_eq!(g.axis(100), Axis::log(axis::Curve::Smooth, 7201.0));
		g.look.curve = axis::Curve::FixedA;
		assert_eq!(g.axis(100), Axis::log(axis::Curve::FixedA, 7201.0));
		assert_ne!(g.axis(100), Axis::log(axis::Curve::Smooth, 7201.0));
	}

	// a busy hour, then an idle one, ending now
	fn two_hours() -> Graph {
		let mut g = Graph::new(Look::default());
		let now = g.now();
		g.hist
			.add(now - 7200.0, now - 3600.0, &[0.0, 0.0, 0.8, 0.0]);
		g.hist.add(now - 3600.0, now, &[0.0, 0.0, 0.1, 0.0]);
		g
	}

	// Test ID: decc6
	#[test]
	#[cfg_attr(miri, ignore = "the clock tip calls nl_langinfo, which is C")]
	fn the_hover_tip_says_when_and_what() {
		let mut g = two_hours();
		// 130 px wide, 1 px border: 128 columns, as render would draw them
		let (w, h) = (130, 20);
		let (a, b) = g.span_at(1, w, h, 1).unwrap();
		assert!(
			near_secs(a, g.hist.first().unwrap()),
			"left edge is the first sample"
		);
		assert!(b > a);
		let (a, b) = g.span_at(128, w, h, 1).unwrap();
		assert!(near_secs(b, g.hist.last().unwrap()), "right edge is now");
		assert!(a < b);
		// the border reads as the plot's edge
		assert_eq!(g.span_at(0, w, h, 1), g.span_at(1, w, h, 1));
		assert_eq!(g.span_at(500, w, h, 1), g.span_at(128, w, h, 1));
		let left = g.hover_tip(1, w, h, 1);
		assert!(left.starts_with("1 hr ago\nCPU 80%\nUser 80%"), "{left}");
		let right = g.hover_tip(128, w, h, 1);
		assert!(right.starts_with("just now\nCPU 10%"), "{right}");
		g.look.said = Said::Clock;
		let tip = g.hover_tip(128, w, h, 1);
		assert_eq!(tip.lines().next().map(str::len), Some(8), "{tip}");
		// device columns on a scale 2 screen land on the same times
		assert_eq!(g.span_at(2, 2 * w, 2 * h, 2), g.span_at(1, w, h, 1));
	}

	fn near_secs(a: f64, b: f64) -> bool {
		(a - b).abs() < 1e-6
	}

	// Test ID: decc7
	#[test]
	fn the_hover_tip_shows_gaps_and_no_history() {
		let g = Graph::new(Look::default());
		assert_eq!(g.span_at(5, 130, 20, 1), None);
		assert_eq!(g.hover_tip(5, 130, 20, 1), "CPU");
		let mut g = Graph::new(Look {
			log_axis: false,
			..Look::default()
		});
		let now = g.now();
		g.hist.add(now - 10.0, now, &[0.1; 4]);
		// linear at 500 ms covers 64 s, so the left edge is before the first sample
		assert!(g.hover_tip(1, 130, 20, 1).ends_with("\nNo data"));
		assert!(g.hover_tip(128, 130, 20, 1).contains("CPU 40%"));
	}

	// Test ID: decc8
	#[test]
	fn the_hover_line_is_drawn_where_the_pointer_is() {
		let mut g = two_hours();
		g.look.hover = config::Rgb(1, 2, 3);
		let (w, h) = (40, 10);
		let plain = g.render(w, h, 1).to_vec();
		assert!(!plain.contains(&0xff01_0203));
		g.hover = Some(17);
		let px = g.render(w, h, 1).to_vec();
		for r in 0..h {
			for x in 0..w {
				let lit = px[r * w + x] == 0xff01_0203;
				assert_eq!(lit, x == 17 && (1..h - 1).contains(&r), "{x},{r}");
				if !lit {
					assert_eq!(px[r * w + x], plain[r * w + x]);
				}
			}
		}
		g.hover = None;
		assert_eq!(g.render(w, h, 1), plain.as_slice());
	}

	fn scratch(name: &str) -> std::path::PathBuf {
		std::env::temp_dir().join(format!(
			"test_tukzmon_graph_{}_{name}.hist",
			std::process::id()
		))
	}

	// Test ID: decbp
	#[test]
	fn saved_history_comes_back_with_a_gap() {
		let path = scratch("gap");
		let mut g = Graph::new(Look::default());
		g.sample();
		std::thread::sleep(std::time::Duration::from_millis(30));
		g.sample();
		let (first, last) = (g.hist.first().unwrap(), g.hist.last().unwrap());
		g.save(&path).unwrap();
		std::thread::sleep(std::time::Duration::from_millis(30));
		let mut back = Graph::new(Look::default());
		back.load(&path).unwrap();
		assert_eq!(
			(back.hist.first(), back.hist.last()),
			(Some(first), Some(last))
		);
		// the first read after a load only primes, so the time between is a gap
		back.sample();
		assert_eq!(back.hist.last(), Some(last));
		assert!(back.read_at.unwrap() >= last + 0.03);
		std::fs::remove_file(&path).unwrap();
	}

	// Test ID: decbq
	#[test]
	fn a_clock_set_back_still_takes_new_samples() {
		let path = scratch("back");
		let mut g = Graph::new(Look::default());
		g.hist.add(g.now() + 3600.0, g.now() + 3601.0, &[0.1; 4]);
		let saved = g.hist.last().unwrap();
		g.save(&path).unwrap();
		let mut back = Graph::new(Look::default());
		back.load(&path).unwrap();
		back.sample();
		std::thread::sleep(std::time::Duration::from_millis(30));
		back.sample();
		assert!(back.hist.last().unwrap() > saved);
		std::fs::remove_file(&path).unwrap();
	}

	// Test ID: decbr
	#[test]
	fn load_trims_to_the_max_age_and_refuses_junk() {
		let path = scratch("trim");
		let mut g = Graph::new(Look::default());
		let now = g.now();
		g.hist
			.add(now - 3.0 * 86400.0, now - 3.0 * 86400.0 + 60.0, &[0.1; 4]);
		g.hist.add(now - 60.0, now - 59.0, &[0.2; 4]);
		g.save(&path).unwrap();
		let mut back = Graph::new(Look {
			max_days: 1,
			..Look::default()
		});
		back.load(&path).unwrap();
		assert!(back.hist.first().unwrap() > now - 86401.0);
		assert_eq!(back.hist.last(), Some(now - 59.0));
		std::fs::write(&path, b"junk").unwrap();
		assert!(back.load(&path).is_err());
		assert_eq!(back.hist.last(), Some(now - 59.0), "kept what it had");
		std::fs::remove_file(&path).unwrap();
	}
}
