// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! History on disk, one file per plugin instance, with a named section per
//! graph. See design.md, History.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAGIC: &[u8; 8] = b"tukzhist";
// 2 put each graph's series count at the head of its section, 2026103010491713
const VERSION: u32 = 2;

/// `$XDG_DATA_HOME/tukzmon`, else `~/.local/share/tukzmon`.
pub fn data_dir() -> PathBuf {
	let base = std::env::var_os("XDG_DATA_HOME")
		.filter(|v| !v.is_empty())
		.map(PathBuf::from)
		.or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
		.unwrap_or_else(|| PathBuf::from("."));
	base.join("tukzmon")
}

fn bad(what: &str) -> io::Error {
	io::Error::new(io::ErrorKind::InvalidData, what)
}

/// Writes beside the file and renames over it, so a crash mid-write leaves
/// the old one.
///
/// # Errors
///
/// Whatever creating the folder, writing, or renaming hit.
pub fn write(path: &Path, sections: &[(&str, &[u8])]) -> io::Result<()> {
	let out = pack(sections)?;
	if let Some(dir) = path.parent() {
		std::fs::create_dir_all(dir)?;
	}
	let tmp = temp_of(path, std::process::id());
	let put = || {
		let mut f = std::fs::File::create(&tmp)?;
		f.write_all(&out)?;
		f.sync_all()?;
		drop(f);
		std::fs::rename(&tmp, path)
	};
	put().inspect_err(|_| {
		// the write's error is the one to give
		let _ = remove(&tmp);
	})
}

fn pack(sections: &[(&str, &[u8])]) -> io::Result<Vec<u8>> {
	let mut out = Vec::with_capacity(
		16 + sections
			.iter()
			.map(|(n, b)| 9 + n.len() + b.len())
			.sum::<usize>(),
	);
	out.extend_from_slice(MAGIC);
	out.extend_from_slice(&VERSION.to_le_bytes());
	out.extend_from_slice(&(sections.len() as u32).to_le_bytes());
	for (name, body) in sections {
		let len = u8::try_from(name.len()).map_err(|_| bad("section name"))?;
		out.push(len);
		out.extend_from_slice(name.as_bytes());
		out.extend_from_slice(&(body.len() as u64).to_le_bytes());
		out.extend_from_slice(body);
	}
	Ok(out)
}

// by pid, since 2 sessions can each run an item with one id
fn temp_of(path: &Path, pid: u32) -> PathBuf {
	let mut tmp = path.as_os_str().to_owned();
	tmp.push(format!(".{pid}.tmp"));
	PathBuf::from(tmp)
}

/// Every section, by name. A version 1 file's sections come back with a
/// series count of 0 at their head, for "saved before there was one".
///
/// # Errors
///
/// A missing file is `NotFound`, and one that isn't whole is `InvalidData`.
pub fn read(path: &Path) -> io::Result<Vec<(String, Vec<u8>)>> {
	sections(&std::fs::read(path)?)
}

fn sections(bytes: &[u8]) -> io::Result<Vec<(String, Vec<u8>)>> {
	let rest = bytes
		.strip_prefix(MAGIC)
		.ok_or_else(|| bad("not a history file"))?;
	let (version, rest) = rest.split_first_chunk::<4>().ok_or_else(|| bad("short"))?;
	let uncounted = match u32::from_le_bytes(*version) {
		1 => true,
		VERSION => false,
		_ => return Err(bad("history file version")),
	};
	let (count, mut rest) = rest.split_first_chunk::<4>().ok_or_else(|| bad("short"))?;
	let mut sections = Vec::new();
	for _ in 0..u32::from_le_bytes(*count) {
		let (&len, r) = rest.split_first().ok_or_else(|| bad("short"))?;
		let (name, r) = r
			.split_at_checked(len as usize)
			.ok_or_else(|| bad("short"))?;
		let (size, r) = r.split_first_chunk::<8>().ok_or_else(|| bad("short"))?;
		let size = usize::try_from(u64::from_le_bytes(*size)).map_err(|_| bad("short"))?;
		let (body, r) = r.split_at_checked(size).ok_or_else(|| bad("short"))?;
		let name = std::str::from_utf8(name).map_err(|_| bad("section name"))?;
		let body = if uncounted {
			[&0u32.to_le_bytes()[..], body].concat()
		} else {
			body.to_vec()
		};
		sections.push((name.to_owned(), body));
		rest = r;
	}
	if !rest.is_empty() {
		return Err(bad("trailing bytes"));
	}
	Ok(sections)
}

