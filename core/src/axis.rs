// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! Where a pixel column sits in time. `u` runs from 0 at the right edge (now)
//! to 1 at the left edge, and `age` is seconds before now. See design.md, Time
//! axis.

/// Most a quarter may cover, right to left. The last one has no cap.
pub const CAPS: [f64; 3] = [60.0, 3600.0, 86400.0];

/// How hard the log curve squeezes before a cap is reached. The newest pixel
/// covers about ln(1+K)/K of the whole run, spread over the width.
const K: f64 = 50.0;

/// Seconds near now where the curve stops being log and goes flat, so the
/// newest pixels each cover a fraction of a second rather than nothing.
const A0: f64 = 5.0;

const QUARTER: f64 = 0.25;

/// Which log curve the graph follows. See design.md, Time axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Curve {
	/// `ln(1 + age/A0)` across the whole run, with no caps.
	#[default]
	Smooth,
	/// The caps over a log scaled to the run, so they pin early.
	FixedA,
	/// The caps over the smooth curve, so each pins only once it's reached.
	FixedB,
}

impl Curve {
	/// As `log_curve` in settings.shcl spells it.
	pub fn parse(key: &str) -> Option<Curve> {
		match key {
			"smooth" => Some(Curve::Smooth),
			"fixed_a" => Some(Curve::FixedA),
			"fixed_b" => Some(Curve::FixedB),
			_ => None,
		}
	}
}

#[derive(Clone, Debug, PartialEq)]
pub enum Axis {
	/// Knot values and slopes in g = ln(1 + age/A0), at u = 0, 1/4, 1/2, 3/4, 1.
	Log { g: [f64; 5], m: [f64; 5] },
	/// The whole width covers this many seconds.
	Linear(f64),
}

#[cfg(test)]
fn log_share(u: f64) -> f64 {
	((1.0 + K).powf(u) - 1.0) / K
}

fn to_g(age: f64) -> f64 {
	(1.0 + age / A0).ln()
}

impl Axis {
	/// The boundaries between quarters for a run of `total` seconds. With
	/// caps, each is the smaller of its cap and where the free curve would put
	/// it, and once a quarter is pinned, the quarters left share the rest of
	/// the run the same way.
	pub fn knots(curve: Curve, total: f64) -> [f64; 5] {
		let mut b = [0.0; 5];
		if total <= 0.0 {
			return b;
		}
		let a = match curve {
			Curve::Smooth => {
				let end = to_g(total);
				for (i, k) in b.iter_mut().enumerate().take(4) {
					*k = A0 * (end * i as f64 * QUARTER).exp_m1();
				}
				b[4] = total;
				return b;
			}
			Curve::FixedA => total / K,
			Curve::FixedB => A0,
		};
		let gr = |age: f64| (1.0 + age / a).ln();
		let end = gr(total);
		for i in 1..4 {
			let left = 1.0 - (i - 1) as f64 * QUARTER;
			let g = gr(b[i - 1]);
			let free = a * (g + (end - g) * QUARTER / left).exp_m1();
			b[i] = free.min(CAPS[i - 1]).max(b[i - 1]);
		}
		b[4] = total;
		b
	}

	pub fn log(curve: Curve, total: f64) -> Axis {
		let b = Axis::knots(curve, total);
		let g = b.map(to_g);
		Axis::Log {
			g,
			m: pchip_slopes(&g),
		}
	}

	pub fn linear(seconds: f64) -> Axis {
		Axis::Linear(seconds.max(0.0))
	}

	pub fn age(&self, u: f64) -> f64 {
		let u = u.clamp(0.0, 1.0);
		match self {
			Axis::Linear(s) => u * s,
			Axis::Log { g, m } => {
				let k = ((u / QUARTER) as usize).min(3);
				let t = (u - k as f64 * QUARTER) / QUARTER;
				let (t2, t3) = (t * t, t * t * t);
				let v = (2.0 * t3 - 3.0 * t2 + 1.0) * g[k]
					+ (t3 - 2.0 * t2 + t) * QUARTER * m[k]
					+ (-2.0 * t3 + 3.0 * t2) * g[k + 1]
					+ (t3 - t2) * QUARTER * m[k + 1];
				(A0 * v.exp_m1()).max(0.0)
			}
		}
	}
}

