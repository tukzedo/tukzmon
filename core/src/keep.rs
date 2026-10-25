// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! History on disk, one file per plugin instance, with a named section per
//! graph. See design.md, History.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 8] = b"tukzhist";
const VERSION: u32 = 1;

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
	if let Some(dir) = path.parent() {
		std::fs::create_dir_all(dir)?;
	}
	let mut tmp = path.as_os_str().to_owned();
	tmp.push(".tmp");
	let tmp = PathBuf::from(tmp);
	let mut f = std::fs::File::create(&tmp)?;
	f.write_all(&out)?;
	f.sync_all()?;
	drop(f);
	std::fs::rename(&tmp, path)
}

/// Every section, by name.
///
/// # Errors
///
/// A missing file is `NotFound`, and one that isn't whole is `InvalidData`.
pub fn read(path: &Path) -> io::Result<Vec<(String, Vec<u8>)>> {
	let bytes = std::fs::read(path)?;
	let rest = bytes
		.strip_prefix(MAGIC)
		.ok_or_else(|| bad("not a history file"))?;
	let (version, rest) = rest.split_first_chunk::<4>().ok_or_else(|| bad("short"))?;
	if u32::from_le_bytes(*version) != VERSION {
		return Err(bad("history file version"));
	}
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
		sections.push((name.to_owned(), body.to_vec()));
		rest = r;
	}
	if !rest.is_empty() {
		return Err(bad("trailing bytes"));
	}
	Ok(sections)
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
	fn sections_round_trip_and_no_tmp_is_left() {
		let dir = scratch("trip");
		let path = dir.join("sub/plugin-1.hist");
		write(&path, &[("cpu", b"abc"), ("mem", b"")]).unwrap();
		let got = read(&path).unwrap();
		assert_eq!(
			got,
			vec![("cpu".into(), b"abc".to_vec()), ("mem".into(), Vec::new())]
		);
		assert!(!dir.join("sub/plugin-1.hist.tmp").exists());
		std::fs::remove_file(&path).unwrap();
		std::fs::remove_dir(dir.join("sub")).unwrap();
		std::fs::remove_dir(&dir).unwrap();
	}

	// Test ID: decbm
	#[test]
	fn every_cut_short_file_is_refused() {
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

	// Test ID: decbn
	#[test]
	fn a_missing_file_is_not_found() {
		let e = read(Path::new("/nonexistent/tukzmon/plugin-1.hist")).unwrap_err();
		assert_eq!(e.kind(), io::ErrorKind::NotFound);
	}
}
