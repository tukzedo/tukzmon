// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! The part of tukzmon every panel shares. No GUI toolkit in here.

pub mod axis;
pub mod config;
pub mod cpu;
pub mod history;
pub mod render;

use std::time::Instant;

use axis::Axis;
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
			read_at: None,
			latest: None,
			px: Vec::new(),
		}
	}

	fn now(&self) -> f64 {
		self.clock.elapsed().as_secs_f64()
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
			Axis::log(total)
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
}
