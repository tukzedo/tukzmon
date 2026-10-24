// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

fn main() {
	#[cfg(feature = "qt")]
	qt();
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
