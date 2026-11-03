// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! Samples kept in tiers of coarser and coarser buckets. See design.md,
//! History.
//!
//! A sample is a share of the time between 2 reads, so it's added over that
//! whole interval, and each bucket keeps how many seconds of data it has and
//! where in the bucket they are.

use std::collections::VecDeque;

/// Bucket length and how long it's kept, in seconds, finest first.
/// Each fine tier keeps a linear graph at the width slider's top, at any rate
/// the next tier is too coarse for.
pub const TIERS: [(f64, f64); 6] = [
	(0.05, 256.0),
	(0.25, 2048.0),
	(2.0, 3.0 * 3600.0),
	(30.0, 3.0 * 86400.0),
	(300.0, 90.0 * 86400.0),
	(3600.0, f64::INFINITY),
];

/// One span of time at a tier's resolution.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bucket<const N: usize> {
	/// Each value times the seconds it lasted.
	pub sum: [f32; N],
	/// The least of each value.
	pub min: [f32; N],
	/// The most of each value.
	pub max: [f32; N],
	/// Seconds of data in the bucket. 0 is a gap.
	pub secs: f32,
	/// Where the data starts and ends, in seconds from the bucket's start.
	pub lo: f64,
	/// See `lo`.
	pub hi: f64,
}

impl<const N: usize> Default for Bucket<N> {
	fn default() -> Self {
		Bucket {
			sum: [0.0; N],
			min: [f32::INFINITY; N],
			max: [f32::NEG_INFINITY; N],
			secs: 0.0,
			lo: f64::INFINITY,
			hi: 0.0,
		}
	}
}

impl<const N: usize> Bucket<N> {
	fn add(&mut self, lo: f64, hi: f64, v: &[f32; N]) {
		let w = (hi - lo) as f32;
		for (i, &x) in v.iter().enumerate() {
			self.sum[i] += x * w;
			self.min[i] = self.min[i].min(x);
			self.max[i] = self.max[i].max(x);
		}
		self.secs += w;
		self.lo = self.lo.min(lo);
		self.hi = self.hi.max(hi);
	}
	// as add makes them, or an untouched gap
	fn sane(&self) -> bool {
		if *self == Bucket::default() {
			return true;
		}
		let finite = |xs: &[f32]| xs.iter().all(|x| x.is_finite());
		self.secs > 0.0
			&& finite(&self.sum)
			&& finite(&self.min)
			&& finite(&self.max)
			&& self.secs.is_finite()
			&& self.lo.is_finite()
			&& self.hi.is_finite()
			&& self.lo <= self.hi
	}
	/// The time-weighted mean of value `i`, or 0 for a gap.
	pub fn mean(&self, i: usize) -> f32 {
		if self.secs > 0.0 {
			self.sum[i] / self.secs
		} else {
			0.0
		}
	}
}

/// Buckets a block covers at each level of [`Tier::blocks`], as a power of
/// 2, so a bucket's block is a shift. Each level's blocks are made of whole
/// blocks of the level below.
const BLOCK_BITS: [u32; 2] = [3, 6];
/// Buckets a block covers, at each level.
const BLOCKS: [i64; 2] = [1 << BLOCK_BITS[0], 1 << BLOCK_BITS[1]];

/// What [`History::span`] adds up over buckets it covers whole.
#[derive(Clone, Copy, Debug)]
struct Total<const N: usize> {
	weight: f64,
	sums: [f64; N],
	max: [f32; N],
}

impl<const N: usize> Total<N> {
	fn zero() -> Total<N> {
		Total {
			weight: 0.0,
			sums: [0.0; N],
			max: [f32::NEG_INFINITY; N],
		}
	}

	fn join(&mut self, other: &Total<N>) {
		self.weight += other.weight;
		for s in 0..N {
			self.sums[s] += other.sums[s];
			self.max[s] = self.max[s].max(other.max[s]);
		}
	}
}

/// The totals of one size of block.
#[derive(Clone, Debug, Default)]
struct Level<const N: usize> {
	totals: VecDeque<Total<N>>,
	/// Block number of `totals[0]`, counted from time 0.
	first: i64,
}

#[derive(Clone, Debug)]
struct Tier<const N: usize> {
	len: f64,
	keep: f64,
	/// Bucket number of `buckets[0]`, counted from time 0.
	first: i64,
	buckets: VecDeque<Bucket<N>>,
	/// A total per block at each size in [`BLOCKS`], so a wide span reads
	/// the biggest blocks it covers whole, then smaller ones, and only the
	/// buckets at its ends one at a time. Only a block whose buckets are all
	/// still kept is read.
	blocks: [Level<N>; BLOCKS.len()],
}

impl<const N: usize> Tier<N> {
	fn new(len: f64, keep: f64) -> Tier<N> {
		Tier {
			len,
			keep,
			first: 0,
			buckets: VecDeque::new(),
			blocks: Default::default(),
		}
	}

