// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! CPU use from `/proc/stat`.

use crate::collect::{Collector, Kept};

/// Series names, bottom of the stack first.
pub const SERIES: [&str; 4] = ["iowait", "system", "user", "nice"];

/// The jiffy counters off the `cpu` line. Guest time is already in user.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ticks {
	/// Time in user mode, guest included.
	pub user: u64,
	/// Time in user mode at low priority.
	pub nice: u64,
	/// Time in the kernel.
	pub system: u64,
	/// Time idle.
	pub idle: u64,
	/// Time idle waiting on I/O.
	pub iowait: u64,
	/// Time serving interrupts.
	pub irq: u64,
	/// Time serving soft interrupts.
	pub softirq: u64,
	/// Time a hypervisor gave to another guest.
	pub steal: u64,
}

impl Ticks {
	/// None past `u64::MAX`, which only a lying file reaches.
	fn total(&self) -> Option<u64> {
		[
			self.nice,
			self.system,
			self.idle,
			self.iowait,
			self.irq,
			self.softirq,
			self.steal,
		]
		.into_iter()
		.try_fold(self.user, u64::checked_add)
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
	let rise = Ticks {
		user: d(was.user, now.user)?,
		nice: d(was.nice, now.nice)?,
		system: d(was.system, now.system)?,
		idle: d(was.idle, now.idle)?,
		iowait: d(was.iowait, now.iowait)?,
		irq: d(was.irq, now.irq)?,
		softirq: d(was.softirq, now.softirq)?,
		steal: d(was.steal, now.steal)?,
	};
	let total = rise.total()?;
	if total == 0 {
		return None;
	}
	// each part is within total
	let system = rise.system + rise.irq + rise.softirq;
	let t = total as f64;
	let f = |x: u64| (x as f64 / t).clamp(0.0, 1.0) as f32;
	Some([f(rise.iowait), f(system), f(rise.user), f(rise.nice)])
}

/// Reads `/proc/stat` for the CPU graph.
#[derive(Debug)]
pub struct CpuReader {
	was: Option<Ticks>,
	stat: Kept,
	buf: String,
}

impl Default for CpuReader {
	fn default() -> Self {
		CpuReader {
			was: None,
			stat: Kept::new("/proc/stat"),
			buf: String::new(),
		}
	}
}

impl Collector<4> for CpuReader {
	const SERIES: [&'static str; 4] = SERIES;

	fn probe() -> Option<Self> {
		let mut r = CpuReader::default();
		r.read();
		r.was.is_some().then_some(r)
	}

	/// Shares since the last call. The first call only primes it.
	fn read(&mut self) -> Option<[f32; 4]> {
		let now = parse_stat(self.stat.read(&mut self.buf)?)?;
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

	// Test ID: debhp
	#[test]
	fn all_cpu_line() {
		let t = parse_stat(STAT).unwrap();
		assert_eq!(t.user, 100);
		assert_eq!(t.iowait, 40);
		assert_eq!(t.softirq, 5);
		assert_eq!(t.total(), Some(1000));
	}

	// Test ID: debhq
	#[test]
	fn old_kernel_short_line() {
		let t = parse_stat("cpu  1 2 3 4\n").unwrap();
		assert_eq!(t.idle, 4);
		assert_eq!(t.steal, 0);
	}

	// Test ID: debhr
	#[test]
	fn junk_gives_none() {
		assert!(parse_stat("cpu  1 x 3 4\n").is_none());
		assert!(parse_stat("intr 1\n").is_none());
		assert!(parse_stat("").is_none());
	}

	// Test ID: debhs
	#[test]
	fn cpu_shares_sum() {
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

	// Test ID: debht
	#[test]
	fn backwards_time_gives_none() {
		let a = parse_stat(STAT).unwrap();
		assert!(shares(&a, &a).is_none());
		let mut b = a;
		b.idle += 10;
		b.user -= 1;
		assert!(shares(&a, &b).is_none());
	}

	// Test ID: debhv
	#[test]
	fn live_proc_stat_reads() {
		let mut r = CpuReader::default();
		assert!(r.read().is_none());
		assert!(CpuReader::probe().is_some_and(|p| p.was.is_some()));
		std::thread::sleep(std::time::Duration::from_millis(30));
		// a busy box can tick nothing in 30 ms, so only check the sum when it did
		if let Some(s) = r.read() {
			assert!(s.iter().sum::<f32>() <= 1.0001);
		}
	}

	fn stat_text(v: &[u64]) -> String {
		let cols = |v: &[u64]| v.iter().map(u64::to_string).collect::<Vec<_>>().join(" ");
		format!(
			"cpu  {}\ncpu0 {}\nintr 9 0 3\nctxt 5\nbtime 1700000000\nprocesses 7\nprocs_running 1\n",
			cols(v),
			cols(&v[..v.len() / 2])
		)
	}

	// Test ID: df4gj
	#[test]
	fn fuzz_cpu_stat_range() {
		let tally = crate::fuzz::run("cpu stat", |dice| {
			// 4 columns before 2.6.11, 10 now
			let cols = 4 + dice.below(7);
			let was: Vec<u64> = (0..cols).map(|_| crate::fuzz::count(dice)).collect();
			let now: Vec<u64> = was
				.iter()
				.map(|v| v.wrapping_add(dice.below(1000) as u64))
				.collect();
			let mut ticks = [None, None];
			let mut good = true;
			for (slot, v) in ticks.iter_mut().zip([&was, &now]) {
				let (text, whole) = crate::fuzz::maybe_hurt(dice, stat_text(v));
				*slot = parse_stat(&text);
				if whole {
					let at = |i: usize| v.get(i).copied().unwrap_or(0);
					let want = Ticks {
						user: at(0),
						nice: at(1),
						system: at(2),
						idle: at(3),
						iowait: at(4),
						irq: at(5),
						softirq: at(6),
						steal: at(7),
					};
					assert_eq!(*slot, Some(want), "{text:?}");
				}
				good &= slot.is_some();
			}
			if let [Some(a), Some(b)] = ticks
				&& let Some(s) = shares(&a, &b)
			{
				assert!(s.iter().all(|x| (0.0..=1.0).contains(x)), "{s:?}");
				assert!(s.iter().sum::<f32>() <= 1.0001, "{s:?}");
				let fields = |t: &Ticks| {
					[
						t.user, t.nice, t.system, t.idle, t.iowait, t.irq, t.softirq, t.steal,
					]
				};
				let rose = fields(&a).iter().zip(fields(&b)).all(|(x, y)| *x <= y);
				assert!(rose, "a counter went down: {a:?} {b:?}");
			}
			good
		});
		tally.assert_mixed();
	}
}
