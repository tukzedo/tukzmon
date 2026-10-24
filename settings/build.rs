// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

fn main() {
	built();
	#[cfg(feature = "qt")]
	qt();
}

// Build time as Unix seconds, shown in the viewer's own local date by --version.
// A release build (TUKZMON_RELEASE=1) leaves it out.
fn built() {
	let secs = std::env::var("SOURCE_DATE_EPOCH")
		.ok()
		.and_then(|s| s.parse::<u64>().ok())
		.unwrap_or_else(|| {
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.map_or(0, |d| d.as_secs())
		});
	let release = std::env::var("TUKZMON_RELEASE").is_ok_and(|v| v == "1");
	println!(
		"cargo:rustc-env=TUKZMON_BUILT={}",
		if release { 0 } else { secs }
	);
	println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
	println!("cargo:rerun-if-env-changed=TUKZMON_RELEASE");
	println!("cargo:rerun-if-changed=src");
	println!("cargo:rerun-if-changed=../core/src");
	println!("cargo:rerun-if-changed=../knobs/src");
}

#[cfg(feature = "qt")]
fn qt() {
	let lib = match pkg_config::Config::new()
		.atleast_version("6.2")
		.cargo_metadata(false)
		.probe("Qt6Widgets")
	{
		Ok(lib) => lib,
		Err(e) => panic!(
			"Qt 6 Widgets wasn't found, so the settings window can't be built: {e}\nOn Debian: apt install qt6-base-dev. Or build with --no-default-features."
		),
	};
	let mut b = cxx_build::bridge("src/bridge.rs");
	b.file("src/qt.cpp")
		.std("c++17")
		.flag_if_supported("-fPIC")
		.warnings(true);
	for dir in &lib.include_paths {
		b.include(dir);
	}
	for (k, v) in &lib.defines {
		b.define(k, v.as_deref());
	}
	b.compile("tukzmon-settings-qt");
	// after the archive, or the linker has already passed Qt by when it's needed
	for dir in &lib.link_paths {
		println!("cargo:rustc-link-search=native={}", dir.display());
	}
	for name in &lib.libs {
		println!("cargo:rustc-link-lib={name}");
	}
	println!("cargo:rerun-if-changed=src/qt.cpp");
	println!("cargo:rerun-if-changed=src/qt.h");
}
