// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

// Made-up days of history for the default graphs, ending now, for the demo's
// long-run scene. Workdays are busy, nights are quiet, a few builds run hot,
// and the box was off one night, so that shows as a gap.
//   cargo run --release -p tukzmon-core --example demo-history -- OUT.hist [DAYS] [SEED]

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use tukzmon_core::config::Which;
use tukzmon_core::history::History;
use tukzmon_core::keep;

const HOUR: f64 = 3600.0;
const DAY: f64 = 24.0 * HOUR;
// the last stretch gets 1 s steps, like a live panel, and the rest 10 s
const FINE_FOR: f64 = 2.0 * HOUR;
// The made-up clock reads mid-afternoon now, whenever it's run, so the
// newest hours are busy ones.
const NOW_AT: f64 = 15.5 * HOUR;

struct Dice(u64);

impl Dice {
	// xorshift64*, plenty for noise
	fn next(&mut self) -> f64 {
		self.0 ^= self.0 >> 12;
		self.0 ^= self.0 << 25;
		self.0 ^= self.0 >> 27;
		(self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 11) as f64 / (1u64 << 53) as f64
	}
}

/// A slow wander toward a target, so the lines don't look like static.
struct Drift {
	at: f64,
	rate: f64,
}

impl Drift {
	fn toward(&mut self, target: f64, dt: f64, dice: &mut Dice, jitter: f64) -> f64 {
		let k = 1.0 - (-dt / self.rate).exp();
		self.at += (target - self.at) * k + (dice.next() - 0.5) * jitter * k.sqrt();
		self.at
	}
}

/// How busy the box is at a time of day on the made-up clock, 0 to 1.
fn daytime(t: f64) -> f64 {
	let h = (t.rem_euclid(DAY)) / HOUR;
	let up = ((h - 7.5) / 1.5).clamp(0.0, 1.0);
	let down = ((22.5 - h) / 2.0).clamp(0.0, 1.0);
	let lunch = 1.0 - 0.35 * (-((h - 12.5) / 0.6).powi(2)).exp();
	up * down * lunch
}

struct Box {
	cpu: History<4>,
	mem: History<3>,
	vram: History<1>,
	disk: History<2>,
	temp: History<2>,
	swap: History<1>,
}

