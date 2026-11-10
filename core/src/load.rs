// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! The 1 minute load from `/proc/loadavg`.

use crate::collect::{Collector, Kept};

const LOADAVG: &str = "/proc/loadavg";

/// The first column. None on junk, or on a value no load can be.
pub fn parse_loadavg(text: &str) -> Option<f32> {
	let load: f32 = text.split_ascii_whitespace().next()?.parse().ok()?;
	(load.is_finite() && load >= 0.0).then_some(load)
}

/// Reads `/proc/loadavg` for the load graph.
#[derive(Debug)]
pub struct LoadReader {
	file: Kept,
	buf: String,
}

impl Default for LoadReader {
	fn default() -> Self {
		LoadReader {
			file: Kept::new(LOADAVG),
			buf: String::new(),
		}
	}
}

impl Collector<1> for LoadReader {
	const SERIES: [&'static str; 1] = ["load"];

	fn probe() -> Option<Self> {
		let mut r = LoadReader::default();
		r.read()?;
		Some(r)
	}

	fn read(&mut self) -> Option<[f32; 1]> {
		parse_loadavg(self.file.read(&mut self.buf)?).map(|x| [x])
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	// Test ID: defnj
	#[test]
	fn the_first_column_is_the_load() {
		assert_eq!(
			parse_loadavg("15.59 15.59 17.06 20/5598 4128420\n"),
			Some(15.59)
		);
		assert_eq!(parse_loadavg("0.00 0.01 0.05 1/90 12\n"), Some(0.0));
		for junk in [
			"",
			"\n",
			"lots 1.0 1.0 1/2 3\n",
			"-1.00 0 0 1/2 3\n",
			"NaN 0 0\n",
			"inf 0 0\n",
		] {
			assert_eq!(parse_loadavg(junk), None, "{junk:?}");
		}
	}

	// Test ID: defnk
	#[test]
	fn live_proc_loadavg_reads() {
		let mut r = LoadReader::probe().unwrap();
		assert!(r.read().is_some_and(|[x]| x >= 0.0));
	}

	// Test ID: df4gk
	#[test]
	fn fuzz_loadavg_gives_a_load_or_none() {
		let tally = crate::fuzz::run("loadavg", |dice| {
			let load = dice.below(100_000) as f32 / 100.0;
			let text = format!(
				"{load:.2} {:.2} 0.59 {}/{} {}\n",
				dice.unit() * 9.0,
				dice.below(9),
				dice.below(9999),
				dice.below(4_000_000)
			);
			let (text, whole) = crate::fuzz::maybe_hurt(dice, text);
			let got = parse_loadavg(&text);
			if whole {
				assert!(got.is_some_and(|x| (x - load).abs() < 0.01), "{text:?}");
			}
			assert!(got.is_none_or(|x| x.is_finite() && x >= 0.0), "{got:?}");
			got.is_some()
		});
		tally.assert_mixed();
	}
}
