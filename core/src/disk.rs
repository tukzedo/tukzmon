// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! Disk reads and writes from each drive's `stat` in `/sys/class/block`.
//! See design.md, Data sources on Linux.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::collect::{Climb, Collector, Kept};

const BLOCK: &str = "/sys/block";
const CLASS: &str = "/sys/class/block";

/// How often the drives are listed again, so one plugged in is counted.
const RELIST: Duration = Duration::from_secs(10);

/// `stat` counts 512 byte sectors, whatever the disk's own sector size.
const SECTOR: f64 = 512.0;

/// Which drives the graph counts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Drives {
	/// Every disk with hardware behind it. That leaves out partitions, and
	/// what sits on top of a disk, such as LVM, LUKS, RAID and loop
	/// devices, so nothing is counted twice. zram isn't a disk either.
	All,
	/// These, by their kernel names, as in `/proc/diskstats`, such as `sda`
	/// or `nvme0n1`.
	Only(Vec<String>),
}

/// Sectors read and written, from a drive's `stat`. Kernels before 4.18
/// have 11 columns, and later ones more.
pub(crate) fn parse_stat(text: &str) -> Option<[u64; 2]> {
	let mut stats = text.split_ascii_whitespace().map(str::parse::<u64>);
	let read = stats.nth(2)?.ok()?;
	let written = stats.nth(3)?.ok()?;
	Some([read, written])
}

fn bin(_: &str, [read, written]: [u64; 2], sum: &mut [f64; 2]) {
	sum[0] += written as f64 * SECTOR;
	sum[1] += read as f64 * SECTOR;
}

#[derive(Debug)]
struct Picker {
	drives: Drives,
	/// Where [`Drives::All`] lists the disks.
	block: String,
	/// Where every drive's `stat` is, partitions too.
	class: String,
}

impl Picker {
	fn new(drives: Drives, block: &str, class: &str) -> Self {
		Picker {
			drives,
			block: block.to_owned(),
			class: class.to_owned(),
		}
	}

	/// What's counted now, sorted for [`Drives::All`].
	fn names(&self) -> Vec<String> {
		match &self.drives {
			// owned, since each one goes into the disk list
			Drives::Only(names) => names.clone(),
			Drives::All => {
				let Ok(dir) = std::fs::read_dir(&self.block) else {
					return Vec::new();
				};
				let mut names: Vec<String> = dir
					.filter_map(|e| e.ok()?.file_name().into_string().ok())
					// partitions sit a level down, under their disk
					.filter(|n| Path::new(&self.block).join(n).join("device").exists())
					.collect();
				names.sort();
				names
			}
		}
	}

	fn stat(&self, name: &str) -> Kept {
		// sysfs writes a / in a kernel name as !, as in cciss!c0d0
		Kept::new(format!("{}/{}/stat", self.class, name.replace('/', "!")))
	}
}

/// Bytes per second read and written over the drives picked. A drive that
/// comes or goes, or a counter that wraps or resets, makes a gap.
#[derive(Debug)]
pub struct DiskReader {
	picker: Picker,
	disks: Vec<(String, Kept)>,
	listed: Option<Instant>,
	climb: Climb<2>,
	buf: String,
}

impl DiskReader {
	/// None for [`Drives::All`] on a box with no disk. A chosen drive that
	/// isn't there yet reads as 0.
	pub fn new(drives: Drives) -> Option<Self> {
		Self::new_at(drives, BLOCK, CLASS, Instant::now())
	}

	fn new_at(drives: Drives, block: &str, class: &str, at: Instant) -> Option<Self> {
		let all = drives == Drives::All;
		let mut r = DiskReader {
			picker: Picker::new(drives, block, class),
			disks: Vec::new(),
			listed: None,
			climb: Climb::default(),
			buf: String::new(),
		};
		r.read_at(at);
		let any = !all || !r.climb.last().is_empty();
		(r.climb.primed() && any).then_some(r)
	}

	/// The drives' files stay open from one list to the next.
	fn list(&mut self, at: Instant) {
		let mut was = std::mem::take(&mut self.disks);
		for name in self.picker.names() {
			let stat = match was.iter().position(|(n, _)| *n == name) {
				Some(i) => was.swap_remove(i).1,
				None => self.picker.stat(&name),
			};
			self.disks.push((name, stat));
		}
		self.listed = Some(at);
	}

	fn read_at(&mut self, at: Instant) -> Option<[f32; 2]> {
		if self
			.listed
			.is_none_or(|then| at.saturating_duration_since(then) >= RELIST)
		{
			self.list(at);
		}
		let rows = self.climb.next();
		for (name, stat) in &mut self.disks {
			// one that doesn't read is left out, so its going is a gap
			if let Some(counts) = stat.read(&mut self.buf).and_then(parse_stat) {
				rows.push(name, counts);
			}
		}
		self.climb.step(at, bin)
	}
}

impl DiskReader {
	pub fn drives(&self) -> &Drives {
		&self.picker.drives
	}
}

