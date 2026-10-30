// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! Network traffic from `/proc/net/dev`. See design.md, Data sources on
//! Linux.

use std::time::Instant;

use crate::collect::{Climb, Collector, Counts, slurp};
use crate::config::Split;
use crate::lan::{self, LanReader};

const NET_DEV: &str = "/proc/net/dev";

/// Received and sent bytes for each interface. None on a row that doesn't
/// read.
pub(crate) fn parse_net_dev(text: &str, out: &mut Counts<2>) -> Option<()> {
	for line in text.lines() {
		// the 2 header lines have no colon, and a name can't hold one
		let Some((name, rest)) = line.split_once(':') else {
			continue;
		};
		let mut cols = rest.split_ascii_whitespace().map(str::parse::<u64>);
		let received = cols.next()?.ok()?;
		let sent = cols.nth(7)?.ok()?;
		out.push(name.trim(), [received, sent]);
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

fn rates(climb: &mut Climb<2>, text: &str, at: Instant) -> Option<[f32; 3]> {
	parse_net_dev(text, climb.next())?;
	climb.step(at, bin)
}

/// Bytes per second over every interface. An interface that comes or goes,
/// or a counter that wraps or resets, makes a gap.
#[derive(Debug, Default)]
pub struct NetReader {
	climb: Climb<2>,
	buf: String,
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
			slurp(NET_DEV, &mut self.buf)?,
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
	use std::fmt::Write;
	use std::time::Duration;

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
			parse_net_dev(&net_dev(&[("lo", 5, 5), ("enp8s0", 300, 40)]), &mut c),
			Some(())
		);
		assert_eq!(c.len(), 2);
		// old kernels ran a long count into the colon
		let mut c = Counts::default();
		assert_eq!(
			parse_net_dev("eth0:4294967296 1 0 0 0 0 0 0 12 1 0 0 0 0 0 0\n", &mut c),
			Some(())
		);
		let mut climb = Climb::default();
		let t = Instant::now();
		rates(&mut climb, "eth0:1 1 0 0 0 0 0 0 1 1 0 0 0 0 0 0\n", t);
		assert_eq!(
			rates(
				&mut climb,
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
			assert_eq!(parse_net_dev(junk, &mut Counts::default()), None, "{junk}");
		}
		let mut c = Counts::default();
		assert_eq!(parse_net_dev("", &mut c), Some(()));
		assert_eq!(c.len(), 0);
	}

	// Test ID: defna
	#[test]
	fn loopback_is_local_and_the_rest_sent_and_received() {
		let t = Instant::now();
		let mut climb = Climb::default();
		let rows = [("lo", 100, 100), ("enp8s0", 1000, 500), ("wlan0", 0, 0)];
		assert_eq!(rates(&mut climb, &net_dev(&rows), t), None);
		let rows = [
			("lo", 300, 300),
			("enp8s0", 5000, 900),
			("wlan0", 2000, 100),
		];
		assert_eq!(
			rates(&mut climb, &net_dev(&rows), t + Duration::from_secs(2)),
			Some([100.0, 250.0, 3000.0])
		);
	}

	// Test ID: defnb
	#[test]
	fn a_reset_or_an_interface_coming_or_going_is_a_gap() {
		let t = Instant::now();
		let s = Duration::from_secs;
		let mut climb = Climb::default();
		let both = [("lo", 100, 100), ("eth0", 1000, 1000), ("tun0", 50, 50)];
		rates(&mut climb, &net_dev(&both), t);
		let gone = [("lo", 100, 100), ("eth0", 1010, 1010)];
		assert_eq!(rates(&mut climb, &net_dev(&gone), t + s(1)), None);
		let gone = [("lo", 100, 100), ("eth0", 1020, 1030)];
		assert_eq!(
			rates(&mut climb, &net_dev(&gone), t + s(2)),
			Some([0.0, 20.0, 10.0])
		);
		// back with its counters from 0
		let back = [("lo", 100, 100), ("eth0", 1020, 1030), ("tun0", 5, 5)];
		assert_eq!(rates(&mut climb, &net_dev(&back), t + s(3)), None);
		let reset = [("lo", 100, 100), ("eth0", 3, 1030), ("tun0", 5, 5)];
		assert_eq!(rates(&mut climb, &net_dev(&reset), t + s(4)), None);
		let reset = [("lo", 100, 100), ("eth0", 4, 1030), ("tun0", 5, 5)];
		assert_eq!(
			rates(&mut climb, &net_dev(&reset), t + s(5)),
			Some([0.0, 0.0, 1.0])
		);
		// a read that doesn't parse leaves the last good one to rate from
		assert_eq!(rates(&mut climb, "lo: junk\n", t + s(6)), None);
		let later = [("lo", 100, 100), ("eth0", 14, 1030), ("tun0", 5, 5)];
		assert_eq!(
			rates(&mut climb, &net_dev(&later), t + s(7)),
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
