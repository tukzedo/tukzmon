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

/// How one graph spreads time across its width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timing {
	Linear,
	Log(Curve),
}

impl Timing {
	/// As a graph's own `<graph>_time` row spells it. None, for `same` or
	/// anything else, means the graph follows the Time settings.
	pub fn parse(key: &str) -> Option<Timing> {
		match key {
			"linear" => Some(Timing::Linear),
			_ => Curve::parse(key).map(Timing::Log),
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

	pub fn of(basis: &Basis) -> Axis {
		match *basis {
			Basis::Log { curve, span } => Axis::log(curve, span.map_or(0.0, |(a, b)| b - a)),
			Basis::Linear(s) => Axis::linear(s),
		}
	}
}

/// Seconds a jump in the axis takes to ease in.
pub const EASE: f64 = 0.5;

/// How finely the axis drawn when a jump came is kept, in steps of `u`.
const STEPS: usize = 64;

/// Anything that says how old the data at `u` is: an axis, or one easing in.
pub trait Ages {
	fn age(&self, u: f64) -> f64;
}

impl Ages for Axis {
	fn age(&self, u: f64) -> f64 {
		Axis::age(self, u)
	}
}

/// What an axis is built from. A change that isn't just time going on is a
/// jump, such as history loaded, a gap ending, or a new curve or width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Basis {
	/// A log curve over history from its first to its last time, if any.
	Log {
		curve: Curve,
		span: Option<(f64, f64)>,
	},
	/// The whole width covers this many seconds.
	Linear(f64),
}

impl Basis {
	/// `dt` is how far the clock moved since `self`, and `slack` how far the
	/// newest sample may trail it.
	fn jumps_to(&self, next: &Basis, dt: f64, slack: f64) -> bool {
		match (self, next) {
			(Basis::Linear(a), Basis::Linear(b)) => a != b,
			(
				Basis::Log {
					curve: c0,
					span: Some((f0, l0)),
				},
				Basis::Log {
					curve: c1,
					span: Some((f1, l1)),
				},
			) => c0 != c1 || f0 != f1 || l1 < l0 || l1 - l0 > dt + slack,
			// the first sample isn't a jump, there was nothing drawn
			(Basis::Log { curve: c0, .. }, Basis::Log { curve: c1, .. }) => c0 != c1,
			(Basis::Linear(_), Basis::Log { .. }) | (Basis::Log { .. }, Basis::Linear(_)) => true,
		}
	}
}

fn smoothstep(x: f64) -> f64 {
	let x = x.clamp(0.0, 1.0);
	x * x * (3.0 - 2.0 * x)
}

#[derive(Clone, Debug)]
struct Drawn {
	basis: Basis,
	axis: Axis,
	at: f64,
}

/// Eases a graph's axis over [`EASE`] after a jump, from what was on screen
/// to where it should be. Time going on needs none, since the axis already
/// moves smoothly then. See design.md, Time axis.
#[derive(Clone, Debug, Default)]
pub struct Ease {
	/// `g` at even steps of `u`, as drawn when the jump came, and when.
	from: Option<(Box<[f64; STEPS + 1]>, f64)>,
	drawn: Option<Drawn>,
}

impl Ease {
	/// Takes the axis about to be drawn at time `t`, and starts easing if it
	/// jumped from the last one.
	pub fn update(&mut self, axis: &Axis, basis: Basis, t: f64, slack: f64) {
		if let Some(d) = &self.drawn
			&& d.basis.jumps_to(&basis, t - d.at, slack)
		{
			let was = self.view(&d.axis, d.at);
			let g = std::array::from_fn(|i| to_g(was.age(i as f64 / STEPS as f64)));
			self.from = Some((Box::new(g), t));
		}
		if self.from.as_ref().is_some_and(|(_, at)| t - at >= EASE) {
			self.from = None;
		}
		self.drawn = Some(Drawn {
			basis,
			axis: axis.clone(),
			at: t,
		});
	}

	/// The axis to draw at time `t`, with `to` where it's headed.
	pub fn view<'a>(&'a self, to: &'a Axis, t: f64) -> Eased<'a> {
		let from = self.from.as_ref().and_then(|(g, at)| {
			let s = smoothstep((t - at) / EASE);
			(s < 1.0).then_some((&**g, s))
		});
		Eased { from, to }
	}

