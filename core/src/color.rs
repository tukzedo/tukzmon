// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

// How far apart 2 colors look, as CIEDE2000. The themes' tests and Randomize
// both measure with it. See design.md Theme.

use crate::config::Rgb;

/// 2 graphs' leads at least this far apart read as 2 colors at a glance.
pub const APART: f64 = 10.0;

/// CIELAB, D65.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lab {
	pub l: f64,
	pub a: f64,
	pub b: f64,
}

impl Lab {
	pub fn of(c: Rgb) -> Lab {
		let lin = |v: u8| {
			let v = f64::from(v) / 255.0;
			if v <= 0.04045 {
				v / 12.92
			} else {
				((v + 0.055) / 1.055).powf(2.4)
			}
		};
		let (r, g, b) = (lin(c.0), lin(c.1), lin(c.2));
		let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.950_47;
		let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
		let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.088_83;
		let f = |t: f64| {
			if t > 0.008_856 {
				t.cbrt()
			} else {
				7.787 * t + 16.0 / 116.0
			}
		};
		let (fx, fy, fz) = (f(x), f(y), f(z));
		Lab {
			l: 116.0 * fy - 16.0,
			a: 500.0 * (fx - fy),
			b: 200.0 * (fy - fz),
		}
	}
}

/// Sharma, Wu and Dalal's form, which their test pairs check.
pub fn ciede2000(x: Lab, y: Lab) -> f64 {
	let c7 = |c: f64| c.powi(7);
	let mean_c = f64::midpoint(x.a.hypot(x.b), y.a.hypot(y.b));
	let g = 0.5 * (1.0 - (c7(mean_c) / (c7(mean_c) + c7(25.0))).sqrt());
	let (a1, a2) = (x.a * (1.0 + g), y.a * (1.0 + g));
	let (c1, c2) = (a1.hypot(x.b), a2.hypot(y.b));
	let hue = |b: f64, a: f64| {
		if b == 0.0 && a == 0.0 {
			0.0
		} else {
			b.atan2(a).to_degrees().rem_euclid(360.0)
		}
	};
	let (h1, h2) = (hue(x.b, a1), hue(y.b, a2));
	let gray = c1 * c2 == 0.0;
	let mut dh = h2 - h1;
	if gray {
		dh = 0.0;
	} else if dh > 180.0 {
		dh -= 360.0;
	} else if dh < -180.0 {
		dh += 360.0;
	}
	let big_dh = 2.0 * (c1 * c2).sqrt() * (dh / 2.0).to_radians().sin();
	let mean_l = f64::midpoint(x.l, y.l);
	let mean_cp = f64::midpoint(c1, c2);
	let mean_h = if gray {
		h1 + h2
	} else if (h1 - h2).abs() <= 180.0 {
		f64::midpoint(h1, h2)
	} else if h1 + h2 < 360.0 {
		(h1 + h2 + 360.0) / 2.0
	} else {
		(h1 + h2 - 360.0) / 2.0
	};
	let cos = |deg: f64| deg.to_radians().cos();
	let t =
		1.0 - 0.17 * cos(mean_h - 30.0) + 0.24 * cos(2.0 * mean_h) + 0.32 * cos(3.0 * mean_h + 6.0)
			- 0.20 * cos(4.0 * mean_h - 63.0);
	let turn = 30.0 * (-((mean_h - 275.0) / 25.0).powi(2)).exp();
	let rc = 2.0 * (c7(mean_cp) / (c7(mean_cp) + c7(25.0))).sqrt();
	let sl = 1.0 + 0.015 * (mean_l - 50.0).powi(2) / (20.0 + (mean_l - 50.0).powi(2)).sqrt();
	let sc = 1.0 + 0.045 * mean_cp;
	let sh = 1.0 + 0.015 * mean_cp * t;
	let rt = -(2.0 * turn).to_radians().sin() * rc;
	let (dl, dc, dh) = ((y.l - x.l) / sl, (c2 - c1) / sc, big_dh / sh);
	(dl * dl + dc * dc + dh * dh + rt * dc * dh).sqrt()
}

pub fn apart(a: Rgb, b: Rgb) -> f64 {
	ciede2000(Lab::of(a), Lab::of(b))
}

/// The 2 that look most alike, by index, and how far apart they are.
pub fn closest(colors: &[Rgb]) -> Option<(f64, usize, usize)> {
	let labs: Vec<Lab> = colors.iter().map(|c| Lab::of(*c)).collect();
	let mut near: Option<(f64, usize, usize)> = None;
	for i in 0..labs.len() {
		for j in i + 1..labs.len() {
			let d = ciede2000(labs[i], labs[j]);
			if near.is_none_or(|(n, _, _)| d < n) {
				near = Some((d, i, j));
			}
		}
	}
	near
}

#[cfg(test)]
mod tests {
	use super::*;

