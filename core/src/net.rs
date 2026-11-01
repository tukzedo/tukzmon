// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! Network traffic from `/proc/net/dev`. See design.md, Data sources on
//! Linux.

use std::path::Path;
use std::time::Instant;

use crate::collect::{Climb, Collector, Counts, Kept};
use crate::config::Split;
use crate::lan::{self, LanReader};

const NET_DEV: &str = "/proc/net/dev";
const CLASS: &str = "/sys/class/net";

fn names(text: &str) -> impl Iterator<Item = &str> {
	// the 2 header lines have no colon, and a name can't hold one
	text.lines()
		.filter_map(|line| line.split_once(':'))
		.map(|(name, _)| name.trim())
}

/// Which interfaces count: `lo`, and those on hardware, which have a
/// `device` link in `/sys/class/net`. Bridges, VPN tunnels and VM and
/// container links have none, and what they carry either crosses a counted
/// one too or never leaves the box. A box with no hardware interface, such
/// as a container, counts them all.
#[derive(Debug)]
struct Wired {
	class: String,
	known: Vec<(String, bool)>,
	any: bool,
}

impl Wired {
	fn new(class: &str) -> Self {
		Wired {
			class: class.to_owned(),
			known: Vec::new(),
			any: false,
		}
	}

	/// Looks again only when the interfaces changed.
	fn sort(&mut self, text: &str) {
		let same = names(text).count() == self.known.len()
			&& names(text).zip(&self.known).all(|(n, (k, _))| n == k);
		if same {
			return;
		}
		let class = Path::new(&self.class);
		self.known = names(text)
			.map(|n| {
				(
					n.to_owned(),
					n != "lo" && class.join(n).join("device").exists(),
				)
			})
			.collect();
		self.any = self.known.iter().any(|(_, hw)| *hw);
	}

	fn counts(&self, name: &str) -> bool {
		name == "lo" || !self.any || self.known.iter().any(|(n, hw)| *hw && n == name)
	}
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
/// and sent and received hold it all.
#[derive(Debug, Default)]
pub struct Traffic {
	net: NetReader,
	lan: Option<LanReader>,
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
			Split::Sockets if self.lan.is_none() => self.lan = LanReader::probe(),
			Split::Sockets => {}
		}
	}
}

impl Collector<5> for Traffic {
	const SERIES: [&'static str; 5] = ["local", "LAN sent", "LAN received", "sent", "received"];

	fn probe() -> Option<Self> {
		NetReader::probe().map(|net| Traffic { net, lan: None })
	}

	/// A failed LAN poll is a gap, like a failed `/proc/net/dev` read, so
	/// the LAN's bytes never show as internet.
	fn read(&mut self) -> Option<[f32; 5]> {
		let net = self.net.read();
		let lan = match &mut self.lan {
			Some(r) => r.read()?,
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

	// Test ID: demw0
	#[test]
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
}
