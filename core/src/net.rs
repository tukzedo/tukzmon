// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! Network traffic from `/proc/net/dev`. See design.md, Data sources on
//! Linux.

use std::ffi::CStr;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::collect::{Climb, Collector, Counts, Kept};
use crate::config::Split;
use crate::lan::{self, Away, LanReader};

const NET_DEV: &str = "/proc/net/dev";
const CLASS: &str = "/sys/class/net";

/// How often the LAN reader's away addresses are read again, besides when
/// the interfaces change. One read cost about 0.4 ms on b23.
const RELIST: Duration = Duration::from_secs(10);

fn names(text: &str) -> impl Iterator<Item = &str> {
	// the 2 header lines have no colon, and a name can't hold one
	text.lines()
		.filter_map(|line| line.split_once(':'))
		.map(|(name, _)| name.trim())
}

/// What an interface is to [`Wired`].
#[derive(Debug, Clone, Copy, PartialEq)]
enum Link {
	/// Has a `device` link, and counts.
	Hardware,
	/// A bridge, bond or VLAN over a hardware one, which carries it too.
	Stacked,
	/// A bridge, VPN tunnel, or VM or container link with no hardware under
	/// it, so what it carries never crosses a counted one as itself.
	Virtual,
}

/// Which interfaces count: `lo`, and those on hardware, which have a
/// `device` link in `/sys/class/net`. Bridges, VPN tunnels and VM and
/// container links have none, and what they carry either crosses a counted
/// one too or never leaves the box. A box with no hardware interface, such
/// as a container, counts them all.
#[derive(Debug)]
struct Wired {
	class: String,
	known: Vec<(String, Link)>,
	any: bool,
	/// Goes up each time it looks again.
	looks: u32,
}

impl Wired {
	fn new(class: &str) -> Self {
		Wired {
			class: class.to_owned(),
			known: Vec::new(),
			any: false,
			looks: 0,
		}
	}

	/// Looks again only when the interfaces changed.
	fn sort(&mut self, text: &str) {
		let same = names(text).count() == self.known.len()
			&& names(text).zip(&self.known).all(|(n, (k, _))| n == k);
		if same {
			return;
		}
		self.looks = self.looks.wrapping_add(1);
		let class = Path::new(&self.class);
		self.known = names(text)
			.map(|n| {
				let hw = n != "lo" && class.join(n).join("device").exists();
				(
					n.to_owned(),
					if hw { Link::Hardware } else { Link::Virtual },
				)
			})
			.collect();
		self.any = self.known.iter().any(|(_, link)| *link == Link::Hardware);
		// each one's upper_ links name what's stacked on it, such as br0 on
		// enp8s0 and br0.5 on br0
		let mut todo: Vec<usize> = (0..self.known.len())
			.filter(|i| self.known[*i].1 == Link::Hardware)
			.collect();
		while let Some(at) = todo.pop() {
			let uppers = std::fs::read_dir(class.join(&self.known[at].0))
				.into_iter()
				.flatten()
				.flatten()
				.filter_map(|e| Some(e.file_name().to_str()?.strip_prefix("upper_")?.to_owned()));
			for up in uppers {
				if let Some(i) = self
					.known
					.iter()
					.position(|(n, link)| *n == up && *link == Link::Virtual)
				{
					self.known[i].1 = Link::Stacked;
					todo.push(i);
				}
			}
		}
	}

	fn link(&self, name: &str) -> Option<Link> {
		self.known
			.iter()
			.find(|(n, _)| n == name)
			.map(|(_, link)| *link)
	}

	fn counts(&self, name: &str) -> bool {
		name == "lo" || !self.any || self.link(name) == Some(Link::Hardware)
	}

	/// True when what goes out by `name` never crosses a counted interface,
	/// as with a VM's NAT bridge, a container's or a VPN tunnel.
	fn away(&self, name: &str) -> bool {
		self.any && name != "lo" && self.link(name) == Some(Link::Virtual)
	}
}

/// A bridge's ports that are [`Link::Virtual`], by port number, from its
/// `brif` folder.
fn virtual_ports(wired: &Wired, bridge: &str) -> Vec<u16> {
	let brif = Path::new(&wired.class).join(bridge).join("brif");
	std::fs::read_dir(brif)
		.into_iter()
		.flatten()
		.flatten()
		.filter(|port| port.file_name().to_str().and_then(|n| wired.link(n)) == Some(Link::Virtual))
		.filter_map(|port| {
			let text = std::fs::read_to_string(port.path().join("port_no")).ok()?;
			u16::from_str_radix(text.trim().trim_start_matches("0x"), 16).ok()
		})
		.collect()
}

