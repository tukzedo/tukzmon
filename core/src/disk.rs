// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! Disk reads and writes from `/proc/diskstats`. See design.md, Data sources
//! on Linux.

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use crate::collect::{Climb, Collector, Counts, slurp};

const DISKSTATS: &str = "/proc/diskstats";

/// diskstats counts 512 byte sectors, whatever the disk's own sector size.
const SECTOR: f64 = 512.0;

/// Which drives the graph counts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Drives {
	/// Every disk with hardware behind it. That leaves out partitions, and
	/// what sits on top of a disk, such as LVM, LUKS, RAID and loop
	/// devices, so nothing is counted twice. zram isn't a disk either.
	All,
	/// These, by their `/proc/diskstats` names, such as `sda` or `nvme0n1`.
	Only(Vec<String>),
}

/// Sectors read and written for each device `wants` takes. None on a row
/// it takes that doesn't read.
pub(crate) fn parse_diskstats(
	text: &str,
	mut wants: impl FnMut(&str) -> bool,
	out: &mut Counts<2>,
) -> Option<()> {
	for line in text.lines() {
		let mut cols = line.split_ascii_whitespace();
		let Some(name) = cols.nth(2) else {
			continue;
		};
		if !wants(name) {
			continue;
		}
		let mut stats = cols.map(str::parse::<u64>);
		let read = stats.nth(2)?.ok()?;
		let written = stats.nth(3)?.ok()?;
		out.push(name, [read, written]);
	}
	Some(())
}

fn bin(_: &str, [read, written]: [u64; 2], sum: &mut [f64; 2]) {
	sum[0] += written as f64 * SECTOR;
	sum[1] += read as f64 * SECTOR;
}

fn rates(
	climb: &mut Climb<2>,
	text: &str,
	wants: impl FnMut(&str) -> bool,
	at: Instant,
) -> Option<[f32; 2]> {
	parse_diskstats(text, wants, climb.next())?;
	climb.step(at, bin)
}

#[derive(Debug)]
struct Picker {
	drives: Drives,
	block: String,
	/// Whether each name seen is a disk, so `/sys` is asked once a name.
	is_disk: HashMap<String, bool>,
}

impl Picker {
	fn new(drives: Drives, block: &str) -> Self {
		Picker {
			drives,
			block: block.to_owned(),
			is_disk: HashMap::new(),
		}
	}

	fn wants(&mut self, name: &str) -> bool {
		match &self.drives {
			Drives::Only(names) => names.iter().any(|n| n == name),
			Drives::All => {
				if let Some(&known) = self.is_disk.get(name) {
					return known;
				}
				// partitions sit a level down, under their disk
				let found = Path::new(&self.block).join(name).join("device").exists();
				self.is_disk.insert(name.to_owned(), found);
				found
			}
		}
	}
}

/// Bytes per second read and written over the drives picked. A drive that
/// comes or goes, or a counter that wraps or resets, makes a gap.
#[derive(Debug)]
pub struct DiskReader {
	picker: Picker,
	climb: Climb<2>,
	buf: String,
}

impl DiskReader {
	/// None when `/proc/diskstats` doesn't read, or for [`Drives::All`] on a
	/// box with no disk. A chosen drive that isn't there yet reads as 0.
	pub fn new(drives: Drives) -> Option<Self> {
		let all = drives == Drives::All;
		let mut r = DiskReader {
			picker: Picker::new(drives, "/sys/block"),
			climb: Climb::default(),
			buf: String::new(),
		};
		r.read();
		let any = !all || !r.climb.last().is_empty();
		(r.climb.primed() && any).then_some(r)
	}
}

