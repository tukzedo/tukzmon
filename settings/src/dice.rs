// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

// Randomize on the Themes tab. One scheme from one seed: a lead hue for the
// user share, the quiet series that hue muted, and the ones to watch kept
// red and orange like every built-in. See design.md Theme.

use crate::pick::{self, Hsv};

/// What a theme member is for, which sets how its color is picked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
	/// The lead color, the user share.
	Lead,
	/// Nice, cache: the lead hue, muted.
	Calm,
	/// Buffers, which sit on cache: muted too, on the other side of the lead,
	/// so the 2 are told apart.
	Hush,
	/// I/O wait.
	Alarm,
	/// System, which has the interrupts in it.
	Warn,
	/// The pale end of a gradient.
	Tint,
	Border,
	Top,
	Bottom,
	Hover,
	/// A graph's mode, which Randomize always sets to Detailed.
	Mode,
}

/// Every member of the theme group, by role. A member missing here gets no
/// random value, and `dice::tests::every_theme_member_has_a_role` fails.
pub const ROLES: &[(&str, Role)] = &[
	("cpu_mode", Role::Mode),
	("iowait", Role::Alarm),
	("system", Role::Warn),
	("user", Role::Lead),
	("nice", Role::Calm),
	("cpu_low", Role::Calm),
	("cpu_medium", Role::Lead),
	("cpu_high", Role::Tint),
	("mem_mode", Role::Mode),
	("mem_user", Role::Lead),
	("mem_buffers", Role::Hush),
	("mem_cache", Role::Calm),
	("mem_low", Role::Calm),
	("mem_medium", Role::Lead),
	("mem_high", Role::Tint),
	("swap_mode", Role::Mode),
	("swap_used", Role::Lead),
	("swap_low", Role::Calm),
	("swap_medium", Role::Lead),
	("swap_high", Role::Tint),
	("battery_mode", Role::Mode),
	("battery_charging", Role::Lead),
	("battery_discharging", Role::Warn),
	("battery_critical", Role::Alarm),
	("battery_low", Role::Calm),
	("battery_medium", Role::Lead),
	("battery_high", Role::Tint),
	("border_color", Role::Border),
	("back_top", Role::Top),
	("back_bottom", Role::Bottom),
	("hover_color", Role::Hover),
];

// Lead hues stay out of the red and orange the alarms use, in turns.
const LEAD_HUES: (f64, f64) = (75.0 / 360.0, 300.0 / 360.0);
const ALARM_HUES: (f64, f64) = (-10.0 / 360.0, 2.0 / 360.0);
const WARN_HUES: (f64, f64) = (18.0 / 360.0, 32.0 / 360.0);

// splitmix64, so a seed always rolls the same theme
struct Roll(u64);

impl Roll {
	fn next(&mut self) -> u64 {
		self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
		let mut z = self.0;
		z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
		z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
		z ^ (z >> 31)
	}

	fn unit(&mut self) -> f64 {
		(self.next() >> 11) as f64 / (1u64 << 53) as f64
	}

	fn within(&mut self, (lo, hi): (f64, f64)) -> f64 {
		lo + (hi - lo) * self.unit()
	}
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
	a + (b - a) * t
}

/// The colors of one scheme, before they're handed out by role.
#[derive(Clone, Copy, Debug)]
struct Scheme {
	lead: Hsv,
	calm: Hsv,
	hush: Hsv,
	alarm: Hsv,
	warn: Hsv,
	tint: Hsv,
	border: Hsv,
	top: Hsv,
	bottom: Hsv,
}

