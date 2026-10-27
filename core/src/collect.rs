// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! What every graph's source sits behind. See design.md, Data sources on
//! Linux.

use std::io::Read;
use std::time::Instant;

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

/// Named rows of counters off one read, such as bytes per interface. The
/// names keep their memory between reads.
#[derive(Debug, Default)]
pub(crate) struct Counts<const K: usize> {
	names: Vec<String>,
	values: Vec<[u64; K]>,
}

impl<const K: usize> Counts<K> {
	pub(crate) fn push(&mut self, name: &str, values: [u64; K]) {
		let row = self.values.len();
		if let Some(slot) = self.names.get_mut(row) {
			slot.clear();
			slot.push_str(name);
		} else {
			self.names.push(name.to_owned());
		}
		self.values.push(values);
	}

	#[cfg(test)]
	pub(crate) fn len(&self) -> usize {
		self.values.len()
	}

	pub(crate) fn is_empty(&self) -> bool {
		self.values.is_empty()
	}

	fn names(&self) -> &[String] {
		&self.names[..self.values.len()]
	}

	/// Each row's rise since `was`. None when the rows aren't the same ones,
	/// or a counter went down, as when it wraps or resets.
	fn rise_since<'a>(
		&'a self,
		was: &'a Self,
	) -> Option<impl Iterator<Item = (&'a str, [u64; K])>> {
		if self.names() != was.names() {
			return None;
		}
		let pairs = || self.values.iter().zip(&was.values);
		if pairs().any(|(now, then)| now.iter().zip(then).any(|(n, t)| n < t)) {
			return None;
		}
		Some(
			self.names()
				.iter()
				.map(String::as_str)
				.zip(pairs().map(|(now, then)| std::array::from_fn(|i| now[i] - then[i]))),
		)
	}
}

/// Counters that only go up, read as rates per second.
#[derive(Debug, Default)]
pub(crate) struct Climb<const K: usize> {
	now: Counts<K>,
	was: Counts<K>,
	at: Option<Instant>,
}

impl<const K: usize> Climb<K> {
	/// The next read's rows, emptied for the caller to fill before
	/// [`Self::step`].
	pub(crate) fn next(&mut self) -> &mut Counts<K> {
		self.now.values.clear();
		&mut self.now
	}

	/// Whether a read has gone through, so the next step can give rates.
	pub(crate) fn primed(&self) -> bool {
		self.at.is_some()
	}

	/// The rows of the last read stepped.
	pub(crate) fn last(&self) -> &Counts<K> {
		&self.was
	}

	/// Rates since the last step, with `bin` adding each row's rise into
	/// its series. None on the first step, when no time passed, and as a
	/// gap rather than a spike when [`Counts::rise_since`] gives none.
	pub(crate) fn step<const N: usize>(
		&mut self,
		at: Instant,
		bin: impl Fn(&str, [u64; K], &mut [f64; N]),
	) -> Option<[f32; N]> {
		let secs = self
			.at
			.replace(at)
			.map(|then| at.saturating_duration_since(then).as_secs_f64());
		let out = secs.filter(|s| *s > 0.0).and_then(|secs| {
			let mut sum = [0.0; N];
			for (name, rise) in self.now.rise_since(&self.was)? {
				bin(name, rise, &mut sum);
			}
			Some(sum.map(|x| (x / secs) as f32))
		});
		std::mem::swap(&mut self.now, &mut self.was);
		out
	}
}

/// A made-up /proc or /sys for tests, removed file by file when dropped. A
/// path ending in `/` is a folder.
#[cfg(test)]
pub(crate) struct Tree {
	root: std::path::PathBuf,
	made: Vec<std::path::PathBuf>,
}

#[cfg(test)]
impl Tree {
	pub(crate) fn new(name: &str, files: &[(&str, &str)]) -> Tree {
		let root = std::env::temp_dir().join(format!("test_tukzmon_{}_{name}", std::process::id()));
		std::fs::create_dir(&root).unwrap();
		let mut t = Tree {
			root,
			made: Vec::new(),
		};
		for (rel, text) in files {
			let path = t.root.join(rel);
			let is_dir = rel.ends_with('/');
			let missing: Vec<std::path::PathBuf> = path
				.ancestors()
				.skip(usize::from(!is_dir))
				.take_while(|d| *d != t.root && !d.exists())
				.map(std::path::Path::to_path_buf)
				.collect();
			for dir in missing.into_iter().rev() {
				std::fs::create_dir(&dir).unwrap();
				t.made.push(dir);
			}
			if !is_dir {
				std::fs::write(&path, text).unwrap();
				t.made.push(path);
			}
		}
		t
	}

	pub(crate) fn at(&self, rel: &str) -> String {
		self.root.join(rel).to_string_lossy().into_owned()
	}
}

#[cfg(test)]
impl Drop for Tree {
	fn drop(&mut self) {
		self.made
			.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
		for p in &self.made {
			let _ = std::fs::remove_file(p).or_else(|_| std::fs::remove_dir(p));
		}
		let _ = std::fs::remove_dir(&self.root);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::time::Duration;

	fn add(_: &str, [x]: [u64; 1], sum: &mut [f64; 1]) {
		sum[0] += x as f64;
	}

	fn step(c: &mut Climb<1>, at: Instant, rows: &[(&str, u64)]) -> Option<[f32; 1]> {
		let next = c.next();
		for (name, x) in rows {
			next.push(name, [*x]);
		}
		c.step(at, add)
	}

	// Test ID: defnd
	#[test]
	fn climb_primes_then_gives_rates_and_gaps() {
		let t = Instant::now();
		let s = Duration::from_secs;
		let mut c = Climb::default();
		assert!(!c.primed());
		assert_eq!(step(&mut c, t, &[("a", 10), ("b", 5)]), None);
		assert!(c.primed());
		assert_eq!(step(&mut c, t + s(2), &[("a", 30), ("b", 9)]), Some([12.0]));
		// no time passed
		assert_eq!(step(&mut c, t + s(2), &[("a", 30), ("b", 9)]), None);
		// same count of rows, but another name
		assert_eq!(step(&mut c, t + s(3), &[("a", 31), ("c", 9)]), None);
		assert_eq!(step(&mut c, t + s(4), &[("a", 31), ("c", 10)]), Some([1.0]));
		assert_eq!(c.last().len(), 2);
		assert_eq!(step(&mut c, t + s(5), &[("a", 0), ("c", 10)]), None);
		assert_eq!(step(&mut c, t + s(7), &[]), None);
		assert_eq!(step(&mut c, t + s(9), &[]), Some([0.0]));
	}

	// Test ID: decaw
	#[test]
	fn slurp_reuses_and_misses() {
		let mut buf = String::from("left over");
		assert!(slurp("/proc/self/stat", &mut buf).is_some_and(|s| !s.contains("left over")));
		assert!(slurp("/proc/no/such/file", &mut buf).is_none());
	}
}
