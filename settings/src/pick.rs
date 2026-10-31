// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

// The color picker's model, the same as SilkTerm's: a saturation and
// brightness square, a hue strip, and value boxes, all over one HSV color.

use tukzmon_core::config::Rgb;

/// Hue 0 to 1 (wrapping), saturation and brightness 0 to 1. The picker holds
/// this rather than the bytes, since a gray has no hue to read back and black
/// has no saturation either.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hsv {
	pub h: f64,
	pub s: f64,
	pub v: f64,
}

/// The fully lit color at this hue, 0 to 1 per channel.
fn hue_rgb(h: f64) -> [f64; 3] {
	let k = h.rem_euclid(1.0) * 6.0;
	[
		((k - 3.0).abs() - 1.0).clamp(0.0, 1.0),
		(2.0 - (k - 2.0).abs()).clamp(0.0, 1.0),
		(2.0 - (k - 4.0).abs()).clamp(0.0, 1.0),
	]
}

pub fn to_rgb(c: Hsv) -> Rgb {
	let hue = hue_rgb(c.h);
	let (s, v) = (c.s.clamp(0.0, 1.0), c.v.clamp(0.0, 1.0));
	let chan = |x: f64| ((1.0 - s + s * x) * v * 255.0).round().clamp(0.0, 255.0) as u8;
	Rgb(chan(hue[0]), chan(hue[1]), chan(hue[2]))
}

/// `keep` gives what the bytes can't, so typing 0 into Brightness doesn't
/// reset the hue and saturation just set.
pub fn from_rgb(c: Rgb, keep: Hsv) -> Hsv {
	let [r, g, b] = [c.0, c.1, c.2].map(|x| f64::from(x) / 255.0);
	let max = r.max(g).max(b);
	let span = max - r.min(g).min(b);
	let h = if span <= 0.0 {
		keep.h
	} else if max == r {
		((g - b) / span).rem_euclid(6.0) / 6.0
	} else if max == g {
		((b - r) / span + 2.0) / 6.0
	} else {
		((r - g) / span + 4.0) / 6.0
	};
	Hsv {
		h,
		s: if max <= 0.0 { keep.s } else { span / max },
		v: max,
	}
}

pub fn hex(c: Rgb) -> String {
	format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
}

/// The value boxes, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
	Red,
	Green,
	Blue,
	Brightness,
	Saturation,
	Hex,
}

impl Field {
	pub const ALL: [Field; 6] = [
		Field::Red,
		Field::Green,
		Field::Blue,
		Field::Brightness,
		Field::Saturation,
		Field::Hex,
	];

	pub fn label(self) -> &'static str {
		match self {
			Field::Red => "Red %",
			Field::Green => "Green %",
			Field::Blue => "Blue %",
			Field::Brightness => "Brightness %",
			Field::Saturation => "Saturation %",
			Field::Hex => "Hex",
		}
	}

	fn channel(self, c: &mut Rgb) -> Option<&mut u8> {
		match self {
			Field::Red => Some(&mut c.0),
			Field::Green => Some(&mut c.1),
			Field::Blue => Some(&mut c.2),
			Field::Brightness | Field::Saturation | Field::Hex => None,
		}
	}

	/// The whole percent the box shows while nobody types in it, or None
	/// for the hex box.
	pub fn percent(self, c: Hsv) -> Option<i32> {
		let mut rgb = to_rgb(c);
		let x = match self {
			Field::Hex => return None,
			Field::Brightness => c.v,
			Field::Saturation => c.s,
			Field::Red | Field::Green | Field::Blue => self
				.channel(&mut rgb)
				.map_or(0.0, |x| f64::from(*x) / 255.0),
		};
		Some((x * 100.0).round().clamp(0.0, 100.0) as i32)
	}

	/// A typed value read back into the color. None for one that says nothing
	/// yet, such as a half-typed hex or an empty box.
	pub fn apply(self, c: Hsv, typed: &str) -> Option<Hsv> {
		match self {
			Field::Hex => {
				let t = typed.trim();
				let t = t.strip_prefix('#').unwrap_or(t);
				Rgb::parse(&format!("#{t}")).map(|rgb| from_rgb(rgb, c))
			}
			Field::Brightness => pct(typed).map(|v| Hsv { v, ..c }),
			Field::Saturation => pct(typed).map(|s| Hsv { s, ..c }),
			Field::Red | Field::Green | Field::Blue => {
				let p = pct(typed)?;
				let mut rgb = to_rgb(c);
				*self.channel(&mut rgb)? = (p * 255.0).round() as u8;
				Some(from_rgb(rgb, c))
			}
		}
	}
}