impl Collector<2> for DiskReader {
	const SERIES: [&'static str; 2] = ["write", "read"];

	fn probe() -> Option<Self> {
		DiskReader::new(Drives::All)
	}

	/// Rates since the last call. The first call only primes it.
	fn read(&mut self) -> Option<[f32; 2]> {
		rates(
			&mut self.climb,
			slurp(DISKSTATS, &mut self.buf)?,
			|name| self.picker.wants(name),
			Instant::now(),
		)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::fmt::Write;
	use std::time::Duration;

	/// One row per device, as 6.12 writes them, with 20 columns.
	fn diskstats(rows: &[(&str, u64, u64)]) -> String {
		let mut text = String::new();
		for (name, read, written) in rows {
			writeln!(
				text,
				" 259       0 {name} 9 0 {read} 3 9 0 {written} 4 0 5 6 0 0 0 0 0 0"
			)
			.unwrap();
		}
		text
	}

	// Test ID: defne
	#[test]
	fn reads_diskstats() {
		let mut c = Counts::default();
		let text = diskstats(&[("sda", 8, 16), ("sda1", 8, 16), ("dm-0", 8, 16)]);
		assert_eq!(parse_diskstats(&text, |n| n == "sda", &mut c), Some(()));
		assert_eq!(c.len(), 1);
		// kernels before 4.18 have 14 columns
		let old = "   8       0 sda 100 0 200 0 50 0 400 0 0 0 0\n";
		let mut climb = Climb::default();
		let t = Instant::now();
		rates(&mut climb, old, |_| true, t);
		let old = "   8       0 sda 100 0 202 0 50 0 404 0 0 0 0\n";
		assert_eq!(
			rates(&mut climb, old, |_| true, t + Duration::from_secs(2)),
			Some([1024.0, 512.0])
		);
		for junk in ["   8 0 sda 1 0 2 0 3 0\n", "   8 0 sda 1 0 x 0 3 0 4\n"] {
			assert_eq!(
				parse_diskstats(junk, |_| true, &mut Counts::default()),
				None,
				"{junk}"
			);
		}
		// junk in a row not taken doesn't matter, and short lines are skipped
		let text = format!("\n  8\n   8 0 sdz junk\n{}", diskstats(&[("sda", 1, 1)]));
		let mut c = Counts::default();
		assert_eq!(parse_diskstats(&text, |n| n == "sda", &mut c), Some(()));
		assert_eq!(c.len(), 1);
	}

	// Test ID: defnf
	#[test]
	fn all_leaves_out_partitions_and_stacked_devices() {
		let root = std::env::temp_dir().join(format!("test_tukzmon_disk_{}", std::process::id()));
		let made = [
			"",
			"sda",
			"sda/device",
			"dm-0",
			"loop0",
			"nvme0n1",
			"nvme0n1/device",
		];
		for rel in made {
			let p = root.join(rel);
			if rel.ends_with("device") {
				std::fs::write(&p, "").unwrap();
			} else {
				std::fs::create_dir(&p).unwrap();
			}
		}
		let mut all = Picker::new(Drives::All, &root.to_string_lossy());
		let taken: Vec<&str> = [
			"sda",
			"sda1",
			"dm-0",
			"loop0",
			"nvme0n1",
			"nvme0n1p1",
			"zram0",
		]
		.into_iter()
		.filter(|n| all.wants(n))
		.collect();
		for rel in made.iter().rev() {
			let p = root.join(rel);
			std::fs::remove_file(&p)
				.or_else(|_| std::fs::remove_dir(&p))
				.unwrap();
		}
		assert_eq!(taken, ["sda", "nvme0n1"]);
		// asked once a name
		assert!(all.wants("sda"));

		let mut only = Picker::new(
			Drives::Only(vec!["sda1".into(), "dm-0".into()]),
			"/proc/no/such",
		);
		let taken: Vec<&str> = ["sda", "sda1", "dm-0"]
			.into_iter()
			.filter(|n| only.wants(n))
			.collect();
		assert_eq!(taken, ["sda1", "dm-0"]);
	}

	// Test ID: defng
	#[test]
	fn the_drives_picked_add_up_and_a_change_is_a_gap() {
		let t = Instant::now();
		let s = Duration::from_secs;
		let mut climb = Climb::default();
		let only = |n: &str| n.starts_with("sd") && n.len() == 3;
		let rows = [("sda", 0, 0), ("sda1", 0, 0), ("sdb", 100, 100)];
		assert_eq!(rates(&mut climb, &diskstats(&rows), only, t), None);
		let rows = [("sda", 4, 8), ("sda1", 4, 8), ("sdb", 104, 108)];
		assert_eq!(
			rates(&mut climb, &diskstats(&rows), only, t + s(2)),
			Some([4096.0, 2048.0])
		);
		// a drive plugged in, then its counters back from 0 after a replug
		let rows = [("sda", 4, 8), ("sdb", 104, 108), ("sdc", 50, 0)];
		assert_eq!(rates(&mut climb, &diskstats(&rows), only, t + s(3)), None);
		let rows = [("sda", 4, 8), ("sdb", 104, 108), ("sdc", 52, 0)];
		assert_eq!(
			rates(&mut climb, &diskstats(&rows), only, t + s(4)),
			Some([0.0, 1024.0])
		);
		let rows = [("sda", 4, 8), ("sdb", 104, 108), ("sdc", 1, 0)];
		assert_eq!(rates(&mut climb, &diskstats(&rows), only, t + s(5)), None);
		// a partition coming or going isn't a change in what's counted
		let rows = [
			("sda", 4, 8),
			("sdb", 104, 110),
			("sdc", 1, 0),
			("sdc1", 1, 0),
		];
		assert_eq!(
			rates(&mut climb, &diskstats(&rows), only, t + s(6)),
			Some([1024.0, 0.0])
		);
	}

	// Test ID: defnh
	#[test]
	fn live_diskstats_reads() {
		let Some(mut r) = DiskReader::probe() else {
			// a container can have no disk
			return;
		};
		// every disk the box lists, and none of its partitions
		let block = std::fs::read_dir("/sys/class/block").unwrap();
		let partitions: Vec<String> = block
			.filter_map(|e| e.ok()?.file_name().into_string().ok())
			.filter(|n| {
				Path::new("/sys/class/block")
					.join(n)
					.join("partition")
					.exists()
			})
			.collect();
		assert!(
			partitions.iter().all(|p| !r.picker.wants(p)),
			"{partitions:?}"
		);
		std::thread::sleep(Duration::from_millis(20));
		if let Some(v) = r.read() {
			assert!(v.iter().all(|x| x.is_finite() && *x >= 0.0), "{v:?}");
		}
		assert!(DiskReader::new(Drives::Only(vec!["no-such-drive".into()])).is_some());
	}
}