// `soft` runs from the standard themes at 0 to the pastel ones at 1, and the
// numbers at each end are the built-ins'.
fn scheme(seed: u64) -> Scheme {
	let mut r = Roll(seed);
	let soft = r.unit();
	let h = r.within(LEAD_HUES);
	let s = lerp(1.0, 0.33, soft) * r.within((0.85, 1.0));
	let lead = Hsv {
		h,
		s,
		v: r.within((0.85, 1.0)),
	};
	// analogous, up to 30 degrees off the lead
	let calm_h = h + r.within((-1.0 / 12.0, 1.0 / 12.0));
	let calm = Hsv {
		h: calm_h,
		s: s * r.within((0.55, 0.75)),
		v: lerp(0.37, 0.54, soft) * r.within((0.9, 1.1)),
	};
	let alarm = Hsv {
		h: r.within(ALARM_HUES).rem_euclid(1.0),
		s: lerp(1.0, 0.62, soft) * r.within((0.95, 1.0)),
		v: r.within((0.95, 1.0)),
	};
	let warn = Hsv {
		h: r.within(WARN_HUES),
		s: lerp(1.0, 0.53, soft) * r.within((0.95, 1.0)),
		v: lerp(0.78, 0.94, soft) * r.within((0.95, 1.05)),
	};
	// the background takes the calm hue or the lead's
	let back_h = if r.unit() < 0.5 { h } else { calm_h };
	// mirrored, and pushed out so it's 30 degrees or more from calm
	let off = calm_h - h;
	let hush = Hsv {
		h: (h - off.signum() * (1.0 / 12.0 - off.abs() / 2.0)).rem_euclid(1.0),
		s: calm.s,
		v: (calm.v * 1.2).min(lead.v * 0.7),
	};
	Scheme {
		lead,
		calm,
		hush,
		alarm,
		warn,
		tint: Hsv {
			h,
			s: s * 0.25,
			v: 1.0,
		},
		border: Hsv {
			h: back_h,
			s: s * 0.5,
			v: lerp(0.5, 0.63, soft),
		},
		top: Hsv {
			h: back_h,
			s: s * 0.5,
			v: lerp(0.33, 0.29, soft),
		},
		bottom: Hsv {
			h: back_h,
			s: s * 0.25,
			v: lerp(0.0, 0.08, soft),
		},
	}
}

fn hex(c: Hsv) -> String {
	pick::hex(pick::to_rgb(c))
}

/// Every theme member in `ROLES` with its rolled value, as the setting
/// takes it.
pub fn theme(seed: u64) -> Vec<(&'static str, String)> {
	let k = scheme(seed);
	ROLES
		.iter()
		.map(|&(id, role)| {
			let v = match role {
				Role::Mode => "detailed".to_string(),
				Role::Lead => hex(k.lead),
				Role::Calm => hex(k.calm),
				Role::Hush => hex(k.hush),
				Role::Alarm => hex(k.alarm),
				Role::Warn => hex(k.warn),
				Role::Tint => hex(k.tint),
				Role::Border => hex(k.border),
				Role::Top => hex(k.top),
				Role::Bottom => hex(k.bottom),
				// black, as in every built-in
				Role::Hover => "#000000".to_string(),
			};
			(id, v)
		})
		.collect()
}

/// A seed nobody picked.
pub fn fresh_seed() -> u64 {
	use std::hash::{BuildHasher, Hasher};
	let mut h = std::collections::hash_map::RandomState::new().build_hasher();
	if let Ok(t) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
		h.write_u128(t.as_nanos());
	}
	h.finish()
}

#[cfg(test)]
mod tests {
	use super::*;
	use knobs::Value;
	use tukzmon_core::config::{self, Rgb, THEME};

	fn hsv_of(text: &str) -> Hsv {
		pick::from_rgb(Rgb::parse(text).unwrap(), Hsv::default())
	}

	fn role_of(t: &[(&str, String)], role: Role) -> Hsv {
		let (id, _) = ROLES.iter().find(|(_, r)| *r == role).unwrap();
		hsv_of(&t.iter().find(|(i, _)| i == id).unwrap().1)
	}

	// distance round the hue circle, 0 to 0.5
	fn hue_gap(a: f64, b: f64) -> f64 {
		let d = (a - b).rem_euclid(1.0);
		d.min(1.0 - d)
	}

	const SEEDS: std::ops::Range<u64> = 0..2000;

	// Test ID: decdr
	#[test]
	fn every_theme_member_has_a_role() {
		let spec = config::spec();
		let members = &spec.group(THEME).unwrap().members;
		for id in members {
			assert!(ROLES.iter().any(|(r, _)| r == id), "{id} has no role");
		}
		for (id, _) in ROLES {
			assert!(members.iter().any(|m| m == id), "{id} isn't in the theme");
		}
	}

	// Test ID: decds
	#[test]
	fn every_roll_is_a_value_its_setting_takes_and_never_a_gradient() {
		let spec = config::spec();
		for seed in SEEDS {
			for (id, v) in theme(seed) {
				let s = spec.get(id).unwrap();
				assert!(
					spec.valid(s, &Value::Text(v.clone())).is_ok(),
					"{seed} {id} {v}"
				);
				if s.control == knobs::Control::Dropdown {
					assert_eq!(v, "detailed", "{seed} {id}");
				}
			}
		}
	}

