// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! Stacked column graphs into a pixel buffer. Pixels are cairo's ARGB32:
//! premultiplied, native-endian `0xAARRGGBB`, and always opaque here.

use crate::axis::Axis;
use crate::config::{Look, Rgb};
use crate::history::History;

/// Seconds a column can cover before its spike emphasis is down by half.
const SPIKE_FADE: f64 = 20.0;

/// A column with less data than this is a gap.
const MIN_COVER: f32 = 0.5;

fn mix(a: Rgb, b: Rgb, t: f32) -> [f32; 3] {
	let m = |x: u8, y: u8| x as f32 + (y as f32 - x as f32) * t;
	[m(a.0, b.0), m(a.1, b.1), m(a.2, b.2)]
}

fn pack(c: [f32; 3]) -> u32 {
	let q = |v: f32| v.round().clamp(0.0, 255.0) as u32;
	0xff00_0000 | q(c[0]) << 16 | q(c[1]) << 8 | q(c[2])
}

/// Heights of each series in a column, 0 to 1, bottom first. Squeezed columns
/// reach part way from their mean to their peak, less the more time they
/// cover. A stack over 1 is scaled down to fit.
pub fn column<const N: usize>(mean: &[f32; N], max: &[f32; N], secs: f64, spikes: f32) -> [f32; N] {
	let reach = spikes * (1.0 / (1.0 + secs / SPIKE_FADE)) as f32;
	let mut v: [f32; N] =
		std::array::from_fn(|i| (mean[i] + (max[i] - mean[i]).max(0.0) * reach).clamp(0.0, 1.0));
	let total: f32 = v.iter().sum();
	if total > 1.0 {
		for x in &mut v {
			*x /= total;
		}
	}
	v
}

pub struct Frame<'a, const N: usize> {
	pub hist: &'a History<N>,
	pub axis: &'a Axis,
	/// Time at the right edge.
	pub now: f64,
	pub look: &'a Look,
	pub colors: &'a [Rgb; N],
}

