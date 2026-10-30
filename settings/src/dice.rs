// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

// Randomize on the Themes tab. One scheme from one seed: a lead hue of its
// own for each graph, the quiet series that hue muted, and the ones to watch
// kept red and orange like every built-in. See design.md Theme.

use crate::pick::{self, Hsv};
use tukzmon_core::color::{APART, Lab, ciede2000};

/// What a theme member is for, which sets how its color is picked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
	/// The graph's lead color, such as the user share.
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
	/// LAN sent: Hush's hue, between the lead and Tint, and kept apart from
	/// the network's other colors.
	Pale,
	/// LAN received: Hush's hue, between Calm and the lead, kept apart too.
	Deep,
	Border,
	Top,
	Bottom,
	Hover,
	/// A graph's mode, which Randomize always sets to Detailed.
	Mode,
	/// A margin. A roll is colors, so it leaves these as the theme has them.
	Margin,
	/// A checkbox that puts the Outline sub-tab's colors over every graph's.
	/// Left alone, like the margins.
	Switch,
}

/// Which graph a member colors. Each graph gets its own lead hue, and its
/// background goes with it. The Outline sub-tab's go with CPU's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
	Cpu = 0,
	Mem = 1,
	Swap = 2,
	Battery = 3,
	Zswap = 4,
	Net = 5,
	Load = 6,
	Disk = 7,
	Temp = 8,
	Own = 9,
	Gpu = 10,
	Vram = 11,
	Back = 12,
}

const GRAPHS: usize = 12;

