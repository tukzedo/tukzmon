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
/// Each fine tier holds a linear graph at the width slider's top, at any rate
/// the next tier is too coarse for.
pub const TIERS: [(f64, f64); 6] = [
	(0.05, 256.0),
	(0.25, 2048.0),
	(2.0, 3.0 * 3600.0),
	(30.0, 3.0 * 86400.0),
	(300.0, 90.0 * 86400.0),
	(3600.0, f64::INFINITY),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bucket<const N: usize> {
	/// Each value times the seconds it lasted.
	pub sum: [f32; N],
	pub min: [f32; N],
	pub max: [f32; N],
	/// Seconds of data in the bucket. 0 is a gap.
	pub secs: f32,
	/// Where the data starts and ends, in seconds from the bucket's start.
	pub lo: f64,
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
	pub fn mean(&self, i: usize) -> f32 {
		if self.secs > 0.0 {
			self.sum[i] / self.secs
		} else {
			0.0
		}
	}
}

/// Buckets a total in [`Tier::blocks`] covers.
const BLOCK: i64 = 32;

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

#[derive(Clone, Debug)]
struct Tier<const N: usize> {
	len: f64,
	keep: f64,
	/// Bucket number of `buckets[0]`, counted from time 0.
	first: i64,
	buckets: VecDeque<Bucket<N>>,
	/// The total of each block of [`BLOCK`] buckets, so a wide span reads a
	/// block at a time. Only a block whose buckets are all still kept is read.
	blocks: VecDeque<Total<N>>,
	/// Block number of `blocks[0]`, counted from time 0.
	block_first: i64,
}

impl<const N: usize> Tier<N> {
	fn new(len: f64, keep: f64) -> Tier<N> {
		Tier {
			len,
			keep,
			first: 0,
			buckets: VecDeque::new(),
			blocks: VecDeque::new(),
			block_first: 0,
		}
	}

	/// The buckets from `i0` to `i1`, one by one, each over its whole data.
	/// Absolute times, as a span works them out.
	fn total(&self, i0: usize, i1: usize) -> Total<N> {
		let mut out = Total::zero();
		for i in i0..i1 {
			let bk = &self.buckets[i];
			if bk.secs <= 0.0 {
				continue;
			}
			let start = (self.first + i as i64) as f64 * self.len;
			let (lo, hi) = (start + bk.lo, start + bk.hi);
			if hi <= lo {
				continue;
			}
			let w = (hi - lo) * (bk.secs as f64 / (hi - lo)).min(1.0);
			out.weight += w;
			for s in 0..N {
				out.sums[s] += w * bk.mean(s) as f64;
				out.max[s] = out.max[s].max(bk.max[s]);
			}
		}
		out
	}

	/// As [`Self::total`], whole blocks read from `blocks`.
	fn total_over(&self, i0: usize, i1: usize) -> Total<N> {
		let (a0, a1) = (self.first + i0 as i64, self.first + i1 as i64);
		let (b0, b1) = ((a0 + BLOCK - 1).div_euclid(BLOCK), a1.div_euclid(BLOCK));
		if b1 <= b0 {
			return self.total(i0, i1);
		}
		let at = |a: i64| (a - self.first) as usize;
		let mut out = self.total(i0, at(b0 * BLOCK));
		for b in self
			.blocks
			.range((b0 - self.block_first) as usize..(b1 - self.block_first) as usize)
		{
			out.join(b);
		}
		out.join(&self.total(at(b1 * BLOCK), i1));
		out
	}

	/// Works out the blocks again from the one holding bucket `from` on.
	fn reindex(&mut self, from: usize) {
		if self.buckets.is_empty() {
			self.blocks.clear();
			return;
		}
		let mut b0 = (self.first + from as i64).div_euclid(BLOCK);
		if from == 0 || self.blocks.is_empty() {
			self.blocks.clear();
			self.block_first = b0;
		}
		// buckets pushed for a gap come before the ones a sample touched
		b0 = b0.min(self.block_first + self.blocks.len() as i64);
		self.blocks.truncate((b0 - self.block_first) as usize);
		let end = self.first + self.buckets.len() as i64;
		for b in b0..=(end - 1).div_euclid(BLOCK) {
			let lo = (b * BLOCK).max(self.first) - self.first;
			let hi = ((b + 1) * BLOCK).min(end) - self.first;
			self.blocks.push_back(self.total(lo as usize, hi as usize));
		}
	}

	fn pop_front(&mut self) {
		self.buckets.pop_front();
		self.first += 1;
		while !self.blocks.is_empty() && (self.block_first + 1) * BLOCK <= self.first {
			self.blocks.pop_front();
			self.block_first += 1;
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
		if self.buckets.is_empty()
			|| at1 - self.first >= cap.saturating_add(self.buckets.len() as i64)
		{
			// nothing kept would survive the gap
			self.buckets.clear();
			self.first = at0;
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
	pub mean: [f32; N],
	pub max: [f32; N],
	/// Share of the span that had data, 0 to 1.
	pub covered: f32,
}

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
	/// broken file starts fresh.
	pub fn decode(bytes: &[u8]) -> Option<History<N>> {
		let mut rd = Reader(bytes);
		let first = Some(rd.f64()?).filter(|x| x.is_finite());
		let last = Some(rd.f64()?).filter(|x| x.is_finite());
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
			for _ in 0..count {
				let b = Bucket {
					sum: rd.f32s()?,
					min: rd.f32s()?,
					max: rd.f32s()?,
					secs: rd.f32s::<1>()?[0],
					lo: rd.f64()?,
					hi: rd.f64()?,
				};
				tier.buckets.push_back(b);
			}
			tier.reindex(0);
		}
		(rd.0.is_empty() && first.is_some() == last.is_some()).then_some(h)
	}
}

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
		// the fine tier lets the spike go, but keeps the rest of its block
		let block = (spike / 0.05).round() as i64 / BLOCK;
		while h.tiers[0].first <= (spike / 0.05).round() as i64 {
			step(&mut h, &mut t);
		}
		assert_eq!(h.tiers[0].first.div_euclid(BLOCK), block);
		// what the tier lets go, its blocks go with
		let kept = h.tiers[0].buckets.len() as i64;
		assert!(h.tiers[0].blocks.len() as i64 <= kept / BLOCK + 2);
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
		// the 2 s tier has a bucket with a hole in it, read as thin
		let s = h.tiers[2].buckets.len();
		assert_eq!(s, 5);
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
		other[20..28].copy_from_slice(&0.5f64.to_le_bytes());
		assert!(History::<2>::decode(&other).is_none());
		// a bucket count past what the fine tier keeps
		let mut over = bytes.clone();
		over[36..40].copy_from_slice(&9000u32.to_le_bytes());
		assert!(History::<2>::decode(&over).is_none());
	}

	// Test ID: defp7
	#[test]
	fn a_file_from_before_the_finest_tier_still_loads() {
		let h = a_day_of_samples();
		let mut bytes = Vec::new();
		h.encode(&mut bytes);
		// cut tier 0 out, as a file from when there was one less
		let count = u32::from_le_bytes(bytes[36..40].try_into().unwrap()) as usize;
		let mut old = bytes[..16].to_vec();
		old.extend_from_slice(&(TIERS.len() as u32 - 1).to_le_bytes());
		old.extend_from_slice(&bytes[40 + count * 44..]);
		let back = History::<2>::decode(&old).unwrap();
		assert!(back.tiers[0].buckets.is_empty());
		assert_eq!((back.first(), back.last()), (h.first(), h.last()));
		let t0 = 1.76e9;
		for (a, b) in [(100.0, 400.0), (0.0, 90002.0), (4000.0, 5000.0)] {
			assert_eq!(back.span(t0 + a, t0 + b), h.span(t0 + a, t0 + b));
		}
		// a tier more than today's isn't
		let mut more = bytes.clone();
		more[16..20].copy_from_slice(&(TIERS.len() as u32 + 1).to_le_bytes());
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
}
