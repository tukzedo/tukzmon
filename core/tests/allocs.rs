// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! Allocations counted per thread, so tests running side by side don't
//! count each other's.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use knobs::{Model, NoEnv, Value};
use tukzmon_core::config::{self, Look, Mode, Split, Which};
use tukzmon_core::render::Vertical;

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

// Test ID: dezbs
#[test]
fn the_mode_split_and_scale_parsers_dont_allocate() {
	let n = allocs_in(|| {
		assert_eq!(Mode::parse("Gradient"), Some(Mode::Gradient));
		assert_eq!(Mode::parse("detailed"), Some(Mode::Detailed));
		assert_eq!(Mode::parse("gradients"), None);
		assert_eq!(Split::parse("SOCKETS"), Some(Split::Sockets));
		assert_eq!(Split::parse("off"), Some(Split::Off));
		assert_eq!(Split::parse(""), None);
		assert_eq!(Vertical::parse("Log10"), Some(Vertical::Log10));
		assert_eq!(Vertical::parse("squared"), Some(Vertical::Squared));
		assert_eq!(Vertical::parse("lin"), None);
	});
	assert_eq!(n, 0);
}

// Test ID: dezbt
#[test]
#[cfg_attr(miri, ignore = "a spec parse takes about an hour under Miri")]
fn a_look_makes_no_setting_names() {
	let mut m = Model::new(config::spec());
	m.set("user", &Value::Text("#123456".into()), &NoEnv);
	m.set("back_all", &Value::Bool(true), &NoEnv);
	let first = Look::from_model(&m);
	let n = allocs_in(|| assert_eq!(Look::from_model(&m), first));
	// about 23 a graph, from knobs' values. Making its names would add 12.
	let most = 30 * Which::ALL.len();
	assert!(n <= most, "{n} allocations, more than {most}");
}

// Test ID: dezbv
#[test]
#[cfg_attr(miri, ignore = "a spec parse takes about an hour under Miri")]
fn a_saved_theme_loads_without_listing_every_outline_id() {
	let doc = shcl::Document::parse(
		"themes:\n\tnight:\n\t\tname: Night\n\t\tuser: \"#102030\"\n\t\tborder_color: \"#405060\"\n",
	);
	let first = config::saved_themes(&doc);
	assert_eq!(first[0].colors["cpu_border_color"], "#405060");
	let members = first[0].colors.len();
	let n = allocs_in(|| assert_eq!(config::saved_themes(&doc), first));
	// about 20 a value now, nearly all in reading the file. It was 85.
	assert!(n <= 40 * members, "{n} allocations for {members} values");
}

// Test ID: dezd3
#[cfg(unix)]
#[test]
#[cfg_attr(miri, ignore = "Miri has no rewinddir")]
fn a_procs_read_allocates_nothing() {
	use tukzmon_core::collect::Collector;
	use tukzmon_core::procs::ProcsReader;
	let dir = std::env::temp_dir().join(format!(
		"test_tukzmon-core_{}_allocs_procs",
		std::process::id()
	));
	let _ = std::fs::remove_dir_all(&dir);
	for pid in 1..=50 {
		std::fs::create_dir_all(dir.join(pid.to_string())).unwrap();
	}
	let mut r = ProcsReader::at(&dir, 0);
	let first = r.read();
	let n = allocs_in(|| assert_eq!(r.read(), first));
	let _ = std::fs::remove_dir_all(&dir);
	assert_eq!(first.map(|c| c.iter().sum::<f32>()), Some(50.0));
	// it was 2 an entry
	assert_eq!(n, 0);
}