/// Every member of the theme group, by graph and role. A member missing here
/// gets no random value, and `dice::tests::every_theme_member_has_a_role`
/// fails.
pub const ROLES: &[(&str, Part, Role)] = &[
	("cpu_mode", Part::Cpu, Role::Mode),
	("iowait", Part::Cpu, Role::Alarm),
	("system", Part::Cpu, Role::Warn),
	("user", Part::Cpu, Role::Lead),
	("nice", Part::Cpu, Role::Calm),
	("cpu_low", Part::Cpu, Role::Calm),
	("cpu_medium", Part::Cpu, Role::Lead),
	("cpu_high", Part::Cpu, Role::Tint),
	("cpu_border_color", Part::Cpu, Role::Border),
	("cpu_back_top", Part::Cpu, Role::Top),
	("cpu_back_bottom", Part::Cpu, Role::Bottom),
	("mem_mode", Part::Mem, Role::Mode),
	("mem_user", Part::Mem, Role::Lead),
	("mem_buffers", Part::Mem, Role::Hush),
	("mem_cache", Part::Mem, Role::Calm),
	("mem_low", Part::Mem, Role::Calm),
	("mem_medium", Part::Mem, Role::Lead),
	("mem_high", Part::Mem, Role::Tint),
	("mem_border_color", Part::Mem, Role::Border),
	("mem_back_top", Part::Mem, Role::Top),
	("mem_back_bottom", Part::Mem, Role::Bottom),
	("swap_mode", Part::Swap, Role::Mode),
	("swap_used", Part::Swap, Role::Lead),
	("swap_low", Part::Swap, Role::Calm),
	("swap_medium", Part::Swap, Role::Lead),
	("swap_high", Part::Swap, Role::Tint),
	("swap_border_color", Part::Swap, Role::Border),
	("swap_back_top", Part::Swap, Role::Top),
	("swap_back_bottom", Part::Swap, Role::Bottom),
	("zswap_mode", Part::Zswap, Role::Mode),
	("zswap_used", Part::Zswap, Role::Lead),
	("zswap_low", Part::Zswap, Role::Calm),
	("zswap_medium", Part::Zswap, Role::Lead),
	("zswap_high", Part::Zswap, Role::Tint),
	("zswap_border_color", Part::Zswap, Role::Border),
	("zswap_back_top", Part::Zswap, Role::Top),
	("zswap_back_bottom", Part::Zswap, Role::Bottom),
	("net_mode", Part::Net, Role::Mode),
	("net_local", Part::Net, Role::Calm),
	("net_lan_sent", Part::Net, Role::Pale),
	("net_lan_received", Part::Net, Role::Deep),
	("net_sent", Part::Net, Role::Tint),
	("net_received", Part::Net, Role::Lead),
	("net_low", Part::Net, Role::Calm),
	("net_medium", Part::Net, Role::Lead),
	("net_high", Part::Net, Role::Tint),
	("net_border_color", Part::Net, Role::Border),
	("net_back_top", Part::Net, Role::Top),
	("net_back_bottom", Part::Net, Role::Bottom),
	("load_mode", Part::Load, Role::Mode),
	("load_used", Part::Load, Role::Lead),
	("load_low", Part::Load, Role::Calm),
	("load_medium", Part::Load, Role::Lead),
	("load_high", Part::Load, Role::Tint),
	("load_border_color", Part::Load, Role::Border),
	("load_back_top", Part::Load, Role::Top),
	("load_back_bottom", Part::Load, Role::Bottom),
	("disk_mode", Part::Disk, Role::Mode),
	("disk_write", Part::Disk, Role::Tint),
	("disk_read", Part::Disk, Role::Lead),
	("disk_low", Part::Disk, Role::Calm),
	("disk_medium", Part::Disk, Role::Lead),
	("disk_high", Part::Disk, Role::Tint),
	("disk_border_color", Part::Disk, Role::Border),
	("disk_back_top", Part::Disk, Role::Top),
	("disk_back_bottom", Part::Disk, Role::Bottom),
	("temp_mode", Part::Temp, Role::Mode),
	("temp_normal", Part::Temp, Role::Lead),
	("temp_critical", Part::Temp, Role::Alarm),
	("temp_low", Part::Temp, Role::Calm),
	("temp_medium", Part::Temp, Role::Lead),
	("temp_high", Part::Temp, Role::Tint),
	("temp_border_color", Part::Temp, Role::Border),
	("temp_back_top", Part::Temp, Role::Top),
	("temp_back_bottom", Part::Temp, Role::Bottom),
	("battery_mode", Part::Battery, Role::Mode),
	("battery_charging", Part::Battery, Role::Lead),
	("battery_discharging", Part::Battery, Role::Warn),
	("battery_critical", Part::Battery, Role::Alarm),
	("battery_low", Part::Battery, Role::Calm),
	("battery_medium", Part::Battery, Role::Lead),
	("battery_high", Part::Battery, Role::Tint),
	("battery_border_color", Part::Battery, Role::Border),
	("battery_back_top", Part::Battery, Role::Top),
	("battery_back_bottom", Part::Battery, Role::Bottom),
	("self_mode", Part::Own, Role::Mode),
	("self_used", Part::Own, Role::Lead),
	("self_low", Part::Own, Role::Calm),
	("self_medium", Part::Own, Role::Lead),
	("self_high", Part::Own, Role::Tint),
	("self_border_color", Part::Own, Role::Border),
	("self_back_top", Part::Own, Role::Top),
	("self_back_bottom", Part::Own, Role::Bottom),
	("gpu_mode", Part::Gpu, Role::Mode),
	("gpu_busy", Part::Gpu, Role::Lead),
	("gpu_low", Part::Gpu, Role::Calm),
	("gpu_medium", Part::Gpu, Role::Lead),
	("gpu_high", Part::Gpu, Role::Tint),
	("gpu_border_color", Part::Gpu, Role::Border),
	("gpu_back_top", Part::Gpu, Role::Top),
	("gpu_back_bottom", Part::Gpu, Role::Bottom),
	("vram_mode", Part::Vram, Role::Mode),
	("vram_used", Part::Vram, Role::Lead),
	("vram_low", Part::Vram, Role::Calm),
	("vram_medium", Part::Vram, Role::Lead),
	("vram_high", Part::Vram, Role::Tint),
	("vram_border_color", Part::Vram, Role::Border),
	("vram_back_top", Part::Vram, Role::Top),
	("vram_back_bottom", Part::Vram, Role::Bottom),
	("border_all", Part::Back, Role::Switch),
	("border_color", Part::Back, Role::Border),
	("back_all", Part::Back, Role::Switch),
	("back_top", Part::Back, Role::Top),
	("back_bottom", Part::Back, Role::Bottom),
	("hover_color", Part::Back, Role::Hover),
	("margin_tb", Part::Back, Role::Margin),
	("margin_lr", Part::Back, Role::Margin),
];