// Fritsch-Carlson slopes for a monotone cubic through evenly spaced knots, so
// one quarter runs into the next with no kink.
fn pchip_slopes(g: &[f64; 5]) -> [f64; 5] {
	let d: [f64; 4] = std::array::from_fn(|k| (g[k + 1] - g[k]) / QUARTER);
	let mut m = [0.0; 5];
	for k in 1..4 {
		if d[k - 1] > 0.0 && d[k] > 0.0 {
			m[k] = 2.0 / (1.0 / d[k - 1] + 1.0 / d[k]);
		}
	}
	let end = |d0: f64, d1: f64| {
		let s = (3.0 * d0 - d1) / 2.0;
		if s <= 0.0 || d0 <= 0.0 {
			0.0
		} else if d1 <= 0.0 && s > 3.0 * d0 {
			3.0 * d0
		} else {
			s
		}
	};
	m[0] = end(d[0], d[1]);
	m[4] = end(d[3], d[2]);
	m
}

#[cfg(test)]
mod tests {
	use super::*;

	const DAY: f64 = 86400.0;
	const CURVES: [Curve; 3] = [Curve::Smooth, Curve::FixedA, Curve::FixedB];

	fn near(a: f64, b: f64, tol: f64) -> bool {
		(a - b).abs() <= tol * b.abs().max(1.0)
	}

	// the first run length at which boundary i reaches its cap
	fn pins_at(curve: Curve, i: usize) -> Option<f64> {
		let mut t = 1.0;
		while t < 3.0 * 365.0 * DAY {
			if Axis::knots(curve, t)[i + 1] >= CAPS[i] {
				return Some(t);
			}
			t *= 1.001;
		}
		None
	}

	// Test ID: debh9
	#[test]
	fn ends_are_now_and_the_first_sample() {
		for curve in CURVES {
			for total in [0.0, 1.0, 30.0, 3600.0, 40.0 * DAY] {
				let a = Axis::log(curve, total);
				assert!(a.age(0.0).abs() < 1e-9);
				assert!(near(a.age(1.0), total, 1e-9), "{curve:?} {total}");
			}
		}
	}

	// Test ID: debha
	#[test]
	fn always_older_to_the_left() {
		for curve in CURVES {
			for total in [0.5, 45.0, 600.0, 7200.0, 2.0 * DAY, 60.0 * DAY, 400.0 * DAY] {
				let a = Axis::log(curve, total);
				let mut was = -1.0;
				for i in 0..=2000 {
					let age = a.age(i as f64 / 2000.0);
					assert!(age >= was, "{curve:?} {total} at {i}");
					was = age;
				}
			}
		}
	}

	// Test ID: debhb
	#[test]
	fn no_caps_early_on() {
		// 10 minutes in, the right quarter is still under its minute
		let b = Axis::knots(Curve::FixedA, 600.0);
		assert!(b[1] < 60.0);
		assert!(near(b[1], 600.0 * log_share(0.25), 1e-12));
	}

	// Test ID: debhc
	#[test]
	fn caps_pin_in_turn() {
		let b = Axis::knots(Curve::FixedA, 2.0 * 3600.0);
		assert_eq!(b[1], 60.0);
		assert!(b[2] < 3600.0);
		let b = Axis::knots(Curve::FixedA, DAY);
		assert_eq!(b[1..3], [60.0, 3600.0]);
		assert!(b[3] < DAY);
		// Log B pins far later, see caps_pin_when_the_tip_says
		for (curve, total) in [(Curve::FixedA, 30.0 * DAY), (Curve::FixedB, 300.0 * DAY)] {
			let b = Axis::knots(curve, total);
			assert_eq!(b[1..4], [60.0, 3600.0, DAY], "{curve:?}");
			// and the quarter boundaries really sit there
			let a = Axis::log(curve, total);
			for (i, cap) in CAPS.iter().enumerate() {
				assert!(near(a.age((i + 1) as f64 * 0.25), *cap, 1e-9));
			}
		}
	}

