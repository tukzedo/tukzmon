// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! NVML lets go of the GPU once both GPU graphs are hidden. Its own test
//! binary, so no other test holds NVML open in the same process. A box with
//! no NVIDIA GPU opens nothing, and passes on that.

use tukzmon_core::config::{Look, Which};
use tukzmon_core::graphs::Graphs;

fn shown(on: bool) -> Look {
	let mut look = Look::default();
	look[Which::Gpu].show = on;
	look[Which::Vram].show = on;
	look
}

// Test ID: der6k
#[test]
#[cfg_attr(miri, ignore = "Miri can't load a library")]
fn hiding_both_gpu_graphs_lets_go_of_the_nvidia_device() {
	if tukzmon_core::gpu::nvidia_asleep() {
		// NVML isn't asked, so nothing would be open to count
		return;
	}
	let nvidia_files = || {
		std::fs::read_dir("/proc/self/fd")
			.unwrap()
			.filter_map(|e| std::fs::read_link(e.ok()?.path()).ok())
			.filter(|p| p.to_string_lossy().starts_with("/dev/nvidia"))
			.count()
	};
	let mut g = Graphs::new(shown(false));
	assert_eq!(nvidia_files(), 0);
	g.set_look(shown(true), None).unwrap();
	g.sample();
	let open = nvidia_files();
	assert!(
		open == 0 || g.has(Which::Gpu),
		"{open} open with no GPU graph"
	);
	g.set_look(shown(false), None).unwrap();
	assert_eq!(nvidia_files(), 0, "{open} were open while shown");
	g.set_look(shown(true), None).unwrap();
	assert_eq!(nvidia_files(), open, "opened again when shown again");
}