	/// Adds buckets `a0` to `a1` one by one, each over its whole data. Bucket
	/// numbers are counted from time 0.
	fn add_buckets(&self, a0: i64, a1: i64, out: &mut Total<N>) {
		let range = (a0 - self.first) as usize..(a1 - self.first) as usize;
		for (a, bk) in (a0..).zip(self.buckets.range(range)) {
			if bk.secs <= 0.0 {
				continue;
			}
			let start = a as f64 * self.len;
			let (lo, hi) = (start + bk.lo, start + bk.hi);
			if hi <= lo {
				continue;
			}
			// the bucket's mean, weighted by its data, as a span weighs it
			let secs = f64::from(bk.secs);
			let w = secs.min(hi - lo);
			let k = w / secs;
			out.weight += w;
			for s in 0..N {
				out.sums[s] += k * f64::from(bk.sum[s]);
				out.max[s] = out.max[s].max(bk.max[s]);
			}
		}
	}

	/// Adds buckets `a0` to `a1`, reading whole blocks from the lowest
	/// `levels` of [`Self::blocks`], biggest first.
	fn gather(&self, levels: usize, a0: i64, a1: i64, out: &mut Total<N>) {
		let Some(level) = levels.checked_sub(1) else {
			self.add_buckets(a0, a1, out);
			return;
		};
		let (bits, blocks) = (BLOCK_BITS[level], &self.blocks[level]);
		let size = 1 << bits;
		let (b0, b1) = ((a0 + size - 1) >> bits, a1 >> bits);
		if b1 <= b0 {
			self.gather(level, a0, a1, out);
			return;
		}
		self.gather(level, a0, b0 * size, out);
		let at = |b: i64| (b - blocks.first) as usize;
		for t in blocks.totals.range(at(b0)..at(b1)) {
			out.join(t);
		}
		self.gather(level, b1 * size, a1, out);
	}

	/// The buckets from `i0` to `i1`, as [`Self::add_buckets`] adds them.
	fn total_over(&self, i0: usize, i1: usize) -> Total<N> {
		let mut out = Total::zero();
		self.gather(
			BLOCKS.len(),
			self.first + i0 as i64,
			self.first + i1 as i64,
			&mut out,
		);
		out
	}

	/// Works out the blocks again from the ones holding bucket `from` on.
	fn reindex(&mut self, from: usize) {
		if self.buckets.is_empty() {
			for level in &mut self.blocks {
				level.totals.clear();
			}
			return;
		}
		let end = self.first + self.buckets.len() as i64;
		for (level, &bits) in BLOCK_BITS.iter().enumerate() {
			let size = 1 << bits;
			let mut b0 = (self.first + from as i64) >> bits;
			let blocks = &mut self.blocks[level];
			if from == 0 || blocks.totals.is_empty() {
				blocks.totals.clear();
				blocks.first = b0;
			}
			// buckets pushed for a gap come before the ones a sample touched
			b0 = b0.min(blocks.first + blocks.totals.len() as i64);
			blocks.totals.truncate((b0 - blocks.first) as usize);
			for b in b0..=(end - 1) >> bits {
				let mut t = Total::zero();
				// from the level below, done first
				let (a0, a1) = ((b * size).max(self.first), ((b + 1) * size).min(end));
				self.gather(level, a0, a1, &mut t);
				self.blocks[level].totals.push_back(t);
			}
		}
	}

	fn pop_front(&mut self) {
		self.buckets.pop_front();
		self.first += 1;
		for (level, &size) in self.blocks.iter_mut().zip(&BLOCKS) {
			while !level.totals.is_empty() && (level.first + 1) * size <= self.first {
				level.totals.pop_front();
				level.first += 1;
			}
		}
	}

	fn cap(&self) -> i64 {
		if self.keep.is_finite() {
			(self.keep / self.len).ceil() as i64 + 1
		} else {
			i64::MAX
		}
	}

	fn add(&mut self, t0: f64, t1: f64, v: &[f32; N]) {
		let at0 = (t0 / self.len).floor() as i64;
		let at1 = ((t1 / self.len).ceil() as i64 - 1).max(at0);
		let cap = self.cap();
		// only the newest cap buckets stay, so none older is made
		let keep_from = at1.saturating_sub(cap - 1);
		if self.buckets.is_empty() || self.first + self.buckets.len() as i64 <= keep_from {
			// nothing kept would survive the gap
			self.buckets.clear();
			self.first = at0.max(keep_from);
		}
		while self.first + (self.buckets.len() as i64) <= at1 {
			self.buckets.push_back(Bucket::default());
		}
		for at in at0.max(self.first)..=at1 {
			let start = at as f64 * self.len;
			let lo = t0.max(start) - start;
			let hi = t1.min(start + self.len) - start;
			if hi > lo {
				self.buckets[(at - self.first) as usize].add(lo, hi, v);
			}
		}
		self.reindex((at0.max(self.first) - self.first) as usize);
		while self.buckets.len() as i64 > cap {
			self.pop_front();
		}
	}

	fn start(&self) -> f64 {
		self.first as f64 * self.len
	}
}

/// What a span of time averaged out to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span<const N: usize> {
	/// Time-weighted mean of each value.
	pub mean: [f32; N],
	/// The most of each value.
	pub max: [f32; N],
	/// Share of the span that had data, 0 to 1.
	pub covered: f32,
}

