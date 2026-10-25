// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

fn main() {
	built();
	#[cfg(feature = "qt")]
	qt();
}

// The build number is the commit's time, so a rebuild of one commit gets the
// same one. SOURCE_DATE_EPOCH wins when it's set. A tree with changes to what
// goes into the program, or one outside git, isn't any commit, so it takes the
// time of the build. main.rs shows it, and the day too unless
// TUKZMON_RELEASE=1.
fn built() {
	let secs = std::env::var("SOURCE_DATE_EPOCH")
		.ok()
		.and_then(|s| s.parse::<u64>().ok())
		.or_else(commit_time)
		.unwrap_or_else(|| {
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.map_or(0, |d| d.as_secs())
		});
	let release = std::env::var("TUKZMON_RELEASE").is_ok_and(|v| v == "1");
	println!(
		"cargo:rustc-env=TUKZMON_BUILT_MIN={}",
		secs.saturating_sub(946_684_800) / 60
	);
	println!("cargo:rustc-env=TUKZMON_RELEASE={}", u8::from(release));
	println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
	println!("cargo:rerun-if-env-changed=TUKZMON_RELEASE");
	for watched in [
		"src",
		"../core",
		"../knobs",
		"../Cargo.toml",
		"../Cargo.lock",
	] {
		println!("cargo:rerun-if-changed={watched}");
	}
	// A commit or a checkout moves these.
	for name in ["HEAD", "packed-refs"] {
		if let Some(p) = git(&["rev-parse", "--path-format=absolute", "--git-path", name]) {
			println!("cargo:rerun-if-changed={p}");
		}
	}
	if let Some(r) = git(&["symbolic-ref", "-q", "HEAD"])
		.and_then(|r| git(&["rev-parse", "--path-format=absolute", "--git-path", &r]))
	{
		println!("cargo:rerun-if-changed={r}");
	}
}

// HEAD's commit time, or None when the program's sources differ from it.
fn commit_time() -> Option<u64> {
	let changed = git(&[
		"--no-optional-locks",
		"status",
		"--porcelain",
		"--",
		".",
		"../core",
		"../knobs",
		"../Cargo.toml",
		"../Cargo.lock",
	])?;
	if !changed.is_empty() {
		return None;
	}
	git(&["log", "-1", "--format=%ct", "HEAD"])?.parse().ok()
}

fn git(args: &[&str]) -> Option<String> {
	let out = std::process::Command::new("git").args(args).output().ok()?;
	out.status
		.success()
		.then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
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