impl Collector<2> for DiskReader {
	const SERIES: [&'static str; 2] = ["write", "read"];

	fn probe() -> Option<Self> {
		DiskReader::new(Drives::All)
	}

	/// Rates since the last call. The first call only primes it.
	fn read(&mut self) -> Option<[f32; 2]> {
		self.read_at(Instant::now())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::collect::Tree;

	/// A drive's `stat`, as 6.12 writes it, with 17 columns.
	fn stat(read: u64, written: u64) -> String {
		format!("       9        0 {read} 3 9 0 {written} 4 0 5 6 0 0 0 0 0 0\n")
	}

	// Test ID: defne
	#[test]
	fn reads_a_drives_stat() {
		assert_eq!(parse_stat(&stat(8, 16)), Some([8, 16]));
		// kernels before 4.18 have 11 columns
		assert_eq!(
			parse_stat("100 0 200 0 50 0 400 0 0 0 0\n"),
			Some([200, 400])
		);
		for junk in ["1 0 2 0 3 0\n", "1 0 x 0 3 0 4\n", ""] {
			assert_eq!(parse_stat(junk), None, "{junk}");
		}
	}

	// Test ID: defnf
	#[test]
	fn all_leaves_out_partitions_and_stacked_devices() {
		let t = Tree::new(
			"disk_all",
			&[
				("sda/device", ""),
				("dm-0/", ""),
				("loop0/", ""),
				("zram0/", ""),
				("nvme0n1/device", ""),
			],
		);
		let all = Picker::new(Drives::All, &t.at(""), "/proc/no/such");
		assert_eq!(all.names(), ["nvme0n1", "sda"]);
		let only = Picker::new(
			Drives::Only(vec!["sda1".into(), "dm-0".into()]),
			"/proc/no/such",
			"/proc/no/such",
		);
		assert_eq!(only.names(), ["sda1", "dm-0"]);
		assert!(
			Picker::new(Drives::All, "/proc/no/such", "")
				.names()
				.is_empty()
		);
	}

	// Test ID: defng
	#[test]
	fn the_drives_picked_add_up_and_a_change_is_a_gap() {
		let t = Tree::new(
			"disk_rates",
			&[
				("sda/device", ""),
				("sda/stat", &stat(0, 0)),
				("sda1/stat", &stat(0, 0)),
				("sdb/device", ""),
				("sdb/stat", &stat(100, 100)),
				("sdc/stat", &stat(0, 0)),
			],
		);
		let set = |name: &str, read: u64, written: u64| {
			std::fs::write(t.at(&format!("{name}/stat")), stat(read, written)).unwrap();
		};
		let t0 = Instant::now();
		let s = Duration::from_secs;
		let root = t.at("");
		let mut r = DiskReader::new_at(Drives::All, &root, &root, t0).unwrap();
		set("sda", 4, 8);
		set("sda1", 4, 8);
		set("sdb", 104, 108);
		assert_eq!(r.read_at(t0 + s(2)), Some([4096.0, 2048.0]));
		// a drive plugged in counts from the next list, then its counters
		// back from 0 after a replug
		std::fs::write(t.at("sdc/device"), "").unwrap();
		set("sdc", 50, 0);
		assert_eq!(r.read_at(t0 + s(3)), Some([0.0, 0.0]));
		assert_eq!(r.read_at(t0 + s(10)), None);
		set("sdc", 52, 0);
		assert_eq!(r.read_at(t0 + s(11)), Some([0.0, 1024.0]));
		set("sdc", 1, 0);
		assert_eq!(r.read_at(t0 + s(12)), None);
		// a partition coming or going isn't a change in what's counted
		set("sdb", 104, 110);
		std::fs::create_dir(t.at("sdc1")).unwrap();
		set("sdc1", 1, 0);
		assert_eq!(r.read_at(t0 + s(25)), Some([1024.0 / 13.0, 0.0]));
		// the files stay open across a list
		r.list(t0 + s(26));
		let names: Vec<&str> = r.disks.iter().map(|(n, _)| n.as_str()).collect();
		assert_eq!(names, ["sda", "sdb", "sdc"]);
		assert!(r.disks.iter().all(|(_, k)| k.is_open()));
		for made in ["sdc/device", "sdc1/stat"] {
			std::fs::remove_file(t.at(made)).unwrap();
		}
		std::fs::remove_dir(t.at("sdc1")).unwrap();
	}

	// Test ID: defnh
	#[test]
	fn live_diskstats_reads() {
		let Some(mut r) = DiskReader::probe() else {
			// a container can have no disk
			return;
		};
		// every disk the box lists, and none of its partitions
		let block = std::fs::read_dir(CLASS).unwrap();
		let partitions: Vec<String> = block
			.filter_map(|e| e.ok()?.file_name().into_string().ok())
			.filter(|n| Path::new(CLASS).join(n).join("partition").exists())
			.collect();
		assert!(
			partitions
				.iter()
				.all(|p| r.disks.iter().all(|(n, _)| n != p)),
			"{partitions:?}"
		);
		std::thread::sleep(Duration::from_millis(20));
		if let Some(v) = r.read() {
			assert!(v.iter().all(|x| x.is_finite() && *x >= 0.0), "{v:?}");
		}
		assert!(DiskReader::new(Drives::Only(vec!["no-such-drive".into()])).is_some());
	}
}