/// One panel item's history file in `dir`, by its unique id.
pub fn history_file(dir: &Path, id: i32) -> PathBuf {
	dir.join(format!("plugin-{id}.hist"))
}

// the item id of a history file, or of its temp file, as a crash mid-write
// leaves, or a failed one before 2026103115232061
fn history_id(name: &str) -> Option<i32> {
	let name = name.strip_suffix(".tmp").map_or(name, |n| {
		n.rsplit_once('.')
			.filter(|(_, pid)| pid.parse::<u32>().is_ok())
			.map_or(n, |(n, _)| n)
	});
	name.strip_prefix("plugin-")
		.and_then(|n| n.strip_suffix(".hist"))
		.and_then(|id| id.parse().ok())
}

/// The panel item ids with a history file in `dir`, temp files included.
/// A folder that doesn't read has none.
pub fn history_ids(dir: &Path) -> Vec<i32> {
	let Ok(entries) = std::fs::read_dir(dir) else {
		return Vec::new();
	};
	let mut ids: Vec<i32> = entries
		.filter_map(|e| history_id(e.ok()?.file_name().to_str()?))
		.collect();
	ids.sort_unstable();
	ids.dedup();
	ids
}

/// The file whose modified time tells every running panel item to start its
/// graphs over. See [`reset`].
pub fn reset_mark(dir: &Path) -> PathBuf {
	dir.join("reset")
}

/// A missing file is already gone.
///
/// # Errors
///
/// Whatever else removing it hit.
pub fn remove(path: &Path) -> io::Result<()> {
	match std::fs::remove_file(path) {
		Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
		r => r,
	}
}

/// Asks every panel item to start its graphs over. The history file of each
/// item id that `goes` picks goes as well, and its temp files, so an item that
/// isn't running doesn't bring it back. A running one that keeps history
/// deletes its own again, since it may save in between.
///
/// # Errors
///
/// Whatever reading `dir`, deleting, or writing the mark hit.
pub fn reset(dir: &Path, goes: impl Fn(i32) -> bool) -> io::Result<()> {
	std::fs::create_dir_all(dir)?;
	for entry in std::fs::read_dir(dir)? {
		let entry = entry?;
		if entry
			.file_name()
			.to_str()
			.and_then(history_id)
			.is_some_and(&goes)
		{
			remove(&entry.path())?;
		}
	}
	let now = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map_or(0, |d| d.as_secs());
	std::fs::write(reset_mark(dir), format!("{now}\n"))
}

/// Notices a [`reset`] made by the settings window.
#[derive(Debug)]
pub struct ResetWatch {
	mark: PathBuf,
	seen: Option<SystemTime>,
}

impl ResetWatch {
	/// A reset made before this doesn't count.
	pub fn new(dir: &Path) -> ResetWatch {
		let mark = reset_mark(dir);
		let seen = modified(&mark);
		ResetWatch { mark, seen }
	}

	/// True once for each reset since the last call.
	pub fn asked(&mut self) -> bool {
		let t = modified(&self.mark);
		if t == self.seen {
			return false;
		}
		self.seen = t;
		t.is_some()
	}
}