	pub fn easing(&self, t: f64) -> bool {
		self.from.as_ref().is_some_and(|(_, at)| t - at < EASE)
	}
}

/// An axis part way from a jump, mixed in `ln(1 + age / A0)`, so time still
/// only runs older to the left.
#[derive(Clone, Copy, Debug)]
pub struct Eased<'a> {
	from: Option<(&'a [f64; STEPS + 1], f64)>,
	to: &'a Axis,
}

impl Ages for Eased<'_> {
	fn age(&self, u: f64) -> f64 {
		let Some((g, s)) = self.from else {
			return self.to.age(u);
		};
		let x = u.clamp(0.0, 1.0) * STEPS as f64;
		let i = (x as usize).min(STEPS - 1);
		let was = g[i] + (g[i + 1] - g[i]) * (x - i as f64);
		let mixed = was + (to_g(self.to.age(u)) - was) * s;
		(A0 * mixed.exp_m1()).max(0.0)
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
		let Some(s) = spec.get("time_axis") else {
			panic!("no time_axis in settings.shcl");
		};
		// linear, then the curves
		assert_eq!(s.options.len(), CURVES.len() + 1);
		assert_eq!(Timing::parse(&s.options[0].0), Some(Timing::Linear));
		for ((key, _), curve) in s.options.iter().skip(1).zip(CURVES) {
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

	const SLACK: f64 = 1.25;

	fn log(curve: Curve, first: f64, last: f64) -> Basis {
		Basis::Log {
			curve,
			span: Some((first, last)),
		}
	}

	fn step(e: &mut Ease, b: Basis, t: f64) {
		e.update(&Axis::of(&b), b, t, SLACK);
	}

	// Test ID: defpn
	#[test]
	fn a_jump_eases_in_from_what_was_drawn() {
		for curve in CURVES {
			let mut e = Ease::default();
			let short = log(curve, 0.0, 10.0);
			let was = Axis::of(&short);
			step(&mut e, short, 10.0);
			assert!(!e.easing(10.0));
			// days of history from disk
			let long = log(curve, -3.0 * DAY, 10.2);
			let to = Axis::of(&long);
			step(&mut e, long, 10.2);
			assert!(e.easing(10.2), "{curve:?}");
			let v = e.view(&to, 10.2);
			for u in [0.1, 0.3, 0.5, 0.8, 1.0] {
				assert!(near(v.age(u), was.age(u), 1e-3), "{curve:?} at {u}");
			}
			let v = e.view(&to, 10.2 + EASE / 2.0);
			let mut older = 0.0;
			for i in 0..=400 {
				let age = v.age(f64::from(i) / 400.0);
				assert!(age >= older, "{curve:?} went back at {i}");
				older = age;
			}
			assert!(
				older > was.age(1.0) * 2.0 && older < to.age(1.0) / 2.0,
				"{curve:?}"
			);
			let done = 10.2 + EASE;
			step(&mut e, long, done);
			assert!(!e.easing(done));
			assert_eq!(e.view(&to, done).age(0.7), to.age(0.7));
		}
	}

	// Test ID: defpp
	#[test]
	fn time_going_on_is_no_jump_but_a_gap_or_new_axis_is() {
		let mut e = Ease::default();
		step(
			&mut e,
			Basis::Log {
				curve: Curve::Smooth,
				span: None,
			},
			0.0,
		);
		let mut t = 0.0;
		while t < 100.0 {
			t += 0.5;
			step(&mut e, log(Curve::Smooth, 0.0, t), t);
			assert!(!e.easing(t), "{t}");
		}
		// a draw for the pointer between ticks, a late tick, a missed one
		for (last, at) in [(100.0, 100.4), (100.9, 100.9), (101.9, 101.9)] {
			step(&mut e, log(Curve::Smooth, 0.0, last), at);
			assert!(!e.easing(at), "{at}");
		}
		step(&mut e, log(Curve::FixedA, 0.0, 102.0), 102.0);
		assert!(e.easing(102.0), "a new curve");
		step(&mut e, Basis::Linear(64.0), 103.0);
		assert!(e.easing(103.0), "to linear");
		step(&mut e, Basis::Linear(64.0), 104.0);
		assert!(!e.easing(104.0));
		step(&mut e, Basis::Linear(100.0), 104.5);
		assert!(e.easing(104.5), "a new width");

		// loaded, with the clock well past it, then the priming sample, then a gap
		let mut e = Ease::default();
		step(&mut e, log(Curve::Smooth, -50.0, 100.0), 1000.0);
		step(&mut e, log(Curve::Smooth, -50.0, 100.0), 1000.5);
		assert!(!e.easing(1000.5));
		step(&mut e, log(Curve::Smooth, -50.0, 1001.0), 1001.0);
		assert!(e.easing(1001.0), "a gap");
	}

	// Test ID: defpq
	#[test]
	fn a_jump_while_easing_starts_from_what_is_on_screen() {
		let mut e = Ease::default();
		step(&mut e, log(Curve::Smooth, 0.0, 10.0), 10.0);
		let b = log(Curve::FixedA, -DAY, 10.1);
		step(&mut e, b, 10.1);
		step(&mut e, b, 10.3);
		let mid = Axis::of(&b);
		let on_screen: Vec<f64> = (0..=20)
			.map(|i| e.view(&mid, 10.3).age(f64::from(i) / 20.0))
			.collect();
		let c = Basis::Linear(64.0);
		step(&mut e, c, 10.3);
		let to = Axis::of(&c);
		for (i, was) in on_screen.iter().enumerate() {
			let age = e.view(&to, 10.3).age(i as f64 / 20.0);
			// as kept, in steps of u along ln(1 + age / A0)
			assert!(near(to_g(age), to_g(*was), 1e-3), "at {i}: {age} vs {was}");
		}
	}
}
