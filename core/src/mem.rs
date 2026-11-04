// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! Memory and swap from `/proc/meminfo`, and ZFS's ARC for the memory graph.

use std::cell::RefCell;

use crate::collect::{Collector, Kept};

const MEMINFO: &str = "/proc/meminfo";
const ARCSTATS: &str = "/proc/spl/kstat/zfs/arcstats";
// memory graph samples per read of ARCSTATS
const ARC_EVERY: u32 = 10;

/// The `/proc/meminfo` rows used here, in kB.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Meminfo {
	/// `MemTotal`.
	pub total: u64,
	/// `MemFree`.
	pub free: u64,
	/// `Buffers`.
	pub buffers: u64,
	/// `Cached`.
	pub cached: u64,
	/// `SReclaimable`, counted as cache.
	pub sreclaimable: u64,
	/// `SwapTotal`.
	pub swap_total: u64,
	/// `SwapFree`.
	pub swap_free: u64,
	/// zswap's pool, compressed. None on a kernel without the row.
	pub zswap: Option<u64>,
	/// What the pool stores, before compression.
	pub zswapped: Option<u64>,
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
			"SReclaimable" => &mut m.sreclaimable,
			"SwapTotal" => &mut m.swap_total,
			"SwapFree" => &mut m.swap_free,
			"Zswap" => m.zswap.insert(0),
			"Zswapped" => m.zswapped.insert(0),
			_ => continue,
		};
		*slot = rest.split_ascii_whitespace().next()?.parse().ok()?;
	}
	(got_total && got_free).then_some(m)
}

#[derive(Clone, Copy, Debug)]
enum Tick {
	Off,
	Due,
	Read(Option<Meminfo>),
}

/// `/proc/meminfo` for every graph that reads it: memory, swap and zswap.
struct Shared {
	file: Kept,
	buf: String,
	tick: Tick,
	#[cfg(test)]
	reads: u32,
}

thread_local! {
	static SHARED: RefCell<Shared> = RefCell::new(Shared {
		file: Kept::new(MEMINFO),
		buf: String::new(),
		tick: Tick::Off,
		#[cfg(test)]
		reads: 0,
	});
}

/// What `/proc/meminfo` reads now, or read earlier in this
/// [`once_a_tick`].
pub(crate) fn meminfo() -> Option<Meminfo> {
	SHARED.with_borrow_mut(|s| {
		if let Tick::Read(m) = s.tick {
			return m;
		}
		#[cfg(test)]
		{
			s.reads += 1;
		}
		let m = s.file.read(&mut s.buf).and_then(parse_meminfo);
		if let Tick::Due = s.tick {
			s.tick = Tick::Read(m);
		}
		m
	})
}

#[cfg(test)]
pub(crate) fn meminfo_reads() -> u32 {
	SHARED.with_borrow(|s| s.reads)
}

/// Runs `f` with `/proc/meminfo` read at most once, however many graphs
/// read it.
pub(crate) fn once_a_tick<R>(f: impl FnOnce() -> R) -> R {
	struct Over;
	impl Drop for Over {
		fn drop(&mut self) {
			SHARED.with_borrow_mut(|s| s.tick = Tick::Off);
		}
	}
	if SHARED.with_borrow(|s| !matches!(s.tick, Tick::Off)) {
		return f();
	}
	SHARED.with_borrow_mut(|s| s.tick = Tick::Due);
	let _over = Over;
	f()
}

/// The part of ZFS's ARC that would give way to programs, in kB: its `size`
/// over `c_min`, as htop counts it. None without both rows.
pub fn parse_arcstats(text: &str) -> Option<u64> {
	let (mut size, mut min) = (None, None);
	for line in text.lines() {
		let mut cols = line.split_ascii_whitespace();
		let slot = match cols.next() {
			Some("size") => &mut size,
			Some("c_min") => &mut min,
			_ => continue,
		};
		// name, type, data
		*slot = Some(cols.nth(1)?.parse::<u64>().ok()?);
	}
	Some(size?.saturating_sub(min?) / 1024)
}

