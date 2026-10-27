// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

// The cicd profiler step's workload: the panel's tick, sampled on SIGPROF in
// this process, since perf can't record on b23. A week of history goes in
// first, unsampled, so the axis and the draw see a full graph.
//   cargo run --profile profiling --features profiling --example profile -- OUT.svg [ROUNDS]

use std::fmt::Write as _;
use std::fs::File;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::Instant;

use tukzmon_core::config::Look;
use tukzmon_core::graphs::Graphs;

// Under the kernel's 250 Hz tick.
const FREQ: i32 = 199;
// The unwinder, the vdso and the loader stay out, since unwinding from inside
// them isn't safe.
const BLOCKLIST: [&str; 3] = ["libgcc", "vdso", "ld-linux"];
/// The panel's default sample time.
const TICK: f64 = 0.5;
/// The plugin saves every 15 minutes.
const KEEP_EVERY: usize = 1800;

/// pprof files a sample under the start address of each function on its
/// stack, then names the whole bucket from the inline frames of whichever
/// sample opened it. Fat LTO folds most of a tick into a few functions, so one
/// name would take all of each. Answering with the address itself files each
/// instruction on its own. From shcl's, by way of tukfs-hash's.
#[unsafe(no_mangle)]
pub extern "C" fn _Unwind_FindEnclosingFunction(
	pc: *mut std::ffi::c_void,
) -> *mut std::ffi::c_void {
	pc
}

fn cpu_us() -> (i64, i64) {
	// SAFETY: getrusage only writes the struct it's given.
	let ru = unsafe {
		let mut ru: libc::rusage = std::mem::zeroed();
		libc::getrusage(libc::RUSAGE_SELF, &raw mut ru);
		ru
	};
	let us = |t: libc::timeval| t.tv_sec * 1_000_000 + t.tv_usec;
	(us(ru.ru_utime), us(ru.ru_stime))
}

fn tick(g: &mut Graphs, round: usize, w: usize, h: usize, scale: usize) {
	g.cpu.skip(TICK);
	if let Some(m) = &mut g.mem {
		m.skip(TICK);
	}
	g.sample();
	g.trim();
	let x = round % w;
	g.hover(Some((x, h / 2)), w, h, scale);
	black_box(g.render(w, h, scale));
	black_box(g.hover_tip(x, h / 2, w, h, scale));
	black_box(g.tooltip());
}

fn main() -> Result<(), String> {
	let args: Vec<String> = std::env::args().skip(1).collect();
	let out = PathBuf::from(args.first().ok_or("usage: profile OUT.svg [ROUNDS]")?);
	let rounds: usize = args
		.get(1)
		.map_or(Ok(20_000), |s| s.parse())
		.map_err(|e| format!("ROUNDS: {e}"))?;
	let hist = std::env::temp_dir().join(format!("tukzmon-profile_{}.hist", std::process::id()));
	let (w, h, scale) = (96, 48, 2);
	let mut g = Graphs::new(Look::default());
	if g.mem.is_none() {
		return Err("no memory graph here, so half the work would be left out".into());
	}
	// A week at 20 s a step.
	for _ in 0..7 * 4320 {
		g.cpu.skip(20.0);
		if let Some(m) = &mut g.mem {
			m.skip(20.0);
		}
		g.sample();
		g.trim();
	}

	let guard = pprof::ProfilerGuardBuilder::default()
		.frequency(FREQ)
		.blocklist(&BLOCKLIST)
		.build()
		.map_err(|e| format!("profiler: {e}"))?;
	let (wall0, cpu0) = (Instant::now(), cpu_us());
	for round in 1..=rounds {
		tick(&mut g, round, w, h, scale);
		if round % KEEP_EVERY == 0 {
			g.save(&hist)
				.map_err(|e| format!("{}: {e}", hist.display()))?;
			g.load(&hist)
				.map_err(|e| format!("{}: {e}", hist.display()))?;
		}
	}
	let (cpu1, wall) = (cpu_us(), wall0.elapsed().as_secs_f64());
	let report = guard
		.report()
		.build()
		.map_err(|e| format!("profiler: {e}"))?;
	drop(guard);
	let _ = std::fs::remove_file(&hist);

	// Folded by hand, root first, for inferno.
	let mut folded = String::new();
	for (frames, n) in &report.data {
		folded.push_str(&frames.thread_name);
		for frame in frames.frames.iter().rev() {
			for sym in frame.iter().rev() {
				// A name like <[u8; 32] as T>::f would split the stack.
				let _ = write!(folded, ";{}", sym.name().replace(';', ","));
			}
		}
		let _ = writeln!(folded, " {n}");
	}

	let (user, sys) = (
		(cpu1.0 - cpu0.0) as f64 / 1e6,
		(cpu1.1 - cpu0.1) as f64 / 1e6,
	);
	let mut opt = inferno::flamegraph::Options::default();
	opt.title = "tukzmon panel tick".into();
	opt.subtitle = Some(format!(
		"{rounds} ticks of CPU and memory at {w}x{h} scale {scale}: {user:.2} s user + {sys:.2} s system in {wall:.2} s, {:.1} us a tick",
		(user + sys) * 1e6 / rounds as f64
	));
	// Colors by name, so 2 graphs of the same code look alike.
	opt.hash = true;
	let tmp = out.with_extension("svg.tmp");
	let f = File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
	inferno::flamegraph::from_lines(&mut opt, folded.lines(), f)
		.map_err(|e| format!("flamegraph: {e}"))?;
	std::fs::rename(&tmp, &out).map_err(|e| format!("{}: {e}", out.display()))?;
	println!("{}", opt.subtitle.unwrap_or_default());
	Ok(())
}
