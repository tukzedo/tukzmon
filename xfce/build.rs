// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

// Links the runtime libraries by soname, so no -dev packages are needed to
// build. The wrapper process has them all loaded already anyway.

fn main() {
	for lib in [
		"libxfce4panel-2.0.so.4",
		"libgtk-3.so.0",
		"libgdk-3.so.0",
		"libcairo.so.2",
		"libgio-2.0.so.0",
		"libgobject-2.0.so.0",
		"libglib-2.0.so.0",
	] {
		println!("cargo:rustc-link-lib=dylib:+verbatim={lib}");
	}
}