impl<const N: usize> Frame<'_, N> {
	/// Draw into `px`, `w` by `h`, rows top first. `scale` multiplies the
	/// border for high-DPI screens.
	pub fn draw(&self, px: &mut Vec<u32>, w: usize, h: usize, scale: usize) {
		px.clear();
		px.resize(w * h, self.look.border_color.argb());
		let bw = (self.look.border as usize * scale).min(w / 2).min(h / 2);
		let (pw, ph) = (w - 2 * bw, h - 2 * bw);
		if pw == 0 || ph == 0 {
			return;
		}
		let back: Vec<[f32; 3]> = (0..ph)
			.map(|r| {
				mix(
					self.look.top,
					self.look.bottom,
					if ph > 1 {
						r as f32 / (ph - 1) as f32
					} else {
						0.0
					},
				)
			})
			.collect();
		let cols: Vec<[f32; 3]> = self
			.colors
			.iter()
			.map(|c| [c.0 as f32, c.1 as f32, c.2 as f32])
			.collect();
		for cx in 0..pw {
			let u_new = (pw - cx - 1) as f64 / pw as f64;
			let u_old = (pw - cx) as f64 / pw as f64;
			let (a, b) = (
				self.now - self.axis.age(u_old),
				self.now - self.axis.age(u_new),
			);
			let span = self.hist.span(a, b);
			let heights = if span.covered >= MIN_COVER {
				Some(column(&span.mean, &span.max, b - a, self.look.spikes))
			} else {
				None
			};
			for r in 0..ph {
				let mut c = back[r];
				if let Some(hs) = &heights {
					// this pixel's slice of the stack, measured up from the bottom
					let (p0, p1) = ((ph - r - 1) as f32, (ph - r) as f32);
					let mut y = 0.0f32;
					let mut left = 1.0f32;
					let mut acc = [0.0f32; 3];
					for (s, hgt) in hs.iter().enumerate() {
						let y1 = y + hgt * ph as f32;
						let cover = (y1.min(p1) - y.max(p0)).max(0.0);
						if cover > 0.0 {
							for k in 0..3 {
								acc[k] += cols[s][k] * cover;
							}
							left -= cover;
						}
						y = y1;
					}
					let left = left.max(0.0);
					for k in 0..3 {
						c[k] = c[k] * left + acc[k];
					}
				}
				px[(r + bw) * w + cx + bw] = pack(c);
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::axis::Curve;

	fn look() -> Look {
		Look {
			border: 1,
			border_color: Rgb(9, 9, 9),
			top: Rgb(0, 0, 100),
			bottom: Rgb(0, 0, 0),
			..Look::default()
		}
	}

	const COLORS: [Rgb; 2] = [Rgb(255, 0, 0), Rgb(0, 255, 0)];

	fn steady(v: [f32; 2], secs: f64) -> History<2> {
		let mut h = History::default();
		let mut t = 0.0;
		while t < secs {
			h.add(t, t + 0.5, &v);
			t += 0.5;
		}
		h
	}

	fn draw(h: &History<2>, axis: &Axis, w: usize, ht: usize) -> Vec<u32> {
		let l = look();
		let f = Frame {
			hist: h,
			axis,
			now: h.last().unwrap_or(0.0),
			look: &l,
			colors: &COLORS,
		};
		let mut px = Vec::new();
		f.draw(&mut px, w, ht, 1);
		px
	}

	#[test]
	fn border_and_gradient_with_no_data() {
		let h = History::<2>::default();
		let px = draw(&h, &Axis::log(Curve::FixedA, 0.0), 10, 12);
		assert_eq!(px.len(), 120);
		assert_eq!(px[0], 0xff09_0909);
		assert_eq!(px[119], 0xff09_0909);
		// top inside row is the top color, bottom inside row the bottom one
		assert_eq!(px[12 + 1], 0xff00_0064);
		assert_eq!(px[10 * 10 + 1], 0xff00_0000);
	}

	#[test]
	fn stack_heights_are_exact() {
		// a quarter red, a half green, on a 10 px tall plot
		let h = steady([0.25, 0.5], 120.0);
		let px = draw(&h, &Axis::log(Curve::FixedA, 120.0), 22, 12);
		let col = 12;
		let at = |row: usize| px[(row + 1) * 22 + col];
		// rows counted from the bottom of the plot: 0..2.5 red, 2.5..7.5 green
		assert_eq!(at(9), 0xffff_0000);
		assert_eq!(at(8), 0xffff_0000);
		let shared = at(7);
		assert_eq!(shared & 0x00ff_0000, 0x0080_0000);
		assert_eq!(shared & 0x0000_ff00, 0x0000_8000);
		assert_eq!(at(5), 0xff00_ff00);
		assert_ne!(at(1) & 0xff, 0);
	}

	#[test]
	fn gaps_draw_as_background() {
		let mut h = History::<2>::default();
		h.add(0.0, 30.0, &[1.0, 0.0]);
		h.add(90.0, 120.0, &[1.0, 0.0]);
		let px = draw(&h, &Axis::linear(120.0), 122, 6);
		let mid_row = 3;
		let at = |x: usize| px[mid_row * 122 + x + 1];
		assert_eq!(at(10), 0xffff_0000);
		assert_eq!(at(110), 0xffff_0000);
		assert_ne!(at(60), 0xffff_0000);
	}

	#[test]
	fn spike_reach_fades_with_time() {
		let near = column(&[0.1], &[0.9], 0.5, 1.0)[0];
		let far = column(&[0.1], &[0.9], 3600.0, 1.0)[0];
		assert!(near > 0.8);
		assert!(far < 0.11);
		assert_eq!(column(&[0.1], &[0.9], 0.5, 0.0)[0], 0.1);
	}

	#[test]
	fn an_overfull_stack_is_scaled_to_fit() {
		let v = column(&[0.5, 0.4], &[1.0, 1.0], 0.0, 1.0);
		assert!((v[0] + v[1] - 1.0).abs() < 1e-6);
		assert!(v[0] == v[1]);
	}

	#[test]
	fn tiny_and_borderless() {
		let h = steady([0.5, 0.0], 10.0);
		assert!(
			draw(&h, &Axis::log(Curve::FixedA, 10.0), 2, 2)
				.iter()
				.all(|&p| p == 0xff09_0909)
		);
		let l = Look {
			border: 0,
			..look()
		};
		let f = Frame {
			hist: &h,
			axis: &Axis::log(Curve::FixedA, 10.0),
			now: 10.0,
			look: &l,
			colors: &COLORS,
		};
		let mut px = Vec::new();
		f.draw(&mut px, 4, 4, 1);
		assert_eq!(px[3 * 4 + 2], 0xffff_0000);
	}
}
