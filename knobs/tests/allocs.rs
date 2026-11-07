// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Bubbles

//! Allocations counted per thread, so tests running side by side don't
//! count each other's.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use knobs::{Model, NoEnv, Spec, Value};

thread_local! {
	static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

struct Counted;

// SAFETY: every call goes straight to System, with the same arguments.
unsafe impl GlobalAlloc for Counted {
	unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
		let _ = ALLOCS.try_with(|n| n.set(n.get() + 1));
		// SAFETY: as the caller promised us
		unsafe { System.alloc(layout) }
	}
	unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
		// SAFETY: as the caller promised us
		unsafe { System.dealloc(ptr, layout) }
	}
	unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
		let _ = ALLOCS.try_with(|n| n.set(n.get() + 1));
		// SAFETY: as the caller promised us
		unsafe { System.realloc(ptr, layout, size) }
	}
}

#[global_allocator]
static COUNTED: Counted = Counted;

fn allocs_in(f: impl FnOnce()) -> usize {
	let start = ALLOCS.with(Cell::get);
	f();
	ALLOCS.with(Cell::get) - start
}

// Test ID: dezbr
#[test]
fn asking_about_changes_costs_the_same_however_many_there_are() {
	let spec = Spec::parse(include_str!("../demo/demo.shcl")).unwrap();
	let mut m = Model::new(spec);
	m.set("profile", &Value::Text("low".into()), &NoEnv);
	assert_eq!(
		allocs_in(|| assert!(!m.has_changes("perf", &NoEnv))),
		0,
		"nothing changed, so nothing to work out"
	);
	let cost = |m: &Model| {
		allocs_in(|| {
			assert!(m.has_changes("perf", &NoEnv));
			std::hint::black_box(m.source("strength", &NoEnv));
			assert!(m.can_reset("strength", &NoEnv));
			assert_eq!(m.shown_choice("profile", &NoEnv), "Low *");
		})
	};
	m.set("strength", &Value::Int(80), &NoEnv);
	let one = cost(&m);
	for (id, v) in [
		("ease_in", 31),
		("ramp_down", 51),
		("radius", 5),
		("blur", 1),
	] {
		m.set(id, &Value::Int(v), &NoEnv);
	}
	assert!(m.changed("perf", &NoEnv).len() >= 5);
	assert_eq!(cost(&m), one, "no list of the changes is made");
}