/// One graph's samples, at a finer resolution for recent ones.
#[derive(Clone, Debug)]
pub struct History<const N: usize> {
	tiers: Vec<Tier<N>>,
	first: Option<f64>,
	last: Option<f64>,
}

impl<const N: usize> Default for History<N> {
	fn default() -> Self {
		History {
			tiers: TIERS
				.iter()
				.map(|&(len, keep)| Tier::new(len, keep))
				.collect(),
			first: None,
			last: None,
		}
	}
}

impl<const N: usize> History<N> {
	/// Values that held from `t0` to `t1`. Every tier gets them, so none waits
	/// on a roll-up. Time already covered is skipped.
	pub fn add(&mut self, t0: f64, t1: f64, v: &[f32; N]) {
		let t0 = self.last.map_or(t0, |l| t0.max(l));
		if t1 <= t0 || !t0.is_finite() || !t1.is_finite() {
			return;
		}
		for tier in &mut self.tiers {
			tier.add(t0, t1, v);
		}
		self.first.get_or_insert(t0);
		self.last = Some(t1);
	}

	/// Start of the oldest data.
	pub fn first(&self) -> Option<f64> {
		self.first
	}

	/// End of the newest data.
	pub fn last(&self) -> Option<f64> {
		self.last
	}

	/// The finest tier still holding time `a`, or the coarsest one.
	fn tier_for(&self, a: f64) -> &Tier<N> {
		self.tiers
			.iter()
			.find(|t| !t.buckets.is_empty() && t.start() <= a)
			.unwrap_or(&self.tiers[self.tiers.len() - 1])
	}

	/// The time from `a` to `b`, each bucket weighted by how much of its data
	/// falls in the span. Gaps count for nothing.
	pub fn span(&self, a: f64, b: f64) -> Span<N> {
		let mut out = Span {
			mean: [0.0; N],
			max: [0.0; N],
			covered: 0.0,
		};
		if b <= a {
			return out;
		}
		let tier = self.tier_for(a);
		if tier.buckets.is_empty() {
			return out;
		}
		let i0 = ((a / tier.len).floor() as i64 - tier.first).max(0);
		let i1 = ((b / tier.len).ceil() as i64 - tier.first).min(tier.buckets.len() as i64);
		let mut weight = 0.0f64;
		let mut sums = [0.0f64; N];
		let mut max = [f32::NEG_INFINITY; N];
		// only the end buckets can be cut by the span
		if i1 - i0 > 2 {
			let whole = tier.total_over(i0 as usize + 1, i1 as usize - 1);
			(weight, sums, max) = (whole.weight, whole.sums, whole.max);
		}
		let ends = if i1 - i0 > 1 { [i0, i1 - 1] } else { [i0, i1] };
		for i in ends.into_iter().filter(|&i| i < i1) {
			let bk = &tier.buckets[i as usize];
			if bk.secs <= 0.0 {
				continue;
			}
			let start = (tier.first + i) as f64 * tier.len;
			let (lo, hi) = (start + bk.lo, start + bk.hi);
			let overlap = b.min(hi) - a.max(lo);
			if overlap <= 0.0 {
				continue;
			}
			// a bucket with a hole in it is taken as evenly thin
			let w = overlap * (bk.secs as f64 / (hi - lo)).min(1.0);
			weight += w;
			for s in 0..N {
				sums[s] += w * bk.mean(s) as f64;
				max[s] = max[s].max(bk.max[s]);
			}
		}
		if weight > 0.0 {
			for s in 0..N {
				out.mean[s] = (sums[s] / weight) as f32;
				out.max[s] = max[s];
			}
			out.covered = (weight / (b - a)).min(1.0) as f32;
		}
		out
	}

	/// Drops every bucket that ends by `before`, in every tier.
	pub fn trim(&mut self, before: f64) {
		if self.last.is_none_or(|l| l <= before) {
			*self = History::default();
			return;
		}
		for tier in &mut self.tiers {
			while !tier.buckets.is_empty() && tier.start() + tier.len <= before {
				tier.pop_front();
			}
		}
		self.first = self.first.map(|f| f.max(before));
	}

	/// Appends the history as bytes, for [`History::decode`].
	pub fn encode(&self, out: &mut Vec<u8>) {
		let f64s = |out: &mut Vec<u8>, x: f64| out.extend_from_slice(&x.to_le_bytes());
		let f32s = |out: &mut Vec<u8>, xs: &[f32]| {
			for x in xs {
				out.extend_from_slice(&x.to_le_bytes());
			}
		};
		out.extend_from_slice(&(N as u32).to_le_bytes());
		f64s(out, self.first.unwrap_or(f64::NAN));
		f64s(out, self.last.unwrap_or(f64::NAN));
		out.extend_from_slice(&(self.tiers.len() as u32).to_le_bytes());
		for tier in &self.tiers {
			f64s(out, tier.len);
			out.extend_from_slice(&tier.first.to_le_bytes());
			out.extend_from_slice(&(tier.buckets.len() as u32).to_le_bytes());
			for b in &tier.buckets {
				f32s(out, &b.sum);
				f32s(out, &b.min);
				f32s(out, &b.max);
				f32s(out, &[b.secs]);
				f64s(out, b.lo);
				f64s(out, b.hi);
			}
		}
	}

