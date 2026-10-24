// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! What every graph's source sits behind. See design.md, Data sources on
//! Linux.

use std::io::Read;

/// One graph's source, with `N` series.
pub trait Collector<const N: usize>: Sized {
	/// Series names, bottom of the stack first.
	const SERIES: [&'static str; N];

	/// None when the source isn't on this box, so its graph is hidden.
	fn probe() -> Option<Self>;

	/// Each series in the source's own unit, [`Self::SERIES`] order. Shares
	/// are 0 to 1. None is a gap. A source that counts up, like CPU, only
	/// primes on its first call.
	fn read(&mut self) -> Option<[f32; N]>;
}

/// Reads a `/proc` or `/sys` file into `buf`, keeping its memory between reads.
pub(crate) fn slurp<'a>(path: &str, buf: &'a mut String) -> Option<&'a str> {
	buf.clear();
	std::fs::File::open(path).ok()?.read_to_string(buf).ok()?;
	Some(buf)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn slurp_reuses_and_misses() {
		let mut buf = String::from("left over");
		assert!(slurp("/proc/self/stat", &mut buf).is_some_and(|s| !s.contains("left over")));
		assert!(slurp("/proc/no/such/file", &mut buf).is_none());
	}
}
