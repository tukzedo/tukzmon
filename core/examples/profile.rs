// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

// The cicd profiler step's workload: the panel's tick, sampled on SIGPROF in
// this process, since perf can't record on b23. A week of history goes in
// first, unsampled, so the axis and the draw see a full graph. The canvas is
// what the graphs ask a panel for, and the timer runs on the beats the panel
// would wake on, only faster.
//   cargo run --profile profiling --features profiling --example profile -- OUT.svg [ROUNDS]

use std::fmt::Write as _;
use std::fs::File;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::Instant;

use tukzmon_core::config::{Look, Which};
use tukzmon_core::graphs::Graphs;

// Under the kernel's 250 Hz tick.
const FREQ: i32 = 199;
// The unwinder, the vdso and the loader stay out, since unwinding from inside
// them isn't safe.
const BLOCKLIST: [&str; 3] = ["libgcc", "vdso", "ld-linux"];
/// The plugin saves every 15 minutes.
const KEEP_MS: f64 = 900_000.0;
/// A panel's size across in logical px, on a scale 2 screen.
const PANEL: u32 = 24;
const SCALE: usize = 2;
/// The ticks are timed in this many runs, and the report gives the median,
/// since one run moved 20% with the load and no code change.
const RUNS: usize = 5;

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

/// The panel's timer, on simulated time: when it wakes, and how long until
/// the next wake.
struct Timer {
	at: f64,
	next: u32,
	since_save: f64,
}

fn tick(g: &mut Graphs, timer: &mut Timer, round: usize, w: usize, h: usize) {
	let ms = f64::from(timer.next);
	g.skip(ms / 1000.0);
	timer.at += ms;
	timer.since_save += ms;
	timer.next = g.tick_at(timer.at);
	let x = round % w;
	g.hover(Some((x, h / 2)), w, h, SCALE);
	black_box(g.render(w, h, SCALE));
	black_box(g.hover_tip(x, h / 2, w, h, SCALE));
	black_box(g.tooltip());
}

fn median(mut v: Vec<f64>) -> f64 {
	v.sort_by(f64::total_cmp);
	let n = v.len();
	f64::midpoint(v[(n - 1) / 2], v[n / 2])
}

fn main() -> Result<(), String> {
	let args: Vec<String> = std::env::args().skip(1).collect();
	let out = PathBuf::from(args.first().ok_or("usage: profile OUT.svg [ROUNDS]")?);
	let rounds: usize = args
		.get(1)
		.map_or(Ok(20_000), |s| s.parse())
		.map_err(|e| format!("ROUNDS: {e}"))?;
	if rounds == 0 {
		return Err("ROUNDS: at least 1".into());
	}
	let hist = std::env::temp_dir().join(format!("tukzmon-profile_{}.hist", std::process::id()));
	let mut look = Look::default();
	// off by default, and the costliest read when on
	look[Which::Procs].show = true;
	let mut g = Graphs::new(look);
	if !g.has(Which::Mem) {
		return Err("no memory graph here, so half the work would be left out".into());
	}
	if !g.has(Which::Procs) {
		return Err("no processes graph here, so its /proc read would be left out".into());
	}
	let (along, across) = g.wanted(PANEL);
	let (w, h) = (along as usize * SCALE, across as usize * SCALE);
	// A week at 20 s a step.
	for _ in 0..7 * 4320 {
		g.skip(20.0);
		g.sample();
		g.trim();
	}

	let guard = pprof::ProfilerGuardBuilder::default()
		.frequency(FREQ)
		.blocklist(&BLOCKLIST)
		.build()
		.map_err(|e| format!("profiler: {e}"))?;
	let runs = RUNS.min(rounds);
	let mut timer = Timer {
		at: 0.0,
		next: g.tick_at(0.0),
		since_save: 0.0,
	};
	// per run: user and system us a tick
	let mut per_tick = Vec::with_capacity(runs);
	let (wall0, cpu0) = (Instant::now(), cpu_us());
	for run in 0..runs {
		let (first, end) = (rounds * run / runs + 1, rounds * (run + 1) / runs + 1);
		let was = cpu_us();
		for round in first..end {
			tick(&mut g, &mut timer, round, w, h);
			if timer.since_save >= KEEP_MS {
				timer.since_save = 0.0;
				g.trim();
				g.save(&hist)
					.map_err(|e| format!("{}: {e}", hist.display()))?;
				g.load(&hist)
					.map_err(|e| format!("{}: {e}", hist.display()))?;
			}
		}
		let now = cpu_us();
		let n = (end - first) as f64;
		per_tick.push(((now.0 - was.0) as f64 / n, (now.1 - was.1) as f64 / n));
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
	let totals: Vec<f64> = per_tick.iter().map(|(u, s)| u + s).collect();
	let (lo, hi) = totals
		.iter()
		.fold((f64::INFINITY, 0.0_f64), |(lo, hi), &t| {
			(lo.min(t), hi.max(t))
		});
	let mut opt = inferno::flamegraph::Options::default();
	opt.title = "tukzmon panel tick".into();
	opt.subtitle = Some(format!(
		"{rounds} ticks of {} graphs in {:.0} s of panel time at {w}x{h} scale {SCALE}: {user:.2} s user + {sys:.2} s system in {wall:.2} s, {:.1} us a tick, {:.1} of it user, medians of {runs} runs ({lo:.1} to {hi:.1})",
		g.count(),
		timer.at / 1000.0,
		median(totals),
		median(per_tick.iter().map(|p| p.0).collect()),
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