fn modified(p: &Path) -> Option<SystemTime> {
	std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

#[cfg(test)]
mod tests {
	use super::*;

	fn scratch(name: &str) -> PathBuf {
		let dir =
			std::env::temp_dir().join(format!("test_tukzmon_keep_{}_{name}", std::process::id()));
		std::fs::create_dir_all(&dir).unwrap();
		dir
	}

	// Test ID: decbk
	#[test]
	fn sections_round_trip_no_tmp() {
		let dir = scratch("trip");
		let path = dir.join("sub/plugin-1.hist");
		write(&path, &[("cpu", b"abc"), ("mem", b"")]).unwrap();
		let got = read(&path).unwrap();
		assert_eq!(
			got,
			vec![("cpu".into(), b"abc".to_vec()), ("mem".into(), Vec::new())]
		);
		let names: Vec<_> = std::fs::read_dir(dir.join("sub"))
			.unwrap()
			.map(|e| e.unwrap().file_name())
			.collect();
		assert_eq!(names, ["plugin-1.hist"]);
		std::fs::remove_file(&path).unwrap();
		std::fs::remove_dir(dir.join("sub")).unwrap();
		std::fs::remove_dir(&dir).unwrap();
	}

	// Test ID: decbm
	#[test]
	fn truncated_files_refused() {
		let dir = scratch("cut");
		let path = dir.join("h");
		write(&path, &[("cpu", b"abcdef")]).unwrap();
		let full = std::fs::read(&path).unwrap();
		for n in 0..full.len() {
			std::fs::write(&path, &full[..n]).unwrap();
			assert!(read(&path).is_err(), "cut at {n}");
		}
		let mut long = full.clone();
		long.push(0);
		std::fs::write(&path, &long).unwrap();
		assert!(read(&path).is_err());
		std::fs::remove_file(&path).unwrap();
		std::fs::remove_dir(&dir).unwrap();
	}

	// Test ID: demwg
	#[test]
	fn version_1_sections_no_count() {
		let dir = scratch("v1");
		let path = dir.join("h");
		let mut v1 = MAGIC.to_vec();
		for x in [1u32, 1] {
			v1.extend_from_slice(&x.to_le_bytes());
		}
		v1.push(3);
		v1.extend_from_slice(b"cpu");
		v1.extend_from_slice(&2u64.to_le_bytes());
		v1.extend_from_slice(b"ab");
		std::fs::write(&path, &v1).unwrap();
		assert_eq!(
			read(&path).unwrap(),
			vec![("cpu".into(), b"\0\0\0\0ab".to_vec())]
		);
		// one from a later build isn't read
		v1[8..12].copy_from_slice(&3u32.to_le_bytes());
		std::fs::write(&path, &v1).unwrap();
		assert!(read(&path).is_err());
		std::fs::remove_file(&path).unwrap();
		std::fs::remove_dir(&dir).unwrap();
	}

	// Test ID: decbn
	#[test]
	fn missing_file_not_found() {
		let e = read(Path::new("/nonexistent/tukzmon/plugin-1.hist")).unwrap_err();
		assert_eq!(e.kind(), io::ErrorKind::NotFound);
	}

	// Test ID: defxq
	#[test]
	fn reset_seen_once() {
		let dir = scratch("reset");
		let hist = [history_file(&dir, 1), history_file(&dir, 22)];
		let other = [dir.join("plugin-x.hist"), dir.join("notes")];
		for p in hist.iter().chain(&other) {
			write(p, &[("cpu", b"abc")]).unwrap();
		}
		let mut watch = ResetWatch::new(&dir);
		assert!(!watch.asked());
		reset(&dir, |_| false).unwrap();
		assert!(watch.asked());
		assert!(!watch.asked());
		assert!(hist.iter().chain(&other).all(|p| p.exists()));
		// timestamps are coarse, so the next reset must be told apart
		std::thread::sleep(std::time::Duration::from_millis(50));
		reset(&dir, |_| true).unwrap();
		assert!(watch.asked());
		assert!(hist.iter().all(|p| !p.exists()));
		assert!(other.iter().all(|p| p.exists()));
		// one made before the watch started doesn't count
		assert!(!ResetWatch::new(&dir).asked());
		assert!(remove(&hist[0]).is_ok());
		for p in other.iter().chain([&reset_mark(&dir)]) {
			std::fs::remove_file(p).unwrap();
		}
		std::fs::remove_dir(&dir).unwrap();
	}

	// Test ID: deqs8
	#[test]
	fn failed_write_no_temp() {
		let dir = scratch("fail");
		// a folder in the way, so the rename fails after the temp is written
		let path = dir.join("plugin-1.hist");
		std::fs::create_dir_all(path.join("x")).unwrap();
		assert!(write(&path, &[("cpu", b"abc")]).is_err());
		let names: Vec<_> = std::fs::read_dir(&dir)
			.unwrap()
			.map(|e| e.unwrap().file_name())
			.collect();
		assert_eq!(names, ["plugin-1.hist"]);
		assert_ne!(temp_of(&path, 1), temp_of(&path, 2));
		std::fs::remove_dir(path.join("x")).unwrap();
		std::fs::remove_dir(&path).unwrap();
		std::fs::remove_dir(&dir).unwrap();
	}

	// Test ID: deqs9
	#[test]
	fn reset_clears_temp_files() {
		let dir = scratch("tmps");
		let gone = [
			temp_of(&history_file(&dir, 3), 4242),
			dir.join("plugin-3.hist.tmp"),
		];
		let kept = [dir.join("plugin-3.hist.x.tmp"), dir.join("plugin-3.tmp")];
		for p in gone.iter().chain(&kept) {
			std::fs::write(p, b"x").unwrap();
		}
		reset(&dir, |_| true).unwrap();
		assert!(gone.iter().all(|p| !p.exists()));
		assert!(kept.iter().all(|p| p.exists()));
		for p in kept.iter().chain([&reset_mark(&dir)]) {
			std::fs::remove_file(p).unwrap();
		}
		std::fs::remove_dir(&dir).unwrap();
	}

	fn decodes_somehow(body: &[u8]) -> bool {
		use crate::history::History;
		let mut any = false;
		any |= History::<1>::decode(body).is_some();
		any |= History::<2>::decode(body).is_some();
		any |= History::<3>::decode(body).is_some();
		any |= History::<4>::decode(body).is_some();
		any
	}

	// Test ID: df4f2
	#[test]
	fn fuzz_whole_or_nothing() {
		use crate::fuzz::{Dice, history, mangle, run};
		let body = |dice: &mut Dice| {
			let mut out = Vec::new();
			match dice.below(3) {
				0 => history::<2>(dice).encode(&mut out),
				1 => history::<3>(dice).encode(&mut out),
				_ => out.extend((0..dice.below(100)).map(|_| dice.below(256) as u8)),
			}
			out
		};
		let tally = run("history file", |dice| {
			let names = ["cpu", "mem", "net", "swap", "", "\u{e9}t\u{e9}"];
			let parts: Vec<(&str, Vec<u8>)> = (0..dice.below(5))
				.map(|_| (*dice.pick(&names), body(dice)))
				.collect();
			let refs: Vec<(&str, &[u8])> = parts.iter().map(|(n, b)| (*n, &b[..])).collect();
			let mut bytes = pack(&refs).expect("short names pack");
			let whole = dice.one_in(4);
			if whole {
				let got = sections(&bytes).expect("a whole file reads");
				assert!(
					got.iter()
						.map(|(n, b)| (n.as_str(), &b[..]))
						.eq(refs.iter().copied())
				);
			} else {
				mangle(dice, &mut bytes);
			}
			let Ok(got) = sections(&bytes) else {
				return false;
			};
			let refs: Vec<(&str, &[u8])> = got.iter().map(|(n, b)| (n.as_str(), &b[..])).collect();
			let again = sections(&pack(&refs).expect("read names pack")).expect("and read again");
			assert_eq!(again, got);
			for (_, b) in &got {
				decodes_somehow(b);
			}
			true
		});
		tally.assert_mixed();
	}

	// Test ID: df4f3
	#[test]
	fn fuzz_real_file_bit_flips() {
		let real = include_bytes!("../tests/data/plugin-5-tiers.hist");
		let tally = crate::fuzz::run("plugin-5-tiers.hist", |dice| {
			let mut bytes = real.to_vec();
			crate::fuzz::mangle(dice, &mut bytes);
			let Ok(got) = sections(&bytes) else {
				return false;
			};
			let mut any = false;
			for (_, b) in &got {
				any |= decodes_somehow(b);
			}
			any
		});
		tally.assert_mixed();
	}
}