fn main() -> Result<(), String> {
	let args: Vec<String> = std::env::args().skip(1).collect();
	let out = PathBuf::from(
		args.first()
			.ok_or("usage: demo-history OUT.hist [DAYS] [SEED]")?,
	);
	let days: f64 = args
		.get(1)
		.map_or(Ok(4.0), |s| s.parse())
		.map_err(|e| format!("DAYS: {e}"))?;
	let seed: u64 = args
		.get(2)
		.map_or(Ok(7), |s| s.parse())
		.map_err(|e| format!("SEED: {e}"))?;
	if !(0.1..=27.0).contains(&days) {
		return Err("DAYS: 0.1 to 27".into());
	}
	let now = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map_err(|e| format!("clock: {e}"))?
		.as_secs_f64();
	let start = now - days * DAY;
	// off from late one night to the next morning, a couple of days back
	let clock = |t: f64| t - now + NOW_AT;
	let off_from = now - NOW_AT - DAY - 0.8 * HOUR;
	let off_to = off_from + 8.3 * HOUR;
	// builds: an hour of near full CPU and lots of disk, a few a day
	let mut dice = Dice(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
	let mut builds = Vec::new();
	let mut t = start;
	while t < now {
		t += HOUR * (3.0 + 6.0 * dice.next());
		if daytime(clock(t)) > 0.5 {
			builds.push((t, t + HOUR * (0.3 + 0.6 * dice.next())));
		}
	}
	let in_build = |t: f64| builds.iter().any(|&(a, b)| t >= a && t < b);

	let mut b = Box {
		cpu: History::default(),
		mem: History::default(),
		vram: History::default(),
		disk: History::default(),
		temp: History::default(),
		swap: History::default(),
	};
	let slow = |at: f64| Drift { at, rate: 300.0 };
	let (mut load, mut used, mut cache, mut vram, mut heat) =
		(slow(0.05), slow(0.3), slow(0.2), slow(0.2), slow(40.0));
	let mut swap = Drift {
		at: 0.03,
		rate: 4.0 * HOUR,
	};
	let mut t = start;
	while t < now {
		let dt = if now - t < FINE_FOR { 1.0 } else { 10.0 };
		let t1 = (t + dt).min(now);
		if t >= off_from && t < off_to {
			t = t1;
			continue;
		}
		let day = daytime(clock(t));
		let building = in_build(t);
		let busy = if building { 0.88 } else { 0.08 + 0.34 * day };
		let cpu = load.toward(busy, dt, &mut dice, 0.5).clamp(0.01, 0.99);
		let user = cpu * 0.72;
		let system = cpu * 0.2;
		let iowait = if building { 0.04 * dice.next() } else { 0.005 };
		let nice = cpu * 0.04;
		b.cpu.add(
			t,
			t1,
			&[iowait as f32, system as f32, user as f32, nice as f32],
		);

		let u = used
			.toward(
				0.22 + 0.2 * day + if building { 0.18 } else { 0.0 },
				dt,
				&mut dice,
				0.02,
			)
			.clamp(0.1, 0.8);
		let c = cache
			.toward(0.18 + 0.25 * day, dt, &mut dice, 0.02)
			.clamp(0.05, 0.6);
		b.mem.add(t, t1, &[u as f32, 0.02, c.min(0.95 - u) as f32]);

		let evening = ((clock(t).rem_euclid(DAY) / HOUR - 19.0) / 3.0).clamp(0.0, 1.0);
		let v = vram
			.toward(
				0.18 + 0.12 * day + 0.4 * evening * (1.0 - evening),
				dt,
				&mut dice,
				0.03,
			)
			.clamp(0.05, 0.95);
		b.vram.add(t, t1, &[v as f32]);

		// bytes a second: a trickle, bursts while building
		let burst = |dice: &mut Dice, base: f64, top: f64| {
			let x = dice.next();
			if x > 0.97 {
				base + top * dice.next()
			} else {
				base * (0.3 + x)
			}
		};
		let (w, r) = if building {
			(burst(&mut dice, 4e6, 90e6), burst(&mut dice, 3e6, 60e6))
		} else {
			(
				burst(&mut dice, 60e3 + 400e3 * day, 30e6),
				burst(&mut dice, 20e3 + 200e3 * day, 20e6),
			)
		};
		b.disk.add(t, t1, &[w as f32, r as f32]);

		let deg = heat
			.toward(36.0 + 52.0 * cpu, dt, &mut dice, 1.5)
			.clamp(30.0, 99.0);
		b.temp
			.add(t, t1, &[deg.min(95.0) as f32, (deg - 95.0).max(0.0) as f32]);

		let s = swap.toward(
			0.02 + 0.12 * (t - start) / (now - start),
			dt,
			&mut dice,
			0.002,
		);
		b.swap.add(t, t1, &[s.clamp(0.0, 1.0) as f32]);
		t = t1;
	}

	let mut bodies: Vec<(&str, Vec<u8>)> = Vec::new();
	let mut put = |w: Which, f: &dyn Fn(&mut Vec<u8>)| {
		let mut body = Vec::new();
		f(&mut body);
		bodies.push((w.key(), body));
	};
	put(Which::Cpu, &|o| b.cpu.encode(o));
	put(Which::Mem, &|o| b.mem.encode(o));
	put(Which::Vram, &|o| b.vram.encode(o));
	put(Which::Disk, &|o| b.disk.encode(o));
	put(Which::Temp, &|o| b.temp.encode(o));
	put(Which::Swap, &|o| b.swap.encode(o));
	let sections: Vec<(&str, &[u8])> = bodies.iter().map(|(n, b)| (*n, b.as_slice())).collect();
	keep::write(&out, &sections).map_err(|e| format!("{}: {e}", out.display()))
}