/// The link addresses a bridge's `brforward` has on one of `ports`.
fn on_ports(fdb: &[u8], ports: &[u16]) -> Vec<[u8; 6]> {
	// struct __fdb_entry: the address, the port's low byte, is_local, the
	// ageing timer, the port's high byte
	fdb.chunks_exact(16)
		.filter(|e| e[7] == 0 && ports.contains(&(u16::from(e[12]) << 8 | u16::from(e[6]))))
		.filter_map(|e| e[..6].try_into().ok())
		.collect()
}

/// The far ends behind a virtual port of a bridge that has a hardware
/// interface too, such as a VM on b23's br0. The box's own address is on
/// that bridge, but what goes to them never crosses the NIC. False when
/// the neighbors can't be had.
fn guests(wired: &Wired, out: &mut Vec<IpAddr>) -> bool {
	let mut macs = Vec::new();
	for (bridge, _) in wired
		.known
		.iter()
		.filter(|(_, link)| *link == Link::Stacked)
	{
		let ports = virtual_ports(wired, bridge);
		if ports.is_empty() {
			continue;
		}
		if let Ok(fdb) = std::fs::read(Path::new(&wired.class).join(bridge).join("brforward")) {
			macs.extend(on_ports(&fdb, &ports));
		}
	}
	macs.is_empty()
		|| lan::neighbors(|ip, mac| {
			if macs.contains(&mac) {
				out.push(ip);
			}
		})
}

/// Every interface's addresses. False when they can't be had.
fn addresses(mut each: impl FnMut(&str, IpAddr)) -> bool {
	let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
	// SAFETY: getifaddrs only writes the list head, which freeifaddrs below
	// gives back.
	if unsafe { libc::getifaddrs(&raw mut list) } != 0 {
		return false;
	}
	let mut at = list;
	while !at.is_null() {
		// SAFETY: every node and what it points to lives till freeifaddrs,
		// and ifa_addr's family says which sockaddr it is.
		unsafe {
			let ifa = &*at;
			at = ifa.ifa_next;
			if ifa.ifa_addr.is_null() || ifa.ifa_name.is_null() {
				continue;
			}
			let ip = match i32::from((*ifa.ifa_addr).sa_family) {
				libc::AF_INET => {
					let sin = ifa.ifa_addr.cast::<libc::sockaddr_in>().read_unaligned();
					IpAddr::V4(Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr)))
				}
				libc::AF_INET6 => {
					let sin6 = ifa.ifa_addr.cast::<libc::sockaddr_in6>().read_unaligned();
					IpAddr::V6(Ipv6Addr::from(sin6.sin6_addr.s6_addr))
				}
				_ => continue,
			};
			if let Ok(name) = CStr::from_ptr(ifa.ifa_name).to_str() {
				each(name, ip);
			}
		}
	}
	// SAFETY: list came from getifaddrs and is freed once.
	unsafe { libc::freeifaddrs(list) };
	true
}

/// Received and sent bytes for each interface `keep` takes. None on a row
/// that doesn't read.
pub(crate) fn parse_net_dev(
	text: &str,
	out: &mut Counts<2>,
	keep: impl Fn(&str) -> bool,
) -> Option<()> {
	for line in text.lines() {
		let Some((name, rest)) = line.split_once(':') else {
			continue;
		};
		let mut cols = rest.split_ascii_whitespace().map(str::parse::<u64>);
		let received = cols.next()?.ok()?;
		let sent = cols.nth(7)?.ok()?;
		let name = name.trim();
		if keep(name) {
			out.push(name, [received, sent]);
		}
	}
	Some(())
}

/// Everything `lo` sends it also receives, so local is what it received.
fn bin(name: &str, [received, sent]: [u64; 2], sum: &mut [f64; 3]) {
	if name == "lo" {
		sum[0] += received as f64;
		return;
	}
	sum[1] += sent as f64;
	sum[2] += received as f64;
}

fn rates(climb: &mut Climb<2>, wired: &mut Wired, text: &str, at: Instant) -> Option<[f32; 3]> {
	wired.sort(text);
	parse_net_dev(text, climb.next(), |name| wired.counts(name))?;
	climb.step(at, bin)
}