	/// None for anything [`History::encode`] wouldn't have written with
	/// today's tiers, or with today's less the finest ones, so a stale or
	/// broken file starts fresh. Also None for another series count. A count
	/// of 0 is a section from before counts were saved, which only its
	/// layout can refuse.
	///
	/// Times out of order, or buckets outside them, are None too. A bucket
	/// with a value that isn't finite comes back as a gap.
	pub fn decode(bytes: &[u8]) -> Option<History<N>> {
		let mut rd = Reader(bytes);
		let series = rd.u32()? as usize;
		if series != 0 && series != N {
			return None;
		}
		let first = Some(rd.f64()?).filter(|x| x.is_finite());
		let last = Some(rd.f64()?).filter(|x| x.is_finite());
		let span = match (first, last) {
			(Some(f), Some(l)) if (0.0..=l).contains(&f) && l < LATEST => Some((f, l)),
			(None, None) => None,
			_ => return None,
		};
		let mut h = History::<N> {
			first,
			last,
			..History::default()
		};
		// a file from before a finer tier went in has none of it
		let count = rd.u32()? as usize;
		let skip = h.tiers.len().checked_sub(count)?;
		for tier in &mut h.tiers[skip..] {
			if rd.f64()? != tier.len {
				return None;
			}
			tier.first = rd.i64()?;
			let count = rd.u32()? as i64;
			if count > tier.cap() {
				return None;
			}
			// every bucket lies over first to last, give or take one for rounding
			if count > 0 {
				let (f, l) = span?;
				let from = (f / tier.len).floor() as i64 - 1;
				let to = (l / tier.len).ceil() as i64 + 1;
				if tier.first < from || tier.first.checked_add(count)? > to {
					return None;
				}
			}
			for _ in 0..count {
				let b = Bucket {
					sum: rd.f32s()?,
					min: rd.f32s()?,
					max: rd.f32s()?,
					secs: rd.f32s::<1>()?[0],
					lo: rd.f64()?,
					hi: rd.f64()?,
				};
				tier.buckets
					.push_back(if b.sane() { b } else { Bucket::default() });
			}
			tier.reindex(0);
		}
		rd.0.is_empty().then_some(h)
	}
}

