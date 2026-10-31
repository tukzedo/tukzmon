// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! CPU temperature from `hwmon`. See design.md, Data sources on Linux.

use crate::collect::{Collector, Kept, slurp};

const HWMON: &str = "/sys/class/hwmon";

/// Which of a driver's `temp<N>` sensors are the CPU package, from each
/// one's number and label, lowest number first. None for a driver that
/// isn't a CPU's.
fn package_sensors(driver: &str, sensors: &[(u32, Option<&str>)]) -> Vec<u32> {
	let labeled = |want: &str| {
		sensors
			.iter()
			.find(|(_, label)| *label == Some(want))
			.map(|(n, _)| *n)
	};
	let first = || sensors.first().map(|(n, _)| *n);
	match driver {
		"coretemp" => sensors
			.iter()
			.filter(|(_, label)| label.is_some_and(|l| l.starts_with("Package id")))
			.map(|(n, _)| *n)
			.collect(),
		// Tctl can carry an offset for the fans on early Zen, and Tdie shows
		// up only then. Before Zen there are no labels.
		"k10temp" | "zenpower" => labeled("Tdie")
			.or_else(|| labeled("Tctl"))
			.or_else(first)
			.into_iter()
			.collect(),
		// Arm boards, with one sensor for the whole chip
		"cpu_thermal" | "cpu0_thermal" | "soc_thermal" => first().into_iter().collect(),
		_ => Vec::new(),
	}
}

fn degrees(text: &str) -> Option<f32> {
	let milli: i64 = text.trim().parse().ok()?;
	Some(milli as f32 / 1000.0)
}

/// `hwmon<N>` folders in `class`, in number order.
fn hwmon_dirs(class: &str) -> Vec<String> {
	let Ok(dir) = std::fs::read_dir(class) else {
		return Vec::new();
	};
	let mut found: Vec<(u32, String)> = dir
		.filter_map(|e| {
			let name = e.ok()?.file_name().into_string().ok()?;
			let n = name.strip_prefix("hwmon")?.parse().ok()?;
			Some((n, format!("{class}/{name}")))
		})
		.collect();
	found.sort_unstable();
	found.into_iter().map(|(_, path)| path).collect()
}

/// Each `temp<N>_input` in `dir`, with its label, in number order.
fn sensors_in(dir: &str, buf: &mut String) -> Vec<(u32, Option<String>)> {
	let Ok(entries) = std::fs::read_dir(dir) else {
		return Vec::new();
	};
	let mut numbers: Vec<u32> = entries
		.filter_map(|e| {
			let name = e.ok()?.file_name().into_string().ok()?;
			name.strip_prefix("temp")?
				.strip_suffix("_input")?
				.parse()
				.ok()
		})
		.collect();
	numbers.sort_unstable();
	numbers
		.into_iter()
		.map(|n| {
			let label = slurp(&format!("{dir}/temp{n}_label"), buf).map(|s| s.trim().to_owned());
			(n, label)
		})
		.collect()
}

/// Where the graph turns critical on a sensor with no `temp*_crit`, such as
/// `k10temp`. Zen's own limit is 95 C.
pub const CRIT_UNKNOWN: f32 = 95.0;

/// The CPU package in degrees C, the hottest one on a box with more than one
/// socket, split at the critical level.
#[derive(Debug, Default)]
pub struct TempReader {
	inputs: Vec<Kept>,
	crit: Option<f32>,
	buf: String,
}

impl TempReader {
	/// The critical level in degrees C, the lowest of the sensors read. None when
	/// none of them has one.
	pub fn crit(&self) -> Option<f32> {
		self.crit
	}

	/// Where normal ends and critical starts: [`Self::crit`], or else
	/// [`CRIT_UNKNOWN`].
	pub fn limit(&self) -> f32 {
		self.crit.unwrap_or(CRIT_UNKNOWN)
	}

