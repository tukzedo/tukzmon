// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! How many processes there are, by who owns them, from the folders under
//! `/proc`. See design.md, Data sources on Linux.

use std::ffi::{CStr, CString};
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::ptr::NonNull;

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
	dir: Option<Listing>,
}

impl ProcsReader {
	/// Counts the numbered folders under `root`, with `me` as the user.
	pub fn at(root: impl Into<PathBuf>, me: u32) -> Self {
		ProcsReader {
			root: root.into(),
			me,
			dir: None,
		}
	}
}

/// A folder kept open and listed again from the start, so a read is one
/// `fstatat` a process, by name off the folder, with nothing allocated. A
/// `std::fs::ReadDir` can't rewind, and makes 2 strings an entry.
#[derive(Debug)]
struct Listing(NonNull<libc::DIR>);

impl Listing {
	fn open(path: &std::path::Path) -> Option<Self> {
		let path = CString::new(path.as_os_str().as_bytes()).ok()?;
		// SAFETY: path is NUL-terminated, and opendir gives null on failure.
		NonNull::new(unsafe { libc::opendir(path.as_ptr()) }).map(Listing)
	}

	fn count(&mut self, me: u32) -> [u32; 3] {
		let dir = self.0.as_ptr();
		let mut n = [0u32; 3];
		// SAFETY: dir is open until drop.
		let fd = unsafe {
			libc::rewinddir(dir);
			libc::dirfd(dir)
		};
		// SAFETY: as above. Null is the end, or an error, which ends it too.
		while let Some(entry) = NonNull::new(unsafe { libc::readdir(dir) }) {
			// SAFETY: d_name is NUL-terminated, and holds until the next readdir.
			let name = unsafe { CStr::from_ptr((*entry.as_ptr()).d_name.as_ptr()) };
			if !name.to_bytes().iter().all(u8::is_ascii_digit) {
				continue;
			}
			let mut st = MaybeUninit::<libc::stat>::uninit();
			// SAFETY: name is a C string under fd, and st is room for a stat.
			let gone = unsafe {
				libc::fstatat(
					fd,
					name.as_ptr(),
					st.as_mut_ptr(),
					libc::AT_SYMLINK_NOFOLLOW,
				)
			} != 0;
			// gone since the listing
			if gone {
				continue;
			}
			// SAFETY: fstatat filled it.
			let uid = unsafe { st.assume_init_ref() }.st_uid;
			n[owner(uid, me)] += 1;
		}
		n
	}
}

impl Drop for Listing {
	fn drop(&mut self) {
		// SAFETY: opened by Listing::open, and closed only here.
		unsafe { libc::closedir(self.0.as_ptr()) };
	}
}

impl Collector<3> for ProcsReader {
	const SERIES: [&'static str; 3] = ["system", "users", "me"];

	fn probe() -> Option<Self> {
		// Miri has no rewinddir, and a test that turns every graph on runs there
		if cfg!(miri) {
			return None;
		}
		// the owner of a process's folder is the user it runs as, ours too
		let me = std::fs::metadata("/proc/self").ok()?.uid();
		let mut r = ProcsReader::at("/proc", me);
		r.read()?;
		Some(r)
	}

	// The folder's owner, as htop reads it, rather than a status file per
	// process. A process that stopped itself being dumped reads as root's.
	fn read(&mut self) -> Option<[f32; 3]> {
		let dir = match &mut self.dir {
			Some(d) => d,
			None => self.dir.insert(Listing::open(&self.root)?),
		};
		Some(dir.count(self.me).map(|x| x as f32))
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
	#[cfg_attr(miri, ignore = "Miri has no rewinddir")]
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

	// Test ID: dezd2
	#[test]
	#[cfg_attr(miri, ignore = "Miri has no rewinddir")]
	fn each_read_lists_the_folder_again() {
		let dir = std::env::temp_dir().join(format!(
			"test_tukzmon-core_{}_procs_again",
			std::process::id()
		));
		let _ = std::fs::remove_dir_all(&dir);
		std::fs::create_dir_all(dir.join("1")).unwrap();
		let me = std::fs::metadata(&dir).unwrap().uid();
		let mine = owner(me, me);
		let mut r = ProcsReader::at(&dir, me);
		let first = r.read();
		for name in ["2", "3", "4"] {
			std::fs::create_dir(dir.join(name)).unwrap();
		}
		std::fs::remove_dir(dir.join("1")).unwrap();
		let second = r.read();
		let third = r.read();
		let _ = std::fs::remove_dir_all(&dir);
		assert_eq!(first.map(|n| n[mine]), Some(1.0));
		assert_eq!(second.map(|n| n[mine]), Some(3.0));
		assert_eq!(third, second);
	}

	// Test ID: dep4k
	#[test]
	#[cfg_attr(miri, ignore = "Miri has no rewinddir")]
	fn live_proc_counts_this_test_as_mine() {
		let mut r = ProcsReader::probe().unwrap();
		let [system, others, me] = r.read().unwrap();
		assert!(me >= 1.0, "{system} {others} {me}");
		assert!(system >= 0.0 && others >= 0.0);
	}
}