// Lead hues stay out of the red and orange the alarms use, in turns.
const LEAD_HUES: (f64, f64) = (75.0 / 360.0, 300.0 / 360.0);
const ALARM_HUES: (f64, f64) = (-10.0 / 360.0, 2.0 / 360.0);
const WARN_HUES: (f64, f64) = (18.0 / 360.0, 32.0 / 360.0);
// The least gap between 2 graphs' lead hues. 12 of them at this gap leave
// 27 degrees of the lead range to roll in.
const LEAD_GAP: f64 = 18.0 / 360.0;
// Hue draws tried for leads that all read apart, before settling for the last.
const TRIES: usize = 128;

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

	fn below(&mut self, n: usize) -> usize {
		((self.unit() * n as f64) as usize).min(n - 1)
	}
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
	a + (b - a) * t
}

// One lead hue per graph, at least LEAD_GAP apart, in a random order. Sorted
// draws over the room left after the gaps, with a gap added back after each,
// spread every allowed set evenly.
fn lead_hues(r: &mut Roll) -> [f64; GRAPHS] {
	let room = (LEAD_HUES.1 - LEAD_HUES.0) - LEAD_GAP * (GRAPHS - 1) as f64;
	let mut h = [0.0; GRAPHS];
	for x in &mut h {
		*x = room * r.unit();
	}
	h.sort_by(f64::total_cmp);
	for (i, x) in h.iter_mut().enumerate() {
		*x += LEAD_HUES.0 + LEAD_GAP * i as f64;
	}
	for i in (1..GRAPHS).rev() {
		h.swap(i, r.below(i + 1));
	}
	h
}

// Where 2 leads would look alike, one of them steps darker or paler, tried
// in this order. 12 hues share too little of the circle to read apart by hue
// alone, most of all when pale.
fn step(n: usize, s: f64, v: f64) -> (f64, f64) {
	match n {
		0 => (s, v),
		1 => ((s * 1.1).min(1.0), v * 0.72),
		_ => (s * 0.6, 1.0),
	}
}

const STEPS: usize = 3;

// Depth first over the graphs in `Part` order, CPU first.
fn place(
	g: usize,
	pick: &mut [usize; GRAPHS],
	fits: &dyn Fn(&[usize; GRAPHS], usize, usize) -> bool,
) -> bool {
	if g == GRAPHS {
		return true;
	}
	let choices = if g == Part::Cpu as usize { 1 } else { STEPS };
	for n in 0..choices {
		if fits(pick, g, n) {
			pick[g] = n;
			if place(g + 1, pick, fits) {
				return true;
			}
		}
	}
	false
}

// Each graph's step, or None when no choice reads apart from the rest and
// from the alarms. CPU's lead is the roll's own, and the rest fit round it.
fn steps(hues: &[f64; GRAPHS], s: f64, v: f64, alarms: &[Lab]) -> Option<[usize; GRAPHS]> {
	let lab = |g: usize, n: usize| {
		let (s, v) = step(n, s, v);
		Lab::of(pick::to_rgb(Hsv { h: hues[g], s, v }))
	};
	let labs: Vec<Lab> = (0..GRAPHS * STEPS)
		.map(|i| lab(i / STEPS, i % STEPS))
		.collect();
	let clear: Vec<bool> = labs
		.iter()
		.map(|c| alarms.iter().all(|a| ciede2000(*c, *a) >= APART))
		.collect();
	let near: Vec<bool> = (0..labs.len() * labs.len())
		.map(|i| ciede2000(labs[i / labs.len()], labs[i % labs.len()]) < APART)
		.collect();
	let at = |g: usize, n: usize| g * STEPS + n;
	let fits = |pick: &[usize; GRAPHS], g: usize, n: usize| {
		clear[at(g, n)] && (0..g).all(|o| !near[at(g, n) * labs.len() + at(o, pick[o])])
	};
	let mut pick = [0; GRAPHS];
	place(0, &mut pick, &fits).then_some(pick)
}

/// One graph's colors.
#[derive(Clone, Copy, Debug)]
struct Hues {
	lead: Hsv,
	calm: Hsv,
	hush: Hsv,
	tint: Hsv,
	pale: Hsv,
	deep: Hsv,
}

#[derive(Clone, Copy, Debug)]
struct Back {
	border: Hsv,
	top: Hsv,
	bottom: Hsv,
}

