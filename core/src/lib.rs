// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! The part of tukzmon every panel shares. No GUI toolkit in here.

pub mod axis;
pub mod collect;
pub mod config;
pub mod cpu;
pub mod history;
pub mod keep;
pub mod mem;
pub mod render;

use std::io;
use std::path::Path;
use std::time::{Instant, SystemTime};

use axis::Axis;
use collect::Collector;
use config::Look;
use cpu::CpuReader;
use history::History;
use render::Frame;

/// One CPU graph: reads, history, and drawing.
pub struct Graph {
	pub look: Look,
	hist: History<4>,
	cpu: CpuReader,
	clock: Instant,
	/// Unix time at `clock`'s start, so saved history lines up across runs.
	epoch: f64,
	read_at: Option<f64>,
	latest: Option<[f32; 4]>,
	px: Vec<u32>,
}

impl Graph {
	pub fn new(look: Look) -> Graph {
		Graph {
			look,
			hist: History::default(),
			cpu: CpuReader::default(),
			clock: Instant::now(),
			epoch: SystemTime::now()
				.duration_since(SystemTime::UNIX_EPOCH)
				.map_or(0.0, |d| d.as_secs_f64()),
			read_at: None,
			latest: None,
			px: Vec::new(),
		}
	}

	fn now(&self) -> f64 {
		self.epoch + self.clock.elapsed().as_secs_f64()
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
	/// As [`keep::read`], or `InvalidData` with no CPU history that reads.
	/// The history in memory is left as it was.
	pub fn load(&mut self, path: &Path) -> io::Result<()> {
		let saved = keep::read(path)?
			.into_iter()
			.find(|(name, _)| name == "cpu")
			.and_then(|(_, body)| History::decode(&body))
			.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no cpu history"))?;
		// a clock set back mustn't hide new samples behind old ones
		if let Some(last) = saved.last() {
			self.epoch += (last - self.now()).max(0.0);
		}
		self.hist = saved;
		self.read_at = None;
		self.trim();
		Ok(())
	}

	/// # Errors
	///
	/// As [`keep::write`].
	pub fn save(&self, path: &Path) -> io::Result<()> {
		let mut body = Vec::new();
		self.hist.encode(&mut body);
		keep::write(path, &[("cpu", &body)])
	}

	/// Read the CPU and keep it. The first call only primes the counters.
	pub fn sample(&mut self) {
		let now = self.now();
		let shares = self.cpu.read();
		if let (Some(v), Some(was)) = (shares, self.read_at) {
			self.hist.add(was, now, &v);
			self.latest = Some(v);
		}
		self.read_at = Some(now);
	}

	pub fn axis(&self, columns: usize) -> Axis {
		if self.look.log_axis {
			let total = match (self.hist.first(), self.hist.last()) {
				(Some(a), Some(b)) => b - a,
				_ => 0.0,
			};
			Axis::log(self.look.curve, total)
		} else {
			Axis::linear(columns as f64 * self.look.sample_ms as f64 / 1000.0)
		}
	}

	/// The graph as cairo ARGB32 pixels, `w` by `h`.
	pub fn render(&mut self, w: usize, h: usize, scale: usize) -> &[u32] {
		let columns = w.saturating_sub(2 * self.look.border as usize * scale);
		let axis = self.axis(columns);
		let mut px = std::mem::take(&mut self.px);
		Frame {
			hist: &self.hist,
			axis: &axis,
			now: self.hist.last().unwrap_or(0.0),
			look: &self.look,
			colors: &self.look.cpu,
		}
		.draw(&mut px, w, h, scale);
		self.px = px;
		&self.px
	}

	/// The latest read, as the panel's tooltip.
	pub fn tooltip(&self) -> String {
		let Some(v) = self.latest else {
			return "CPU".into();
		};
		let pc = |x: f32| (x * 100.0).round() as i32;
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
}

#[cfg(test)]
mod tests {
	use super::*;

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

	#[test]
	fn linear_axis_follows_the_sample_rate() {
		let g = Graph::new(Look {
			log_axis: false,
			sample_ms: 250,
			..Look::default()
		});
		assert_eq!(g.axis(100), Axis::linear(25.0));
	}

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

	fn scratch(name: &str) -> std::path::PathBuf {
		std::env::temp_dir().join(format!(
			"test_tukzmon_graph_{}_{name}.hist",
			std::process::id()
		))
	}

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
