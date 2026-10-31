// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

// Draws made-up CPU history to a PPM file, to eyeball the log axis.
//   cargo run --example preview -- OUT.ppm WIDTH HEIGHT HOURS [SETTINGS]

use tukzmon_core::axis::{Axis, Curve, Timing};
use tukzmon_core::config::Look;
use tukzmon_core::history::History;
use tukzmon_core::render::Frame;

fn main() {
	let args: Vec<String> = std::env::args().skip(1).collect();
	let out = args
		.first()
		.cloned()
		.unwrap_or_else(|| "preview.ppm".into());
	let num = |i: usize, d: f64| args.get(i).and_then(|s| s.parse().ok()).unwrap_or(d);
	let (w, h, hours) = (num(1, 512.0) as usize, num(2, 64.0) as usize, num(3, 72.0));
	let end = hours * 3600.0;

	let mut hist = History::<4>::default();
	let mut t = 0.0;
	while t < end {
		// off for 2 hours, a day and a half back
		let off = (end - 129_600.0..end - 122_400.0).contains(&t);
		let step = if end - t < 600.0 { 0.5 } else { 5.0 };
		if !off {
			let day = (t / 86400.0 * std::f64::consts::TAU).sin() as f32;
			let hourly_spike = (t % 3600.0) < 20.0;
			let user = if hourly_spike { 0.85 } else { 0.15 + 0.1 * day };
			let minute_blip = (t % 60.0) < 2.0;
			let iowait = if minute_blip { 0.3 } else { 0.02 };
			hist.add(t, t + step, &[iowait, 0.08, user, 0.03]);
		}
		t += step;
	}

	let look = match args.get(4).map(|p| Look::load(std::path::Path::new(p))) {
		None => Look::default(),
		Some(Ok(l)) => l,
		Some(Err(e)) => {
			eprintln!("{}: {e}", args[4]);
			std::process::exit(1);
		}
	};
	let curve = match look.time {
		Timing::Log(c) => c,
		Timing::Linear => Curve::Smooth,
	};
	let axis = Axis::log(curve, end);
	let mut px = Vec::new();
	Frame {
		hist: &hist,
		axis: &axis,
		now: end,
		look: &look,
		paint: look.cpu_paint(),
		outline: look.cpu_outline,
	}
	.draw(&mut px, w, h, 1);

	let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
	for p in px {
		ppm.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
	}
	if let Err(e) = std::fs::write(&out, ppm) {
		eprintln!("{out}: {e}");
		std::process::exit(1);
	}
}
