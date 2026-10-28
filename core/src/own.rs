// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! tukzmon's own CPU use, as a share of every CPU's time. It's the CPU time
//! of the process tukzmon runs in, which is all tukzmon's on X11 and the
//! whole panel's on Wayland. See design.md, Data sources on Linux.

use std::time::Instant;

use crate::collect::{Collector, slurp};

/// The process the graph is measured in, from `/proc/self/comm`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Host {
	/// The panel's wrapper on X11, which runs only this plugin.
	#[default]
	Own,
	/// xfce4-panel itself, on Wayland, with every other plugin in it.
	Panel,
	/// tukzmon-settings, drawing the graph list.
	Settings,
}

impl Host {
	/// The kernel cuts a process name to 15 bytes.
	pub fn of(comm: &str) -> Host {
		match comm.trim_end() {
			"xfce4-panel" => Host::Panel,
			c if c.starts_with("tukzmon-setting") => Host::Settings,
			_ => Host::Own,
		}
	}

	pub fn name(self) -> &'static str {
		match self {
			Host::Own => "tukzmon",
			Host::Panel => "Panel",
			Host::Settings => "Settings window",
		}
	}
}

/// CPU time spent over `wall_secs` of wall time, as a share of `cpus` CPUs.
/// None when no time passed.
pub fn share(cpu_ns: u64, wall_secs: f64, cpus: f32) -> Option<f32> {
	let all = wall_secs * f64::from(cpus) * 1e9;
	(all > 0.0).then(|| (cpu_ns as f64 / all).clamp(0.0, 1.0) as f32)
}

// one syscall, with no file to read or parse
#[cfg(not(miri))]
fn cpu_ns() -> Option<u64> {
	let mut ts = libc::timespec {
		tv_sec: 0,
		tv_nsec: 0,
	};
	// SAFETY: ts is a valid timespec for the call to fill.
	let r = unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &raw mut ts) };
	let secs = u64::try_from(ts.tv_sec).ok()?;
	let ns = u64::try_from(ts.tv_nsec).ok()?;
	(r == 0).then(|| secs * 1_000_000_000 + ns)
}

// Miri has no CPU time clock
#[cfg(miri)]
fn cpu_ns() -> Option<u64> {
	None
}

#[derive(Debug, Default)]
pub struct OwnReader {
	pub(crate) host: Host,
	was: Option<(Instant, u64)>,
}

impl OwnReader {
	pub fn host(&self) -> Host {
		self.host
	}
}

impl Collector<1> for OwnReader {
	const SERIES: [&'static str; 1] = ["used"];

	fn probe() -> Option<Self> {
		let mut buf = String::new();
		let mut r = OwnReader {
			host: slurp("/proc/self/comm", &mut buf).map_or(Host::Own, Host::of),
			was: None,
		};
		r.read();
		r.was.is_some().then_some(r)
	}

	/// The share since the last call. The first call only primes it.
	fn read(&mut self) -> Option<[f32; 1]> {
		let now = (Instant::now(), cpu_ns()?);
		let out = self.was.and_then(|(at, ns)| {
			let wall = now.0.saturating_duration_since(at).as_secs_f64();
			share(now.1.checked_sub(ns)?, wall, crate::cpus())
		});
		self.was = Some(now);
		out.map(|x| [x])
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	// Test ID: dejb3
	#[test]
	fn cpu_time_is_a_share_of_every_cpu() {
		// 2 ms of CPU in half a second on 4 CPUs
		assert_eq!(share(2_000_000, 0.5, 4.0), Some(0.001));
		assert_eq!(share(0, 0.5, 4.0), Some(0.0));
		assert_eq!(share(4_000_000_000, 0.5, 4.0), Some(1.0));
		assert_eq!(share(5, 0.0, 4.0), None);
		assert_eq!(share(5, 1.0, 0.0), None);
	}

	// Test ID: dejb4
	#[test]
	fn the_host_is_named_from_the_process() {
		assert_eq!(Host::of("wrapper-2.0\n"), Host::Own);
		assert_eq!(Host::of("xfce4-panel\n"), Host::Panel);
		// cut to 15 bytes by the kernel
		assert_eq!(Host::of("tukzmon-setting\n"), Host::Settings);
		assert_eq!(Host::of("tukzmon-settings"), Host::Settings);
		assert_eq!(Host::of("tukzmon\n"), Host::Own);
		assert_eq!(Host::of(""), Host::Own);
	}

	// Test ID: dejb5
	#[test]
	#[cfg_attr(miri, ignore = "Miri has no CPU time clock")]
	fn live_cpu_time_reads_and_rises_with_work() {
		let mut r = OwnReader::probe().unwrap();
		let start = Instant::now();
		let mut spin = 0u64;
		while start.elapsed().as_millis() < 30 {
			spin = std::hint::black_box(spin.wrapping_add(1));
		}
		let [x] = r.read().unwrap();
		assert!(x > 0.0 && x <= 1.0, "{x}");
	}
}