/// User, buffers and cache, as shares of all memory. User is what's left
/// once free, buffers and cache are taken out. Cache takes in reclaimable
/// slab, as `free` counts it (backlog 2026102508354893), and `arc` kB of
/// ZFS's ARC, which `Cached` leaves out.
pub fn memory_shares(m: &Meminfo, arc: u64) -> Option<[f32; 3]> {
	if m.total == 0 {
		return None;
	}
	let used = m
		.total
		.saturating_sub(m.free)
		.saturating_sub(m.buffers + m.cached + m.sreclaimable);
	// the two files aren't read at the same instant
	let arc = arc.min(used);
	let (user, cache) = (used - arc, m.cached + m.sreclaimable + arc);
	let t = m.total as f64;
	let f = |x: u64| (x as f64 / t).clamp(0.0, 1.0) as f32;
	Some([f(user), f(m.buffers), f(cache)])
}

/// Share of swap in use. None with no swap.
pub fn swap_share(m: &Meminfo) -> Option<[f32; 1]> {
	if m.swap_total == 0 {
		return None;
	}
	let used = m.swap_total.saturating_sub(m.swap_free);
	Some([(used as f64 / m.swap_total as f64).clamp(0.0, 1.0) as f32])
}

/// Reads `/proc/meminfo` for the memory graph, and ZFS's ARC where it's
/// loaded.
#[derive(Debug)]
pub struct MemoryReader {
	arc: Kept,
	buf: String,
	arc_was: u64,
	reads: u32,
}

impl Default for MemoryReader {
	fn default() -> Self {
		MemoryReader::with_arc(ARCSTATS)
	}
}

impl MemoryReader {
	fn with_arc(path: &str) -> Self {
		MemoryReader {
			arc: Kept::new(path),
			buf: String::new(),
			arc_was: 0,
			reads: 0,
		}
	}

	// arcstats costs about 8 meminfo reads, and the ARC moves slowly. Still
	// tried, so a pool imported after start counts.
	fn arc_kb(&mut self) -> u64 {
		if self.reads.is_multiple_of(ARC_EVERY) {
			self.arc_was = self
				.arc
				.read(&mut self.buf)
				.and_then(parse_arcstats)
				.unwrap_or(0);
		}
		self.reads = (self.reads + 1) % ARC_EVERY;
		self.arc_was
	}
}

impl Collector<3> for MemoryReader {
	const SERIES: [&'static str; 3] = ["user", "buffers", "cache"];

	fn probe() -> Option<Self> {
		let mut r = MemoryReader::default();
		r.read()?;
		Some(r)
	}

	fn read(&mut self) -> Option<[f32; 3]> {
		let m = meminfo()?;
		memory_shares(&m, self.arc_kb())
	}
}

/// A box with no swap at start gets none, and swap turned off later reads as
/// a gap.
#[derive(Debug, Default)]
pub struct SwapReader;

impl Collector<1> for SwapReader {
	const SERIES: [&'static str; 1] = ["used"];

	fn probe() -> Option<Self> {
		let mut r = SwapReader;
		r.read()?;
		Some(r)
	}

	fn read(&mut self) -> Option<[f32; 1]> {
		swap_share(&meminfo()?)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::collect::Tree;

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

	// Test ID: decax
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
				sreclaimable: 0,
				swap_total: 8_000_000,
				swap_free: 6_000_000,
				zswap: Some(0),
				zswapped: None,
			}
		);
	}

	// Test ID: decay
	#[test]
	fn shares() {
		let m = parse_meminfo(MEMINFO_TEXT).unwrap();
		assert_eq!(memory_shares(&m, 0), Some([0.5, 0.0625, 0.1875]));
		assert_eq!(swap_share(&m), Some([0.25]));
	}

	// Test ID: decaz
	#[test]
	fn missing_rows() {
		assert!(parse_meminfo("MemTotal: 100 kB\n").is_none());
		assert!(parse_meminfo("").is_none());
		let m = parse_meminfo("MemFree: 25 kB\nMemTotal: 100 kB\n").unwrap();
		assert_eq!(memory_shares(&m, 0), Some([0.75, 0.0, 0.0]));
		assert_eq!(swap_share(&m), None);
	}

