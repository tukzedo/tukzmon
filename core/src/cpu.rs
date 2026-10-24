// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! CPU use from `/proc/stat`.

/// Series names, bottom of the stack first.
pub const SERIES: [&str; 4] = ["iowait", "system", "user", "nice"];

/// The jiffy counters off the `cpu` line. Guest time is already in user.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ticks {
	pub user: u64,
	pub nice: u64,
	pub system: u64,
	pub idle: u64,
	pub iowait: u64,
	pub irq: u64,
	pub softirq: u64,
	pub steal: u64,
}

impl Ticks {
	fn total(&self) -> u64 {
		self.user
			+ self.nice
			+ self.system
			+ self.idle
			+ self.iowait
			+ self.irq
			+ self.softirq
			+ self.steal
	}
}

/// The all-CPU line of `/proc/stat`. Kernels older than 2.6.11 have fewer
/// columns, and the missing ones read as 0.
pub fn parse_stat(text: &str) -> Option<Ticks> {
	let line = text.lines().find(|l| l.starts_with("cpu "))?;
	let mut n = line.split_ascii_whitespace().skip(1).map(str::parse::<u64>);
	let mut next = || n.next().unwrap_or(Ok(0)).ok();
	Some(Ticks {
		user: next()?,
		nice: next()?,
		system: next()?,
		idle: next()?,
		iowait: next()?,
		irq: next()?,
		softirq: next()?,
		steal: next()?,
	})
}

/// Shares of the time between 2 reads, in [`SERIES`] order. None when no time
/// passed or a counter went backwards, as after a CPU goes offline.
pub fn shares(was: &Ticks, now: &Ticks) -> Option<[f32; 4]> {
	let d = |a: u64, b: u64| b.checked_sub(a);
	let total = d(was.total(), now.total())?;
	if total == 0 {
		return None;
	}
	let system = d(was.system, now.system)? + d(was.irq, now.irq)? + d(was.softirq, now.softirq)?;
	let t = total as f64;
	let f = |x: u64| (x as f64 / t).clamp(0.0, 1.0) as f32;
	Some([
		f(d(was.iowait, now.iowait)?),
		f(system),
		f(d(was.user, now.user)?),
		f(d(was.nice, now.nice)?),
	])
}

#[derive(Default)]
pub struct CpuReader {
	was: Option<Ticks>,
}

impl CpuReader {
	/// Shares since the last call. The first call only primes it.
	pub fn read(&mut self) -> Option<[f32; 4]> {
		let now = parse_stat(&std::fs::read_to_string("/proc/stat").ok()?)?;
		let out = self.was.and_then(|w| shares(&w, &now));
		self.was = Some(now);
		out
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const STAT: &str =
		"cpu  100 20 30 800 40 5 5 0 0 0\ncpu0 50 10 15 400 20 2 3 0 0 0\nintr 1 2 3\n";

	#[test]
	fn reads_the_all_cpu_line() {
		let t = parse_stat(STAT).unwrap();
		assert_eq!(t.user, 100);
		assert_eq!(t.iowait, 40);
		assert_eq!(t.softirq, 5);
		assert_eq!(t.total(), 1000);
	}

	#[test]
	fn short_old_kernel_line() {
		let t = parse_stat("cpu  1 2 3 4\n").unwrap();
		assert_eq!(t.idle, 4);
		assert_eq!(t.steal, 0);
	}

	#[test]
	fn junk_is_none() {
		assert!(parse_stat("cpu  1 x 3 4\n").is_none());
		assert!(parse_stat("intr 1\n").is_none());
		assert!(parse_stat("").is_none());
	}

	#[test]
	fn shares_add_up() {
		let a = parse_stat(STAT).unwrap();
		let b = Ticks {
			user: a.user + 30,
			nice: a.nice + 10,
			system: a.system + 10,
			irq: a.irq + 5,
			softirq: a.softirq + 5,
			iowait: a.iowait + 20,
			idle: a.idle + 20,
			steal: 0,
		};
		let s = shares(&a, &b).unwrap();
		assert_eq!(s, [0.2, 0.2, 0.3, 0.1]);
	}

	#[test]
	fn no_time_or_backwards_is_none() {
		let a = parse_stat(STAT).unwrap();
		assert!(shares(&a, &a).is_none());
		let mut b = a;
		b.idle += 10;
		b.user -= 1;
		assert!(shares(&a, &b).is_none());
	}

	#[test]
	fn live_proc_stat_reads() {
		let mut r = CpuReader::default();
		assert!(r.read().is_none());
		std::thread::sleep(std::time::Duration::from_millis(30));
		// a busy box can tick nothing in 30 ms, so only check the sum when it did
		if let Some(s) = r.read() {
			assert!(s.iter().sum::<f32>() <= 1.0001);
		}
	}
}