/// Unix seconds past this are taken as a broken file, and it keeps every
/// bucket number far inside an i64. Year 5138.
const LATEST: f64 = 1e11;

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
	fn take<const K: usize>(&mut self) -> Option<[u8; K]> {
		let (head, rest) = self.0.split_first_chunk::<K>()?;
		self.0 = rest;
		Some(*head)
	}
	fn f64(&mut self) -> Option<f64> {
		self.take().map(f64::from_le_bytes)
	}
	fn i64(&mut self) -> Option<i64> {
		self.take().map(i64::from_le_bytes)
	}
	fn u32(&mut self) -> Option<u32> {
		self.take().map(u32::from_le_bytes)
	}
	fn f32s<const K: usize>(&mut self) -> Option<[f32; K]> {
		let mut v = [0.0; K];
		for x in &mut v {
			*x = f32::from_le_bytes(self.take()?);
		}
		Some(v)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn close(a: f32, b: f32) -> bool {
		(a - b).abs() < 1e-5
	}

	// Test ID: dejcp
	#[test]
	fn a_spike_reads_in_a_wide_span_only_while_its_bucket_is_kept() {
		let mut h = History::<1>::default();
		let t0 = 1000.0;
		let spike = t0 + 100.0;
		let mut t = t0;
		let step = |h: &mut History<1>, t: &mut f64| {
			let v = if (*t - spike).abs() < 0.01 { 0.9 } else { 0.1 };
			h.add(*t, *t + 0.05, &[v]);
			*t += 0.05;
		};
		while t < t0 + 200.0 {
			step(&mut h, &mut t);
		}
		assert!(close(h.span(t0 + 10.0, t0 + 190.0).max[0], 0.9));
		assert!(close(h.span(t0 + 101.0, t0 + 190.0).max[0], 0.1));
		// partway into the blocks at either end
		assert!(close(h.span(t0 + 10.0, spike + 0.2).max[0], 0.9));
		assert!(close(h.span(spike - 0.2, t0 + 190.0).max[0], 0.9));
		// the fine tier lets the spike go, but keeps the rest of its blocks
		let at = (spike / 0.05).round() as i64;
		while h.tiers[0].first <= at {
			step(&mut h, &mut t);
		}
		let kept = h.tiers[0].buckets.len() as i64;
		for (level, &size) in h.tiers[0].blocks.iter().zip(&BLOCKS) {
			assert_eq!(h.tiers[0].first.div_euclid(size), at.div_euclid(size));
			// what the tier lets go, its blocks go with
			assert!(level.totals.len() as i64 <= kept / size + 2);
		}
		let a = h.tiers[0].start();
		assert!(close(h.span(a, a + 60.0).max[0], 0.1));
		let mut bytes = Vec::new();
		h.encode(&mut bytes);
		let back = History::<1>::decode(&bytes).unwrap();
		assert!(close(back.span(a, a + 60.0).max[0], 0.1));
		assert!(close(back.span(a, a + 60.0).mean[0], 0.1));
		// a new spike after a gap the fine tier can't keep
		t += 1000.0;
		let spike = t + 30.0;
		for _ in 0..1200 {
			let v = if (t - spike).abs() < 0.01 { 0.9 } else { 0.1 };
			h.add(t, t + 0.05, &[v]);
			t += 0.05;
		}
		let a = h.tiers[0].start();
		assert!(close(h.span(a, a + 50.0).max[0], 0.9));
		assert!(close(h.span(a, a + 29.0).max[0], 0.1));
	}

	// Test ID: debhw
	#[test]
	fn mean_and_max_of_a_span() {
		let mut h = History::<1>::default();
		for (i, v) in [0.2f32, 0.4, 0.6, 0.8].iter().enumerate() {
			let t = 100.0 + i as f64 * 0.25;
			h.add(t, t + 0.25, &[*v]);
		}
		let s = h.span(100.0, 101.0);
		assert!(close(s.mean[0], 0.5));
		assert!(close(s.max[0], 0.8));
		assert!(close(s.covered, 1.0));
	}

	// Test ID: debhx
	#[test]
	fn a_long_sample_fills_every_bucket_it_covers() {
		// half-second samples on quarter-second buckets leave no holes
		let mut h = History::<1>::default();
		h.add(10.0, 10.5, &[0.3]);
		h.add(10.5, 11.0, &[0.7]);
		for k in 0..4 {
			let a = 10.0 + k as f64 * 0.25;
			let s = h.span(a, a + 0.05);
			assert!(close(s.covered, 1.0), "bucket {k}");
		}
		assert!(close(h.span(10.0, 11.0).mean[0], 0.5));
	}

	// Test ID: debhy
	#[test]
	fn part_buckets_count_in_part() {
		let mut h = History::<1>::default();
		h.add(10.0, 10.25, &[0.0]);
		h.add(10.25, 10.5, &[1.0]);
		// a quarter of the first bucket, all of the second
		let s = h.span(10.1875, 10.5);
		assert!(close(s.mean[0], 0.8));
	}

	// Test ID: debhz
	#[test]
	fn the_newest_part_bucket_is_full_where_it_has_data() {
		let mut h = History::<1>::default();
		h.add(10.0, 10.1, &[0.4]);
		let s = h.span(10.05, 10.1);
		assert!(close(s.covered, 1.0));
		assert!(close(s.mean[0], 0.4));
		assert_eq!(h.span(10.1, 10.2).covered, 0.0);
	}

	// Test ID: debj0
	#[test]
	fn gaps_are_left_out_not_zero() {
		let mut h = History::<1>::default();
		h.add(0.0, 0.25, &[0.5]);
		h.add(9.0, 9.25, &[0.5]);
		let s = h.span(0.0, 9.25);
		assert!(close(s.mean[0], 0.5));
		assert!(s.covered < 0.1);
		assert_eq!(h.span(3.0, 4.0).covered, 0.0);
		// the 2 s tier keeps the gap as empty buckets
		let s = h.tiers[2].buckets.len();
		assert_eq!(s, 5);
		// a 2 s bucket with a hole in it, read once the finer tiers let it go
		let mut holed = History::<1>::default();
		holed.add(0.0, 0.25, &[0.5]);
		holed.add(1.5, 1.75, &[0.5]);
		holed.add(3000.0, 3000.25, &[0.5]);
		assert!(holed.tiers[1].start() > 2.0);
		let s = holed.span(0.0, 1.75);
		assert!(close(s.mean[0], 0.5));
		assert!(close(s.covered, 0.5 / 1.75), "{}", s.covered);
	}

	// Test ID: debj1
	#[test]
	fn old_buckets_age_out_of_the_fine_tier() {
		let mut h = History::<1>::default();
		for i in 0..10000 {
			let t = i as f64 * 0.25;
			h.add(t, t + 0.25, &[0.5]);
		}
		assert!(h.tiers[1].buckets.len() as f64 <= 2048.0 / 0.25 + 1.0);
		// an old span falls through to the 2 s tier and still reads
		let s = h.span(1.0, 3.0);
		assert!(close(s.mean[0], 0.5));
		assert!(close(s.covered, 1.0));
	}

	// Test ID: debj2
	#[test]
	fn a_long_gap_resets_a_tier_instead_of_filling_it() {
		let mut h = History::<1>::default();
		h.add(0.0, 0.5, &[0.1]);
		let t = 86400.0 * 10.0;
		h.add(t, t + 0.5, &[0.9]);
		assert_eq!(h.tiers[1].buckets.len(), 2);
		assert_eq!(h.tiers[3].buckets.len(), 1);
		// the 1 hour tier keeps the gap as empty buckets
		assert_eq!(h.tiers[5].buckets.len(), 241);
		assert_eq!(h.first(), Some(0.0));
	}

	// Test ID: debj3
	#[test]
	fn overlap_and_backwards_time_are_dropped() {
		let mut h = History::<1>::default();
		h.add(4.0, 5.0, &[0.1]);
		h.add(3.0, 4.5, &[0.9]);
		assert_eq!(h.last(), Some(5.0));
		h.add(4.5, 5.5, &[0.9]);
		assert!(close(h.span(4.0, 5.0).max[0], 0.1));
		assert!(close(h.span(5.0, 5.5).mean[0], 0.9));
	}

	// Test ID: defp6
	#[test]
	fn a_linear_graph_as_wide_as_the_slider_goes_keeps_a_sample_a_column() {
		let (_, widest) = crate::config::slider_range("width").unwrap();
		let columns = widest as usize;
		// up to the rate where the 2 s tier would do anyway
		for rate in [0.05, 0.1, 0.2, 0.5, 1.0, 1.75] {
			let mut h = History::<1>::default();
			for i in 0..columns {
				let t = i as f64 * rate;
				h.add(t, t + rate, &[(i % 2) as f32]);
			}
			for i in [0, 1, 2, columns / 2] {
				let t = i as f64 * rate;
				let s = h.span(t, t + rate);
				assert!(close(s.mean[0], (i % 2) as f32), "{rate} s, column {i}");
			}
		}
	}

	fn a_day_of_samples() -> History<2> {
		let mut h = History::<2>::default();
		let t0 = 1.76e9;
		for i in 0..2000 {
			let t = t0 + i as f64 * 0.5;
			h.add(t, t + 0.5, &[(i % 7) as f32 / 7.0, 0.25]);
		}
		// an hour off, then a day later
		h.add(t0 + 4600.0, t0 + 4601.0, &[0.9, 0.1]);
		h.add(t0 + 90000.0, t0 + 90002.0, &[0.3, 0.3]);
		h
	}

	// Test ID: decbg
	#[test]
	fn encode_and_decode_give_back_the_same_history() {
		let h = a_day_of_samples();
		let mut bytes = Vec::new();
		h.encode(&mut bytes);
		let back = History::<2>::decode(&bytes).unwrap();
		let mut again = Vec::new();
		back.encode(&mut again);
		assert_eq!(bytes, again);
		assert_eq!((back.first(), back.last()), (h.first(), h.last()));
		let t0 = 1.76e9;
		for (a, b) in [(0.0, 1.0), (100.0, 400.0), (0.0, 90002.0), (4000.0, 5000.0)] {
			assert_eq!(back.span(t0 + a, t0 + b), h.span(t0 + a, t0 + b));
		}
		assert!(History::<2>::decode(&[]).is_none());
		let mut empty = Vec::new();
		History::<2>::default().encode(&mut empty);
		let e = History::<2>::decode(&empty).unwrap();
		assert_eq!((e.first(), e.last()), (None, None));
	}

	// Test ID: decbh
	#[test]
	fn decode_refuses_what_encode_wouldnt_write() {
		let h = a_day_of_samples();
		let mut bytes = Vec::new();
		h.encode(&mut bytes);
		// cut short, a byte over, or saved with another series count
		for n in [0, 7, 16, 20, 100, bytes.len() - 1] {
			assert!(History::<2>::decode(&bytes[..n]).is_none(), "cut at {n}");
		}
		let mut long = bytes.clone();
		long.push(0);
		assert!(History::<2>::decode(&long).is_none());
		assert!(History::<3>::decode(&bytes).is_none());
		// tiers that don't match today's
		let mut other = bytes.clone();
		other[24..32].copy_from_slice(&0.5f64.to_le_bytes());
		assert!(History::<2>::decode(&other).is_none());
		// a bucket count past what the file has
		let mut over = bytes.clone();
		over[40..44].copy_from_slice(&9000u32.to_le_bytes());
		assert!(History::<2>::decode(&over).is_none());
		// one bucket past what a full tier keeps, though the file has it
		let mut full = History::<2>::default();
		for i in 0..600 {
			let t = 1.76e9 + f64::from(i) * 0.5;
			full.add(t, t + 0.5, &[0.5, 0.25]);
		}
		assert_eq!(full.tiers[0].buckets.len() as i64, full.tiers[0].cap());
		assert!((full.tiers[1].buckets.len() as i64) < full.tiers[1].cap());
		let mut bytes = Vec::new();
		full.encode(&mut bytes);
		assert!(History::<2>::decode(&one_more(&bytes, 0)).is_none());
		assert!(History::<2>::decode(&one_more(&bytes, 1)).is_some());
	}

	// a copy of tier `k`'s first bucket put in front of it, and the
	// history's start moved back to cover it
	fn one_more(bytes: &[u8], k: usize) -> Vec<u8> {
		const BUCKET: usize = 4 * (3 * 2 + 1) + 2 * 8;
		let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
		let mut at = 24;
		for _ in 0..k {
			at += 20 + u32_at(at + 16) as usize * BUCKET;
		}
		let first = i64::from_le_bytes(bytes[at + 8..at + 16].try_into().unwrap());
		let mut out = bytes.to_vec();
		out[4..12].copy_from_slice(&0.0f64.to_le_bytes());
		out[at + 8..at + 16].copy_from_slice(&(first - 1).to_le_bytes());
		out[at + 16..at + 20].copy_from_slice(&(u32_at(at + 16) + 1).to_le_bytes());
		let copy = bytes[at + 20..at + 20 + BUCKET].to_vec();
		out.splice(at + 20..at + 20, copy);
		out
	}

	// Test ID: demwf
	#[test]
	fn another_series_count_is_refused_by_its_count() {
		// no buckets, so nothing but the count tells 3 series from 5
		let mut bytes = Vec::new();
		History::<3>::default().encode(&mut bytes);
		assert_eq!(bytes[..4], 3u32.to_le_bytes());
		assert!(History::<5>::decode(&bytes).is_none());
		assert!(History::<3>::decode(&bytes).is_some());
		// a version 1 file's section has no count, and goes by its layout
		let mut old = bytes.clone();
		old[..4].copy_from_slice(&0u32.to_le_bytes());
		assert!(History::<5>::decode(&old).is_some());
		let h = a_day_of_samples();
		let mut bytes = Vec::new();
		h.encode(&mut bytes);
		bytes[..4].copy_from_slice(&0u32.to_le_bytes());
		let back = History::<2>::decode(&bytes).unwrap();
		assert_eq!((back.first(), back.last()), (h.first(), h.last()));
		assert!(History::<3>::decode(&bytes).is_none());
	}

	// Test ID: defp7
	#[test]
	fn a_file_from_before_the_finest_tier_still_loads() {
		let h = a_day_of_samples();
		let mut bytes = Vec::new();
		h.encode(&mut bytes);
		// cut tier 0 out, as a file from when there was one less
		let count = u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize;
		let mut old = bytes[..20].to_vec();
		old.extend_from_slice(&(TIERS.len() as u32 - 1).to_le_bytes());
		old.extend_from_slice(&bytes[44 + count * 44..]);
		let back = History::<2>::decode(&old).unwrap();
		assert!(back.tiers[0].buckets.is_empty());
		assert_eq!((back.first(), back.last()), (h.first(), h.last()));
		let t0 = 1.76e9;
		for (a, b) in [(100.0, 400.0), (0.0, 90002.0), (4000.0, 5000.0)] {
			assert_eq!(back.span(t0 + a, t0 + b), h.span(t0 + a, t0 + b));
		}
		// a tier more than today's isn't
		let mut more = bytes.clone();
		more[20..24].copy_from_slice(&(TIERS.len() as u32 + 1).to_le_bytes());
		assert!(History::<2>::decode(&more).is_none());
	}

	// Test ID: decbj
	#[test]
	fn trim_drops_only_what_is_older() {
		let mut h = a_day_of_samples();
		let t0 = 1.76e9;
		let before = h.span(t0 + 4600.0, t0 + 90002.0);
		h.trim(t0 + 1000.0);
		assert_eq!(h.first(), Some(t0 + 1000.0));
		// a coarse bucket across the cut keeps its part, so look before it
		assert_eq!(h.span(t0, t0 + 399.0).covered, 0.0);
		assert_eq!(h.span(t0 + 4600.0, t0 + 90002.0), before);
		assert!(
			h.tiers
				.iter()
				.all(|t| t.buckets.is_empty() || t.start() + t.len > t0 + 1000.0)
		);
		h.trim(t0 + 1e6);
		assert_eq!((h.first(), h.last()), (None, None));
	}

	// Test ID: deqqq
	#[test]
	fn a_long_add_makes_no_more_buckets_than_a_tier_keeps() {
		let mut h = History::<4>::default();
		let t1 = 1.76e9;
		h.add(t1 - 3.0 * 86400.0, t1, &[0.5; 4]);
		for t in h.tiers.iter().filter(|t| t.keep.is_finite()) {
			let cap = t.cap() as usize;
			assert!(t.buckets.len() <= cap, "{} s tier", t.len);
			assert!(t.buckets.capacity() <= 2 * cap, "{} s tier", t.len);
			for (level, &size) in t.blocks.iter().zip(&BLOCKS) {
				assert!(level.totals.capacity() <= 2 * cap / size as usize + 2);
			}
		}
		let s = h.span(t1 - 200.0, t1);
		assert!(s.covered > 0.999 && (s.mean[0] - 0.5).abs() < 1e-6, "{s:?}");
		assert!(h.span(t1 - 2.0 * 86400.0, t1 - 86400.0).covered > 0.999);
	}

	// Test ID: deqs5
	#[test]
	fn times_out_of_order_are_refused_and_a_bucket_that_isnt_finite_is_a_gap() {
		let h = a_day_of_samples();
		let mut bytes = Vec::new();
		h.encode(&mut bytes);
		let at = |b: &[u8], i: usize| f64::from_le_bytes(b[i..i + 8].try_into().unwrap());
		let (first, last) = (at(&bytes, 4), at(&bytes, 12));
		let with = |i: usize, x: &[u8]| {
			let mut b = bytes.clone();
			b[i..i + x.len()].copy_from_slice(x);
			History::<2>::decode(&b)
		};
		// last before first, a high bit flipped in it, first before 1970
		assert!(with(12, &(first - 1.0).to_le_bytes()).is_none());
		let flipped = f64::from_bits(last.to_bits() ^ (1 << 62));
		assert!(with(12, &flipped.to_le_bytes()).is_none());
		assert!(with(4, &(-1.0f64).to_le_bytes()).is_none());
		// tier 0's buckets past last, well before first, or near the top of i64
		let t0 = i64::from_le_bytes(bytes[32..40].try_into().unwrap());
		for bad in [t0 + 10, t0 - 3_000_000, i64::MAX - 3] {
			assert!(with(32, &bad.to_le_bytes()).is_none(), "{bad}");
		}
		// tier 0's first bucket, with a NaN mean or an inf max
		for (i, x) in [(44, f32::NAN), (44 + 16 + 4, f32::INFINITY)] {
			let back = with(i, &x.to_le_bytes()).unwrap();
			assert_eq!(back.tiers[0].buckets[0], Bucket::default());
			assert_eq!(back.tiers[0].buckets[1], h.tiers[0].buckets[1]);
			let a = back.tiers[0].start();
			let s = back.span(a, a + 1.0);
			assert!(s.mean.iter().chain(&s.max).all(|x| x.is_finite()), "{s:?}");
		}
		// a history with no times has no buckets
		let mut empty = Vec::new();
		History::<2>::default().encode(&mut empty);
		let mut one = History::<2>::default();
		one.add(first, first + 0.05, &[0.5; 2]);
		let mut timed = Vec::new();
		one.encode(&mut timed);
		timed[4..20].copy_from_slice(&empty[4..20]);
		assert!(History::<2>::decode(&timed).is_none());
	}

	fn same_total(a: &Total<2>, b: &Total<2>) -> bool {
		let near = |x: f64, y: f64| (x - y).abs() <= 1e-9 * (1.0 + x.abs().max(y.abs()));
		near(a.weight, b.weight)
			&& (0..2).all(|s| near(a.sums[s], b.sums[s]) && a.max[s] == b.max[s])
	}

	// every block a tier still has whole matches its buckets, and so does any
	// run of buckets read through the blocks
	fn blocks_match_buckets(h: &History<2>, rand: &mut impl FnMut() -> f64) {
		for t in &h.tiers {
			let end = t.first + t.buckets.len() as i64;
			let plain = |a0: i64, a1: i64| {
				let mut out = Total::zero();
				t.add_buckets(a0, a1, &mut out);
				out
			};
			for (level, &size) in t.blocks.iter().zip(&BLOCKS) {
				if !t.buckets.is_empty() {
					let last = level.first + level.totals.len() as i64 - 1;
					assert_eq!(last, (end - 1).div_euclid(size), "{} s tier, {size}", t.len);
				}
				// what the tier lets go, its blocks go with
				assert!(level.totals.len() as i64 <= t.buckets.len() as i64 / size + 2);
				for (b, total) in (level.first..).zip(&level.totals) {
					if b * size >= t.first {
						let want = plain(b * size, ((b + 1) * size).min(end));
						assert!(
							same_total(total, &want),
							"{} s tier, block {b} of {size}",
							t.len
						);
					}
				}
			}
			let n = t.buckets.len();
			for _ in 0..20.min(n) {
				let i0 = (rand() * n as f64) as usize;
				let i1 = i0 + (rand() * (n - i0) as f64) as usize;
				let want = plain(t.first + i0 as i64, t.first + i1 as i64);
				assert!(
					same_total(&t.total_over(i0, i1), &want),
					"{} s tier, {i0} to {i1}",
					t.len
				);
			}
		}
	}

	// Test ID: der45
	#[test]
	fn block_totals_add_up_to_their_buckets_through_gaps_trims_and_reloads() {
		let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
		let mut rand = move || {
			seed ^= seed << 13;
			seed ^= seed >> 7;
			seed ^= seed << 17;
			(seed >> 11) as f64 / (1u64 << 53) as f64
		};
		let mut h = History::<2>::default();
		let mut t = 1.76e9;
		for round in 0..600 {
			// mostly short samples, now and then a long one or a gap
			let r = rand();
			let len = if r < 0.9 {
				0.03 + rand() * 0.5
			} else {
				rand() * 3000.0
			};
			if r > 0.97 {
				t += rand() * 20000.0;
			}
			h.add(t, t + len, &[rand() as f32, rand() as f32]);
			t += len;
			if round % 40 == 39 {
				blocks_match_buckets(&h, &mut rand);
			}
			if round % 100 == 99 {
				h.trim(t - rand() * 3000.0);
				blocks_match_buckets(&h, &mut rand);
			}
			if round % 130 == 129 {
				let mut bytes = Vec::new();
				h.encode(&mut bytes);
				h = History::<2>::decode(&bytes).unwrap();
				blocks_match_buckets(&h, &mut rand);
			}
		}
	}
}