	// Test ID: decbs
	#[test]
	fn smooth_is_one_log_curve_with_no_caps() {
		for total in [10.0, 600.0, DAY, 365.0 * DAY] {
			let a = Axis::log(Curve::Smooth, total);
			for i in 0..=100 {
				let u = i as f64 / 100.0;
				let want = to_g(total) * u;
				assert!(near(to_g(a.age(u)), want, 1e-9), "{total} at {u}");
			}
		}
		// a year in, the right quarter still covers more than its minute
		assert!(Axis::knots(Curve::Smooth, 365.0 * DAY)[1] > 4.0 * 60.0);
	}

	// Test ID: decbt
	#[test]
	fn fixed_b_is_smooth_until_a_cap() {
		for total in [10.0, 600.0, DAY] {
			let b = Axis::knots(Curve::FixedB, total);
			let s = Axis::knots(Curve::Smooth, total);
			for i in 0..5 {
				assert!(near(b[i], s[i], 1e-9), "{total} knot {i}");
			}
		}
	}

	// Test ID: decbv
	#[test]
	fn caps_pin_when_the_tip_says() {
		// the log_curve tip in settings.shcl gives these
		let a = pins_at(Curve::FixedA, 0).unwrap_or(f64::MAX);
		assert!((25.0 * 60.0..35.0 * 60.0).contains(&a), "{a}");
		let a = (0..3)
			.filter_map(|i| pins_at(Curve::FixedA, i))
			.fold(0.0, f64::max);
		assert!((5.0 * DAY..8.0 * DAY).contains(&a), "{a}");
		let b = pins_at(Curve::FixedB, 0).unwrap_or(f64::MAX);
		assert!((1.4 * DAY..1.8 * DAY).contains(&b), "{b}");
		let b = (0..3)
			.filter_map(|i| pins_at(Curve::FixedB, i))
			.fold(0.0, f64::max);
		assert!((110.0 * DAY..140.0 * DAY).contains(&b), "{b}");
	}

	// Test ID: debhd
	#[test]
	fn boundaries_never_jump_as_time_goes_on() {
		for curve in CURVES {
			let mut was = Axis::knots(curve, 0.0);
			let mut t = 0.0;
			while t < 200.0 * DAY {
				t += 1.0 + t * 0.001;
				let b = Axis::knots(curve, t);
				for i in 1..4 {
					assert!(
						b[i] >= was[i] - 1e-9,
						"{curve:?} boundary {i} moved back at {t}"
					);
					assert!(
						b[i] - was[i] <= (t - was[4]) + 1e-9,
						"{curve:?} boundary {i} jumped at {t}"
					);
				}
				was = b;
			}
		}
	}

	// Test ID: debhe
	#[test]
	fn no_seam_at_quarter_boundaries() {
		// a pixel covers the same time either side of each boundary
		for curve in CURVES {
			for total in [90.0, 4000.0, 2.0 * DAY, 60.0 * DAY, 300.0 * DAY] {
				let a = Axis::log(curve, total);
				let h = 1e-6;
				for q in [0.25, 0.5, 0.75] {
					let left = (a.age(q) - a.age(q - h)) / h;
					let right = (a.age(q + h) - a.age(q)) / h;
					assert!(
						near(left, right, 1e-3),
						"{curve:?} {total} at {q}: {left} vs {right}"
					);
				}
			}
		}
	}

	// Test ID: decbw
	#[test]
	fn curves_parse_as_the_spec_spells_them() {
		let spec = crate::config::spec();
		let Some(s) = spec.get("log_curve") else {
			panic!("no log_curve in settings.shcl");
		};
		assert_eq!(s.options.len(), CURVES.len());
		for ((key, _), curve) in s.options.iter().zip(CURVES) {
			assert_eq!(Curve::parse(key), Some(curve), "{key}");
		}
		assert_eq!(Curve::parse(s.default.as_text()), Some(Curve::default()));
	}

	// Test ID: debhf
	#[test]
	fn linear_is_plain() {
		let a = Axis::linear(64.0);
		assert_eq!(a.age(0.5), 32.0);
		assert_eq!(a.age(2.0), 64.0);
	}
}
