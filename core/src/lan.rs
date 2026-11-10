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
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::Instant;

use crate::collect::{Collector, Kept};

const SOCK_DIAG_BY_FAMILY: u16 = 20;
/// The old `inet_diag_req` dump, which asks for both families at once.
const TCPDIAG_GETSOCK: u16 = 18;
const NLMSG_ERROR: u16 = 2;
const NLMSG_DONE: u16 = 3;
const NLM_F_REQUEST: u16 = 1;
const NLM_F_DUMP: u16 = 0x300;
const INET_DIAG_INFO: u16 = 2;
const RTM_NEWNEIGH: u16 = 28;
const RTM_GETNEIGH: u16 = 30;
const NDA_DST: u16 = 1;
const NDA_LLADDR: u16 = 2;
/// `struct ndmsg`.
const ND_MSG: usize = 12;
const HEADER: usize = 16;
/// `struct inet_diag_msg`.
const DIAG_MSG: usize = 72;

// TCP states, as bits in `idiag_states`. A listener moves no bytes, and a
// socket in SYN_RECV or TIME_WAIT has no `tcp_info`.
const SYN_RECV: u32 = 3;
const TIME_WAIT: u32 = 6;
const LISTEN: u32 = 10;
const STATES: u32 = 0xfff & !(1 << SYN_RECV | 1 << TIME_WAIT | 1 << LISTEN);

/// Where the TCP opens since boot are counted, which says when a socket the
/// last census can't have seen may be there.
const SNMP: &str = "/proc/net/snmp";
/// Lookups sent in one write. Their answers are all queued before it
/// returns, so this keeps them well inside the socket's receive buffer.
const ASKS: usize = 32;

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

fn mapped(ip: IpAddr) -> IpAddr {
	match ip {
		IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
		IpAddr::V4(_) => ip,
	}
}

/// Whether the far end is on the LAN. None for loopback, which the network
/// graph counts as local from `lo`. On IPv6 a global address in this end's
/// own /64 is on the LAN too.
fn on_lan(near: IpAddr, far: IpAddr) -> Option<bool> {
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

/// A dump request's header, at the head of `r`.
fn header(r: &mut [u8], kind: u16, seq: u32) {
	let len = r.len() as u32;
	r[0..4].copy_from_slice(&len.to_ne_bytes());
	r[4..6].copy_from_slice(&kind.to_ne_bytes());
	r[6..8].copy_from_slice(&(NLM_F_REQUEST | NLM_F_DUMP).to_ne_bytes());
	r[8..12].copy_from_slice(&seq.to_ne_bytes());
}

/// The dump request for one address family.
fn request(family: u8, seq: u32) -> [u8; HEADER + 56] {
	let mut r = [0; HEADER + 56];
	header(&mut r, SOCK_DIAG_BY_FAMILY, seq);
	// struct inet_diag_req_v2: family, protocol, the extensions wanted,
	// padding, then the states
	r[HEADER] = family;
	r[HEADER + 1] = libc::IPPROTO_TCP as u8;
	r[HEADER + 2] = 1 << (INET_DIAG_INFO - 1);
	r[HEADER + 4..HEADER + 8].copy_from_slice(&STATES.to_ne_bytes());
	r
}

/// The dump of every TCP socket of both families in one walk of the
/// kernel's table, which `request` takes one walk a family for.
fn census(seq: u32) -> [u8; HEADER + 60] {
	let mut r = [0; HEADER + 60];
	header(&mut r, TCPDIAG_GETSOCK, seq);
	// struct inet_diag_req: family, src_len, dst_len, the extensions
	// wanted, the 48-byte id, then the states
	r[HEADER + 3] = 1 << (INET_DIAG_INFO - 1);
	r[HEADER + 52..HEADER + 56].copy_from_slice(&STATES.to_ne_bytes());
	r
}

/// A lookup of one socket by its addresses, ports, interface and cookie,
/// which costs the kernel a hash lookup rather than a walk.
fn ask(sock: &Sock, seq: u32) -> [u8; HEADER + 56] {
	let mut r = request(sock.family, seq);
	r[6..8].copy_from_slice(&NLM_F_REQUEST.to_ne_bytes());
	r[HEADER + 8..].copy_from_slice(&sock.id);
	r
}

/// `ActiveOpens` plus `PassiveOpens` from `/proc/net/snmp`. Every TCP socket of
/// both families with a far end got there by one or the other.
pub(crate) fn opens(snmp: &str) -> Option<u64> {
	let mut rows = snmp.lines().filter_map(|l| l.strip_prefix("Tcp:"));
	let (names, values) = (rows.next()?, rows.next()?);
	names
		.split_whitespace()
		.zip(values.split_whitespace())
		.filter(|(name, _)| matches!(*name, "ActiveOpens" | "PassiveOpens"))
		// only compared for a change, so a wrap does no harm
		.try_fold(0, |sum: u64, (_, v)| {
			Some(sum.wrapping_add(v.parse().ok()?))
		})
}

/// A TCP socket with a private far end: bytes sent and received on the
/// wire, and whether it's the LAN's. One that's away is kept too, so when it
/// stops being away only what it moves from then on counts.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Sock {
	moved: [u64; 2],
	lan: bool,
	family: u8,
	/// Its `inet_diag_sockid`, to look it up by.
	id: [u8; 48],
}

/// Sockets with a private far end that still aren't the LAN's, since they
/// never cross an interface the network reader counts.
#[derive(Debug, Default)]
pub(crate) struct Away {
	/// This box's addresses on a VM's NAT bridge, a container's link or a
	/// VPN tunnel. A socket from one isn't the LAN's.
	pub(crate) mine: Vec<IpAddr>,
	/// Far ends behind a virtual port of a bridge that also has the NIC,
	/// such as a VM on b23's br0.
	pub(crate) guests: Vec<IpAddr>,
}