	// Test ID: decdt
	#[test]
	fn the_lead_rules_and_the_alarms_stay_red_and_orange() {
		for seed in SEEDS {
			let t = theme(seed);
			let lead = role_of(&t, Role::Lead);
			let alarm = role_of(&t, Role::Alarm);
			let warn = role_of(&t, Role::Warn);
			assert!(hue_gap(alarm.h, 0.0) <= 11.0 / 360.0, "{seed} {alarm:?}");
			assert!(
				(17.0 / 360.0..=33.0 / 360.0).contains(&warn.h),
				"{seed} {warn:?}"
			);
			for a in [alarm, warn] {
				assert!(a.s >= 0.45 && a.v >= 0.7, "{seed} {a:?}");
				assert!(
					hue_gap(a.h, lead.h) >= 40.0 / 360.0,
					"{seed} {lead:?} {a:?}"
				);
			}
			assert!(hue_gap(alarm.h, warn.h) >= 15.0 / 360.0, "{seed}");
			assert!(lead.v >= 0.84, "{seed} {lead:?}");
		}
	}

	// Test ID: decdv
	#[test]
	fn the_quiet_ones_are_muted_and_near_the_lead() {
		for seed in SEEDS {
			let t = theme(seed);
			let lead = role_of(&t, Role::Lead);
			let calm = role_of(&t, Role::Calm);
			let border = role_of(&t, Role::Border);
			let top = role_of(&t, Role::Top);
			let bottom = role_of(&t, Role::Bottom);
			assert!(
				calm.v < lead.v * 0.75 && calm.s <= lead.s,
				"{seed} {lead:?} {calm:?}"
			);
			// a dark gray-ish calm reads its hue back a few degrees off
			assert!(hue_gap(calm.h, lead.h) <= 36.0 / 360.0, "{seed}");
			assert!(
				bottom.v < top.v && top.v < border.v && border.v < lead.v,
				"{seed}"
			);
		}
	}

	// Test ID: decdw
	#[test]
	fn the_gradient_runs_from_calm_through_the_lead() {
		for seed in SEEDS {
			let t = theme(seed);
			let get = |id: &str| t.iter().find(|(i, _)| *i == id).unwrap().1.clone();
			assert_eq!(get("cpu_low"), get("nice"));
			assert_eq!(get("cpu_medium"), get("user"));
			assert!(
				hsv_of(&get("cpu_high")).s < hsv_of(&get("user")).s,
				"{seed}"
			);
		}
	}

	// Test ID: dedq0
	#[test]
	fn buffers_and_cache_are_muted_and_told_apart() {
		for seed in SEEDS {
			let t = theme(seed);
			let get = |id: &str| t.iter().find(|(i, _)| *i == id).unwrap().1.clone();
			let lead = hsv_of(&get("mem_user"));
			let buffers = hsv_of(&get("mem_buffers"));
			assert_eq!(get("mem_user"), get("user"), "{seed}");
			assert_eq!(get("mem_cache"), get("nice"), "{seed}");
			assert_ne!(get("mem_buffers"), get("mem_cache"), "{seed}");
			assert!(
				buffers.v < lead.v * 0.75 && buffers.s <= lead.s,
				"{seed} {lead:?} {buffers:?}"
			);
			// a dark gray-ish one reads its hue back a few degrees off
			assert!(hue_gap(buffers.h, lead.h) <= 36.0 / 360.0, "{seed}");
			let calm = scheme(seed);
			assert!(
				hue_gap(calm.hush.h, calm.calm.h) >= 30.0 / 360.0 - 1e-9,
				"{seed}"
			);
		}
	}

	// Test ID: decdx
	#[test]
	fn a_seed_rolls_the_same_theme_and_seeds_differ() {
		assert_eq!(theme(7), theme(7));
		let leads: std::collections::BTreeSet<String> = SEEDS
			.map(|s| theme(s).into_iter().find(|(i, _)| *i == "user").unwrap().1)
			.collect();
		assert!(leads.len() > 1900, "{}", leads.len());
		assert_ne!(fresh_seed(), fresh_seed());
	}
}
