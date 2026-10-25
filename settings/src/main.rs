// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! tukzmon-settings: the settings window, a program of its own so every panel
//! and platform can share it.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

#[cfg(feature = "qt")]
mod bridge;
mod dialog;
mod dice;
mod pick;

const USAGE: &str = "Usage: tukzmon-settings [--config FILE]

Opens the settings window for tukzmon's graphs.

  --config FILE    Settings file to edit. Default: ~/.config/tukzmon/tukzmon.shcl
  --snapshot PNG   Save a picture of each tab and the color picker, and quit, for
                   checking the layout.
  --help           Show this and quit.
  --version        Show the version and quit.";

fn main() -> ExitCode {
	let mut config = None;
	let mut snapshot = String::new();
	let mut args = std::env::args().skip(1);
	while let Some(a) = args.next() {
		match a.as_str() {
			"--config" | "--snapshot" => {
				let Some(v) = args.next() else {
					eprintln!("{a} needs a value.\n\n{USAGE}");
					return ExitCode::from(2);
				};
				if a == "--config" {
					config = Some(PathBuf::from(v));
				} else {
					snapshot = v;
				}
			}
			"--help" | "-h" => {
				println!("{USAGE}");
				return ExitCode::SUCCESS;
			}
			"--version" => {
				println!("{}\n{COPYRIGHT}", version());
				return ExitCode::SUCCESS;
			}
			_ => {
				eprintln!("Unknown option: {a}\n\n{USAGE}");
				return ExitCode::from(2);
			}
		}
	}
	let path = config.unwrap_or_else(tukzmon_core::config::config_file);
	let mut d = dialog::Dialog::open(path);
	run(&mut d, &snapshot)
}

const COPYRIGHT: &str = "Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)";

fn version() -> String {
	version_line(
		env!("TUKZMON_BUILT_MIN").parse().unwrap_or(0),
		env!("TUKZMON_RELEASE") == "1",
	)
}

// "tukzmon-settings v0.1.0 build decp8 (2026-10-24)". The build number is
// minutes since 2000 in Crockford base 32, and a dev build adds the day in
// the local time zone of whoever asks.
fn version_line(minutes: u64, release: bool) -> String {
	const DIGITS: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";
	let mut build = Vec::new();
	let mut n = minutes;
	loop {
		build.push(DIGITS[(n % 32) as usize]);
		n /= 32;
		if n == 0 {
			break;
		}
	}
	build.reverse();
	let mut line = format!(
		"tukzmon-settings v{} build {}",
		env!("CARGO_PKG_VERSION"),
		String::from_utf8_lossy(&build)
	);
	let day = i64::try_from(minutes)
		.ok()
		.and_then(|m| local_date(946_684_800 + m * 60));
	if let (false, Some(d)) = (release, day) {
		let _ = write!(line, " ({d})");
	}
	line
}

fn local_date(secs: i64) -> Option<String> {
	let t = libc::time_t::try_from(secs).ok()?;
	// SAFETY: localtime_r only writes the tm we hand it.
	let tm = unsafe {
		let mut tm: libc::tm = std::mem::zeroed();
		if libc::localtime_r(&raw const t, &raw mut tm).is_null() {
			return None;
		}
		tm
	};
	Some(format!(
		"{:04}-{:02}-{:02}",
		tm.tm_year + 1900,
		tm.tm_mon + 1,
		tm.tm_mday
	))
}

#[cfg(feature = "qt")]
fn run(d: &mut dialog::Dialog, snapshot: &str) -> ExitCode {
	ExitCode::from(bridge::run(d, snapshot).clamp(0, 255) as u8)
}

#[cfg(not(feature = "qt"))]
fn run(_d: &mut dialog::Dialog, _snapshot: &str) -> ExitCode {
	eprintln!("This tukzmon-settings was built without Qt, so it has no window.");
	ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
	use super::*;

	// Test ID: dednz
	#[test]
	fn the_version_line_has_the_build_and_a_dev_build_its_day() {
		let v = env!("CARGO_PKG_VERSION");
		// 2026-10-24 22:00 UTC, the 24th or 25th in every time zone.
		let line = version_line(14_103_240, false);
		let day = line
			.strip_prefix(&format!("tukzmon-settings v{v} build decp8"))
			.unwrap_or_else(|| panic!("{line}"));
		assert!([" (2026-10-24)", " (2026-10-25)"].contains(&day), "{line}");
		assert_eq!(day, format!(" ({})", local_date(1_792_879_200).unwrap()));
		assert_eq!(
			version_line(14_103_240, true),
			format!("tukzmon-settings v{v} build decp8")
		);
		assert!(version_line(0, true).ends_with(" build 0"));
		assert!(version().starts_with(&format!("tukzmon-settings v{v} build ")));
	}
}