/// Bytes per second over the hardware interfaces, as [`Wired`] picks them.
/// A counted interface that comes or goes, or a counter that wraps or
/// resets, makes a gap.
#[derive(Debug)]
pub struct NetReader {
	climb: Climb<2>,
	wired: Wired,
	dev: Kept,
	buf: String,
}

impl Default for NetReader {
	fn default() -> Self {
		NetReader {
			climb: Climb::default(),
			wired: Wired::new(CLASS),
			dev: Kept::new(NET_DEV),
			buf: String::new(),
		}
	}
}

impl Collector<3> for NetReader {
	const SERIES: [&'static str; 3] = ["local", "sent", "received"];

	fn probe() -> Option<Self> {
		let mut r = NetReader::default();
		r.read();
		r.climb.primed().then_some(r)
	}

	/// Rates since the last call. The first call only primes it.
	fn read(&mut self) -> Option<[f32; 3]> {
		rates(
			&mut self.climb,
			&mut self.wired,
			self.dev.read(&mut self.buf)?,
			Instant::now(),
		)
	}
}

/// What the network graph draws: [`NetReader`]'s 3 series split 5 ways by
/// [`lan::split`]. With the split off, or no LAN reader, the LAN's 2 are 0
/// and sent and received have it all.
#[derive(Debug, Default)]
pub struct Traffic {
	net: NetReader,
	lan: Option<LanReader>,
	/// When the LAN reader's away addresses were read, and at which look of
	/// the interfaces.
	listed: Option<(Instant, u32)>,
}

impl Traffic {
	/// True while the LAN is told apart from the internet.
	pub fn split(&self) -> bool {
		self.lan.is_some()
	}

