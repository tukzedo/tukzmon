// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! Zswap, or zram on a box without zswap. See design.md, Data sources on
//! Linux.

use crate::collect::{Collector, Kept, slurp};
use crate::mem::{Meminfo, meminfo, parse_meminfo};

/// How full the pool is, 0 to 1, and how far it squeezes what it holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Squeeze {
	pub used: f32,
	/// None while the pool is empty.
	pub ratio: Option<f32>,
}

fn share(x: u64, of: u64) -> f32 {
	(x as f64 / of as f64).clamp(0.0, 1.0) as f32
}

fn ratio(stored: u64, packed: u64) -> Option<f32> {
	(packed > 0).then(|| (stored as f64 / packed as f64) as f32)
}

/// Used is the pool against the cap zswap stops taking pages at,
/// `max_pool_percent` of all memory. None on a kernel without the rows.
pub fn zswap_squeeze(m: &Meminfo, max_pool_percent: u64) -> Option<Squeeze> {
	let (pool, stored) = (m.zswap?, m.zswapped?);
	let cap = m.total * max_pool_percent / 100;
	(cap > 0).then(|| Squeeze {
		used: share(pool, cap),
		ratio: ratio(stored, pool),
	})
}

/// `orig_data_size` and `compr_data_size`, the first 2 columns of a zram
/// `mm_stat`, in bytes.
pub fn parse_mm_stat(text: &str) -> Option<(u64, u64)> {
	let mut cols = text.split_ascii_whitespace().map(str::parse);
	Some((cols.next()?.ok()?, cols.next()?.ok()?))
}

/// Used is what the devices hold, before compression, against their size.
/// None while none of them is set up.
pub fn zram_squeeze(stored: u64, packed: u64, disksize: u64) -> Option<Squeeze> {
	(disksize > 0).then(|| Squeeze {
		used: share(stored, disksize),
		ratio: ratio(stored, packed),
	})
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Roots {
	/// None for the `/proc/meminfo` the other graphs share.
	meminfo: Option<String>,
	zswap: String,
	block: String,
}

impl Default for Roots {
	fn default() -> Self {
		Roots {
			meminfo: None,
			zswap: "/sys/module/zswap/parameters".into(),
			block: "/sys/block".into(),
		}
	}
}

#[derive(Debug)]
enum Pool {
	Zswap {
		max_pool_percent: Kept,
	},
	/// Each device's `mm_stat` and `disksize`.
	Zram(Vec<[Kept; 2]>),
}

/// Zswap when it's on, else every zram device there was at start.
#[derive(Debug)]
pub struct ZswapReader {
	/// As [`Roots::meminfo`].
	meminfo: Option<Kept>,
	pool: Pool,
	buf: String,
	ratio: Option<f32>,
}

// a default one has no devices, so it reads as a gap
impl Default for ZswapReader {
	fn default() -> Self {
		ZswapReader {
			meminfo: None,
			pool: Pool::Zram(Vec::new()),
			buf: String::new(),
			ratio: None,
		}
	}
}

#[cfg(test)]
impl ZswapReader {
	pub(crate) fn squeezed(ratio: f32) -> Self {
		ZswapReader {
			ratio: Some(ratio),
			..ZswapReader::default()
		}
	}

	/// Zswap on, reading the shared `/proc/meminfo`.
	pub(crate) fn zswap_on(max_pool_percent: &str) -> Self {
		ZswapReader {
			pool: Pool::Zswap {
				max_pool_percent: Kept::new(max_pool_percent),
			},
			..ZswapReader::default()
		}
	}
}

impl ZswapReader {
	/// "Zswap" or "Zram", for the tip.
	pub fn name(&self) -> &'static str {
		match self.pool {
			Pool::Zswap { .. } => "Zswap",
			Pool::Zram(_) => "Zram",
		}
	}

	/// The compression ratio at the last read, for the tip.
	pub fn ratio(&self) -> Option<f32> {
		self.ratio
	}

	fn probe_at(roots: &Roots) -> Option<Self> {
		let mut r = ZswapReader {
			meminfo: roots.meminfo.as_deref().map(Kept::new),
			pool: Pool::Zswap {
				max_pool_percent: Kept::new(format!("{}/max_pool_percent", roots.zswap)),
			},
			buf: String::new(),
			ratio: None,
		};
		let zswap_on =
			slurp(&format!("{}/enabled", roots.zswap), &mut r.buf).is_some_and(|s| s.trim() == "Y");
		if zswap_on && r.read().is_some() {
			return Some(r);
		}
		r.pool = Pool::Zram(zram_devices(&roots.block));
		r.read()?;
		Some(r)
	}

	fn squeeze(&mut self) -> Option<Squeeze> {
		let buf = &mut self.buf;
		match &mut self.pool {
			Pool::Zswap { max_pool_percent } => {
				let m = match &mut self.meminfo {
					Some(file) => file.read(buf).and_then(parse_meminfo),
					None => meminfo(),
				}?;
				let pct = max_pool_percent.read(buf)?.trim().parse().ok()?;
				zswap_squeeze(&m, pct)
			}
			Pool::Zram(devices) => {
				let (mut stored, mut packed, mut size) = (0, 0, 0);
				// a device reset or removed since start just drops out
				for [mm_stat, disksize] in devices {
					let Some((s, p)) = mm_stat.read(buf).and_then(parse_mm_stat) else {
						continue;
					};
					let Some(d) = disksize
						.read(buf)
						.and_then(|t| t.trim().parse::<u64>().ok())
					else {
						continue;
					};
					stored += s;
					packed += p;
					size += d;
				}
				zram_squeeze(stored, packed, size)
			}
		}
	}
}