impl Back {
	// a hue of the graph's, darker and grayer, with the border under a lead
	// that stepped darker
	fn on(h: f64, s: f64, soft: f64, lead: Hsv) -> Back {
		Back {
			border: Hsv {
				h,
				s: s * 0.5,
				v: lerp(0.5, 0.63, soft).min(lead.v * 0.85),
			},
			top: Hsv {
				h,
				s: s * 0.5,
				v: lerp(0.15, 0.14, soft),
			},
			bottom: Hsv {
				h,
				s: s * 0.25,
				v: lerp(0.0, 0.08, soft),
			},
		}
	}
}

/// The colors of one scheme, before they're handed out by role.
#[derive(Clone, Copy, Debug)]
struct Scheme {
	graphs: [Hues; GRAPHS],
	alarm: Hsv,
	warn: Hsv,
	/// The Outline sub-tab's.
	back: Back,
	backs: [Back; GRAPHS],
}

impl Scheme {
	fn of(&self, p: Part) -> &Hues {
		match p {
			Part::Back => &self.graphs[Part::Cpu as usize],
			p => &self.graphs[p as usize],
		}
	}

	fn back_of(&self, p: Part) -> &Back {
		match p {
			Part::Back => &self.back,
			p => &self.backs[p as usize],
		}
	}
}

// `soft` runs from the standard themes at 0 to the pastel ones at 1, and the
// numbers at each end are the built-ins'. Every graph leads at one saturation
// and brightness, so they read as one set, unless it steps darker or paler to
// read apart.
fn scheme(seed: u64) -> Scheme {
	let mut r = Roll(seed);
	let soft = r.unit();
	let s = lerp(1.0, 0.33, soft) * r.within((0.85, 1.0));
	let v = r.within((0.85, 1.0));
	let calm_s = s * r.within((0.55, 0.75));
	let calm_v = lerp(0.37, 0.54, soft) * r.within((0.9, 1.1));
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
	let alarms = [alarm, warn].map(|c| Lab::of(pick::to_rgb(c)));
	let mut hues = lead_hues(&mut r);
	let mut picked = steps(&hues, s, v, &alarms);
	for _ in 1..TRIES {
		if picked.is_some() {
			break;
		}
		hues = lead_hues(&mut r);
		picked = steps(&hues, s, v, &alarms);
	}
	let picked = picked.unwrap_or_default();
	let mut calm_hs = [0.0; GRAPHS];
	let mut graphs: [Hues; GRAPHS] = std::array::from_fn(|i| {
		let h = hues[i];
		let (lead_s, lead_v) = step(picked[i], s, v);
		// a graph that stepped paler takes its other colors along, and one
		// that stepped darker keeps its quiet ones clear of the background
		let (ks, kv) = (lead_s / s, (lead_v / v).max(1.0));
		// analogous, up to 30 degrees off the lead
		let calm_h = h + r.within((-1.0 / 12.0, 1.0 / 12.0));
		calm_hs[i] = calm_h;
		// mirrored, and pushed out so it's 30 degrees or more from calm
		let off = calm_h - h;
		let hush_h = (h - off.signum() * (1.0 / 12.0 - off.abs() / 2.0)).rem_euclid(1.0);
		let calm_sv = (calm_s * ks, (calm_v * kv).min(lead_v * 0.62));
		Hues {
			lead: Hsv {
				h,
				s: lead_s,
				v: lead_v,
			},
			calm: Hsv {
				h: calm_h,
				s: calm_sv.0,
				v: calm_sv.1,
			},
			hush: Hsv {
				h: hush_h,
				s: calm_s * ks,
				v: (calm_v * 1.2 * kv).min(lead_v * 0.72),
			},
			tint: Hsv {
				h,
				s: lead_s * 0.25,
				v: 1.0,
			},
			pale: Hsv {
				h: hush_h,
				s: lead_s * 0.625,
				v: f64::midpoint(lead_v, 1.0),
			},
			deep: Hsv {
				h: hush_h,
				s: f64::midpoint(calm_sv.0, lead_s),
				v: f64::midpoint(calm_sv.1, lead_v),
			},
		}
	});
	// the LAN's 2 sit among local, sent and received, so they move off
	// those till they read apart
	let net = &mut graphs[Part::Net as usize];
	net.deep = clear_of(net.deep, &[net.lead, net.calm, net.tint]);
	net.pale = clear_of(net.pale, &[net.lead, net.calm, net.tint, net.deep]);
	// a background takes its graph's lead hue or its calm one, and the
	// Outline sub-tab's CPU's
	let mut back_on = |i: usize| {
		let h = if r.unit() < 0.5 { hues[i] } else { calm_hs[i] };
		Back::on(h, s, soft, graphs[i].lead)
	};
	let back = back_on(Part::Cpu as usize);
	let backs = std::array::from_fn(back_on);
	Scheme {
		graphs,
		alarm,
		warn,
		back,
		backs,
	}
}

