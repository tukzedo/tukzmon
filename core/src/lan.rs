// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! TCP traffic to and from the LAN, from polling every TCP socket through
//! `sock_diag`, with no root. The network graph's LAN and internet split
//! takes the LAN from here and calls the rest of the interfaces' traffic
//! internet. See design.md, Data sources on Linux.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::mem::offset_of;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::os::fd::{FromRawFd, OwnedFd};
use std::time::Instant;

use crate::collect::Collector;

const SOCK_DIAG_BY_FAMILY: u16 = 20;
const NLMSG_ERROR: u16 = 2;
const NLMSG_DONE: u16 = 3;
const NLM_F_REQUEST: u16 = 1;
const NLM_F_DUMP: u16 = 0x300;
const INET_DIAG_INFO: u16 = 2;
const HEADER: usize = 16;
/// `struct inet_diag_msg`.
const DIAG_MSG: usize = 72;

// TCP states, as bits in `idiag_states`. A listener moves no bytes, and a
// socket in SYN_RECV or TIME_WAIT has no `tcp_info`.
const SYN_RECV: u32 = 3;
const TIME_WAIT: u32 = 6;
const LISTEN: u32 = 10;
const STATES: u32 = 0xfff & !(1 << SYN_RECV | 1 << TIME_WAIT | 1 << LISTEN);

const ACKED: usize = offset_of!(libc::tcp_info, tcpi_bytes_acked);
const RECEIVED: usize = offset_of!(libc::tcp_info, tcpi_bytes_received);
const SEGS_OUT: usize = offset_of!(libc::tcp_info, tcpi_segs_out);
const SEGS_IN: usize = offset_of!(libc::tcp_info, tcpi_segs_in);

/// What a full-size segment carries on the wire besides its payload:
/// Ethernet 14, IP, and TCP 20 with the timestamp option's 12, so the LAN's
/// part of an interface's count isn't short by a few percent.
const fn overhead(v6: bool) -> u64 {
	if v6 { 14 + 40 + 32 } else { 14 + 20 + 32 }
}

