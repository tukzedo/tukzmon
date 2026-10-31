// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! Battery charge and state from `power_supply`. See design.md, Data sources
//! on Linux.

use crate::collect::{Collector, Kept, slurp};

const POWER_SUPPLY: &str = "/sys/class/power_supply";

#[derive(Debug, Clone, Copy, PartialEq)]
enum Unit {
	Energy,
	Charge,
	Percent,
}

/// What one battery said on a read.
#[derive(Debug, Clone, Copy)]
struct Fill {
	unit: Unit,
	now: f64,
	full: f64,
	discharging: bool,
	/// From `capacity_level`. None when it's missing or `Unknown`.
	critical: Option<bool>,
}

/// One `BAT*` folder. The path is kept as a prefix that file names go on.
#[derive(Debug)]
struct Battery {
	path: String,
	base: usize,
	/// Each file read so far, by name.
	files: Vec<(&'static str, Kept)>,
}

impl Battery {
	fn new(path: String) -> Self {
		let base = path.len();
		Battery {
			path,
			base,
			files: Vec::new(),
		}
	}

	fn file(&mut self, name: &str) -> &str {
		self.path.truncate(self.base);
		self.path.push_str(name);
		&self.path
	}

	fn read<'a>(&mut self, name: &'static str, buf: &'a mut String) -> Option<&'a str> {
		let at = if let Some(at) = self.files.iter().position(|(n, _)| *n == name) {
			at
		} else {
			let path = self.file(name).to_owned();
			self.files.push((name, Kept::new(path)));
			self.files.len() - 1
		};
		self.files[at].1.read(buf)
	}
}

fn number(text: &str) -> Option<f64> {
	text.trim().parse().ok().filter(|x: &f64| x.is_finite())
}

fn pair(
	bat: &mut Battery,
	buf: &mut String,
	now: &'static str,
	full: &'static str,
) -> Option<(f64, f64)> {
	let now = bat.read(now, buf).and_then(number)?;
	let full = bat.read(full, buf).and_then(number).filter(|f| *f > 0.0)?;
	Some((now, full))
}

/// None for an empty slot, or one with no charge that reads.
fn fill(bat: &mut Battery, buf: &mut String) -> Option<Fill> {
	if bat.read("present", buf).is_some_and(|s| s.trim() != "1") {
		return None;
	}
	let (unit, (now, full)) = pair(bat, buf, "energy_now", "energy_full")
		.map(|p| (Unit::Energy, p))
		.or_else(|| pair(bat, buf, "charge_now", "charge_full").map(|p| (Unit::Charge, p)))
		.or_else(|| {
			let percent = bat.read("capacity", buf).and_then(number)?;
			Some((Unit::Percent, (percent, 100.0)))
		})?;
	let discharging = bat
		.read("status", buf)
		.is_some_and(|s| s.trim() == "Discharging");
	let critical = bat
		.read("capacity_level", buf)
		.and_then(|s| match s.trim() {
			"Unknown" => None,
			level => Some(level == "Critical"),
		});
	Some(Fill {
		unit,
		now,
		full,
		discharging,
		critical,
	})
}

/// The batteries as one, in [`BatteryReader::SERIES`] order.
fn combine(fills: &[Fill]) -> Option<[f32; 3]> {
	let first = fills.first()?;
	let level = if fills.iter().all(|f| f.unit == first.unit) {
		fills.iter().map(|f| f.now).sum::<f64>() / fills.iter().map(|f| f.full).sum::<f64>()
	} else {
		fills.iter().map(|f| f.now / f.full).sum::<f64>() / fills.len() as f64
	};
	let level = level.clamp(0.0, 1.0);
	let discharging = fills.iter().any(|f| f.discharging);
	// multiload-ng's 4% when a battery doesn't say
	let critical = if fills.iter().all(|f| f.critical.is_some()) {
		fills.iter().all(|f| f.critical == Some(true))
	} else {
		level <= 0.04
	};
	let mut out = [0.0; 3];
	out[match (discharging, critical) {
		(false, _) => 0,
		(true, false) => 1,
		(true, true) => 2,
	}] = level as f32;
	Some(out)
}

/// Every `BAT*` battery's charge, as a share of full, in the series for
/// its state.
// a default one has no battery, so it reads as a gap
#[derive(Debug, Default)]
pub struct BatteryReader {
	batteries: Vec<Battery>,
	fills: Vec<Fill>,
	buf: String,
}

impl BatteryReader {
	fn probe_at(class: &str) -> Option<Self> {
		let mut buf = String::new();
		let mut batteries: Vec<Battery> = std::fs::read_dir(class)
			.ok()?
			.filter_map(|e| {
				let name = e.ok()?.file_name().into_string().ok()?;
				name.starts_with("BAT")
					.then(|| Battery::new(format!("{class}/{name}/")))
			})
			.collect();
		batteries
			.retain_mut(|b| slurp(b.file("type"), &mut buf).is_none_or(|t| t.trim() == "Battery"));
		let mut r = BatteryReader {
			fills: Vec::with_capacity(batteries.len()),
			batteries,
			buf,
		};
		r.read()?;
		Some(r)
	}
}

impl Collector<3> for BatteryReader {
	/// Only one of them holds the charge, by state. Full, `Not charging`
	/// and `Unknown` count as charging, since they mean the power's in.
	/// Critical is discharging at the critical level.
	const SERIES: [&'static str; 3] = ["charging", "discharging", "critical"];

	/// None on a box with no battery in.
	fn probe() -> Option<Self> {
		Self::probe_at(POWER_SUPPLY)
	}

	/// A battery taken out drops out. None when none is left.
	fn read(&mut self) -> Option<[f32; 3]> {
		self.fills.clear();
		for bat in &mut self.batteries {
			if let Some(f) = fill(bat, &mut self.buf) {
				self.fills.push(f);
			}
		}
		combine(&self.fills)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::collect::Tree;

	fn lit(v: Option<[f32; 3]>) -> Option<[bool; 3]> {
		v.map(|v| v.map(|x| x > 0.0))
	}

	// Test ID: defnt
	#[test]
	fn the_charge_goes_in_its_states_series() {
		let t = Tree::new(
			"bat_state",
			&[
				("ps/BAT0/type", "Battery\n"),
				("ps/BAT0/present", "1\n"),
				("ps/BAT0/energy_now", "30000000\n"),
				("ps/BAT0/energy_full", "60000000\n"),
				("ps/BAT0/energy_full_design", "70000000\n"),
				("ps/BAT0/capacity", "51\n"),
				("ps/BAT0/status", "Discharging\n"),
				("ps/BAT0/capacity_level", "Normal\n"),
			],
		);
		let mut r = BatteryReader::probe_at(&t.at("ps")).unwrap();
		assert_eq!(r.read(), Some([0.0, 0.5, 0.0]));
		for status in ["Charging", "Full", "Not charging", "Unknown", ""] {
			std::fs::write(t.at("ps/BAT0/status"), format!("{status}\n")).unwrap();
			assert_eq!(r.read(), Some([0.5, 0.0, 0.0]), "{status}");
		}
		std::fs::write(t.at("ps/BAT0/capacity_level"), "Critical\n").unwrap();
		assert_eq!(r.read(), Some([0.5, 0.0, 0.0]));
		std::fs::write(t.at("ps/BAT0/status"), "Discharging\n").unwrap();
		assert_eq!(r.read(), Some([0.0, 0.0, 0.5]));
		// firmware that counts past full
		std::fs::write(t.at("ps/BAT0/energy_now"), "61000000\n").unwrap();
		assert_eq!(r.read(), Some([0.0, 0.0, 1.0]));
	}

	// Test ID: defnv
	#[test]
	fn without_a_level_critical_is_4_percent() {
		let t = Tree::new(
			"bat_crit",
			&[
				("ps/BAT1/charge_now", "4000\n"),
				("ps/BAT1/charge_full", "100000\n"),
				("ps/BAT1/capacity", "40\n"),
				("ps/BAT1/status", "Discharging\n"),
				("ps/BAT1/capacity_level", "Unknown\n"),
			],
		);
		let mut r = BatteryReader::probe_at(&t.at("ps")).unwrap();
		assert_eq!(lit(r.read()), Some([false, false, true]));
		std::fs::write(t.at("ps/BAT1/charge_now"), "4001\n").unwrap();
		assert_eq!(lit(r.read()), Some([false, true, false]));
		// no charge pair left, so the percent
		std::fs::write(t.at("ps/BAT1/charge_full"), "0\n").unwrap();
		assert_eq!(r.read(), Some([0.0, 0.4, 0.0]));
		std::fs::write(t.at("ps/BAT1/capacity"), "nan\n").unwrap();
		assert_eq!(r.read(), None);
	}

	// Test ID: defnw
	#[test]
	fn two_batteries_read_as_one() {
		let t = Tree::new(
			"bat_two",
			&[
				("ps/BAT0/present", "1\n"),
				("ps/BAT0/energy_now", "10000000\n"),
				("ps/BAT0/energy_full", "40000000\n"),
				("ps/BAT0/capacity", "25\n"),
				("ps/BAT0/status", "Discharging\n"),
				("ps/BAT0/capacity_level", "Normal\n"),
				("ps/BAT1/present", "1\n"),
				("ps/BAT1/energy_now", "30000000\n"),
				("ps/BAT1/energy_full", "40000000\n"),
				("ps/BAT1/capacity", "50\n"),
				("ps/BAT1/status", "Unknown\n"),
				("ps/BAT1/capacity_level", "Critical\n"),
			],
		);
		let mut r = BatteryReader::probe_at(&t.at("ps")).unwrap();
		// one critical and one not is not critical
		assert_eq!(r.read(), Some([0.0, 0.5, 0.0]));
		// mixed units can't add up, so the mean of the shares
		std::fs::write(t.at("ps/BAT1/energy_full"), "junk\n").unwrap();
		assert_eq!(r.read(), Some([0.0, 0.375, 0.0]));
		std::fs::write(t.at("ps/BAT0/present"), "0\n").unwrap();
		assert_eq!(r.read(), Some([0.5, 0.0, 0.0]));
		std::fs::write(t.at("ps/BAT1/present"), "0\n").unwrap();
		assert_eq!(r.read(), None);
	}

	// Test ID: defnx
	#[test]
	fn no_battery_gives_no_collector() {
		let none = Tree::new(
			"bat_none",
			&[
				("ps/AC/type", "Mains\n"),
				("ps/AC/online", "1\n"),
				// a mouse's
				("ps/hidpp_battery_1/type", "Battery\n"),
				("ps/hidpp_battery_1/capacity", "70\n"),
				("ps/hidpp_battery_1/status", "Discharging\n"),
				// an empty slot
				("ps/BAT1/present", "0\n"),
				("ps/BAT1/capacity", "0\n"),
				("ps/BATX/type", "UPS\n"),
				("ps/BATX/capacity", "90\n"),
				("ps/BAT2/status", "Unknown\n"),
			],
		);
		assert!(BatteryReader::probe_at(&none.at("ps")).is_none());
		let empty = Tree::new("bat_empty", &[("ps/", "")]);
		assert!(BatteryReader::probe_at(&empty.at("ps")).is_none());
		assert!(BatteryReader::probe_at("/proc/no/such").is_none());
	}

	// Test ID: defny
	#[test]
	fn live_power_supply_reads() {
		let Some(mut r) = BatteryReader::probe() else {
			// a desktop, or a container
			return;
		};
		let v = r.read().unwrap();
		assert!(v.iter().all(|x| (0.0..=1.0).contains(x)));
		assert!(v.iter().filter(|x| **x > 0.0).count() <= 1);
	}
}