/// The nearest color to `want` that reads apart from each of `others`, by
/// hue, saturation and brightness steps. `want` when none does.
fn clear_of(want: Hsv, others: &[Hsv]) -> Hsv {
	let lab = |c: Hsv| Lab::of(pick::to_rgb(c));
	let others: Vec<Lab> = others.iter().map(|&c| lab(c)).collect();
	let mut steps: Vec<(f64, f64, f64)> = Vec::with_capacity(5 * 9 * 9);
	for dh in [0.0, -1.0 / 36.0, 1.0 / 36.0, -1.0 / 18.0, 1.0 / 18.0] {
		for ds in -4..=4 {
			for dv in -4..=4 {
				steps.push((dh, f64::from(ds) * 0.08, f64::from(dv) * 0.08));
			}
		}
	}
	steps.sort_by(|a, b| {
		let far = |(h, s, v): &(f64, f64, f64)| h.abs() * 3.0 + s.abs() + v.abs();
		far(a).total_cmp(&far(b))
	});
	steps
		.into_iter()
		.map(|(dh, ds, dv)| Hsv {
			h: (want.h + dh).rem_euclid(1.0),
			s: (want.s + ds).clamp(0.0, 1.0),
			v: (want.v + dv).clamp(0.0, 1.0),
		})
		.find(|&c| {
			let c = lab(c);
			others.iter().all(|o| ciede2000(c, *o) >= APART + 1.0)
		})
		.unwrap_or(want)
}

fn hex(c: Hsv) -> String {
	pick::hex(pick::to_rgb(c))
}