/// Whether the far end is on the LAN. None for loopback, which the network
/// graph counts as local from `lo`. On IPv6 a global address in this end's
/// own /64 is on the LAN too.
fn on_lan(near: IpAddr, far: IpAddr) -> Option<bool> {
	let mapped = |ip: IpAddr| match ip {
		IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
		IpAddr::V4(_) => ip,
	};
	let (near, far) = (mapped(near), mapped(far));
	// a socket to this box's own address goes through lo too
	if far.is_loopback() || far.is_unspecified() || far == near {
		return None;
	}
	Some(match (near, far) {
		(_, IpAddr::V4(f)) => f.is_private() || f.is_link_local(),
		(IpAddr::V6(n), IpAddr::V6(f)) => {
			let unique_local = f.segments()[0] & 0xfe00 == 0xfc00;
			let link_local = f.segments()[0] & 0xffc0 == 0xfe80;
			unique_local || link_local || n.segments()[..4] == f.segments()[..4]
		}
		(IpAddr::V4(_), IpAddr::V6(_)) => false,
	})
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
	Some(u16::from_ne_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
	Some(u32::from_ne_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], at: usize) -> Option<u64> {
	Some(u64::from_ne_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// Netlink pads every message and attribute to 4 bytes.
const fn align(n: usize) -> usize {
	(n + 3) & !3
}

/// The dump request for one address family.
fn request(family: u8, seq: u32) -> [u8; HEADER + 56] {
	let mut r = [0; HEADER + 56];
	r[0..4].copy_from_slice(&((HEADER + 56) as u32).to_ne_bytes());
	r[4..6].copy_from_slice(&SOCK_DIAG_BY_FAMILY.to_ne_bytes());
	r[6..8].copy_from_slice(&(NLM_F_REQUEST | NLM_F_DUMP).to_ne_bytes());
	r[8..12].copy_from_slice(&seq.to_ne_bytes());
	// struct inet_diag_req_v2: family, protocol, the extensions wanted,
	// padding, then the states
	r[HEADER] = family;
	r[HEADER + 1] = libc::IPPROTO_TCP as u8;
	r[HEADER + 2] = 1 << (INET_DIAG_INFO - 1);
	r[HEADER + 4..HEADER + 8].copy_from_slice(&STATES.to_ne_bytes());
	r
}

/// One LAN socket: its cookie, and bytes sent and received on the wire.
type Moved = (u64, [u64; 2]);

/// One `inet_diag_msg` and its attributes. None when it isn't a LAN socket
/// with a `tcp_info` new enough to count bytes.
fn lan_socket(msg: &[u8]) -> Option<Moved> {
	let v6 = match i32::from(*msg.first()?) {
		libc::AF_INET => false,
		libc::AF_INET6 => true,
		_ => return None,
	};
	let addr = |at: usize| -> Option<IpAddr> {
		let b = msg.get(at..at + 16)?;
		Some(if v6 {
			IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(b).ok()?))
		} else {
			IpAddr::V4(Ipv4Addr::new(b[0], b[1], b[2], b[3]))
		})
	};
	if !on_lan(addr(8)?, addr(24)?)? {
		return None;
	}
	let cookie = u64::from(u32_at(msg, 44)?) | u64::from(u32_at(msg, 48)?) << 32;
	let mut at = DIAG_MSG;
	while at + 4 <= msg.len() {
		let len = usize::from(u16_at(msg, at)?);
		if len < 4 {
			return None;
		}
		if u16_at(msg, at + 2)? == INET_DIAG_INFO {
			let info = msg.get(at + 4..at + len)?;
			let segs = |at: usize| u64::from(u32_at(info, at).unwrap_or(0)) * overhead(v6);
			let sent = u64_at(info, ACKED)? + segs(SEGS_OUT);
			let received = u64_at(info, RECEIVED)? + segs(SEGS_IN);
			return Some((cookie, [sent, received]));
		}
		at += align(len);
	}
	None
}

/// How far one read of a dump got.
#[derive(Debug, PartialEq)]
enum Dump {
	More,
	Done,
	Bad,
}

/// Every message in one read of a dump, LAN sockets into `out`.
fn parse(buf: &[u8], out: &mut HashMap<u64, [u64; 2]>) -> Dump {
	let mut at = 0;
	while at + HEADER <= buf.len() {
		let (Some(len), Some(kind)) = (u32_at(buf, at), u16_at(buf, at + 4)) else {
			return Dump::Bad;
		};
		let len = len as usize;
		let Some(body) = buf.get(at + HEADER..at + len).filter(|_| len >= HEADER) else {
			return Dump::Bad;
		};
		match kind {
			NLMSG_DONE => return Dump::Done,
			NLMSG_ERROR => return Dump::Bad,
			SOCK_DIAG_BY_FAMILY => {
				if let Some((cookie, moved)) = lan_socket(body) {
					out.insert(cookie, moved);
				}
			}
			_ => {}
		}
		at += align(len);
	}
	Dump::More
}

/// Bytes moved since `was`, sent and received. A socket that's new since
/// then moved all it has. One that closed since then is gone, with what it
/// moved after `was`.
fn rise(was: &HashMap<u64, [u64; 2]>, now: &HashMap<u64, [u64; 2]>) -> [u64; 2] {
	now.iter().fold([0; 2], |sum, (cookie, now)| {
		let then = was.get(cookie).copied().unwrap_or_default();
		[0, 1].map(|i| sum[i] + now[i].saturating_sub(then[i]))
	})
}

/// The 5 series of the split network graph, from the 3 of
/// [`crate::net::NetReader`] and the 2 of [`LanReader`]: local, LAN sent,
/// LAN received, internet sent and internet received. The LAN can't be more
/// than the interfaces carried, and what isn't the LAN's, UDP and QUIC
/// among it, is the internet's.
pub fn split([local, sent, received]: [f32; 3], [lan_sent, lan_received]: [f32; 2]) -> [f32; 5] {
	let lan_sent = lan_sent.clamp(0.0, sent);
	let lan_received = lan_received.clamp(0.0, received);
	[
		local,
		lan_sent,
		lan_received,
		sent - lan_sent,
		received - lan_received,
	]
}

/// Bytes per second over TCP to and from the LAN, sent and received. UDP,
/// QUIC among it, isn't counted, and a socket that closes between reads
/// loses what it moved since the last one.
#[derive(Debug)]
pub struct LanReader {
	sock: File,
	buf: Vec<u8>,
	seq: u32,
	was: HashMap<u64, [u64; 2]>,
	now: HashMap<u64, [u64; 2]>,
	at: Option<Instant>,
}

impl LanReader {
	fn open() -> Option<LanReader> {
		// SAFETY: socket(2) with constant arguments touches no memory of ours.
		let fd = unsafe {
			libc::socket(
				libc::AF_NETLINK,
				libc::SOCK_RAW | libc::SOCK_CLOEXEC,
				libc::NETLINK_SOCK_DIAG,
			)
		};
		if fd < 0 {
			return None;
		}
		// SAFETY: fd is a new descriptor that nothing else owns.
		let sock = File::from(unsafe { OwnedFd::from_raw_fd(fd) });
		Some(LanReader {
			sock,
			// the kernel caps one read of a dump at 32 KiB
			buf: vec![0; 32 * 1024],
			seq: 0,
			was: HashMap::new(),
			now: HashMap::new(),
			at: None,
		})
	}

	/// Every LAN socket of both families into `now`. False when a dump
	/// failed, since a socket it missed would count all it has next time.
	fn poll(&mut self) -> bool {
		self.now.clear();
		for family in [libc::AF_INET, libc::AF_INET6] {
			self.seq = self.seq.wrapping_add(1);
			if self
				.sock
				.write_all(&request(family as u8, self.seq))
				.is_err()
			{
				return false;
			}
			loop {
				let Ok(n) = self.sock.read(&mut self.buf) else {
					return false;
				};
				match parse(&self.buf[..n], &mut self.now) {
					Dump::More if n > 0 => {}
					Dump::Done => break,
					Dump::More | Dump::Bad => return false,
				}
			}
		}
		true
	}
}

impl Collector<2> for LanReader {
	const SERIES: [&'static str; 2] = ["LAN sent", "LAN received"];

	fn probe() -> Option<Self> {
		let mut r = LanReader::open()?;
		r.read();
		r.at.is_some().then_some(r)
	}

	/// Rates since the last call. The first call only primes it, and a
	/// failed poll is a gap that keeps the last good one to rate from.
	fn read(&mut self) -> Option<[f32; 2]> {
		if !self.poll() {
			return None;
		}
		let at = Instant::now();
		let secs = self
			.at
			.replace(at)
			.map(|then| at.saturating_duration_since(then).as_secs_f64());
		let out = secs
			.filter(|s| *s > 0.0)
			.map(|secs| rise(&self.was, &self.now).map(|x| (x as f64 / secs) as f32));
		std::mem::swap(&mut self.was, &mut self.now);
		out
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn ip(s: &str) -> IpAddr {
		s.parse().unwrap()
	}

	/// One `SOCK_DIAG_BY_FAMILY` message, with a `tcp_info` holding the
	/// bytes and segments given.
	fn message(near: &str, far: &str, cookie: u64, bytes: [u64; 2], segs: [u32; 2]) -> Vec<u8> {
		let (near, far) = (ip(near), ip(far));
		let mut body = vec![0u8; DIAG_MSG];
		body[0] = (if near.is_ipv6() {
			libc::AF_INET6
		} else {
			libc::AF_INET
		}) as u8;
		for (at, a) in [(8, near), (24, far)] {
			match a {
				IpAddr::V4(v4) => body[at..at + 4].copy_from_slice(&v4.octets()),
				IpAddr::V6(v6) => body[at..at + 16].copy_from_slice(&v6.octets()),
			}
		}
		body[44..48].copy_from_slice(&(cookie as u32).to_ne_bytes());
		body[48..52].copy_from_slice(&((cookie >> 32) as u32).to_ne_bytes());
		let mut info = vec![0u8; size_of::<libc::tcp_info>()];
		info[ACKED..ACKED + 8].copy_from_slice(&bytes[0].to_ne_bytes());
		info[RECEIVED..RECEIVED + 8].copy_from_slice(&bytes[1].to_ne_bytes());
		info[SEGS_OUT..SEGS_OUT + 4].copy_from_slice(&segs[0].to_ne_bytes());
		info[SEGS_IN..SEGS_IN + 4].copy_from_slice(&segs[1].to_ne_bytes());
		// an attribute before it that isn't wanted, 5 bytes and padded
		body.extend_from_slice(&9u16.to_ne_bytes());
		body.extend_from_slice(&1u16.to_ne_bytes());
		body.extend_from_slice(&[7, 7, 7, 7, 7, 0, 0, 0]);
		body.extend_from_slice(&((info.len() + 4) as u16).to_ne_bytes());
		body.extend_from_slice(&INET_DIAG_INFO.to_ne_bytes());
		body.extend(info);
		wrap(SOCK_DIAG_BY_FAMILY, &body)
	}

	fn wrap(kind: u16, body: &[u8]) -> Vec<u8> {
		let mut m = Vec::new();
		m.extend_from_slice(&((HEADER + body.len()) as u32).to_ne_bytes());
		m.extend_from_slice(&kind.to_ne_bytes());
		m.extend_from_slice(&[0; 10]);
		m.extend_from_slice(body);
		m.resize(align(m.len()), 0);
		m
	}

	// Test ID: dekq5
	#[test]
	fn the_lan_is_private_link_local_and_this_ends_own_prefix() {
		let lan = |near: &str, far: &str| on_lan(ip(near), ip(far));
		for far in ["10.1.2.3", "172.16.0.9", "192.168.1.182", "169.254.3.4"] {
			assert_eq!(lan("192.168.1.123", far), Some(true), "{far}");
		}
		for far in ["8.8.8.8", "172.32.0.1", "100.64.0.1", "2606:4700::1111"] {
			assert_eq!(lan("192.168.1.123", far), Some(false), "{far}");
		}
		assert_eq!(lan("2001:db8:1:2::5", "2001:db8:1:2::9"), Some(true));
		assert_eq!(lan("2001:db8:1:2::5", "2001:db8:1:3::9"), Some(false));
		assert_eq!(lan("2001:db8::5", "fd12::1"), Some(true));
		assert_eq!(lan("2001:db8::5", "fe80::1"), Some(true));
		assert_eq!(
			lan("::ffff:192.168.1.123", "::ffff:192.168.1.9"),
			Some(true)
		);
		assert_eq!(lan("::ffff:192.168.1.123", "::ffff:1.1.1.1"), Some(false));
		// lo carries these, which the graph counts as local
		assert_eq!(lan("127.0.0.1", "127.0.0.1"), None);
		assert_eq!(lan("::1", "::1"), None);
		assert_eq!(lan("192.168.1.123", "192.168.1.123"), None);
		assert_eq!(lan("::ffff:10.0.0.2", "10.0.0.2"), None);
		assert_eq!(lan("0.0.0.0", "0.0.0.0"), None);
	}

	// Test ID: dekq6
	#[test]
	fn a_dump_gives_lan_sockets_with_their_wire_bytes() {
		let mut buf = message(
			"192.168.1.123",
			"192.168.1.182",
			1 << 40 | 7,
			[1000, 5000],
			[10, 20],
		);
		buf.extend(message("192.168.1.123", "8.8.8.8", 8, [9, 9], [1, 1]));
		buf.extend(message("2001:db8::5", "fd00::2", 9, [100, 0], [1, 0]));
		buf.extend(message("127.0.0.1", "127.0.0.1", 10, [9, 9], [1, 1]));
		let mut out = HashMap::new();
		assert_eq!(parse(&buf, &mut out), Dump::More);
		assert_eq!(out.len(), 2);
		assert_eq!(out[&(1 << 40 | 7)], [1000 + 10 * 66, 5000 + 20 * 66]);
		assert_eq!(out[&9], [100 + 86, 0]);
		assert_eq!(parse(&wrap(NLMSG_DONE, &[0; 4]), &mut out), Dump::Done);
		assert_eq!(parse(&wrap(NLMSG_ERROR, &[0; 20]), &mut out), Dump::Bad);
		// a length past the read, or under a header
		let mut cut = message("10.0.0.1", "10.0.0.2", 1, [1, 1], [1, 1]);
		cut.truncate(100);
		assert_eq!(parse(&cut, &mut out), Dump::Bad);
		let mut short = wrap(NLMSG_DONE, &[0; 4]);
		short[0..4].copy_from_slice(&8u32.to_ne_bytes());
		assert_eq!(parse(&short, &mut out), Dump::Bad);
		// a tcp_info too old to count bytes is skipped
		let mut old = message("10.0.0.1", "10.0.0.2", 77, [1, 1], [1, 1]);
		let info_len = HEADER + DIAG_MSG + 12;
		old[info_len..info_len + 2].copy_from_slice(&100u16.to_ne_bytes());
		assert_eq!(parse(&old, &mut out), Dump::More);
		assert!(!out.contains_key(&77));
		assert_eq!(request(libc::AF_INET as u8, 3).len(), 72);
	}

	// Test ID: dekq7
	#[test]
	fn a_new_socket_moved_all_it_has_and_a_closed_one_nothing() {
		let was = HashMap::from([(1, [100, 1000]), (2, [50, 50])]);
		let now = HashMap::from([(1, [150, 1600]), (3, [10, 20])]);
		assert_eq!(rise(&was, &now), [60, 620]);
		assert_eq!(rise(&now, &now), [0, 0]);
		assert_eq!(rise(&now, &HashMap::new()), [0, 0]);
	}

	// Test ID: dekq8
	#[test]
	fn the_lan_is_capped_by_the_interfaces_and_the_rest_is_internet() {
		assert_eq!(
			split([5.0, 100.0, 300.0], [40.0, 200.0]),
			[5.0, 40.0, 200.0, 60.0, 100.0]
		);
		// headers guessed a little high, or a read a moment apart
		assert_eq!(
			split([0.0, 100.0, 300.0], [120.0, 300.5]),
			[0.0, 100.0, 300.0, 0.0, 0.0]
		);
		assert_eq!(
			split([0.0, 10.0, 10.0], [0.0, 0.0]),
			[0.0, 0.0, 0.0, 10.0, 10.0]
		);
	}

	// Test ID: dekq9
	#[test]
	fn live_sockets_read_with_no_root() {
		let mut r = LanReader::probe().expect("sock_diag needs no root");
		std::thread::sleep(std::time::Duration::from_millis(20));
		let v = r.read().expect("a poll after the first gives rates");
		assert!(v.iter().all(|x| x.is_finite() && *x >= 0.0), "{v:?}");
	}
}