	/// Starts or stops the LAN reader. One that can't start leaves the split
	/// off, and the next pick tries again.
	pub fn set_split(&mut self, split: Split) {
		match split {
			Split::Off => self.lan = None,
			Split::Sockets if self.lan.is_none() => {
				self.lan = LanReader::probe();
				self.listed = None;
			}
			Split::Sockets => {}
		}
	}
}

impl Collector<5> for Traffic {
	const SERIES: [&'static str; 5] = ["local", "LAN sent", "LAN received", "sent", "received"];

	fn probe() -> Option<Self> {
		NetReader::probe().map(|net| Traffic {
			net,
			lan: None,
			listed: None,
		})
	}

	/// A failed LAN poll is a gap, like a failed `/proc/net/dev` read, so
	/// the LAN's bytes never show as internet. A socket that goes out by a
	/// VM's, container's or VPN's interface isn't the LAN's, since the
	/// network reader doesn't count those.
	fn read(&mut self) -> Option<[f32; 5]> {
		let net = self.net.read();
		let lan = match &mut self.lan {
			Some(r) => {
				let wired = &self.net.wired;
				let at = Instant::now();
				let due = self.listed.is_none_or(|(then, looks)| {
					looks != wired.looks || at.saturating_duration_since(then) >= RELIST
				});
				let mut fresh = Away::default();
				// keep the last lists when there are no new ones, or the
				// away sockets would count all they have once they're left
				// out again
				if due
					&& addresses(|name, ip| {
						if wired.away(name) {
							fresh.mine.push(ip);
						}
					}) && guests(wired, &mut fresh.guests)
				{
					r.away = fresh;
					self.listed = Some((at, wired.looks));
				}
				r.read()?
			}
			None => [0.0; 2],
		};
		Some(lan::split(net?, lan))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::collect::Tree;
	use std::fmt::Write;
	use std::time::Duration;

	/// No /sys to look in, so every interface counts.
	fn all() -> Wired {
		Wired::new("/nonexistent/tukzmon")
	}

	fn net_dev(rows: &[(&str, u64, u64)]) -> String {
		let mut text = String::from(
			"Inter-|   Receive                                                |  Transmit\n \
			 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n",
		);
		for (name, received, sent) in rows {
			writeln!(
				text,
				"{name:>6}: {received} 9 0 0 0 0 0 0 {sent} 7 0 0 0 0 0 0"
			)
			.unwrap();
		}
		text
	}

	// Test ID: defn9
	#[test]
	fn reads_net_dev() {
		let mut c = Counts::default();
		assert_eq!(
			parse_net_dev(
				&net_dev(&[("lo", 5, 5), ("enp8s0", 300, 40)]),
				&mut c,
				|_| true
			),
			Some(())
		);
		assert_eq!(c.len(), 2);
		// old kernels ran a long count into the colon
		let mut c = Counts::default();
		assert_eq!(
			parse_net_dev(
				"eth0:4294967296 1 0 0 0 0 0 0 12 1 0 0 0 0 0 0\n",
				&mut c,
				|_| true
			),
			Some(())
		);
		let mut climb = Climb::default();
		let mut w = all();
		let t = Instant::now();
		rates(
			&mut climb,
			&mut w,
			"eth0:1 1 0 0 0 0 0 0 1 1 0 0 0 0 0 0\n",
			t,
		);
		assert_eq!(
			rates(
				&mut climb,
				&mut w,
				"eth0:4294967297 1 0 0 0 0 0 0 13 1 0 0 0 0 0 0\n",
				t + Duration::from_secs(1)
			),
			Some([0.0, 12.0, 4_294_967_296.0])
		);
		for junk in [
			"lo: 1 2 3\n",
			"lo: x 0 0 0 0 0 0 0 1\n",
			"lo: 1 0 0 0 0 0 0 0 -1\n",
		] {
			assert_eq!(
				parse_net_dev(junk, &mut Counts::default(), |_| true),
				None,
				"{junk}"
			);
		}
		let mut c = Counts::default();
		assert_eq!(parse_net_dev("", &mut c, |_| true), Some(()));
		assert_eq!(c.len(), 0);
	}

	// Test ID: defna
	#[test]
	fn loopback_is_local_and_the_rest_sent_and_received() {
		let t = Instant::now();
		let mut climb = Climb::default();
		let mut w = all();
		let rows = [("lo", 100, 100), ("enp8s0", 1000, 500), ("wlan0", 0, 0)];
		assert_eq!(rates(&mut climb, &mut w, &net_dev(&rows), t), None);
		let rows = [
			("lo", 300, 300),
			("enp8s0", 5000, 900),
			("wlan0", 2000, 100),
		];
		assert_eq!(
			rates(
				&mut climb,
				&mut w,
				&net_dev(&rows),
				t + Duration::from_secs(2)
			),
			Some([100.0, 250.0, 3000.0])
		);
	}

	// Test ID: defnb
	#[test]
	fn a_reset_or_an_interface_coming_or_going_is_a_gap() {
		let t = Instant::now();
		let s = Duration::from_secs;
		let mut climb = Climb::default();
		let mut w = all();
		let both = [("lo", 100, 100), ("eth0", 1000, 1000), ("tun0", 50, 50)];
		rates(&mut climb, &mut w, &net_dev(&both), t);
		let gone = [("lo", 100, 100), ("eth0", 1010, 1010)];
		assert_eq!(rates(&mut climb, &mut w, &net_dev(&gone), t + s(1)), None);
		let gone = [("lo", 100, 100), ("eth0", 1020, 1030)];
		assert_eq!(
			rates(&mut climb, &mut w, &net_dev(&gone), t + s(2)),
			Some([0.0, 20.0, 10.0])
		);
		// back with its counters from 0
		let back = [("lo", 100, 100), ("eth0", 1020, 1030), ("tun0", 5, 5)];
		assert_eq!(rates(&mut climb, &mut w, &net_dev(&back), t + s(3)), None);
		let reset = [("lo", 100, 100), ("eth0", 3, 1030), ("tun0", 5, 5)];
		assert_eq!(rates(&mut climb, &mut w, &net_dev(&reset), t + s(4)), None);
		let reset = [("lo", 100, 100), ("eth0", 4, 1030), ("tun0", 5, 5)];
		assert_eq!(
			rates(&mut climb, &mut w, &net_dev(&reset), t + s(5)),
			Some([0.0, 0.0, 1.0])
		);
		// a read that doesn't parse leaves the last good one to rate from
		assert_eq!(rates(&mut climb, &mut w, "lo: junk\n", t + s(6)), None);
		let later = [("lo", 100, 100), ("eth0", 14, 1030), ("tun0", 5, 5)];
		assert_eq!(
			rates(&mut climb, &mut w, &net_dev(&later), t + s(7)),
			Some([0.0, 0.0, 5.0])
		);
	}

	// Test ID: defnc
	#[test]
	fn live_net_dev_reads() {
		let mut r = NetReader::probe().expect("every Linux box has /proc/net/dev");
		assert!(!r.climb.last().is_empty());
		std::thread::sleep(Duration::from_millis(20));
		// an interface coming or going meanwhile is a gap
		if let Some(v) = r.read() {
			assert!(v.iter().all(|x| x.is_finite() && *x >= 0.0), "{v:?}");
		}
	}

	// Test ID: deqr3
	#[test]
	fn only_hardware_interfaces_count_unless_there_are_none() {
		let t = Instant::now();
		let s = Duration::from_secs;
		let sys = Tree::new(
			"net_wired",
			&[
				("enp8s0/device/", ""),
				("wlan0/device/", ""),
				("virbr0/", ""),
			],
		);
		let mut w = Wired::new(&sys.at(""));
		let mut climb = Climb::default();
		let mut rows = vec![
			("lo", 100, 100),
			("enp8s0", 1000, 500),
			("wlan0", 0, 0),
			("virbr0", 0, 0),
			("vnet3", 0, 0),
			("tun0", 0, 0),
		];
		rates(&mut climb, &mut w, &net_dev(&rows), t);
		// a VM pulling through the bridge, and a VPN, cross the NIC too
		rows[1] = ("enp8s0", 3000, 900);
		rows[2] = ("wlan0", 10, 10);
		rows[3] = ("virbr0", 50_000, 2000);
		rows[4] = ("vnet3", 2000, 50_000);
		rows[5] = ("tun0", 1500, 300);
		assert_eq!(
			rates(&mut climb, &mut w, &net_dev(&rows), t + s(1)),
			Some([0.0, 410.0, 2010.0])
		);
		// a container's link coming up is no gap
		rows.push(("veth1a2b", 7, 7));
		assert_eq!(
			rates(&mut climb, &mut w, &net_dev(&rows), t + s(2)),
			Some([0.0, 0.0, 0.0])
		);
		// in a container, all there is counts
		let mut w = Wired::new(&sys.at("none"));
		let mut climb = Climb::default();
		let rows = [("lo", 0, 0), ("eth0", 100, 100), ("tun0", 0, 0)];
		rates(&mut climb, &mut w, &net_dev(&rows), t);
		let rows = [("lo", 0, 0), ("eth0", 300, 100), ("tun0", 50, 0)];
		assert_eq!(
			rates(&mut climb, &mut w, &net_dev(&rows), t + s(1)),
			Some([0.0, 0.0, 250.0])
		);
	}

	// Test ID: deqt3
	#[test]
	fn only_what_has_no_hardware_under_it_is_away() {
		// br0 bridges the NIC and the VMs on vnet0, and br0.5 is a VLAN on
		// it. virbr0 is a NAT bridge, and nordlynx a VPN.
		let sys = Tree::new(
			"net_away",
			&[
				("br0.5/", ""),
				("enp8s0/device/", ""),
				("enp8s0/upper_br0", ""),
				("br0/upper_br0.5", ""),
				("vnet0/upper_br0", ""),
				("virbr0/", ""),
				("vnet1/upper_virbr0", ""),
				("nordlynx/", ""),
			],
		);
		let rows = [
			("lo", 0, 0),
			("br0.5", 0, 0),
			("enp8s0", 0, 0),
			("br0", 0, 0),
			("vnet0", 0, 0),
			("virbr0", 0, 0),
			("vnet1", 0, 0),
			("nordlynx", 0, 0),
		];
		let mut w = Wired::new(&sys.at(""));
		w.sort(&net_dev(&rows));
		let away: Vec<&str> = rows.iter().map(|r| r.0).filter(|n| w.away(n)).collect();
		assert_eq!(away, ["vnet0", "virbr0", "vnet1", "nordlynx"]);
		assert!(!w.away("wlan9"), "one it hasn't seen");
		let counted: Vec<&str> = rows.iter().map(|r| r.0).filter(|n| w.counts(n)).collect();
		assert_eq!(counted, ["lo", "enp8s0"]);
		// in a container nothing is away, since all of it counts
		let mut w = Wired::new(&sys.at("none"));
		w.sort(&net_dev(&rows));
		assert!(rows.iter().all(|r| !w.away(r.0)));
	}

	// Test ID: deqt8
	#[test]
	fn a_vm_behind_the_nics_bridge_is_found_by_its_port() {
		let sys = Tree::new(
			"net_guests",
			&[
				("enp8s0/device/", ""),
				("enp8s0/upper_br0", ""),
				("br0/brif/enp8s0/port_no", "0x1\n"),
				("br0/brif/vnet0/port_no", "0x2\n"),
				("br0/brif/vnet1/port_no", "0x104\n"),
				("vnet0/upper_br0", ""),
				("vnet1/upper_br0", ""),
			],
		);
		let rows = [
			("lo", 0, 0),
			("enp8s0", 0, 0),
			("br0", 0, 0),
			("vnet0", 0, 0),
			("vnet1", 0, 0),
		];
		let mut w = Wired::new(&sys.at(""));
		w.sort(&net_dev(&rows));
		let mut ports = virtual_ports(&w, "br0");
		ports.sort_unstable();
		assert_eq!(ports, [2, 0x104]);
		let entry = |mac: u8, port: u16, local: bool| {
			let mut e = [0u8; 16];
			e[..6].copy_from_slice(&[0x52, 0x54, 0, 0, 0, mac]);
			e[6] = port as u8;
			e[7] = u8::from(local);
			e[12] = (port >> 8) as u8;
			e
		};
		let fdb: Vec<u8> = [
			entry(1, 1, false),
			entry(2, 2, false),
			entry(3, 2, true),
			entry(4, 0x104, false),
			entry(5, 4, false),
		]
		.concat();
		let macs: Vec<u8> = on_ports(&fdb, &ports).iter().map(|m| m[5]).collect();
		assert_eq!(macs, [2, 4]);
	}

	// Test ID: deqt5
	#[test]
	#[cfg_attr(miri, ignore = "getifaddrs is C, which Miri can't call")]
	fn this_boxs_addresses_include_loopback() {
		let mut lo = Vec::new();
		assert!(addresses(|name, ip| if name == "lo" {
			lo.push(ip);
		}));
		assert!(lo.contains(&IpAddr::V4(Ipv4Addr::LOCALHOST)), "{lo:?}");
	}

	// Test ID: demw0
	#[test]
	#[cfg_attr(miri, ignore = "Miri has no netlink sockets")]
	fn with_the_split_off_sent_and_received_hold_it_all() {
		let mut t = Traffic::probe().expect("every Linux box has /proc/net/dev");
		assert!(!t.split());
		std::thread::sleep(Duration::from_millis(20));
		let v = t.read().expect("a read after the first gives rates");
		assert_eq!(v[1..3], [0.0, 0.0], "{v:?}");
		t.set_split(Split::Sockets);
		assert!(t.split());
		std::thread::sleep(Duration::from_millis(20));
		let v = t.read().expect("the LAN reader came primed");
		assert!(v.iter().all(|x| x.is_finite() && *x >= 0.0), "{v:?}");
	}

	// Test ID: df4g4
	#[test]
	fn fuzz_brforward_gives_only_whole_entries_on_its_ports() {
		let tally = crate::fuzz::run("brforward", |dice| {
			let ports: Vec<u16> = (0..dice.below(4))
				.map(|_| *dice.pick(&[1, 2, 0xff, 0x100, 0x104, u16::MAX]))
				.collect();
			let (mut fdb, mut want, mut fields) = (Vec::new(), Vec::new(), Vec::new());
			for _ in 0..dice.below(12) {
				let at = fdb.len();
				let mut e = dice.next().to_le_bytes().repeat(2);
				let port = if dice.one_in(2) && !ports.is_empty() {
					*dice.pick(&ports)
				} else {
					dice.next() as u16
				};
				e[6] = port as u8;
				e[7] = u8::from(dice.one_in(3));
				e[12] = (port >> 8) as u8;
				if e[7] == 0 && ports.contains(&port) {
					want.push(<[u8; 6]>::try_from(&e[..6]).unwrap());
				}
				fields.extend([at + 6, at + 7, at + 12]);
				fdb.extend(e);
			}
			if dice.one_in(4) {
				assert_eq!(on_ports(&fdb, &ports), want);
			} else if dice.one_in(2) && !fields.is_empty() {
				let at = *dice.pick(&fields);
				crate::fuzz::lie(dice, &mut fdb, at, 1);
			} else {
				crate::fuzz::mangle(dice, &mut fdb);
			}
			let got = on_ports(&fdb, &ports);
			let entries: Vec<&[u8]> = fdb.chunks_exact(16).collect();
			assert!(got.len() <= entries.len());
			let mut left = entries.iter();
			for mac in &got {
				// each from a whole entry of its own, in order, on one of the
				// ports and not one of the box's own
				let found = left.find(|e| {
					e[..6] == mac[..]
						&& e[7] == 0 && ports.contains(&u16::from_le_bytes([e[6], e[12]]))
				});
				assert!(found.is_some(), "{mac:?} isn't from an entry on {ports:?}");
			}
			!got.is_empty()
		});
		tally.assert_mixed();
	}
}
