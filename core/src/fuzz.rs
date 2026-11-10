// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! Fuzz cases as plain tests. Each case is a pure function of its seed, so
//! a failure names one seed and replays from it. See design.md, Fuzzing.
//!
//! - `TUKZMON_FUZZ_SECONDS`: keep going past the fixed cases for this long,
//!   per test.
//! - `TUKZMON_FUZZ_SEED`: the first seed, 0 by default.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant};

use crate::history::History;

// Miri runs about 1000 times slower
const CASES: u64 = if cfg!(miri) { 4 } else { 128 };

// splitmix64
pub(crate) struct Dice(u64);

impl Dice {
	pub(crate) fn next(&mut self) -> u64 {
		self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
		let mut z = self.0;
		z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
		z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
		z ^ (z >> 31)
	}

	/// 0 to `n` - 1, or 0 when `n` is 0.
	pub(crate) fn below(&mut self, n: usize) -> usize {
		if n == 0 {
			return 0;
		}
		(self.next() % n as u64) as usize
	}

	pub(crate) fn one_in(&mut self, n: usize) -> bool {
		self.below(n) == 0
	}

	pub(crate) fn unit(&mut self) -> f64 {
		(self.next() >> 11) as f64 / (1u64 << 53) as f64
	}

	pub(crate) fn pick<'a, T>(&mut self, from: &'a [T]) -> &'a T {
		&from[self.below(from.len())]
	}
}

// numbers a length or count field is most likely to trip on
const LIARS: [u64; 9] = [
	0,
	1,
	2,
	0x7f,
	0xff,
	0xffff,
	0x7fff_ffff,
	0xffff_ffff,
	u64::MAX,
];

/// Puts a number a count or length is likely to trip on in the `width`
/// bytes at `at`, as far as they go.
pub(crate) fn lie(dice: &mut Dice, bytes: &mut [u8], at: usize, width: usize) {
	let lie = dice
		.pick(&LIARS)
		.wrapping_add(dice.below(3) as u64)
		.wrapping_sub(1);
	for (b, l) in bytes
		.iter_mut()
		.skip(at)
		.zip(lie.to_le_bytes().iter().take(width))
	{
		*b = *l;
	}
}

/// A few random hurts to `bytes`: bit flips, bytes set, lying fields, cuts,
/// and runs copied, dropped or put in.
pub(crate) fn mangle(dice: &mut Dice, bytes: &mut Vec<u8>) {
	for _ in 0..=dice.below(4) {
		let at = dice.below(bytes.len());
		match dice.below(7) {
			0 | 1 if !bytes.is_empty() => bytes[at] ^= 1 << dice.below(8),
			2 if !bytes.is_empty() => bytes[at] = *dice.pick(&[0, 1, 0x7f, 0x80, 0xff]),
			3 => {
				let width = *dice.pick(&[1, 2, 4, 8]);
				lie(dice, bytes, at, width);
			}
			4 => bytes.truncate(at),
			5 => {
				let end = (at + 1 + dice.below(64)).min(bytes.len());
				let run = bytes[at..end].to_vec();
				let to = dice.below(bytes.len() + 1);
				bytes.splice(to..to, run);
			}
			_ => {
				let end = (at + 1 + dice.below(64)).min(bytes.len());
				bytes.drain(at..end);
			}
		}
	}
}

/// How many cases ran, and how many of those `case` called accepted.
#[derive(Debug)]
pub(crate) struct Tally {
	pub(crate) cases: u64,
	pub(crate) accepted: u64,
}

impl Tally {
	/// Fails unless some inputs were taken and some refused, so a generator
	/// that stopped making either one shows. Miri runs too few to judge.
	pub(crate) fn assert_mixed(&self) {
		if cfg!(miri) {
			return;
		}
		assert!(self.accepted > self.cases / 8, "too few taken: {self:?}");
		assert!(self.accepted < self.cases, "none refused: {self:?}");
	}
}

fn env_number<T: std::str::FromStr>(name: &str) -> Option<T> {
	let text = std::env::var(name).ok().filter(|v| !v.is_empty())?;
	let Ok(v) = text.parse() else {
		panic!("{name} isn't a number: '{text}'");
	};
	Some(v)
}

/// Runs `case` over the fixed seeds, then on until the budget is spent.
/// `case` gives true when the input it made was read as good. A panic in
/// one fails the test with its seed.
pub(crate) fn run(name: &str, case: impl Fn(&mut Dice) -> bool) -> Tally {
	let first: u64 = env_number("TUKZMON_FUZZ_SEED").unwrap_or(0);
	let budget = env_number::<f64>("TUKZMON_FUZZ_SECONDS")
		.filter(|_| !cfg!(miri))
		.map_or(Duration::ZERO, |s| Duration::from_secs_f64(s.max(0.0)));
	let start = Instant::now();
	let mut tally = Tally {
		cases: 0,
		accepted: 0,
	};
	while tally.cases < CASES || start.elapsed() < budget {
		let seed = first.wrapping_add(tally.cases);
		let Ok(good) = catch_unwind(AssertUnwindSafe(|| case(&mut Dice(seed)))) else {
			panic!("{name}: seed {seed} failed, TUKZMON_FUZZ_SEED={seed} replays it");
		};
		tally.accepted += u64::from(good);
		tally.cases += 1;
	}
	tally
}

/// A history as the graphs would have kept it: runs of samples at a few
/// rates, with gaps and trims.
pub(crate) fn history<const N: usize>(dice: &mut Dice) -> History<N> {
	let mut h = History::default();
	let mut t = dice.unit() * 2e9;
	for _ in 0..dice.below(64) {
		let len = dice.pick(&[0.05, 0.05, 0.25, 0.5, 2.0]) * (0.5 + dice.unit());
		let v: [f32; N] = std::array::from_fn(|_| dice.unit() as f32);
		h.add(t, t + len, &v);
		t += len;
		if dice.one_in(20) {
			t += dice.unit() * 5000.0;
		}
		if dice.one_in(40) {
			h.trim(t - dice.unit() * 3000.0);
		}
	}
	h
}

#[cfg(test)]
mod tests {
	use super::*;

	// Test ID: df4f0
	#[test]
	fn a_failing_seed_is_named_and_the_rest_are_pure() {
		let draw = |seed| {
			let mut d = Dice(seed);
			let mut v = (0..32).map(|_| d.below(256) as u8).collect::<Vec<_>>();
			mangle(&mut d, &mut v);
			v
		};
		assert_eq!(draw(7), draw(7));
		assert_ne!(draw(7), draw(8));
		let tally = run("half", |d| d.0 % 2 == 0);
		assert!(tally.cases >= CASES);
		assert_eq!(tally.accepted * 2, tally.cases, "{tally:?}");
		let all = run("all", |_| true);
		assert!(cfg!(miri) || catch_unwind(|| all.assert_mixed()).is_err());
		let failed = catch_unwind(|| {
			run("bust", |d| {
				assert!(d.0 % 4 != 3);
				true
			})
		});
		let text = failed
			.err()
			.and_then(|e| e.downcast_ref::<String>().cloned())
			.unwrap_or_default();
		let seed = text
			.split_once("TUKZMON_FUZZ_SEED=")
			.and_then(|(_, s)| s.split(' ').next()?.parse::<u64>().ok());
		assert!(
			seed.is_some_and(|s| s % 4 == 3),
			"a panic in a case names its seed: '{text}'"
		);
	}
}