fn zram_devices(block: &str) -> Vec<[Kept; 2]> {
	let Ok(dir) = std::fs::read_dir(block) else {
		return Vec::new();
	};
	let mut names: Vec<String> = dir
		.filter_map(|e| e.ok()?.file_name().into_string().ok())
		.filter(|n| n.starts_with("zram"))
		.collect();
	names.sort();
	names
		.iter()
		.map(|n| {
			[
				Kept::new(format!("{block}/{n}/mm_stat")),
				Kept::new(format!("{block}/{n}/disksize")),
			]
		})
		.collect()
}

impl Collector<1> for ZswapReader {
	const SERIES: [&'static str; 1] = ["used"];

	/// None with neither, or with zram devices that aren't set up.
	fn probe() -> Option<Self> {
		Self::probe_at(&Roots::default())
	}

	fn read(&mut self) -> Option<[f32; 1]> {
		let got = self.squeeze();
		self.ratio = got.and_then(|s| s.ratio);
		got.map(|s| [s.used])
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::collect::Tree;

	fn roots(t: &Tree) -> Roots {
		Roots {
			meminfo: Some(t.at("meminfo")),
			zswap: t.at("zswap"),
			block: t.at("block"),
		}
	}

	const MEMINFO_ZSWAP: &str =
		"MemTotal: 1000000 kB\nMemFree: 1 kB\nZswap: 50000 kB\nZswapped: 210000 kB\n";
	const MEMINFO_OLD: &str = "MemTotal: 1000000 kB\nMemFree: 1 kB\n";

	// Test ID: defmz
	#[test]
	fn zswap_is_the_pool_against_its_cap() {
		let m = parse_meminfo(MEMINFO_ZSWAP).unwrap();
		assert_eq!((m.zswap, m.zswapped), (Some(50_000), Some(210_000)));
		assert_eq!(
			zswap_squeeze(&m, 20),
			Some(Squeeze {
				used: 0.25,
				ratio: Some(4.2)
			})
		);
		// a pool over its cap, as when the cap is cut, stays in range
		assert_eq!(zswap_squeeze(&m, 1).map(|s| s.used), Some(1.0));
		assert_eq!(zswap_squeeze(&m, 0), None);
		let empty = parse_meminfo("MemTotal: 100 kB\nMemFree: 1 kB\nZswap: 0 kB\nZswapped: 0 kB\n");
		assert_eq!(
			zswap_squeeze(&empty.unwrap(), 20),
			Some(Squeeze {
				used: 0.0,
				ratio: None
			})
		);
		assert_eq!(
			zswap_squeeze(&parse_meminfo(MEMINFO_OLD).unwrap(), 20),
			None
		);
	}

	// Test ID: defn0
	#[test]
	fn zram_reads_mm_stat() {
		// 6.12's columns, in bytes
		let text =
			"  4194304  1048576  1310720        0  1310720        0        0        0        0\n";
		assert_eq!(parse_mm_stat(text), Some((4_194_304, 1_048_576)));
		assert_eq!(parse_mm_stat("4194304\n"), None);
		assert_eq!(parse_mm_stat("lots 1\n"), None);
		assert_eq!(parse_mm_stat(""), None);
		assert_eq!(
			zram_squeeze(4, 1, 16),
			Some(Squeeze {
				used: 0.25,
				ratio: Some(4.0)
			})
		);
		assert_eq!(zram_squeeze(0, 0, 16).map(|s| s.ratio), Some(None));
		assert_eq!(zram_squeeze(4, 1, 0), None);
	}

	// Test ID: defn1
	#[test]
	fn zswap_wins_and_zram_sums_its_devices() {
		let on = Tree::new(
			"zswap_on",
			&[
				("meminfo", MEMINFO_ZSWAP),
				("zswap/enabled", "Y\n"),
				("zswap/max_pool_percent", "20\n"),
				("block/zram0/mm_stat", "300 100 0 0 0 0 0 0 0\n"),
				("block/zram0/disksize", "1200\n"),
			],
		);
		let mut r = ZswapReader::probe_at(&roots(&on)).unwrap();
		assert_eq!(
			(r.name(), r.read(), r.ratio()),
			("Zswap", Some([0.25]), Some(4.2))
		);

		// off, or on with no rows to read, falls to zram
		for (name, enabled, meminfo) in [("off", "N\n", MEMINFO_ZSWAP), ("old", "Y\n", MEMINFO_OLD)]
		{
			let t = Tree::new(
				&format!("zswap_{name}"),
				&[
					("meminfo", meminfo),
					("zswap/enabled", enabled),
					("zswap/max_pool_percent", "20\n"),
					("block/zram0/mm_stat", "300 100 0 0 0 0 0 0 0\n"),
					("block/zram0/disksize", "1200\n"),
					("block/zram1/mm_stat", "0 0 0 0 0 0 0 0 0\n"),
					("block/zram1/disksize", "0\n"),
					("block/zram2/mm_stat", "300 50 0 0 0 0 0 0 0\n"),
					("block/zram2/disksize", "1200\n"),
					("block/sda/", ""),
				],
			);
			let mut r = ZswapReader::probe_at(&roots(&t)).unwrap();
			assert_eq!(
				(r.name(), r.read(), r.ratio()),
				("Zram", Some([0.25]), Some(4.0)),
				"{name}"
			);
		}
	}

	// Test ID: defn2
	#[test]
	fn neither_gives_no_collector() {
		let none = Tree::new(
			"zswap_none",
			&[
				("meminfo", MEMINFO_ZSWAP),
				("zswap/enabled", "N\n"),
				("block/sda/", ""),
			],
		);
		assert!(ZswapReader::probe_at(&roots(&none)).is_none());
		// zram loaded but never given a size
		let unset = Tree::new(
			"zswap_unset",
			&[
				("meminfo", MEMINFO_OLD),
				("block/zram0/mm_stat", "0 0 0 0 0 0 0 0 0\n"),
				("block/zram0/disksize", "0\n"),
			],
		);
		assert!(ZswapReader::probe_at(&roots(&unset)).is_none());
		assert!(
			ZswapReader::probe_at(&Roots {
				meminfo: Some("/proc/no/such".into()),
				zswap: "/proc/no/such".into(),
				block: "/proc/no/such".into(),
			})
			.is_none()
		);
	}

	// Test ID: defn3
	#[test]
	fn live_zswap_or_zram_reads() {
		let Some(mut r) = ZswapReader::probe() else {
			// the box may have neither
			return;
		};
		assert!(
			r.read().is_some_and(|[x]| (0.0..=1.0).contains(&x)),
			"{}",
			r.name()
		);
		assert!(r.ratio().is_none_or(|x| x > 0.0));
	}

	// Out since 2026-10-28: the ratio moved from text over the graph to the
	// tip, per backlog 2026102814061810, so there's no overlay to check. dejab
	// has the rest.
	// // Test ID: dehy3
	// #[test]
	// fn the_graph_gives_the_ratio_to_write_over_the_pool() {
	// 	use crate::Source;
	// 	use crate::config::Look;
	// 	use crate::render::Overlay;
	// 	let on = Tree::new(
	// 		"zswap_graph",
	// 		&[
	// 			("meminfo", MEMINFO_ZSWAP),
	// 			("zswap/enabled", "Y\n"),
	// 			("zswap/max_pool_percent", "20\n"),
	// 		],
	// 	);
	// 	assert!(!Look::default().show_zswap);
	// 	let look = Look {
	// 		show_zswap: true,
	// 		border: 1,
	// 		..Look::default()
	// 	};
	// 	let src = ZswapReader::probe_at(&roots(&on)).unwrap();
	// 	let mut g = crate::Graph::with(look.clone(), src);
	// 	let now = g.now();
	// 	g.hist.add(now - 60.0, now, &[0.25]);
	// 	assert_eq!(
	// 		g.overlay(64, 20, 1),
	// 		Some(Overlay {
	// 			text: "4.2x".into(),
	// 			color: look.zswap_ratio,
	// 			edge: look.bottom,
	// 			x: 1,
	// 			y: 1,
	// 			w: 62,
	// 			h: 18,
	// 		})
	// 	);
	// 	let o = g.overlay(128, 40, 2).unwrap();
	// 	assert_eq!((o.x, o.y, o.w, o.h), (2, 2, 124, 36));
	// 	assert_eq!(g.overlay(2, 20, 1), None);
	// 	let px = g.render(64, 20, 1).to_vec();
	// 	assert_eq!(px[18 * 64 + 60], look.zswap[0].argb());
	// 	let tip = g.hover_tip(60, 64, 20, 1);
	// 	assert!(tip.ends_with("\nZswap 25% used"), "{tip}");
	// 	let fire = Look::from_model(&crate::config::model("theme:\n\tname: fire\n"));
	// 	assert_eq!(
	// 		fire.zswap_paint(),
	// 		crate::render::Paint::Gradient(fire.zswap_gradient)
	// 	);
	// 	// an empty pool has no ratio to write
	// 	let empty = crate::Graph::with(look, ZswapReader::default());
	// 	assert_eq!(empty.tooltip(), "Zswap");
	// 	assert_eq!(empty.src.text(&[0.5]), "Zram 50% used");
	// 	assert_eq!(empty.overlay(64, 20, 1), None);
	// }

	// Test ID: dejab
	#[test]
	fn the_tip_gives_the_latest_ratio() {
		use crate::Source;
		use crate::config::Look;
		let on = Tree::new(
			"zswap_tip",
			&[
				("meminfo", MEMINFO_ZSWAP),
				("zswap/enabled", "Y\n"),
				("zswap/max_pool_percent", "20\n"),
			],
		);
		let look = Look {
			show_zswap: true,
			border: 1,
			zswap_mode: crate::config::Mode::Detailed,
			..Look::default()
		};
		let src = ZswapReader::probe_at(&roots(&on)).unwrap();
		let mut g = crate::Graph::with(look.clone(), src);
		let now = g.now();
		g.hist.add(now - 60.0, now, &[0.25]);
		let px = g.render(64, 20, 1).to_vec();
		assert_eq!(px[18 * 64 + 60], look.zswap[0].argb());
		let tip = g.hover_tip(60, 64, 20, 1);
		assert!(
			tip.ends_with("\nZswap 25% used\nCompressed 4.2x now"),
			"{tip}"
		);
		g.sample();
		g.sample();
		assert_eq!(g.tooltip(), "Zswap 25% used\nCompressed 4.2x now");
		let fire = Look::from_model(&crate::config::model("theme:\n\tname: fire\n"));
		assert_eq!(
			fire.zswap_paint(),
			crate::render::Paint::Gradient(fire.zswap_gradient)
		);
		// an empty pool has no ratio to give
		let empty = crate::Graph::with(look, ZswapReader::default());
		assert_eq!(empty.tooltip(), "Zswap");
		assert_eq!(empty.src.text(&[0.5]), "Zram 50% used");
	}
}
