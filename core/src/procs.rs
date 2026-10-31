// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! How many processes there are, by who owns them, from the folders under
//! `/proc`. See design.md, Data sources on Linux.

use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

use crate::collect::Collector;

/// Below the first login user, as Debian and systemd number them.
const FIRST_USER: u32 = 1000;
const NOBODY: u32 = 65534;

/// Where a process owned by `uid` is counted, in [`ProcsReader::SERIES`]
/// order.
pub fn owner(uid: u32, me: u32) -> usize {
	match uid {
		_ if uid == me => 2,
		0..FIRST_USER | NOBODY => 0,
		_ => 1,
	}
}

/// Counts processes under `/proc` for the procs graph.
#[derive(Debug)]
pub struct ProcsReader {
	root: PathBuf,
	me: u32,
}

impl ProcsReader {
	/// Counts the numbered folders under `root`, with `me` as the user.
	pub fn at(root: impl Into<PathBuf>, me: u32) -> Self {
		ProcsReader {
			root: root.into(),
			me,
		}
	}
}

impl Collector<3> for ProcsReader {
	const SERIES: [&'static str; 3] = ["system", "users", "me"];

	fn probe() -> Option<Self> {
		// the owner of a process's folder is the user it runs as, ours too
		let me = std::fs::metadata("/proc/self").ok()?.uid();
		let mut r = ProcsReader::at("/proc", me);
		r.read()?;
		Some(r)
	}

	// The folder's owner, as htop reads it, rather than a status file per
	// process. A process that stopped itself being dumped reads as root's.
	fn read(&mut self) -> Option<[f32; 3]> {
		let mut n = [0u32; 3];
		for entry in std::fs::read_dir(&self.root).ok()?.flatten() {
			let name = entry.file_name();
			if !name.as_encoded_bytes().iter().all(u8::is_ascii_digit) {
				continue;
			}
			// gone since the listing
			let Ok(meta) = entry.metadata() else {
				continue;
			};
			n[owner(meta.uid(), self.me)] += 1;
		}
		Some(n.map(|x| x as f32))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	// Test ID: dep4h
	#[test]
	fn system_is_under_the_first_user_and_nobody() {
		assert_eq!(owner(0, 1001), 0);
		assert_eq!(owner(999, 1001), 0);
		assert_eq!(owner(NOBODY, 1001), 0);
		assert_eq!(owner(1000, 1001), 1);
		assert_eq!(owner(100_000, 1001), 1);
		assert_eq!(owner(1001, 1001), 2);
		// root running it counts root's as its own
		assert_eq!(owner(0, 0), 2);
	}

	// Test ID: dep4j
	#[test]
	fn only_numbered_entries_are_counted() {
		let dir = std::env::temp_dir().join(format!(
			"test_tukzmon-core_{}_procs_numbered",
			std::process::id()
		));
		let _ = std::fs::remove_dir_all(&dir);
		for name in ["1", "42", "self", "sys", "4x2", "net"] {
			std::fs::create_dir_all(dir.join(name)).unwrap();
		}
		std::fs::write(dir.join("7"), "").unwrap();
		let me = std::fs::metadata(&dir).unwrap().uid();
		let mut r = ProcsReader::at(&dir, me);
		let got = r.read();
		let _ = std::fs::remove_dir_all(&dir);
		let mine = [0, 1, 2].map(|i| if i == owner(me, me) { 3.0 } else { 0.0 });
		assert_eq!(got, Some(mine));
		assert_eq!(ProcsReader::at(dir, me).read(), None);
	}

	// Test ID: dep4k
	#[test]
	fn live_proc_counts_this_test_as_mine() {
		let mut r = ProcsReader::probe().unwrap();
		let [system, others, me] = r.read().unwrap();
		assert!(me >= 1.0, "{system} {others} {me}");
		assert!(system >= 0.0 && others >= 0.0);
	}
}