/// Every theme member in `ROLES` with its rolled value, as the setting
/// takes it. The margins and the override checkboxes aren't rolled.
pub fn theme(seed: u64) -> Vec<(&'static str, String)> {
	let k = scheme(seed);
	ROLES
		.iter()
		.filter_map(|&(id, part, role)| {
			let g = k.of(part);
			let v = match role {
				Role::Margin | Role::Switch => return None,
				Role::Mode => "detailed".to_string(),
				Role::Lead => hex(g.lead),
				Role::Calm => hex(g.calm),
				Role::Hush => hex(g.hush),
				Role::Alarm => hex(k.alarm),
				Role::Warn => hex(k.warn),
				Role::Tint => hex(g.tint),
				Role::Pale => hex(g.pale),
				Role::Deep => hex(g.deep),
				Role::Border => hex(k.back_of(part).border),
				Role::Top => hex(k.back_of(part).top),
				Role::Bottom => hex(k.back_of(part).bottom),
				// black, as in every built-in
				Role::Hover => "#000000".to_string(),
			};
			Some((id, v))
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

	const PARTS: [Part; GRAPHS] = [
		Part::Cpu,
		Part::Mem,
		Part::Swap,
		Part::Battery,
		Part::Zswap,
		Part::Net,
		Part::Load,
		Part::Disk,
		Part::Temp,
		Part::Own,
		Part::Gpu,
		Part::Vram,
	];

	fn hsv_of(text: &str) -> Hsv {
		pick::from_rgb(Rgb::parse(text).unwrap(), Hsv::default())
	}

	// every member of one graph with one role
	fn all_of<'a>(t: &'a [(&str, String)], part: Part, role: Role) -> Vec<&'a str> {
		ROLES
			.iter()
			.filter(|(_, p, r)| *p == part && *r == role)
			.map(|(id, _, _)| t.iter().find(|(i, _)| i == id).unwrap().1.as_str())
			.collect()
	}

	fn role_of(t: &[(&str, String)], part: Part, role: Role) -> Hsv {
		hsv_of(all_of(t, part, role)[0])
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
			assert!(ROLES.iter().any(|(r, _, _)| r == id), "{id} has no role");
		}
		for (id, _, _) in ROLES {
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
			let alarm = role_of(&t, Part::Cpu, Role::Alarm);
			let warn = role_of(&t, Part::Cpu, Role::Warn);
			assert_eq!(
				all_of(&t, Part::Battery, Role::Alarm),
				all_of(&t, Part::Cpu, Role::Alarm)
			);
			assert_eq!(
				all_of(&t, Part::Temp, Role::Alarm),
				all_of(&t, Part::Cpu, Role::Alarm)
			);
			assert_eq!(
				all_of(&t, Part::Battery, Role::Warn),
				all_of(&t, Part::Cpu, Role::Warn)
			);
			assert!(hue_gap(alarm.h, 0.0) <= 11.0 / 360.0, "{seed} {alarm:?}");
			assert!(
				(17.0 / 360.0..=33.0 / 360.0).contains(&warn.h),
				"{seed} {warn:?}"
			);
			assert!(hue_gap(alarm.h, warn.h) >= 15.0 / 360.0, "{seed}");
			for part in PARTS {
				let lead = role_of(&t, part, Role::Lead);
				for a in [alarm, warn] {
					assert!(a.s >= 0.45 && a.v >= 0.7, "{seed} {a:?}");
					assert!(
						hue_gap(a.h, lead.h) >= 40.0 / 360.0,
						"{seed} {part:?} {lead:?} {a:?}"
					);
				}
				// 0.85 at least, or 0.72 of that where a lead steps darker
				assert!(lead.v >= 0.6, "{seed} {part:?} {lead:?}");
			}
		}
	}

	// Test ID: decdv
	#[test]
	fn the_quiet_ones_are_muted_and_near_the_lead() {
		for seed in SEEDS {
			let t = theme(seed);
			for part in PARTS {
				let lead = role_of(&t, part, Role::Lead);
				let calm = role_of(&t, part, Role::Calm);
				assert!(
					calm.v < lead.v * 0.75 && calm.s <= lead.s,
					"{seed} {part:?} {lead:?} {calm:?}"
				);
				// a dark gray-ish calm reads its hue back a few degrees off
				assert!(hue_gap(calm.h, lead.h) <= 36.0 / 360.0, "{seed} {part:?}");
			}
			let lead = role_of(&t, Part::Cpu, Role::Lead);
			let border = role_of(&t, Part::Back, Role::Border);
			let top = role_of(&t, Part::Back, Role::Top);
			let bottom = role_of(&t, Part::Back, Role::Bottom);
			assert!(
				bottom.v < top.v && top.v < border.v && border.v < lead.v,
				"{seed}"
			);
		}
	}

	// Test ID: dejkf
	#[test]
	fn each_graph_rolls_a_background_of_its_own_hue() {
		let mut apart = 0;
		for seed in SEEDS {
			let t = theme(seed);
			let mut tops = Vec::new();
			for part in PARTS {
				let lead = role_of(&t, part, Role::Lead);
				let calm = role_of(&t, part, Role::Calm);
				let border = role_of(&t, part, Role::Border);
				let top = role_of(&t, part, Role::Top);
				let bottom = role_of(&t, part, Role::Bottom);
				assert!(
					bottom.v < top.v && top.v < border.v && border.v < lead.v,
					"{seed} {part:?}"
				);
				// grayed hues read back a few degrees off
				let near = |h: f64| hue_gap(h, lead.h).min(hue_gap(h, calm.h)) <= 12.0 / 360.0;
				assert!(near(top.h) && near(border.h), "{seed} {part:?} {top:?}");
				// on its own hue, a quiet color reads apart by lightness alone
				let lab = |c: Hsv| Lab::of(pick::to_rgb(c));
				for role in [Role::Calm, Role::Hush] {
					for c in all_of(&t, part, role) {
						let d = ciede2000(Lab::of(Rgb::parse(c).unwrap()), lab(top));
						assert!(d >= APART, "{seed} {part:?} {c} {d}");
					}
				}
				tops.push(top);
			}
			apart += usize::from(tops.iter().any(|c| hue_gap(c.h, tops[0].h) > 0.1));
		}
		assert!(apart * 2 > SEEDS.count(), "{apart}");
	}

	// Test ID: decdw
	#[test]
	fn the_gradient_runs_from_calm_through_the_lead() {
		for seed in SEEDS {
			let t = theme(seed);
			let get = |id: &str| t.iter().find(|(i, _)| *i == id).unwrap().1.clone();
			assert_eq!(get("cpu_low"), get("nice"));
			assert_eq!(get("cpu_medium"), get("user"));
			for part in PARTS {
				for role in [Role::Lead, Role::Calm] {
					let all = all_of(&t, part, role);
					assert!(all.iter().all(|c| *c == all[0]), "{seed} {part:?} {all:?}");
				}
				assert!(
					role_of(&t, part, Role::Tint).s < role_of(&t, part, Role::Lead).s,
					"{seed} {part:?}"
				);
			}
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
			assert_ne!(get("mem_user"), get("user"), "{seed}");
			assert_eq!(get("mem_cache"), get("mem_low"), "{seed}");
			assert_ne!(get("mem_buffers"), get("mem_cache"), "{seed}");
			assert!(
				buffers.v < lead.v * 0.75 && buffers.s <= lead.s,
				"{seed} {lead:?} {buffers:?}"
			);
			// a dark gray-ish one reads its hue back a few degrees off
			assert!(hue_gap(buffers.h, lead.h) <= 36.0 / 360.0, "{seed}");
			let mem = scheme(seed).graphs[Part::Mem as usize];
			assert!(
				hue_gap(mem.hush.h, mem.calm.h) >= 30.0 / 360.0 - 1e-9,
				"{seed}"
			);
		}
	}

	// Test ID: demw2
	#[test]
	fn the_lan_rolls_colors_that_read_apart_from_the_rest() {
		use tukzmon_core::color::apart;
		let ids = [
			"net_local",
			"net_sent",
			"net_received",
			"net_lan_received",
			"net_lan_sent",
		];
		for seed in SEEDS {
			let t = theme(seed);
			let rgb = |id: &str| Rgb::parse(&t.iter().find(|(i, _)| *i == id).unwrap().1).unwrap();
			// local, sent and received were rolled before the split
			for (i, lan) in ids.iter().enumerate().skip(3) {
				for other in &ids[..i] {
					let d = apart(rgb(lan), rgb(other));
					assert!(d >= APART, "{seed}: {lan} and {other} are {d:.1} apart");
				}
			}
		}
	}

	// Test ID: defxd
	#[test]
	fn each_graph_leads_with_a_hue_of_its_own() {
		let mut cpu = (1.0_f64, 0.0_f64);
		for seed in SEEDS {
			let t = theme(seed);
			let leads = PARTS.map(|p| role_of(&t, p, Role::Lead).h);
			for (i, a) in leads.iter().enumerate() {
				for b in &leads[i + 1..] {
					// 18 apart as rolled, less a little for the hex round trip
					assert!(hue_gap(*a, *b) >= 13.0 / 360.0, "{seed} {leads:?}");
				}
			}
			cpu = (cpu.0.min(leads[0]), cpu.1.max(leads[0]));
		}
		// the order is shuffled, so CPU isn't stuck at one end of the range
		assert!(cpu.0 < 90.0 / 360.0 && cpu.1 > 285.0 / 360.0, "{cpu:?}");
	}

	// Test ID: dejbv
	#[test]
	fn every_roll_leads_each_graph_with_a_color_that_reads_apart() {
		use tukzmon_core::color::{apart, closest};
		for seed in SEEDS {
			let t = theme(seed);
			let get = |id: &str| t.iter().find(|(i, _)| *i == id).map(|(_, v)| v.as_str());
			let leads = config::leads(get);
			assert_eq!(leads.len(), GRAPHS);
			let (d, a, b) = closest(&leads).unwrap();
			assert!(d >= APART, "{seed}: {a} and {b} are {d:.1} apart");
			for alarm in config::ALARMS {
				let c = Rgb::parse(get(alarm).unwrap()).unwrap();
				for lead in &leads {
					assert!(apart(*lead, c) >= APART, "{seed} {alarm} {lead:?}");
				}
			}
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