	// Test ID: decb0
	#[test]
	fn junk_in_a_used_row_is_none() {
		assert!(parse_meminfo("MemTotal: 100 kB\nMemFree: lots kB\n").is_none());
		assert!(parse_meminfo("MemTotal: 100 kB\nMemFree:\n").is_none());
		// rows not read here can hold anything
		assert!(parse_meminfo("MemTotal: 100 kB\nMemFree: 1 kB\nOdd: x\nno colon\n").is_some());
	}

	// Test ID: decb1
	#[test]
	fn odd_counts_stay_in_range() {
		let m = Meminfo {
			total: 100,
			free: 90,
			buffers: 20,
			cached: 20,
			swap_total: 10,
			swap_free: 20,
			..Meminfo::default()
		};
		assert_eq!(memory_shares(&m, 0), Some([0.0, 0.2, 0.2]));
		assert_eq!(swap_share(&m), Some([0.0]));
		assert_eq!(memory_shares(&Meminfo::default(), 0), None);
	}

	// Test ID: defmy
	#[test]
	fn cache_takes_in_reclaimable_slab_and_user_keeps_the_rest() {
		let text = "MemTotal: 1000 kB\nMemFree: 100 kB\nBuffers: 50 kB\nCached: 200 kB\n\
			Slab: 300 kB\nSReclaimable: 250 kB\nSUnreclaim: 50 kB\n";
		let m = parse_meminfo(text).unwrap();
		assert_eq!(m.sreclaimable, 250);
		// user 400 has the 50 of slab that can't be reclaimed
		assert_eq!(memory_shares(&m, 0), Some([0.4, 0.05, 0.45]));
	}

	const ARCSTATS_TEXT: &str = "\
24 1 0x01 148 40256 21980950453 133770195738518
name                            type data
hits                            4    513978511
c                               4    8589934592
c_min                           4    4294967296
c_max                           4    8589934592
size                            4    8053063680
";

	// Test ID: der6g
	#[test]
	fn arc_over_its_floor_counts_as_cache_not_programs() {
		assert_eq!(parse_arcstats(ARCSTATS_TEXT), Some(3_670_016));
		assert_eq!(parse_arcstats("size 4 100\n"), None);
		assert_eq!(parse_arcstats("c_min 4 900\nsize 4 100\n"), Some(0));
		assert_eq!(parse_arcstats("c_min 4 1\nsize 4 lots\n"), None);
		let m = parse_meminfo(MEMINFO_TEXT).unwrap();
		// programs 8 GB, of which 2 GB is ARC
		assert_eq!(memory_shares(&m, 2_000_000), Some([0.375, 0.0625, 0.3125]));
		// never more than programs had, as the files are read apart
		assert_eq!(memory_shares(&m, 9_000_000), Some([0.0, 0.0625, 0.6875]));
	}

	// Test ID: der6h
	#[test]
	fn the_memory_reader_reads_the_arc_every_tenth_time_and_none_is_none() {
		let t = Tree::new("arcstats", &[("arcstats", ARCSTATS_TEXT)]);
		let mut r = MemoryReader::with_arc(&t.at("arcstats"));
		assert_eq!(r.arc_kb(), 3_670_016);
		std::fs::write(
			t.at("arcstats"),
			ARCSTATS_TEXT.replace("8053063680", "5368709120"),
		)
		.unwrap();
		for _ in 1..ARC_EVERY {
			assert_eq!(r.arc_kb(), 3_670_016);
		}
		assert_eq!(r.arc_kb(), 1_048_576);
		assert_eq!(r.arc_kb(), 1_048_576);
		let mut none = MemoryReader::with_arc(&t.at("no_zfs"));
		assert_eq!(none.arc_kb(), 0);
		assert!(none.read().is_some());
		// the real file, where ZFS is loaded, has both rows
		if let Ok(text) = std::fs::read_to_string(ARCSTATS) {
			assert!(parse_arcstats(&text).is_some(), "{text}");
		}
	}

	// Test ID: decb2
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
