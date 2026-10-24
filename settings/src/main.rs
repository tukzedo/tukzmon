// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! tukzmon-settings: the settings window, a program of its own so every panel
//! and platform can share it.

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
				println!("tukzmon-settings {}", version());
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

// "0.1.0 (2026-10-24)" for a dev build, in the local date of whoever asks.
fn version() -> String {
	let v = env!("CARGO_PKG_VERSION");
	match env!("TUKZMON_BUILT").parse::<i64>() {
		Ok(secs) if secs > 0 => match local_date(secs) {
			Some(d) => format!("{v} ({d})"),
			None => v.to_string(),
		},
		_ => v.to_string(),
	}
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
