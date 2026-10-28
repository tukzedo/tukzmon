// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! Stacked column graphs into a pixel buffer. Pixels are cairo's ARGB32:
//! premultiplied, native-endian `0xAARRGGBB`. Only a transparent background
//! is less than opaque.

use crate::axis::Ages;
use crate::config::{Look, Rgb};
use crate::history::History;

/// Seconds a column can cover before its spike emphasis is down by half.
const SPIKE_FADE: f64 = 20.0;

/// A column with less data than this is a gap.
pub(crate) const MIN_COVER: f32 = 0.5;

fn mix(a: Rgb, b: Rgb, t: f32) -> [f32; 3] {
	let m = |x: u8, y: u8| x as f32 + (y as f32 - x as f32) * t;
	[m(a.0, b.0), m(a.1, b.1), m(a.2, b.2)]
}

/// `c` already premultiplied by `alpha`, 0 to 1.
fn pack_alpha(c: [f32; 3], alpha: f32) -> u32 {
	let q = |v: f32| v.round().clamp(0.0, 255.0) as u32;
	q(alpha * 255.0) << 24 | q(c[0]) << 16 | q(c[1]) << 8 | q(c[2])
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

/// A vertical line `thick` pixels wide from column `x`, over the plot only, so
/// a pointer on the border puts it at the plot's edge. `bw` is the border.
pub fn line(px: &mut [u32], w: usize, h: usize, bw: usize, x: usize, thick: usize, color: Rgb) {
	if px.len() < w * h || w <= 2 * bw || h <= 2 * bw {
		return;
	}
	let (pw, ph) = (w - 2 * bw, h - 2 * bw);
	let thick = thick.clamp(1, pw);
	let x0 = x.clamp(bw, bw + pw - thick);
	let c = color.argb();
	for row in px[bw * w..(bw + ph) * w].chunks_exact_mut(w) {
		row[x0..x0 + thick].fill(c);
	}
}

/// Text a front end writes over a graph in the panel's own font, at the
/// plot's top left. See design.md Code layout.
#[derive(Clone, Debug, PartialEq)]
pub struct Overlay {
	pub text: String,
	pub color: Rgb,
	/// A rim round each letter, so it reads over any column.
	pub edge: Rgb,
	/// The plot, in device pixels from the drawing's top left. Text that
	/// doesn't fit in it is left out.
	pub x: usize,
	pub y: usize,
	pub w: usize,
	pub h: usize,
}

/// How a column is colored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Paint<'a, const N: usize> {
	/// A color per series, stacked from the bottom.
	Stack(&'a [Rgb; N]),
	/// Low, Medium and High. The whole column takes the blend at each
	/// pixel's height in the plot, so the colors stay put as it grows.
	Gradient([Rgb; 3]),
}

/// The gradient's color `t` of the way up, 0 to 1.
pub fn blend(stops: [Rgb; 3], t: f32) -> [f32; 3] {
	let t = t.clamp(0.0, 1.0) * 2.0;
	if t <= 1.0 {
		mix(stops[0], stops[1], t)
	} else {
		mix(stops[1], stops[2], t - 1.0)
	}
}

pub struct Frame<'a, const N: usize> {
	pub hist: &'a History<N>,
	pub axis: &'a dyn Ages,
	/// Time at the right edge.
	pub now: f64,
	pub look: &'a Look,
	pub paint: Paint<'a, N>,
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
		// premultiplied, so a transparent one is all zeros
		let back_alpha = if self.look.transparent { 0.0 } else { 1.0 };
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
				.map(|v| v * back_alpha)
			})
			.collect();
		let cols: Vec<[f32; 3]> = match self.paint {
			Paint::Stack(colors) => colors
				.iter()
				.map(|c| [c.0 as f32, c.1 as f32, c.2 as f32])
				.collect(),
			Paint::Gradient(_) => Vec::new(),
		};
		// row by row, top first, at each row's middle
		let grad: Vec<[f32; 3]> = match self.paint {
			Paint::Gradient(stops) => (0..ph)
				.map(|r| blend(stops, ((ph - r) as f32 - 0.5) / ph as f32))
				.collect(),
			Paint::Stack(_) => Vec::new(),
		};
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
				let mut alpha = back_alpha;
				if let Some(hs) = &heights {
					// this pixel's slice of the stack, measured up from the bottom
					let (p0, p1) = ((ph - r - 1) as f32, (ph - r) as f32);
					let mut y = 0.0f32;
					let mut left = 1.0f32;
					let mut acc = [0.0f32; 3];
					if let Some(g) = grad.get(r) {
						let top = hs.iter().sum::<f32>() * ph as f32;
						let cover = (top.min(p1) - p0).clamp(0.0, 1.0);
						for k in 0..3 {
							c[k] = c[k] * (1.0 - cover) + g[k] * cover;
						}
						alpha = alpha * (1.0 - cover) + cover;
						px[(r + bw) * w + cx + bw] = pack_alpha(c, alpha);
						continue;
					}
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
					alpha = alpha * left + (1.0 - left);
				}
				px[(r + bw) * w + cx + bw] = pack_alpha(c, alpha);
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::axis::{Axis, Curve};

	fn pack(c: [f32; 3]) -> u32 {
		pack_alpha(c, 1.0)
	}

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
			paint: Paint::Stack(&COLORS),
		};
		let mut px = Vec::new();
		f.draw(&mut px, w, ht, 1);
		px
	}

	// Test ID: debj6
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

	// Test ID: debj7
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

	// Test ID: debj8
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

	// Test ID: debj9
	#[test]
	fn spike_reach_fades_with_time() {
		let near = column(&[0.1], &[0.9], 0.5, 1.0)[0];
		let far = column(&[0.1], &[0.9], 3600.0, 1.0)[0];
		assert!(near > 0.8);
		assert!(far < 0.11);
		assert_eq!(column(&[0.1], &[0.9], 0.5, 0.0)[0], 0.1);
	}

	// Test ID: debja
	#[test]
	fn an_overfull_stack_is_scaled_to_fit() {
		let v = column(&[0.5, 0.4], &[1.0, 1.0], 0.0, 1.0);
		assert!((v[0] + v[1] - 1.0).abs() < 1e-6);
		assert!(v[0] == v[1]);
	}

	// Test ID: decc9
	#[test]
	fn the_hover_line_stays_off_the_border() {
		let back = 0xff11_1111;
		let draw_at = |x: usize, thick: usize| {
			let mut px = vec![back; 8 * 4];
			line(&mut px, 8, 4, 1, x, thick, Rgb(0, 0, 0));
			px
		};
		let px = draw_at(3, 2);
		for r in 0..4 {
			let want = |x: usize| (1..3).contains(&r) && (3..5).contains(&x);
			for x in 0..8 {
				assert_eq!(px[r * 8 + x] == 0xff00_0000, want(x), "{x},{r}");
			}
		}
		// pointer on the border, or past it, puts it at the plot's edge
		assert_eq!(draw_at(0, 1)[8 + 1], 0xff00_0000);
		assert_eq!(draw_at(7, 2)[8 + 5], 0xff00_0000);
		assert_eq!(draw_at(99, 2)[8 + 6], 0xff00_0000);
		assert_eq!(draw_at(99, 2)[8 + 7], back);
		// too small to have a plot is left alone
		let mut px = vec![back; 4];
		line(&mut px, 2, 2, 1, 0, 1, Rgb(0, 0, 0));
		assert!(px.iter().all(|&p| p == back));
	}

	// Test ID: decdk
	#[test]
	fn the_gradient_goes_by_height_not_by_series() {
		const LO: Rgb = Rgb(200, 0, 0);
		const MID: Rgb = Rgb(255, 255, 200);
		const HI: Rgb = Rgb(100, 200, 255);
		// a quarter and a half, as in stack_heights_are_exact, on a 10 px plot
		let h = steady([0.25, 0.5], 120.0);
		let l = look();
		let axis = Axis::log(Curve::FixedA, 120.0);
		let paint = |p: Paint<'_, 2>| {
			let f = Frame {
				hist: &h,
				axis: &axis,
				now: 120.0,
				look: &l,
				paint: p,
			};
			let mut px = Vec::new();
			f.draw(&mut px, 22, 12, 1);
			px
		};
		let px = paint(Paint::Gradient([LO, MID, HI]));
		let at = |row: usize| px[(row + 1) * 22 + 12];
		let want = |row: usize| pack(blend([LO, MID, HI], ((10 - row) as f32 - 0.5) / 10.0));
		// rows from the bottom of the plot up to 7.5 take the blend at their height
		for row in 3..10 {
			assert_eq!(at(row), want(row), "{row}");
		}
		assert_ne!(at(9), at(8));
		// the row the column ends in is half blend, half background
		let half = at(2);
		assert_ne!(half, want(2));
		assert_ne!(half, at(0));
		// above it is background, the same as the stack
		let stack = paint(Paint::Stack(&COLORS));
		assert_eq!(at(0), stack[22 + 12]);
		assert_eq!(at(1), stack[2 * 22 + 12]);
		// a full column runs Low at the bottom through Medium to High at the top
		let full = steady([1.0, 0.0], 120.0);
		let f = Frame {
			hist: &full,
			axis: &axis,
			now: 120.0,
			look: &l,
			paint: Paint::Gradient([LO, MID, HI]),
		};
		let mut px = Vec::new();
		f.draw(&mut px, 22, 12, 1);
		let red = |row: usize| (px[(row + 1) * 22 + 12] >> 16) & 0xff;
		let blue = |row: usize| px[(row + 1) * 22 + 12] & 0xff;
		assert!(red(9) > 200 && blue(9) < 25, "{:x}", px[10 * 22 + 12]);
		assert!(blue(0) > 240 && red(0) < 120, "{:x}", px[22 + 12]);
		assert!((0..9).all(|r| blue(r) >= blue(r + 1)));
	}

	// Test ID: decdm
	#[test]
	fn the_blend_hits_its_stops() {
		let stops = [Rgb(10, 20, 30), Rgb(100, 110, 120), Rgb(200, 210, 220)];
		assert_eq!(pack(blend(stops, 0.0)), 0xff0a_141e);
		assert_eq!(pack(blend(stops, 0.5)), 0xff64_6e78);
		assert_eq!(pack(blend(stops, 1.0)), 0xffc8_d2dc);
		assert_eq!(pack(blend(stops, 0.25)), pack(mix(stops[0], stops[1], 0.5)));
		assert_eq!(blend(stops, -1.0), blend(stops, 0.0));
		assert_eq!(blend(stops, 2.0), blend(stops, 1.0));
	}

	// Test ID: decdz
	#[test]
	fn a_transparent_background_lets_the_panel_through() {
		let h = steady([0.25, 0.5], 120.0);
		let axis = Axis::log(Curve::FixedA, 120.0);
		let l = Look {
			transparent: true,
			..look()
		};
		let f = |hist: &History<2>, paint: Paint<'_, 2>| {
			let f = Frame {
				hist,
				axis: &axis,
				now: 120.0,
				look: &l,
				paint,
			};
			let mut px = Vec::new();
			f.draw(&mut px, 22, 12, 1);
			px
		};
		let opaque = draw(&h, &axis, 22, 12);
		for paint in [
			Paint::Stack(&COLORS),
			Paint::Gradient([Rgb(200, 0, 0), Rgb(255, 255, 200), Rgb(100, 200, 255)]),
		] {
			let px = f(&h, paint);
			let at = |row: usize| px[(row + 1) * 22 + 12];
			// border stays, the empty top of the plot is clear
			assert_eq!(px[0], 0xff09_0909);
			assert_eq!(at(0), 0);
			assert_eq!(at(1), 0);
			// the column is opaque, and the stack the same as on a background
			assert_eq!(at(9) >> 24, 0xff);
			assert_eq!(at(4) >> 24, 0xff);
			if let Paint::Stack(_) = paint {
				assert_eq!(at(9), opaque[10 * 22 + 12]);
				assert_eq!(at(5), opaque[6 * 22 + 12]);
			}
			// its top row is half covered, so half clear, and premultiplied
			let edge = at(2);
			let a = edge >> 24;
			assert!((120..136).contains(&a), "{edge:x}");
			assert!(
				[16, 8, 0].iter().all(|s| (edge >> s) & 0xff <= a),
				"{edge:x}"
			);
		}
		// a gap is clear too
		let mut gap = History::<2>::default();
		gap.add(0.0, 30.0, &[1.0, 0.0]);
		gap.add(90.0, 120.0, &[1.0, 0.0]);
		let f = Frame {
			hist: &gap,
			axis: &Axis::linear(120.0),
			now: 120.0,
			look: &l,
			paint: Paint::Stack(&COLORS),
		};
		let mut px = Vec::new();
		f.draw(&mut px, 122, 6, 1);
		assert_eq!(px[3 * 122 + 61], 0);
		assert_eq!(px[3 * 122 + 11], 0xffff_0000);
	}

	// Test ID: debjb
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
			paint: Paint::Stack(&COLORS),
		};
		let mut px = Vec::new();
		f.draw(&mut px, 4, 4, 1);
		assert_eq!(px[3 * 4 + 2], 0xffff_0000);
	}
}