/// One `inet_diag_msg` and its attributes, with its cookie. None when the
/// far end isn't private, or it has no `tcp_info` new enough to count bytes.
fn lan_socket(msg: &[u8], away: &Away) -> Option<(u64, Sock)> {
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
	let (near, far) = (addr(8)?, addr(24)?);
	if !on_lan(near, far)? {
		return None;
	}
	let lan = !away.mine.contains(&mapped(near)) && !away.guests.contains(&mapped(far));
	// a dual-stack socket's IPv4 client sends IPv4 headers
	let wire_v6 = mapped(far).is_ipv6();
	let cookie = u64::from(u32_at(msg, 44)?) | u64::from(u32_at(msg, 48)?) << 32;
	let id = msg.get(4..52)?.try_into().ok()?;
	let mut at = DIAG_MSG;
	while at + 4 <= msg.len() {
		let len = usize::from(u16_at(msg, at)?);
		if len < 4 {
			return None;
		}
		if u16_at(msg, at + 2)? == INET_DIAG_INFO {
			let info = msg.get(at + 4..at + len)?;
			let segs = |at: usize| u64::from(u32_at(info, at).unwrap_or(0)) * overhead(wire_v6);
			let sent = u64_at(info, ACKED)?.saturating_add(segs(SEGS_OUT));
			let received = u64_at(info, RECEIVED)?.saturating_add(segs(SEGS_IN));
			return Some((
				cookie,
				Sock {
					moved: [sent, received],
					lan,
					family: msg[0],
					id,
				},
			));
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

/// Every message of dump `seq` in one read, sockets with a private far end
/// into `out`.
fn parse(buf: &[u8], seq: u32, away: &Away, out: &mut HashMap<u64, Sock>) -> Dump {
	walk(buf, seq, |kind, body| {
		if kind != SOCK_DIAG_BY_FAMILY && kind != TCPDIAG_GETSOCK {
			return;
		}
		if let Some((cookie, sock)) = lan_socket(body, away) {
			out.insert(cookie, sock);
		}
	})
}

/// Each message of dump `seq` in one read, by its type, to `each`. What's
/// left of an earlier dump that failed partway is skipped, or its messages
/// would count in this one.
fn walk(buf: &[u8], seq: u32, mut each: impl FnMut(u16, &[u8])) -> Dump {
	let mut at = 0;
	while at + HEADER <= buf.len() {
		let (Some(len), Some(kind), Some(its_seq)) =
			(u32_at(buf, at), u16_at(buf, at + 4), u32_at(buf, at + 8))
		else {
			return Dump::Bad;
		};
		let len = len as usize;
		let Some(body) = buf.get(at + HEADER..at + len).filter(|_| len >= HEADER) else {
			return Dump::Bad;
		};
		if its_seq != seq {
			at += align(len);
			continue;
		}
		match kind {
			NLMSG_DONE => return Dump::Done,
			NLMSG_ERROR => return Dump::Bad,
			_ => each(kind, body),
		}
		at += align(len);
	}
	Dump::More
}

/// A netlink socket for `protocol`.
fn netlink(protocol: i32) -> Option<File> {
	// SAFETY: socket(2) with constant arguments touches no memory of ours.
	let fd = unsafe {
		libc::socket(
			libc::AF_NETLINK,
			libc::SOCK_RAW | libc::SOCK_CLOEXEC,
			protocol,
		)
	};
	if fd < 0 {
		return None;
	}
	// SAFETY: fd is a new descriptor that nothing else owns.
	Some(File::from(unsafe { OwnedFd::from_raw_fd(fd) }))
}

/// Sends `req` and reads its dump to the end through `parse`. False when
/// any of it fails.
fn dump(sock: &mut File, buf: &mut [u8], req: &[u8], mut parse: impl FnMut(&[u8]) -> Dump) -> bool {
	if sock.write_all(req).is_err() {
		return false;
	}
	loop {
		let Ok(n) = sock.read(buf) else {
			return false;
		};
		match parse(&buf[..n]) {
			Dump::More if n > 0 => {}
			Dump::Done => return true,
			Dump::More | Dump::Bad => return false,
		}
	}
}

/// Answers to asks `first` to `first + n - 1` in one read, the sockets found
/// into `now`, and how many it had. One that closed answers ENOENT, and
/// is left out. None for anything else.
fn answers(
	buf: &[u8],
	first: u32,
	n: u32,
	away: &Away,
	now: &mut HashMap<u64, Sock>,
) -> Option<u32> {
	let mut had = 0;
	let mut at = 0;
	while at + HEADER <= buf.len() {
		let len = u32_at(buf, at)? as usize;
		let (kind, seq) = (u16_at(buf, at + 4)?, u32_at(buf, at + 8)?);
		let body = buf.get(at + HEADER..at + len).filter(|_| len >= HEADER)?;
		at += align(len);
		if seq.wrapping_sub(first) >= n {
			continue;
		}
		match kind {
			SOCK_DIAG_BY_FAMILY => {
				if let Some((cookie, sock)) = lan_socket(body, away) {
					now.insert(cookie, sock);
				}
			}
			NLMSG_ERROR => {
				// ESTALE is its address and port taken by a new socket
				let err = i32::from_ne_bytes(body.get(..4)?.try_into().ok()?).wrapping_neg();
				if err != libc::ENOENT && err != libc::ESTALE {
					return None;
				}
			}
			_ => return None,
		}
		had += 1;
	}
	Some(had)
}

/// Looks up every socket of `was` again, into `now`, `ASKS` to a write.
/// False when an answer is bad or missing, as when the receive buffer
/// overflowed.
fn follow(
	sock: &mut File,
	buf: &mut [u8],
	asks: &mut Vec<u8>,
	seq: &mut u32,
	was: &HashMap<u64, Sock>,
	away: &Away,
	now: &mut HashMap<u64, Sock>,
) -> bool {
	let mut left = was.values().peekable();
	while left.peek().is_some() {
		asks.clear();
		let first = seq.wrapping_add(1);
		let mut n = 0;
		for s in left.by_ref().take(ASKS) {
			*seq = seq.wrapping_add(1);
			asks.extend_from_slice(&ask(s, *seq));
			n += 1;
		}
		if sock.write_all(asks).is_err() {
			return false;
		}
		let mut had = 0;
		while had < n {
			// SAFETY: recv writes at most buf.len() bytes into buf.
			let got = unsafe {
				libc::recv(
					sock.as_raw_fd(),
					buf.as_mut_ptr().cast(),
					buf.len(),
					libc::MSG_DONTWAIT,
				)
			};
			// nothing queued is an answer lost
			let Some(got) = usize::try_from(got).ok().filter(|g| *g > 0) else {
				return false;
			};
			let Some(more) = answers(&buf[..got], first, n, away, now) else {
				return false;
			};
			had += more;
		}
	}
	true
}

/// One `RTM_NEWNEIGH` message's address and link address, when it has both.
fn neighbor(msg: &[u8]) -> Option<(IpAddr, [u8; 6])> {
	let v6 = match i32::from(*msg.first()?) {
		libc::AF_INET => false,
		libc::AF_INET6 => true,
		_ => return None,
	};
	let (mut ip, mut mac) = (None, None);
	let mut at = ND_MSG;
	while at + 4 <= msg.len() {
		let len = usize::from(u16_at(msg, at)?);
		if len < 4 {
			return None;
		}
		let data = msg.get(at + 4..at + len)?;
		match u16_at(msg, at + 2)? {
			NDA_DST if v6 => ip = <[u8; 16]>::try_from(data).ok().map(IpAddr::from),
			NDA_DST => ip = <[u8; 4]>::try_from(data).ok().map(IpAddr::from),
			NDA_LLADDR => mac = <[u8; 6]>::try_from(data).ok(),
			_ => {}
		}
		at += align(len);
	}
	Some((ip?, mac?))
}

/// Every neighbor this box has a link address for, IPv4 and IPv6, as
/// `ip neigh` lists them. False when they can't be had.
pub(crate) fn neighbors(mut each: impl FnMut(IpAddr, [u8; 6])) -> bool {
	let Some(mut sock) = netlink(libc::NETLINK_ROUTE) else {
		return false;
	};
	// AF_UNSPEC, for both families
	let mut req = [0; HEADER + ND_MSG];
	header(&mut req, RTM_GETNEIGH, 1);
	let mut buf = vec![0; 32 * 1024];
	dump(&mut sock, &mut buf, &req, |b| {
		walk(b, 1, |kind, body| {
			if let Some((ip, mac)) = (kind == RTM_NEWNEIGH).then(|| neighbor(body)).flatten() {
				each(mapped(ip), mac);
			}
		})
	})
}

/// Bytes the LAN's sockets moved since `was`, sent and received. A socket
/// that's new since then moved all it has. One that closed since then is
/// gone, with what it moved after `was`. One that was away then counts from
/// what it had then.
fn rise(was: &HashMap<u64, Sock>, now: &HashMap<u64, Sock>) -> [u64; 2] {
	now.iter()
		.filter(|(_, s)| s.lan)
		.fold([0; 2], |sum, (cookie, now)| {
			let then = was.get(cookie).map_or([0; 2], |s| s.moved);
			[0, 1].map(|i| sum[i].saturating_add(now.moved[i].saturating_sub(then[i])))
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
///
/// A dump walks the kernel's whole table of TCP connections, which is sized
/// by RAM, about 9 ms on b23 however few sockets there are. So a read only
/// dumps when a TCP socket opened since the last dump, and otherwise looks
/// up the ones it has.
#[derive(Debug)]
pub struct LanReader {
	/// What isn't the LAN's, though its far end is private.
	pub(crate) away: Away,
	/// None after a failed dump, since the kernel refuses a new dump on a
	/// socket while an old one is still going.
	sock: Option<File>,
	buf: Vec<u8>,
	asks: Vec<u8>,
	seq: u32,
	snmp: Kept,
	text: String,
	/// TCP opens at the last dump. None when the next read must dump.
	opens: Option<u64>,
	was: HashMap<u64, Sock>,
	now: HashMap<u64, Sock>,
	at: Option<Instant>,
}

impl LanReader {
	fn open() -> Option<LanReader> {
		Some(LanReader {
			away: Away::default(),
			sock: Some(netlink(libc::NETLINK_SOCK_DIAG)?),
			// the kernel caps one read of a dump at 32 KiB
			buf: vec![0; 32 * 1024],
			asks: Vec::with_capacity(ASKS * (HEADER + 56)),
			seq: 0,
			snmp: Kept::new(SNMP),
			text: String::new(),
			opens: None,
			was: HashMap::new(),
			now: HashMap::new(),
			at: None,
		})
	}

	/// Every socket with a private far end, of both families, into `now`.
	/// A dump when the TCP opens moved since the last one, or can't be
	/// read, and the sockets of `was` looked up again when they didn't.
	/// False when that failed, since a socket it missed would count all it
	/// has next time. The netlink socket goes with it, which ends what's
	/// left of a dump, so the next poll can start its own, and that one
	/// dumps.
	fn poll(&mut self) -> bool {
		self.now.clear();
		// read before the dump, so a socket opened during it moves them
		let opens = self.snmp.read(&mut self.text).and_then(opens);
		let sock = match &mut self.sock {
			Some(sock) => sock,
			None => match netlink(libc::NETLINK_SOCK_DIAG) {
				Some(sock) => self.sock.insert(sock),
				None => return false,
			},
		};
		let (away, now) = (&self.away, &mut self.now);
		let ok = if opens.is_some() && opens == self.opens {
			follow(
				sock,
				&mut self.buf,
				&mut self.asks,
				&mut self.seq,
				&self.was,
				away,
				now,
			)
		} else {
			self.seq = self.seq.wrapping_add(1);
			let seq = self.seq;
			dump(sock, &mut self.buf, &census(seq), |b| {
				parse(b, seq, away, now)
			})
		};
		self.opens = opens.filter(|_| ok);
		if !ok {
			self.sock = None;
		}
		ok
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
		assert_eq!(parse(&buf, 0, &Away::default(), &mut out), Dump::More);
		assert_eq!(out.len(), 2);
		assert_eq!(out[&(1 << 40 | 7)].moved, [1000 + 10 * 66, 5000 + 20 * 66]);
		assert_eq!(out[&9].moved, [100 + 86, 0]);
		assert_eq!(
			parse(&wrap(NLMSG_DONE, &[0; 4]), 0, &Away::default(), &mut out),
			Dump::Done
		);
		assert_eq!(
			parse(&wrap(NLMSG_ERROR, &[0; 20]), 0, &Away::default(), &mut out),
			Dump::Bad
		);
		// a length past the read, or under a header
		let mut cut = message("10.0.0.1", "10.0.0.2", 1, [1, 1], [1, 1]);
		cut.truncate(100);
		assert_eq!(parse(&cut, 0, &Away::default(), &mut out), Dump::Bad);
		let mut short = wrap(NLMSG_DONE, &[0; 4]);
		short[0..4].copy_from_slice(&8u32.to_ne_bytes());
		assert_eq!(parse(&short, 0, &Away::default(), &mut out), Dump::Bad);
		// a tcp_info too old to count bytes is skipped
		let mut old = message("10.0.0.1", "10.0.0.2", 77, [1, 1], [1, 1]);
		let info_len = HEADER + DIAG_MSG + 12;
		old[info_len..info_len + 2].copy_from_slice(&100u16.to_ne_bytes());
		assert_eq!(parse(&old, 0, &Away::default(), &mut out), Dump::More);
		assert!(!out.contains_key(&77));
		assert_eq!(request(libc::AF_INET as u8, 3).len(), 72);
	}

	// Test ID: deqt2
	#[test]
	fn whats_left_of_an_earlier_dump_is_skipped() {
		let seq = |mut m: Vec<u8>, seq: u32| {
			m[8..12].copy_from_slice(&seq.to_ne_bytes());
			m
		};
		let mut buf = seq(message("10.0.0.1", "10.0.0.2", 1, [5, 5], [0, 0]), 6);
		buf.extend(seq(wrap(NLMSG_ERROR, &[0; 20]), 6));
		buf.extend(seq(wrap(NLMSG_DONE, &[0; 4]), 6));
		buf.extend(seq(message("10.0.0.1", "10.0.0.3", 2, [7, 7], [0, 0]), 7));
		let mut out = HashMap::new();
		assert_eq!(parse(&buf, 7, &Away::default(), &mut out), Dump::More);
		let got: Vec<_> = out.iter().map(|(c, s)| (*c, s.moved, s.lan)).collect();
		assert_eq!(got, [(2, [7, 7], true)]);
		buf.extend(seq(wrap(NLMSG_DONE, &[0; 4]), 7));
		assert_eq!(parse(&buf, 7, &Away::default(), &mut out), Dump::Done);
	}

	// Test ID: deqt4
	#[test]
	fn a_socket_from_an_away_address_isnt_the_lans() {
		let mut buf = message("10.5.0.2", "10.5.0.1", 1, [5, 5], [0, 0]);
		buf.extend(message(
			"192.168.122.1",
			"192.168.122.40",
			2,
			[5, 5],
			[0, 0],
		));
		buf.extend(message(
			"::ffff:10.5.0.2",
			"::ffff:10.20.0.3",
			3,
			[5, 5],
			[0, 0],
		));
		buf.extend(message(
			"fe80::fc54:ff:fe01:2",
			"fe80::5054:ff:fe01:2",
			4,
			[5, 5],
			[0, 0],
		));
		buf.extend(message("192.168.1.123", "192.168.1.9", 5, [5, 5], [0, 0]));
		buf.extend(message("fe80::1:2", "fe80::9", 6, [5, 5], [0, 0]));
		let away = Away {
			mine: vec![
				ip("10.5.0.2"),
				ip("192.168.122.1"),
				ip("fe80::fc54:ff:fe01:2"),
			],
			guests: Vec::new(),
		};
		let mut out = HashMap::new();
		assert_eq!(parse(&buf, 0, &away, &mut out), Dump::More);
		let mut kept: Vec<u64> = out
			.into_iter()
			.filter(|(_, s)| s.lan)
			.map(|(c, _)| c)
			.collect();
		kept.sort_unstable();
		assert_eq!(kept, [5, 6]);
	}

	// Test ID: deqt6
	#[test]
	fn a_socket_to_a_vm_on_the_nics_bridge_isnt_the_lans() {
		let mut buf = message("192.168.1.123", "192.168.1.118", 1, [5, 5], [0, 0]);
		buf.extend(message(
			"::ffff:192.168.1.123",
			"::ffff:192.168.1.118",
			2,
			[5, 5],
			[0, 0],
		));
		buf.extend(message("2001:db8::5", "2001:db8::77", 3, [5, 5], [0, 0]));
		buf.extend(message("192.168.1.123", "192.168.1.9", 4, [5, 5], [0, 0]));
		let away = Away {
			mine: Vec::new(),
			guests: vec![ip("192.168.1.118"), ip("2001:db8::77")],
		};
		let mut out = HashMap::new();
		assert_eq!(parse(&buf, 0, &away, &mut out), Dump::More);
		let kept: Vec<u64> = out
			.into_iter()
			.filter(|(_, s)| s.lan)
			.map(|(c, _)| c)
			.collect();
		assert_eq!(kept, [4]);
	}

	// Test ID: df35c
	#[test]
	fn a_socket_that_stops_being_away_counts_from_its_last_read() {
		let vm = |moved: [u64; 2]| message("192.168.1.123", "192.168.1.118", 1, moved, [0, 0]);
		let mut away = Away {
			mine: Vec::new(),
			guests: vec![ip("192.168.1.118")],
		};
		let (mut was, mut now) = (HashMap::new(), HashMap::new());
		parse(&vm([4_000_000, 9_000_000]), 0, &away, &mut was);
		assert_eq!(rise(&HashMap::new(), &was), [0, 0], "away moves nothing");
		// the VM went idle and aged out of the bridge's table
		away.guests.clear();
		parse(&vm([4_000_500, 9_000_200]), 0, &away, &mut now);
		assert_eq!(rise(&was, &now), [500, 200]);
	}

	// Test ID: df35d
	#[test]
	fn a_dual_stack_socket_to_an_ipv4_client_counts_ipv4_headers() {
		let mut buf = message(
			"::ffff:192.168.1.123",
			"::ffff:192.168.1.9",
			1,
			[1000, 2000],
			[10, 20],
		);
		buf.extend(message("fd00::5", "fd00::9", 2, [1000, 2000], [10, 20]));
		let mut out = HashMap::new();
		parse(&buf, 0, &Away::default(), &mut out);
		assert_eq!(out[&1].moved, [1000 + 10 * 66, 2000 + 20 * 66]);
		assert_eq!(out[&2].moved, [1000 + 10 * 86, 2000 + 20 * 86]);
	}

	// Test ID: df35e
	#[test]
	#[cfg_attr(miri, ignore = "Miri has no sockets")]
	fn a_dump_left_partway_is_one_gap() {
		let mut r = LanReader::probe().expect("sock_diag needs no root");
		// enough sockets that a dump takes more than one read, and opened
		// since the last one, so the next read dumps
		let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
		let to = listener.local_addr().unwrap();
		let pairs: Vec<_> = (0..200)
			.map(|_| {
				let c = std::net::TcpStream::connect(to).unwrap();
				(c, listener.accept().unwrap().0)
			})
			.collect();
		// a read too short for one message cuts it, and the dump fails there
		r.buf.truncate(64);
		assert_eq!(r.read(), None);
		r.buf.resize(32 * 1024, 0);
		assert!(r.read().is_some(), "only the failed read is a gap");
		assert!(r.read().is_some());
		drop(pairs);
	}

	// Test ID: df3cx
	#[test]
	fn the_tcp_opens_are_active_plus_passive() {
		let snmp = "Ip: Forwarding DefaultTTL\nIp: 1 64\n\
			Tcp: RtoAlgorithm RtoMin RtoMax MaxConn ActiveOpens PassiveOpens AttemptFails\n\
			Tcp: 1 200 120000 -1 1234 56 7\n\
			Udp: InDatagrams\nUdp: 9\n";
		assert_eq!(opens(snmp), Some(1290));
		assert_eq!(opens("Tcp: ActiveOpens PassiveOpens\nTcp: 5 x\n"), None);
		assert_eq!(opens("Tcp: ActiveOpens PassiveOpens\n"), None);
		assert_eq!(opens(""), None);
	}

	// Test ID: df3cy
	#[test]
	fn answers_hold_the_sockets_found_and_count_the_gone() {
		let seq = |mut m: Vec<u8>, seq: u32| {
			m[8..12].copy_from_slice(&seq.to_ne_bytes());
			m
		};
		let error = |err: i32, n: u32| {
			let mut body = vec![0u8; 20];
			body[..4].copy_from_slice(&(-err).to_ne_bytes());
			seq(wrap(NLMSG_ERROR, &body), n)
		};
		let mut buf = seq(message("10.0.0.1", "10.0.0.2", 1, [5, 5], [0, 0]), 41);
		buf.extend(error(libc::ENOENT, 42));
		buf.extend(error(libc::ESTALE, 43));
		// an earlier ask's, and one from past this batch
		buf.extend(seq(message("10.0.0.1", "10.0.0.3", 2, [7, 7], [0, 0]), 40));
		buf.extend(error(libc::EINVAL, 44));
		let mut now = HashMap::new();
		assert_eq!(answers(&buf, 41, 3, &Away::default(), &mut now), Some(3));
		assert_eq!(now.keys().copied().collect::<Vec<_>>(), [1]);
		assert_eq!(answers(&buf, 41, 4, &Away::default(), &mut now), None);
		let mut cut = message("10.0.0.1", "10.0.0.2", 1, [5, 5], [0, 0]);
		cut.truncate(100);
		assert_eq!(answers(&cut, 0, 1, &Away::default(), &mut now), None);
		// a lookup is the dump request for its family, asked of one socket
		let sock = now[&1];
		let a = ask(&sock, 9);
		assert_eq!(u16_at(&a, 6), Some(NLM_F_REQUEST));
		assert_eq!(a[HEADER], libc::AF_INET as u8);
		assert_eq!(a[HEADER + 8..], sock.id);
		assert_eq!(u32_at(&sock.id, 40), Some(1), "the cookie");
	}

	// Test ID: df3cz
	#[test]
	#[cfg_attr(miri, ignore = "Miri has no netlink sockets")]
	fn one_census_has_both_families_and_a_lookup_finds_one_socket() {
		let pair = |at: &str| {
			let listener = std::net::TcpListener::bind(at).unwrap();
			let c = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
			(c, listener.accept().unwrap().0)
		};
		let v4 = pair("127.0.0.1:0");
		let v6 = std::net::TcpListener::bind("[::1]:0")
			.is_ok()
			.then(|| pair("[::1]:0"));
		let mut sock = netlink(libc::NETLINK_SOCK_DIAG).unwrap();
		let mut buf = vec![0; 32 * 1024];
		let mut found = Vec::new();
		assert!(dump(&mut sock, &mut buf, &census(8), |b| walk(
			b,
			8,
			|kind, body| {
				if kind == TCPDIAG_GETSOCK {
					let port = u16::from_be_bytes([body[4], body[5]]);
					found.push((
						i32::from(body[0]),
						port,
						<[u8; 48]>::try_from(&body[4..52]).unwrap(),
					));
				}
			}
		)));
		let port = |s: &std::net::TcpStream| s.local_addr().unwrap().port();
		let Some(&(_, _, id)) = found
			.iter()
			.find(|(family, p, _)| *family == libc::AF_INET && *p == port(&v4.0))
		else {
			panic!("the IPv4 pair isn't in the census");
		};
		if let Some((c, _)) = &v6 {
			assert!(
				found
					.iter()
					.any(|(family, p, _)| *family == libc::AF_INET6 && *p == port(c)),
				"the IPv6 pair isn't in the census"
			);
		}
		let target = Sock {
			moved: [0; 2],
			lan: true,
			family: libc::AF_INET as u8,
			id,
		};
		let mut lookup = |seq: u32| {
			sock.write_all(&ask(&target, seq)).unwrap();
			let n = sock.read(&mut buf).unwrap();
			(
				u16_at(&buf[..n], 4),
				buf.get(HEADER + 4..HEADER + 52).map(<[u8]>::to_vec),
			)
		};
		assert_eq!(lookup(9), (Some(SOCK_DIAG_BY_FAMILY), Some(id.to_vec())));
		// a reset leaves no TIME_WAIT behind to be found
		let linger = libc::linger {
			l_onoff: 1,
			l_linger: 0,
		};
		// SAFETY: setsockopt reads size_of::<linger>() bytes of linger.
		let set = unsafe {
			libc::setsockopt(
				v4.0.as_raw_fd(),
				libc::SOL_SOCKET,
				libc::SO_LINGER,
				(&raw const linger).cast(),
				size_of::<libc::linger>() as u32,
			)
		};
		assert_eq!(set, 0);
		drop(v4);
		assert_eq!(lookup(10).0, Some(NLMSG_ERROR));
	}

	// Test ID: df3d0
	#[test]
	#[cfg_attr(miri, ignore = "Miri has no netlink sockets")]
	fn a_read_dumps_only_when_a_tcp_socket_opened() {
		use crate::collect::Tree;
		let snmp = |n: u32| format!("Tcp: ActiveOpens PassiveOpens\nTcp: {n} 0\n");
		let tree = Tree::new("lan_opens", &[("snmp", &snmp(10))]);
		let mut r = LanReader::probe().expect("sock_diag needs no root");
		r.snmp = Kept::new(tree.at("snmp"));
		// with nothing to look up, a read that doesn't dump asks nothing
		let dumps = |r: &mut LanReader| {
			r.was.clear();
			let seq = r.seq;
			assert!(r.read().is_some());
			r.seq.wrapping_sub(seq)
		};
		assert_eq!(dumps(&mut r), 1, "the opens weren't read before");
		assert_eq!(dumps(&mut r), 0);
		assert_eq!(dumps(&mut r), 0);
		std::fs::write(tree.at("snmp"), snmp(11)).unwrap();
		assert_eq!(dumps(&mut r), 1);
		assert_eq!(dumps(&mut r), 0);
		r.snmp = Kept::new(tree.at("gone"));
		assert_eq!(dumps(&mut r), 1, "no opens, so every read dumps");
		assert_eq!(dumps(&mut r), 1);
	}

	// Test ID: df3d1
	#[test]
	#[cfg_attr(miri, ignore = "Miri has no netlink sockets")]
	fn a_read_with_no_new_socket_looks_up_the_ones_it_has() {
		use crate::collect::Tree;
		let tree = Tree::new("lan_follow", &[("snmp", "Tcp: ActiveOpens\nTcp: 3\n")]);
		let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
		let c = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
		let s = listener.accept().unwrap().0;
		let mut r = LanReader::probe().expect("sock_diag needs no root");
		r.snmp = Kept::new(tree.at("snmp"));
		assert!(r.read().is_some());
		// loopback is never kept, so put both ends in as if they were the LAN's
		let mut sock = netlink(libc::NETLINK_SOCK_DIAG).unwrap();
		let ports = [
			c.local_addr().unwrap().port(),
			s.local_addr().unwrap().port(),
		];
		let mut ends = HashMap::new();
		assert!(dump(&mut sock, &mut r.buf, &census(1), |b| walk(
			b,
			1,
			|kind, body| {
				let port = u16::from_be_bytes([body[4], body[5]]);
				if kind == TCPDIAG_GETSOCK
					&& body[0] == libc::AF_INET as u8
					&& ports.contains(&port)
				{
					let id: [u8; 48] = body[4..52].try_into().unwrap();
					let sock = Sock {
						moved: [0; 2],
						lan: true,
						family: body[0],
						id,
					};
					ends.insert(u64::from(port), sock);
				}
			}
		)));
		assert_eq!(ends.len(), 2);
		let asked = |r: &mut LanReader, ends: &HashMap<u64, Sock>| {
			r.was.clone_from(ends);
			let seq = r.seq;
			let ok = r.poll();
			(ok, r.seq.wrapping_sub(seq))
		};
		assert_eq!(asked(&mut r, &ends), (true, 2), "a lookup each, no dump");
		drop(c);
		assert_eq!(
			asked(&mut r, &ends),
			(true, 2),
			"one that's closing isn't a gap"
		);
		r.buf.truncate(64);
		assert!(!asked(&mut r, &ends).0, "an answer cut short is a gap");
		r.buf.resize(32 * 1024, 0);
		assert_eq!(asked(&mut r, &ends), (true, 1), "and the next read dumps");
		drop(s);
	}

	// Test ID: deqt7
	#[test]
	#[cfg_attr(miri, ignore = "Miri has no netlink sockets")]
	fn neighbors_read_with_their_link_addresses() {
		let attr = |kind: u16, data: &[u8]| {
			let mut a = Vec::new();
			a.extend_from_slice(&((4 + data.len()) as u16).to_ne_bytes());
			a.extend_from_slice(&kind.to_ne_bytes());
			a.extend_from_slice(data);
			a.resize(align(a.len()), 0);
			a
		};
		let neigh = |family: i32, dst: &[u8], mac: Option<&[u8]>| {
			let mut body = vec![0u8; ND_MSG];
			body[0] = family as u8;
			body.extend(attr(NDA_DST, dst));
			if let Some(mac) = mac {
				body.extend(attr(NDA_LLADDR, mac));
			}
			wrap(RTM_NEWNEIGH, &body)
		};
		let mac = [0x52, 0x54, 0, 0xea, 0xe3, 0xf3];
		let v6: Ipv6Addr = "2001:db8::77".parse().unwrap();
		let mut buf = neigh(libc::AF_INET, &[192, 168, 1, 118], Some(&mac));
		buf.extend(neigh(libc::AF_INET6, &v6.octets(), Some(&mac)));
		// one still being looked up has no link address
		buf.extend(neigh(libc::AF_INET, &[192, 168, 1, 50], None));
		buf.extend(wrap(NLMSG_DONE, &[0; 4]));
		let mut seen = Vec::new();
		let done = walk(&buf, 0, |kind, body| {
			if let Some(n) = (kind == RTM_NEWNEIGH).then(|| neighbor(body)).flatten() {
				seen.push(n);
			}
		});
		assert_eq!(done, Dump::Done);
		assert_eq!(
			seen,
			[(ip("192.168.1.118"), mac), (ip("2001:db8::77"), mac)]
		);
		assert!(neighbors(|_, _| {}), "a neighbor dump needs no root");
	}

	// Test ID: dekq7
	#[test]
	fn a_new_socket_moved_all_it_has_and_a_closed_one_nothing() {
		let lan = |m: [(u64, [u64; 2]); 2]| {
			m.into_iter()
				.map(|(c, moved)| {
					(
						c,
						Sock {
							moved,
							lan: true,
							family: 0,
							id: [0; 48],
						},
					)
				})
				.collect::<HashMap<_, _>>()
		};
		let was = lan([(1, [100, 1000]), (2, [50, 50])]);
		let now = lan([(1, [150, 1600]), (3, [10, 20])]);
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
	#[cfg_attr(miri, ignore = "Miri has no netlink sockets")]
	fn live_sockets_read_with_no_root() {
		let mut r = LanReader::probe().expect("sock_diag needs no root");
		std::thread::sleep(std::time::Duration::from_millis(20));
		let v = r.read().expect("a poll after the first gives rates");
		assert!(v.iter().all(|x| x.is_finite() && *x >= 0.0), "{v:?}");
	}

	/// A message's type, sequence number and body.
	type Sent = (u16, u32, Vec<u8>);

	/// A read of dump `seq` as the kernel would send it: sockets, neighbors,
	/// errors and the odd other message, some of them another dump's, maybe
	/// ended. With each message as `walk` should give it, and where each
	/// length, type, sequence and error field sits.
	fn rolled_dump(
		dice: &mut crate::fuzz::Dice,
		seq: u32,
	) -> (Vec<u8>, Vec<Sent>, Vec<(usize, usize)>) {
		let addrs = [
			"192.168.1.5",
			"10.0.0.9",
			"8.8.8.8",
			"fe80::1",
			"2001:db8::5",
			"fd12::7",
			"::ffff:192.168.1.9",
		];
		let (mut buf, mut sent, mut fields) = (Vec::new(), Vec::new(), Vec::new());
		for _ in 0..dice.below(12) {
			let at = buf.len();
			let mut attrs = Vec::new();
			let mut m = match dice.below(6) {
				0 | 1 => {
					let bytes = [dice.next() >> dice.below(64), dice.next() >> dice.below(64)];
					let segs = [dice.next() as u32, dice.next() as u32];
					let m = message(
						dice.pick(&addrs),
						dice.pick(&addrs),
						dice.next(),
						bytes,
						segs,
					);
					attrs.extend([HEADER + DIAG_MSG, HEADER + DIAG_MSG + 12]);
					m
				}
				2 | 3 => {
					let dst = ip(dice.pick(&addrs));
					let mut body = vec![0u8; ND_MSG];
					body[0] = (if dst.is_ipv6() {
						libc::AF_INET6
					} else {
						libc::AF_INET
					}) as u8;
					let octets = match dst {
						IpAddr::V4(v4) => v4.octets().to_vec(),
						IpAddr::V6(v6) => v6.octets().to_vec(),
					};
					for (kind, data) in [
						(NDA_DST, octets),
						(NDA_LLADDR, dice.next().to_ne_bytes()[..6].to_vec()),
					] {
						attrs.push(HEADER + body.len());
						body.extend_from_slice(&((data.len() + 4) as u16).to_ne_bytes());
						body.extend_from_slice(&kind.to_ne_bytes());
						body.extend(data);
						body.resize(align(body.len()), 0);
					}
					wrap(RTM_NEWNEIGH, &body)
				}
				4 => {
					let err = *dice.pick(&[0, libc::ENOENT, libc::ESTALE, libc::EINVAL]);
					let mut body = vec![0u8; 20];
					body[..4].copy_from_slice(&(-err).to_ne_bytes());
					attrs.push(HEADER);
					wrap(NLMSG_ERROR, &body)
				}
				_ => wrap(*dice.pick(&[NLMSG_DONE, 0, 99]), &[0; 4]),
			};
			let its_seq = if dice.one_in(5) {
				seq.wrapping_add(1)
			} else {
				seq
			};
			m[8..12].copy_from_slice(&its_seq.to_ne_bytes());
			let kind = u16::from_ne_bytes([m[4], m[5]]);
			let len = u32::from_ne_bytes(m[..4].try_into().unwrap()) as usize;
			sent.push((kind, its_seq, m[HEADER..len].to_vec()));
			fields.extend([(at, 4), (at + 4, 2), (at + 8, 4)]);
			fields.extend(attrs.into_iter().map(|a| (at + a, 4)));
			buf.extend(m);
		}
		(buf, sent, fields)
	}

	fn hurt(dice: &mut crate::fuzz::Dice, buf: &mut Vec<u8>, fields: &[(usize, usize)]) {
		if dice.one_in(2) && !fields.is_empty() {
			let &(at, width) = dice.pick(fields);
			crate::fuzz::lie(dice, buf, at, width);
		} else {
			crate::fuzz::mangle(dice, buf);
		}
	}

	// Test ID: df4fp
	#[test]
	fn lying_counts_and_errors_neither_panic_nor_wrap() {
		let mut body = vec![0u8; 20];
		body[..4].copy_from_slice(&i32::MIN.to_ne_bytes());
		let mut now = HashMap::new();
		assert_eq!(
			answers(&wrap(NLMSG_ERROR, &body), 0, 1, &Away::default(), &mut now),
			None
		);
		let huge = message("10.0.0.1", "10.0.0.2", 1, [u64::MAX; 2], [9, 9]);
		let (_, sock) = lan_socket(&huge[HEADER..], &Away::default()).unwrap();
		assert_eq!(sock.moved, [u64::MAX; 2]);
		let both = HashMap::from([(1, sock), (2, sock)]);
		assert_eq!(rise(&HashMap::new(), &both), [u64::MAX; 2]);
	}

	// Test ID: df4fm
	#[test]
	fn fuzz_dumps_give_their_messages_and_no_more() {
		let away = Away {
			mine: vec![ip("10.0.0.9")],
			guests: vec![],
		};
		let tally = crate::fuzz::run("netlink dump", |dice| {
			let seq = dice.next() as u32;
			let (mut buf, sent, fields) = rolled_dump(dice, seq);
			let whole = dice.one_in(4);
			if !whole {
				hurt(dice, &mut buf, &fields);
			}
			let mut got = Vec::new();
			let end = walk(&buf, seq, |kind, body| got.push((kind, body.to_vec())));
			let mut socks = HashMap::new();
			assert_eq!(parse(&buf, seq, &away, &mut socks), end);
			for (_, body) in &got {
				let _ = neighbor(body);
				let _ = lan_socket(body, &away);
			}
			if whole {
				let ours = sent.iter().filter(|(_, s, _)| *s == seq);
				let to_end = ours.take_while(|(k, _, _)| *k != NLMSG_DONE && *k != NLMSG_ERROR);
				let want: Vec<_> = to_end.map(|(k, _, b)| (*k, b.clone())).collect();
				assert_eq!(got, want);
				let ended = sent
					.iter()
					.find(|(k, s, _)| *s == seq && (*k == NLMSG_DONE || *k == NLMSG_ERROR));
				let want_end = match ended.map(|(k, _, _)| *k) {
					Some(NLMSG_DONE) => Dump::Done,
					Some(_) => Dump::Bad,
					None => Dump::More,
				};
				assert_eq!(end, want_end);
				for (kind, body) in &got {
					if *kind == RTM_NEWNEIGH {
						let (dst, mac) = neighbor(body).expect("a whole neighbor reads");
						let at = if dst.is_ipv6() { 20 } else { 8 };
						assert_eq!(mac[..], body[ND_MSG + at + 4..ND_MSG + at + 10]);
					}
				}
			}
			end != Dump::Bad && !got.is_empty()
		});
		tally.assert_mixed();
	}

	// Test ID: df4fn
	#[test]
	fn fuzz_lookup_answers_take_any_read() {
		let tally = crate::fuzz::run("netlink answers", |dice| {
			let first = dice.next() as u32;
			let n = dice.below(5) as u32;
			let (mut buf, mut fields) = (Vec::new(), Vec::new());
			for i in 0..n + u32::from(dice.one_in(4)) {
				let at = buf.len();
				let mut m = if dice.one_in(2) {
					fields.push((at + HEADER + DIAG_MSG + 12, 4));
					message("10.0.0.1", "10.0.0.2", dice.next(), [1, 2], [3, 4])
				} else {
					let err = *dice.pick(&[libc::ENOENT, libc::ESTALE, libc::EINVAL]);
					let mut body = vec![0u8; 20];
					body[..4].copy_from_slice(&(-err).to_ne_bytes());
					fields.push((at + HEADER, 4));
					wrap(NLMSG_ERROR, &body)
				};
				m[8..12].copy_from_slice(&first.wrapping_add(i).to_ne_bytes());
				fields.extend([(at, 4), (at + 4, 2), (at + 8, 4)]);
				buf.extend(m);
			}
			if !dice.one_in(4) {
				hurt(dice, &mut buf, &fields);
			}
			let mut now = HashMap::new();
			let had = answers(&buf, first, n, &Away::default(), &mut now);
			assert!(now.len() <= buf.len() / (HEADER + DIAG_MSG));
			had.is_some_and(|h| h > 0)
		});
		tally.assert_mixed();
	}
}