	fn probe_at(class: &str) -> Option<Self> {
		let mut buf = String::new();
		let mut inputs = Vec::new();
		let mut crit: Option<f32> = None;
		for dir in hwmon_dirs(class) {
			let Some(driver) = slurp(&format!("{dir}/name"), &mut buf).map(|s| s.trim().to_owned())
			else {
				continue;
			};
			let sensors = sensors_in(&dir, &mut buf);
			let labeled: Vec<(u32, Option<&str>)> =
				sensors.iter().map(|(n, l)| (*n, l.as_deref())).collect();
			for n in package_sensors(&driver, &labeled) {
				let level = slurp(&format!("{dir}/temp{n}_crit"), &mut buf).and_then(degrees);
				if let Some(c) = level.filter(|c| *c > 0.0) {
					crit = Some(crit.map_or(c, |was| was.min(c)));
				}
				inputs.push(Kept::new(format!("{dir}/temp{n}_input")));
			}
		}
		let mut r = TempReader { inputs, crit, buf };
		r.read()?;
		Some(r)
	}
}

impl Collector<2> for TempReader {
	/// Up to the limit, and what's over it, so the 2 add up to the reading.
	const SERIES: [&'static str; 2] = ["normal", "critical"];

	/// None when no CPU sensor is found.
	fn probe() -> Option<Self> {
		Self::probe_at(HWMON)
	}

	/// A sensor that stops reading drops out. None when all of them have.
	fn read(&mut self) -> Option<[f32; 2]> {
		let buf = &mut self.buf;
		let limit = self.crit.unwrap_or(CRIT_UNKNOWN);
		self.inputs
			.iter_mut()
			.filter_map(|input| input.read(buf).and_then(degrees))
			.reduce(f32::max)
			.map(|x| [x.min(limit), (x - limit).max(0.0)])
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::collect::Tree;

	// Test ID: defnm
	#[test]
	fn the_package_sensor_is_picked_per_driver() {
		let intel = [
			(1, Some("Package id 0")),
			(2, Some("Core 0")),
			(3, Some("Core 1")),
		];
		assert_eq!(package_sensors("coretemp", &intel), [1]);
		let zen = [(1, Some("Tctl")), (3, Some("Tccd1")), (4, Some("Tccd2"))];
		assert_eq!(package_sensors("k10temp", &zen), [1]);
		let early_zen = [(1, Some("Tctl")), (2, Some("Tdie")), (3, Some("Tccd1"))];
		assert_eq!(package_sensors("k10temp", &early_zen), [2]);
		assert_eq!(package_sensors("zenpower", &early_zen), [2]);
		assert_eq!(package_sensors("k10temp", &[(1, None)]), [1]);
		assert_eq!(package_sensors("cpu_thermal", &[(1, None)]), [1]);
		// not a CPU's, though they have temperatures
		for driver in ["nvme", "acpitz", "it8792", "iwlwifi_1", "amdgpu"] {
			assert!(package_sensors(driver, &[(1, Some("Composite")), (2, None)]).is_empty());
		}
		assert!(package_sensors("coretemp", &[(2, Some("Core 0"))]).is_empty());
		assert!(package_sensors("k10temp", &[]).is_empty());
	}

	// Test ID: defnn
	#[test]
	fn reads_the_package_and_skips_the_rest() {
		let t = Tree::new(
			"temp_zen",
			&[
				("hwmon/hwmon0/name", "acpitz\n"),
				("hwmon/hwmon0/temp1_input", "90000\n"),
				("hwmon/hwmon1/name", "nvme\n"),
				("hwmon/hwmon1/temp1_input", "95000\n"),
				("hwmon/hwmon1/temp1_crit", "84850\n"),
				("hwmon/hwmon1/temp1_label", "Composite\n"),
				("hwmon/hwmon5/name", "k10temp\n"),
				("hwmon/hwmon5/temp1_input", "48250\n"),
				("hwmon/hwmon5/temp1_label", "Tctl\n"),
				("hwmon/hwmon5/temp3_input", "70000\n"),
				("hwmon/hwmon5/temp3_label", "Tccd1\n"),
				("hwmon/hwmon7/name", "hidpp_battery_1\n"),
			],
		);
		let mut r = TempReader::probe_at(&t.at("hwmon")).unwrap();
		assert_eq!((r.read(), r.crit()), (Some([48.25, 0.0]), None));
		assert_eq!(r.limit(), CRIT_UNKNOWN);
	}

	// Test ID: defnp
	#[test]
	fn two_sockets_give_the_hottest_and_the_lowest_crit() {
		let t = Tree::new(
			"temp_intel",
			&[
				("hwmon/hwmon2/name", "coretemp\n"),
				("hwmon/hwmon2/temp1_input", "41000\n"),
				("hwmon/hwmon2/temp1_label", "Package id 0\n"),
				("hwmon/hwmon2/temp1_crit", "100000\n"),
				("hwmon/hwmon2/temp2_input", "99000\n"),
				("hwmon/hwmon2/temp2_label", "Core 0\n"),
				("hwmon/hwmon10/name", "coretemp\n"),
				("hwmon/hwmon10/temp1_input", "57500\n"),
				("hwmon/hwmon10/temp1_label", "Package id 1\n"),
				("hwmon/hwmon10/temp1_crit", "95000\n"),
			],
		);
		let mut r = TempReader::probe_at(&t.at("hwmon")).unwrap();
		assert_eq!((r.read(), r.crit()), (Some([57.5, 0.0]), Some(95.0)));
		// one socket's sensor going away leaves the other
		std::fs::write(t.at("hwmon/hwmon10/temp1_input"), "junk\n").unwrap();
		assert_eq!(r.read(), Some([41.0, 0.0]));
		std::fs::write(t.at("hwmon/hwmon2/temp1_input"), "\n").unwrap();
		assert_eq!(r.read(), None);
	}

	// Test ID: defnq
	#[test]
	fn no_cpu_sensor_gives_no_collector() {
		let none = Tree::new(
			"temp_none",
			&[
				("hwmon/hwmon0/name", "nvme\n"),
				("hwmon/hwmon0/temp1_input", "40000\n"),
				("hwmon/hwmon0/temp1_crit", "84850\n"),
				("hwmon/hwmon1/temp1_input", "40000\n"),
				("hwmon/notes/", ""),
			],
		);
		assert!(TempReader::probe_at(&none.at("hwmon")).is_none());
		// a package sensor there that doesn't read
		let junk = Tree::new(
			"temp_junk",
			&[
				("hwmon/hwmon0/name", "k10temp\n"),
				("hwmon/hwmon0/temp1_input", "hot\n"),
				("hwmon/hwmon0/temp1_label", "Tctl\n"),
			],
		);
		assert!(TempReader::probe_at(&junk.at("hwmon")).is_none());
		assert!(TempReader::probe_at("/proc/no/such").is_none());
	}

	// Test ID: dej0s
	#[test]
	fn over_the_crit_is_the_critical_part() {
		let t = Tree::new(
			"temp_hot",
			&[
				("hwmon/hwmon0/name", "coretemp\n"),
				("hwmon/hwmon0/temp1_input", "103500\n"),
				("hwmon/hwmon0/temp1_label", "Package id 0\n"),
				("hwmon/hwmon0/temp1_crit", "100000\n"),
			],
		);
		let mut r = TempReader::probe_at(&t.at("hwmon")).unwrap();
		assert_eq!((r.read(), r.limit()), (Some([100.0, 3.5]), 100.0));
		std::fs::write(t.at("hwmon/hwmon0/temp1_input"), "100000\n").unwrap();
		assert_eq!(r.read(), Some([100.0, 0.0]));
		// no crit, so it's over at Zen's own limit
		std::fs::remove_file(t.at("hwmon/hwmon0/temp1_crit")).unwrap();
		std::fs::write(t.at("hwmon/hwmon0/temp1_input"), "97000\n").unwrap();
		let mut r = TempReader::probe_at(&t.at("hwmon")).unwrap();
		assert_eq!(r.read(), Some([CRIT_UNKNOWN, 2.0]));
		assert_eq!(TempReader::default().read(), None);
	}

	// Test ID: defnr
	#[test]
	fn live_hwmon_reads() {
		let Some(mut r) = TempReader::probe() else {
			// the box, or a container, may have no CPU sensor
			return;
		};
		assert!(
			r.read()
				.is_some_and(|[n, c]| (-40.0..=150.0).contains(&(n + c)) && c >= 0.0)
		);
		assert!(r.crit().is_none_or(|c| c > 0.0));
	}
}