fn pct(typed: &str) -> Option<f64> {
	let v = typed.trim().parse::<f64>().ok()?;
	v.is_finite().then(|| (v / 100.0).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
	use super::*;

	fn hsv(h: f64, s: f64, v: f64) -> Hsv {
		Hsv { h, s, v }
	}

	// Every edit crosses this both ways, so a color that shifted on the way
	// would creep each time the picker opened.
	// Test ID: decd5
	#[test]
	fn a_color_survives_the_trip_through_hsv() {
		let seed = hsv(0.25, 0.5, 0.5);
		for r in (0..=255).step_by(5) {
			for g in (0..=255).step_by(5) {
				for b in (0..=255).step_by(5) {
					let want = Rgb(r as u8, g as u8, b as u8);
					assert_eq!(to_rgb(from_rgb(want, seed)), want);
				}
			}
		}
	}

	// Test ID: decd6
	#[test]
	fn a_gray_keeps_the_hue_and_saturation_it_had() {
		let held = hsv(0.6, 0.8, 0.4);
		let gray = from_rgb(Rgb(128, 128, 128), held);
		assert_eq!(gray.h, held.h);
		assert_eq!(gray.s, 0.0);
		let black = from_rgb(Rgb(0, 0, 0), held);
		assert_eq!((black.h, black.s, black.v), (held.h, held.s, 0.0));
	}

	// Test ID: decd7
	#[test]
	fn the_hue_ramp_hits_the_primaries() {
		for (h, want) in [
			(0.0, Rgb(255, 0, 0)),
			(1.0 / 6.0, Rgb(255, 255, 0)),
			(2.0 / 6.0, Rgb(0, 255, 0)),
			(3.0 / 6.0, Rgb(0, 255, 255)),
			(4.0 / 6.0, Rgb(0, 0, 255)),
			(5.0 / 6.0, Rgb(255, 0, 255)),
			(1.0, Rgb(255, 0, 0)),
		] {
			assert_eq!(to_rgb(hsv(h, 1.0, 1.0)), want, "hue {h}");
		}
		// the square's corners
		assert_eq!(to_rgb(hsv(0.3, 0.0, 1.0)), Rgb(255, 255, 255));
		assert_eq!(to_rgb(hsv(0.3, 1.0, 0.0)), Rgb(0, 0, 0));
	}

	// Test ID: decd8
	#[test]
	fn a_typed_value_reaches_what_it_names() {
		let start = from_rgb(Rgb(10, 20, 30), hsv(0.0, 0.0, 0.0));
		let red = Field::Red.apply(start, "100").unwrap();
		assert_eq!(to_rgb(red), Rgb(255, 20, 30));
		assert_eq!(
			to_rgb(Field::Green.apply(start, "0").unwrap()),
			Rgb(10, 0, 30)
		);
		assert_eq!(
			to_rgb(Field::Hex.apply(start, "#a0b1c2").unwrap()),
			Rgb(0xa0, 0xb1, 0xc2)
		);
		assert_eq!(
			to_rgb(Field::Hex.apply(start, "A0B1C2").unwrap()),
			Rgb(0xa0, 0xb1, 0xc2),
			"the # is optional, as in the row's own box"
		);
		assert_eq!(Field::Brightness.apply(start, "250").unwrap().v, 1.0);
		// half-typed says nothing yet, rather than jumping to black
		assert!(Field::Hex.apply(start, "#a0b").is_none());
		assert!(Field::Red.apply(start, "").is_none());
		assert!(Field::Red.apply(start, "inf").is_none());
		assert_eq!(
			Field::Blue.percent(from_rgb(Rgb(0, 0, 255), start)),
			Some(100)
		);
		assert_eq!(Field::Hex.percent(from_rgb(Rgb(0, 0, 255), start)), None);
		assert_eq!(hex(to_rgb(from_rgb(Rgb(0, 0, 255), start))), "#0000ff");
		assert_eq!(Field::Saturation.percent(hsv(0.5, 0.42, 1.0)), Some(42));
	}
}
