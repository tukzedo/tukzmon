// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! A point in the graph's history as text, for the hover line.

/// Which way the hover text gives a time. `hover_time` in settings.shcl.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Said {
	/// "7 hrs ago".
	Ago,
	/// The local time, with the date too when it's before today.
	#[default]
	Clock,
}

impl Said {
	pub fn parse(key: &str) -> Option<Said> {
		match key {
			"ago" => Some(Said::Ago),
			"clock" => Some(Said::Clock),
			_ => None,
		}
	}
}

const MINUTE: f64 = 60.0;
const HOUR: f64 = 3600.0;
const DAY: f64 = 86400.0;
const YEAR: f64 = 365.2425 * DAY;
const MONTH: f64 = YEAR / 12.0;

/// How long ago, rounded down to the biggest unit that fits.
pub fn ago(secs: f64) -> String {
	let secs = if secs.is_finite() { secs.max(0.0) } else { 0.0 };
	if secs < 1.0 {
		return "just now".into();
	}
	let (n, one, many) = if secs < MINUTE {
		(secs, "sec", "sec")
	} else if secs < HOUR {
		(secs / MINUTE, "min", "min")
	} else if secs < DAY {
		(secs / HOUR, "hr", "hrs")
	} else if secs < 7.0 * DAY {
		(secs / DAY, "day", "days")
	} else if secs < MONTH {
		(secs / (7.0 * DAY), "wk", "wks")
	} else if secs < YEAR {
		(secs / MONTH, "mo", "mos")
	} else {
		(secs / YEAR, "yr", "yrs")
	};
	let n = n.floor() as u64;
	format!("{n} {} ago", if n == 1 { one } else { many })
}

#[cfg(unix)]
fn local(t: f64) -> Option<libc::tm> {
	let t = t.floor() as libc::time_t;
	// SAFETY: localtime_r only writes the tm we hand it.
	unsafe {
		let mut tm: libc::tm = std::mem::zeroed();
		if libc::localtime_r(&raw const t, &raw mut tm).is_null() {
			return None;
		}
		Some(tm)
	}
}

/// `t` in local time, in the locale's own short formats: just the time if it's
/// on the same day as `now`, else the date and the time. Whether that's 12 or
/// 24 hour is `LC_TIME`'s call, which GTK has set by the time a panel calls.
#[cfg(unix)]
pub fn clock(t: f64, now: f64) -> Option<String> {
	let tm = local(t)?;
	let today = local(now).is_some_and(|n| (n.tm_year, n.tm_yday) == (tm.tm_year, tm.tm_yday));
	let item = |i| {
		// SAFETY: nl_langinfo gives a NUL-terminated string owned by the
		// locale, copied before anything could change it.
		unsafe { std::ffi::CStr::from_ptr(libc::nl_langinfo(i)) }
			.to_bytes()
			.to_vec()
	};
	let mut fmt = Vec::with_capacity(32);
	if !today {
		fmt.extend(item(libc::D_FMT));
		fmt.push(b' ');
	}
	fmt.extend(item(libc::T_FMT));
	fmt.push(0);
	let mut out = [0u8; 128];
	// SAFETY: fmt ends in NUL, and strftime writes at most out.len() bytes.
	let n = unsafe {
		libc::strftime(
			out.as_mut_ptr().cast(),
			out.len(),
			fmt.as_ptr().cast(),
			&raw const tm,
		)
	};
	if n == 0 {
		return None;
	}
	Some(String::from_utf8_lossy(&out[..n]).into_owned())
}

#[cfg(not(unix))]
pub fn clock(_t: f64, _now: f64) -> Option<String> {
	None
}

#[cfg(test)]
mod tests {
	use super::*;

	// Test ID: decca
	#[test]
	fn ago_reads_like_the_backlog_says() {
		for (secs, want) in [
			(0.2, "just now"),
			(-5.0, "just now"),
			(f64::NAN, "just now"),
			(1.0, "1 sec ago"),
			(42.9, "42 sec ago"),
			(60.0, "1 min ago"),
			(5.0 * MINUTE + 50.0, "5 min ago"),
			(HOUR, "1 hr ago"),
			(7.0 * HOUR + 59.0 * MINUTE, "7 hrs ago"),
			(DAY, "1 day ago"),
			(3.5 * DAY, "3 days ago"),
			(7.0 * DAY, "1 wk ago"),
			(20.0 * DAY, "2 wks ago"),
			(29.9 * DAY, "4 wks ago"),
			(31.0 * DAY, "1 mo ago"),
			(9.0 * MONTH + DAY, "9 mos ago"),
			(364.0 * DAY, "11 mos ago"),
			(366.0 * DAY, "1 yr ago"),
			(2.5 * YEAR, "2 yrs ago"),
		] {
			assert_eq!(ago(secs), want, "{secs}");
		}
	}

	// Test ID: deccb
	#[test]
	fn said_parses_as_the_spec_spells_it() {
		let spec = crate::config::spec();
		let Some(s) = spec.get("hover_time") else {
			panic!("no hover_time in settings.shcl");
		};
		let all: Vec<Said> = s
			.options
			.iter()
			.filter_map(|(key, _)| Said::parse(key))
			.collect();
		assert_eq!(all, [Said::Ago, Said::Clock]);
		assert_eq!(Said::parse(s.default.as_text()), Some(Said::default()));
	}

	// tests run in the C locale: %H:%M:%S, and %m/%d/%y for the date
	// Test ID: deccc
	#[cfg(unix)]
	#[test]
	#[cfg_attr(miri, ignore = "nl_langinfo and strftime are C, which Miri can't call")]
	fn clock_gives_the_date_only_before_today() {
		let now = std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_secs_f64();
		let today = clock(now, now).unwrap();
		assert_eq!(today.len(), 8, "{today}");
		assert_eq!(today.matches(':').count(), 2, "{today}");
		assert!(!today.contains('/'), "{today}");
		let old = clock(now - 3.0 * DAY, now).unwrap();
		assert_eq!(old.matches('/').count(), 2, "{old}");
		assert!(old.ends_with(&clock(now - 3.0 * DAY, now - 3.0 * DAY).unwrap()));
	}
}
