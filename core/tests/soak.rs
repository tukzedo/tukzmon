// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! The panel's work in a loop, days of it with the clock skipped on, and the
//! heap read at round N and 2N. Past the first max age the history is as big
//! as it gets, so what's left to grow is a leak. cicd.bash runs it as the
//! Soak leak check. `TUKZMON_SOAK_DIR` is where the report goes, and the dhat
//! profile when it fails.

use std::fmt::Write as _;
use std::path::PathBuf;

use tukzmon_core::config::Look;
use tukzmon_core::graphs::Graphs;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// Simulated seconds per round. A day is 4320 of them.
const STEP: f64 = 20.0;
/// Round N, at 50 hours. With the 1 day max age the test sets, the heap
/// stops growing at about 22.
const N: usize = 9000;
/// Rounds between a save and load. N is a multiple, so both reads come
/// straight after a load, when the history's buffers are sized alike.
const KEEP_EVERY: usize = 100;
/// Room for a bucket more or less in each tier at round 2N.
const MARGIN_BYTES: usize = 8 * 1024;
const MARGIN_BLOCKS: usize = 16;

fn out_dir() -> PathBuf {
	std::env::var_os("TUKZMON_SOAK_DIR").map_or_else(std::env::temp_dir, PathBuf::from)
}

// Test ID: dee6p
#[test]
#[ignore = "the soak leak check, cicd.bash runs it"]
fn the_heap_stops_growing_once_the_history_is_full() {
	let dir = out_dir();
	std::fs::create_dir_all(&dir).unwrap();
	let _prof = dhat::Profiler::builder()
		.testing()
		.file_name(dir.join("soak-heap.json"))
		.build();
	let hist = dir.join(format!("soak_{}.hist", std::process::id()));
	let look = Look {
		max_days: 1,
		..Look::default()
	};
	let mut g = Graphs::new(look);
	assert!(
		g.mem.is_some(),
		"no memory graph here, so half the work is left out"
	);
	let (w, h, scale) = (96, 48, 2);
	// sized up front, or its own growth reads as the leak
	let mut report = String::with_capacity((2 * N / KEEP_EVERY + 2) * 96);
	report.push_str("round  sim_hours  curr_bytes  curr_blocks  max_bytes  total_blocks\n");
	let mut at_n = None;
	for round in 1..=2 * N {
		g.cpu.skip(STEP);
		if let Some(m) = &mut g.mem {
			m.skip(STEP);
		}
		g.sample();
		g.trim();
		let x = round % w;
		g.hover(Some((x, h / 2)), w, h, scale);
		std::hint::black_box(g.render(w, h, scale));
		std::hint::black_box(g.hover_tip(x, h / 2, w, h, scale));
		std::hint::black_box(g.tooltip());
		if round % KEEP_EVERY == 0 {
			g.save(&hist).unwrap();
			g.load(&hist).unwrap();
			let s = dhat::HeapStats::get();
			let _ = writeln!(
				report,
				"{round:>5}  {:>9.1}  {:>10}  {:>11}  {:>9}  {:>12}",
				round as f64 * STEP / 3600.0,
				s.curr_bytes,
				s.curr_blocks,
				s.max_bytes,
				s.total_blocks
			);
			if round == N {
				at_n = Some(s);
			}
		}
	}
	let _ = std::fs::remove_file(&hist);
	let (n, end) = (at_n.unwrap(), dhat::HeapStats::get());
	let _ = writeln!(
		report,
		"round {N}: {} bytes in {} blocks; round {}: {} bytes in {} blocks; margin {MARGIN_BYTES} bytes, {MARGIN_BLOCKS} blocks",
		n.curr_bytes,
		n.curr_blocks,
		2 * N,
		end.curr_bytes,
		end.curr_blocks
	);
	std::fs::write(dir.join("soak-heap.txt"), &report).unwrap();
	dhat::assert!(
		end.curr_bytes <= n.curr_bytes + MARGIN_BYTES,
		"the heap grew from {} to {} bytes",
		n.curr_bytes,
		end.curr_bytes
	);
	dhat::assert!(
		end.curr_blocks <= n.curr_blocks + MARGIN_BLOCKS,
		"the heap grew from {} to {} blocks",
		n.curr_blocks,
		end.curr_blocks
	);
}
