// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! Memory and swap from `/proc/meminfo`.

use crate::collect::{Collector, slurp};

const MEMINFO: &str = "/proc/meminfo";

/// The `/proc/meminfo` rows used here, in kB.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Meminfo {
	pub total: u64,
	pub free: u64,
	pub buffers: u64,
	pub cached: u64,
	pub swap_total: u64,
	pub swap_free: u64,
}

/// None without `MemTotal` and `MemFree`, or with junk in a row used here.
/// Any other row that's missing reads as 0.
pub fn parse_meminfo(text: &str) -> Option<Meminfo> {
	let mut m = Meminfo::default();
	let (mut got_total, mut got_free) = (false, false);
	for line in text.lines() {
		let Some((key, rest)) = line.split_once(':') else {
			continue;
		};
		let slot = match key {
			"MemTotal" => {
				got_total = true;
				&mut m.total
			}
			"MemFree" => {
				got_free = true;
				&mut m.free
			}
			"Buffers" => &mut m.buffers,
			"Cached" => &mut m.cached,
			"SwapTotal" => &mut m.swap_total,
			"SwapFree" => &mut m.swap_free,
			_ => continue,
		};
		*slot = rest.split_ascii_whitespace().next()?.parse().ok()?;
	}
	(got_total && got_free).then_some(m)
}

/// User, buffers and cache, as shares of all memory. User is what's left
/// once free, buffers and cache are taken out, the way multiload-ng counts it.
pub fn memory_shares(m: &Meminfo) -> Option<[f32; 3]> {
	if m.total == 0 {
		return None;
	}
	let user = m
		.total
		.saturating_sub(m.free)
		.saturating_sub(m.buffers + m.cached);
	let t = m.total as f64;
	let f = |x: u64| (x as f64 / t).clamp(0.0, 1.0) as f32;
	Some([f(user), f(m.buffers), f(m.cached)])
}

/// Share of swap in use. None with no swap.
pub fn swap_share(m: &Meminfo) -> Option<[f32; 1]> {
	if m.swap_total == 0 {
		return None;
	}
	let used = m.swap_total.saturating_sub(m.swap_free);
	Some([(used as f64 / m.swap_total as f64).clamp(0.0, 1.0) as f32])
}

#[derive(Debug, Default)]
pub struct MemoryReader {
	buf: String,
}

impl Collector<3> for MemoryReader {
	const SERIES: [&'static str; 3] = ["user", "buffers", "cache"];

	fn probe() -> Option<Self> {
		let mut r = MemoryReader::default();
		r.read()?;
		Some(r)
	}

	fn read(&mut self) -> Option<[f32; 3]> {
		memory_shares(&parse_meminfo(slurp(MEMINFO, &mut self.buf)?)?)
	}
}

/// A box with no swap at start gets none, and swap turned off later reads as
/// a gap.
#[derive(Debug, Default)]
pub struct SwapReader {
	buf: String,
}

impl Collector<1> for SwapReader {
	const SERIES: [&'static str; 1] = ["used"];

	fn probe() -> Option<Self> {
		let mut r = SwapReader::default();
		r.read()?;
		Some(r)
	}

	fn read(&mut self) -> Option<[f32; 1]> {
		swap_share(&parse_meminfo(slurp(MEMINFO, &mut self.buf)?)?)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	// trimmed from a 6.12 kernel, with the column spacing it uses
	const MEMINFO_TEXT: &str = "\
MemTotal:       16000000 kB
MemFree:         4000000 kB
MemAvailable:    9000000 kB
Buffers:         1000000 kB
Cached:          3000000 kB
SwapCached:        10000 kB
Active(anon):    2000000 kB
SwapTotal:       8000000 kB
SwapFree:        6000000 kB
Zswap:                 0 kB
HugePages_Total:       0
Hugepagesize:       2048 kB
";

	#[test]
	fn reads_the_rows() {
		let m = parse_meminfo(MEMINFO_TEXT).unwrap();
		assert_eq!(
			m,
			Meminfo {
				total: 16_000_000,
				free: 4_000_000,
				buffers: 1_000_000,
				cached: 3_000_000,
				swap_total: 8_000_000,
				swap_free: 6_000_000,
			}
		);
	}

	#[test]
	fn shares() {
		let m = parse_meminfo(MEMINFO_TEXT).unwrap();
		assert_eq!(memory_shares(&m), Some([0.5, 0.0625, 0.1875]));
		assert_eq!(swap_share(&m), Some([0.25]));
	}

	#[test]
	fn missing_rows() {
		assert!(parse_meminfo("MemTotal: 100 kB\n").is_none());
		assert!(parse_meminfo("").is_none());
		let m = parse_meminfo("MemFree: 25 kB\nMemTotal: 100 kB\n").unwrap();
		assert_eq!(memory_shares(&m), Some([0.75, 0.0, 0.0]));
		assert_eq!(swap_share(&m), None);
	}

	#[test]
	fn junk_in_a_used_row_is_none() {
		assert!(parse_meminfo("MemTotal: 100 kB\nMemFree: lots kB\n").is_none());
		assert!(parse_meminfo("MemTotal: 100 kB\nMemFree:\n").is_none());
		// rows not read here can hold anything
		assert!(parse_meminfo("MemTotal: 100 kB\nMemFree: 1 kB\nOdd: x\nno colon\n").is_some());
	}

	#[test]
	fn odd_counts_stay_in_range() {
		let m = Meminfo {
			total: 100,
			free: 90,
			buffers: 20,
			cached: 20,
			swap_total: 10,
			swap_free: 20,
		};
		assert_eq!(memory_shares(&m), Some([0.0, 0.2, 0.2]));
		assert_eq!(swap_share(&m), Some([0.0]));
		assert_eq!(memory_shares(&Meminfo::default()), None);
	}

	#[test]
	fn live_proc_meminfo_reads() {
		let mut r = MemoryReader::probe().unwrap();
		let s = r.read().unwrap();
		assert!(s.iter().all(|x| (0.0..=1.0).contains(x)));
		assert!(s.iter().sum::<f32>() <= 1.0001);
		// the box may have no swap
		let swap_total = parse_meminfo(&std::fs::read_to_string(MEMINFO).unwrap())
			.unwrap()
			.swap_total;
		match SwapReader::probe() {
			Some(mut sw) => assert!(sw.read().is_some_and(|[x]| (0.0..=1.0).contains(&x))),
			None => assert_eq!(swap_total, 0),
		}
	}
}