	// Test ID: dejbs
	#[test]
	fn the_published_test_pairs_come_out_right() {
		// Sharma, Wu and Dalal 2005, table 1: Lab 1, Lab 2, and the distance
		// to 4 places
		#[rustfmt::skip]
		let pairs: [([f64; 3], [f64; 3], f64); 34] = [
			([50.0000,   2.6772, -79.7751], [50.0000,   0.0000, -82.7485],  2.0425),
			([50.0000,   3.1571, -77.2803], [50.0000,   0.0000, -82.7485],  2.8615),
			([50.0000,   2.8361, -74.0200], [50.0000,   0.0000, -82.7485],  3.4412),
			([50.0000,  -1.3802, -84.2814], [50.0000,   0.0000, -82.7485],  1.0000),
			([50.0000,  -1.1848, -84.8006], [50.0000,   0.0000, -82.7485],  1.0000),
			([50.0000,  -0.9009, -85.5211], [50.0000,   0.0000, -82.7485],  1.0000),
			([50.0000,   0.0000,   0.0000], [50.0000,  -1.0000,   2.0000],  2.3669),
			([50.0000,  -1.0000,   2.0000], [50.0000,   0.0000,   0.0000],  2.3669),
			([50.0000,   2.4900,  -0.0010], [50.0000,  -2.4900,   0.0009],  7.1792),
			([50.0000,   2.4900,  -0.0010], [50.0000,  -2.4900,   0.0010],  7.1792),
			([50.0000,   2.4900,  -0.0010], [50.0000,  -2.4900,   0.0011],  7.2195),
			([50.0000,   2.4900,  -0.0010], [50.0000,  -2.4900,   0.0012],  7.2195),
			([50.0000,  -0.0010,   2.4900], [50.0000,   0.0009,  -2.4900],  4.8045),
			([50.0000,  -0.0010,   2.4900], [50.0000,   0.0010,  -2.4900],  4.8045),
			([50.0000,  -0.0010,   2.4900], [50.0000,   0.0011,  -2.4900],  4.7461),
			([50.0000,   2.5000,   0.0000], [50.0000,   0.0000,  -2.5000],  4.3065),
			([50.0000,   2.5000,   0.0000], [73.0000,  25.0000, -18.0000], 27.1492),
			([50.0000,   2.5000,   0.0000], [61.0000,  -5.0000,  29.0000], 22.8977),
			([50.0000,   2.5000,   0.0000], [56.0000, -27.0000,  -3.0000], 31.9030),
			([50.0000,   2.5000,   0.0000], [58.0000,  24.0000,  15.0000], 19.4535),
			([50.0000,   2.5000,   0.0000], [50.0000,   3.1736,   0.5854],  1.0000),
			([50.0000,   2.5000,   0.0000], [50.0000,   3.2972,   0.0000],  1.0000),
			([50.0000,   2.5000,   0.0000], [50.0000,   1.8634,   0.5757],  1.0000),
			([50.0000,   2.5000,   0.0000], [50.0000,   3.2592,   0.3350],  1.0000),
			([60.2574, -34.0099,  36.2677], [60.4626, -34.1751,  39.4387],  1.2644),
			([63.0109, -31.0961,  -5.8663], [62.8187, -29.7946,  -4.0864],  1.2630),
			([61.2901,   3.7196,  -5.3901], [61.4292,   2.2480,  -4.9620],  1.8731),
			([35.0831, -44.1164,   3.7933], [35.0232, -40.0716,   1.5901],  1.8645),
			([22.7233,  20.0904, -46.6940], [23.0331,  14.9730, -42.5619],  2.0373),
			([36.4612,  47.8580,  18.3852], [36.2715,  50.5065,  21.2231],  1.4146),
			([90.8027,  -2.0831,   1.4410], [91.1528,  -1.6435,   0.0447],  1.4441),
			([90.9257,  -0.5406,  -0.9208], [88.6381,  -0.8985,  -0.7239],  1.5381),
			([ 6.7747,  -0.2908,  -2.4247], [ 5.8714,  -0.0985,  -2.2286],  0.6377),
			([ 2.0776,   0.0795,  -1.1350], [ 0.9033,  -0.0636,  -0.5514],  0.9082),
		];
		let lab = |v: [f64; 3]| Lab {
			l: v[0],
			a: v[1],
			b: v[2],
		};
		for (n, (x, y, want)) in pairs.into_iter().enumerate() {
			for got in [ciede2000(lab(x), lab(y)), ciede2000(lab(y), lab(x))] {
				assert!(
					(got - want).abs() < 5e-5,
					"pair {}: {got} not {want}",
					n + 1
				);
			}
		}
	}

	// Test ID: dejbt
	#[test]
	fn rgb_turns_into_lab_and_the_closest_pair_is_found() {
		let near = |c: Lab, l: f64, a: f64, b: f64| {
			assert!(
				(c.l - l).abs() < 0.05 && (c.a - a).abs() < 0.05 && (c.b - b).abs() < 0.05,
				"{c:?}"
			);
		};
		// published sRGB values, to 2 places
		near(Lab::of(Rgb(0, 0, 0)), 0.0, 0.0, 0.0);
		near(Lab::of(Rgb(255, 255, 255)), 100.0, 0.0, 0.0);
		near(Lab::of(Rgb(255, 0, 0)), 53.24, 80.09, 67.20);
		near(Lab::of(Rgb(0, 0, 255)), 32.30, 79.19, -107.86);
		near(Lab::of(Rgb(128, 128, 128)), 53.59, 0.0, 0.0);
		assert_eq!(apart(Rgb(1, 2, 3), Rgb(1, 2, 3)), 0.0);
		let colors = [
			Rgb(255, 0, 0),
			Rgb(0, 0, 255),
			Rgb(250, 0, 0),
			Rgb(0, 255, 0),
		];
		let (d, a, b) = closest(&colors).unwrap();
		assert_eq!((a, b), (0, 2));
		assert!(d > 0.0 && d < 2.0, "{d}");
		assert_eq!(closest(&colors[..1]), None);
	}
}
