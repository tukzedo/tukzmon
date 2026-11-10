// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! Whether this desktop is shown remotely, so the graphs can draw less often.
//! See design.md Drawing.
//!
//! A session a remote desktop server started, such as an RDP or NoMachine
//! virtual desktop, is told once, by what it was started with. A viewer that
//! joins a local desktop, such as x11vnc, RustDesk or NoMachine on the
//! physical display, comes and goes, so [`Watch`] looks for one again every
//! few seconds.

use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::{Duration, Instant};

/// Variables a remote desktop server sets in the sessions it starts: xrdp,
/// X2Go, Chrome Remote Desktop, and the classic vncserver script.
const REMOTE_VARS: [&str; 4] = [
	"XRDP_SESSION",
	"X2GO_SESSION",
	"CHROME_REMOTE_DESKTOP_SESSION",
	"VNCDESKTOP",
];

/// The PAM services logind names for sessions a remote desktop server opens:
/// NoMachine's virtual desktops and xrdp.
const REMOTE_SERVICES: [&str; 2] = ["nx", "xrdp-sesman"];

/// How soon a viewer that joins or leaves the desktop is seen.
pub const CHECK_EVERY: Duration = Duration::from_secs(5);

/// Where the desktop this runs on is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Session {
	/// On this box's own screen, or not known to be anywhere else.
	#[default]
	Local,
	/// In a remote desktop session, where each frame goes over the network.
	Remote,
}

/// The session this process was started in.
pub fn session() -> Session {
	if detected() {
		Session::Remote
	} else {
		Session::Local
	}
}

/// True when this process was started in a remote desktop session.
pub fn detected() -> bool {
	let var = |name: &str| std::env::var_os(name).is_some_and(|v| !v.is_empty());
	// logind's own record of the session, as sd_session_is_remote reads it
	let session = std::env::var("XDG_SESSION_ID")
		.ok()
		.filter(|id| !id.is_empty() && !id.contains('/'))
		.and_then(|id| std::fs::read_to_string(format!("/run/systemd/sessions/{id}")).ok());
	detected_by(var, session.as_deref())
}

/// As [`detected`], from whether each variable is set, and the text of the
/// session's logind file, if there is one.
pub fn detected_by(var: impl Fn(&str) -> bool, session: Option<&str>) -> bool {
	REMOTE_VARS.iter().any(|v| var(v))
		|| session.is_some_and(|s| {
			s.lines().any(|line| match line.split_once('=') {
				Some(("REMOTE", v)) => v == "1",
				Some(("SERVICE", v)) => REMOTE_SERVICES.contains(&v),
				_ => false,
			})
		})
}

/// Follows where the desktop is shown: the session it was started in, read
/// once, and a remote viewer on its display, looked for at most once each
/// [`CHECK_EVERY`].
#[derive(Debug)]
pub struct Watch {
	started: Session,
	/// The X display number this runs on, if any.
	display: Option<u32>,
	checked: Option<Instant>,
	viewed: Session,
	probe: Probe,
}

impl Watch {
	/// Reads the session this process started in, and its display.
	pub fn start() -> Self {
		let display = std::env::var("DISPLAY").ok();
		Self::of(session(), display.as_deref().and_then(display_number))
	}

	/// A watch that stays at `session` and never looks, as for a test.
	pub fn fixed(session: Session) -> Self {
		Self::of(session, None)
	}

	fn of(started: Session, display: Option<u32>) -> Self {
		Self {
			started,
			display,
			checked: None,
			viewed: Session::Local,
			probe: Probe::default(),
		}
	}

	/// The session at `now`, which looks for a viewer again only once
	/// [`CHECK_EVERY`] has gone by since the last look.
	pub fn session(&mut self, now: Instant) -> Session {
		// out for the look, since session_by takes all of self
		let mut probe = std::mem::take(&mut self.probe);
		let session = self.session_by(now, |display| probe.viewed(display, &SHARERS));
		self.probe = probe;
		session
	}

	fn session_by(&mut self, now: Instant, viewed: impl FnOnce(u32) -> bool) -> Session {
		if self.started == Session::Remote {
			return Session::Remote;
		}
		let Some(display) = self.display else {
			return Session::Local;
		};
		// a timer set to CHECK_EVERY, as the settings window's, can wake a
		// little early, and waiting for the next one would double the wait
		let due = CHECK_EVERY.saturating_sub(CHECK_EVERY / 10);
		if self
			.checked
			.is_none_or(|t| now.saturating_duration_since(t) >= due)
		{
			self.checked = Some(now);
			self.viewed = if viewed(display) {
				Session::Remote
			} else {
				Session::Local
			};
		}
		self.viewed
	}
}

/// What tells that a sharer's peer is connected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sign {
	/// It holds the peer's connection itself.
	Holds,
	/// RustDesk's server runs as root, and starts this connection manager in
	/// the desktop only while a peer is connected.
	RustDeskCm,
	/// NoMachine's node keeps this up for the physical display between
	/// connections too, so it counts only while NoMachine has a connection.
	NoMachineMonitor,
}

/// The servers that share a desktop that's already up, by process name, and
/// the sign of a peer for each.
const SHARERS: [(&str, Sign); 6] = [
	("x11vnc", Sign::Holds),
	("x0vncserver", Sign::Holds),
	("krfb", Sign::Holds),
	("freerdp-shadow-cli", Sign::Holds),
	("rustdesk", Sign::RustDeskCm),
	("nxrunner.bin", Sign::NoMachineMonitor),
];

/// The kind of server a connection is for, by its usual ports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Server {
	Vnc,
	Rdp,
	NoMachine,
	RustDesk,
}

fn server_of(local: u16, remote: u16) -> Option<Server> {
	match (local, remote) {
		(5900..=5999, _) => Some(Server::Vnc),
		(3389, _) => Some(Server::Rdp),
		(4000, _) => Some(Server::NoMachine),
		// a direct peer, or out to a relay
		(21118, _) | (_, 21117) => Some(Server::RustDesk),
		_ => None,
	}
}

/// An established connection on a remote desktop server's port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Link {
	server: Server,
	inode: u64,
}

/// The [`Link`]s in the text of `/proc/net/tcp` or `/proc/net/tcp6`.
fn links_in(text: &str) -> impl Iterator<Item = Link> + '_ {
	let port = |address: &str| u16::from_str_radix(address.rsplit_once(':')?.1, 16).ok();
	text.lines().skip(1).filter_map(move |line| {
		let mut fields = line.split_whitespace();
		let local = port(fields.nth(1)?)?;
		let remote = port(fields.next()?)?;
		// 01 is TCP_ESTABLISHED
		if fields.next()? != "01" {
			return None;
		}
		let server = server_of(local, remote)?;
		let inode = fields.nth(5)?.parse().ok()?;
		Some(Link { server, inode })
	})
}

/// The X display number in a name such as `:0`, `:0.0` or `unix:1`.
fn display_number(name: &str) -> Option<u32> {
	let (_, after) = name.rsplit_once(':')?;
	after.split('.').next()?.parse().ok()
}

/// The display a process shows: its `-display` argument, as x11vnc takes,
/// or else its `DISPLAY`.
fn display_of(args: &[String], env: Option<&str>) -> Option<u32> {
	args.iter()
		.position(|a| a == "-display" || a == "--display")
		.and_then(|i| args.get(i + 1))
		.map(String::as_str)
		.or(env)
		.and_then(display_number)
}

/// A process of a server in [`SHARERS`], as read from `/proc`.
#[derive(Debug)]
struct Sharer {
	sign: Sign,
	args: Vec<String>,
	display: Option<u32>,
	/// The inodes of its sockets, read only for [`Sign::Holds`].
	sockets: Vec<u64>,
}

/// True when a sharer on `display` has a peer connected, by its [`Sign`].
fn viewed_by(display: u32, links: &[Link], sharers: &[Sharer]) -> bool {
	let has = |server: Server| links.iter().any(|l| l.server == server);
	let arg = |p: &Sharer, a: &str| p.args.iter().any(|x| x == a);
	sharers
		.iter()
		.filter(|p| p.display == Some(display))
		.any(|p| match p.sign {
			Sign::Holds => p
				.sockets
				.iter()
				.any(|s| links.iter().any(|l| l.inode == *s)),
			Sign::RustDeskCm => has(Server::RustDesk) && arg(p, "--cm"),
			Sign::NoMachineMonitor => has(Server::NoMachine) && arg(p, "--monitor"),
		})
}

/// The TCP opens since boot, and the connections up now, from the text of
/// `/proc/net/snmp`. The established connections seldom change with neither
/// moving.
fn tcp_counts(snmp: &str) -> Option<[u64; 2]> {
	let mut rows = snmp.lines().filter_map(|l| l.strip_prefix("Tcp:"));
	let (names, values) = (rows.next()?, rows.next()?);
	let established = names
		.split_whitespace()
		.zip(values.split_whitespace())
		.find(|(name, _)| *name == "CurrEstab")?
		.1
		.parse()
		.ok()?;
	Some([crate::lan::opens(snmp)?, established])
}

/// Looks for a remote viewer, with no root. The kernel walks its whole
/// connection table for each read of `/proc/net/tcp`, a few ms on a box
/// with a lot of memory, so the [`Link`]s found are kept until the TCP
/// counters move.
#[derive(Debug, Default)]
struct Probe {
	counts: Option<[u64; 2]>,
	links: Vec<Link>,
}

impl Probe {
	/// True when a remote viewer is connected to X display `display`. This
	/// user's processes are read only while a connection is on a remote
	/// desktop server's port.
	fn viewed(&mut self, display: u32, names: &[(&str, Sign)]) -> bool {
		let counts = std::fs::read_to_string("/proc/net/snmp")
			.ok()
			.as_deref()
			.and_then(tcp_counts);
		if counts.is_none() || counts != self.counts {
			self.counts = counts;
			self.links.clear();
			for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
				if let Ok(text) = std::fs::read_to_string(table) {
					self.links.extend(links_in(&text));
				}
			}
		}
		// nearly every look ends here
		if self.links.is_empty() {
			return false;
		}
		viewed_by(display, &self.links, &sharers(names))
	}
}

/// The processes of this user named in `names`.
fn sharers(names: &[(&str, Sign)]) -> Vec<Sharer> {
	let Ok(me) = std::fs::metadata("/proc/self").map(|m| m.uid()) else {
		return Vec::new();
	};
	let Ok(dir) = std::fs::read_dir("/proc") else {
		return Vec::new();
	};
	dir.flatten()
		.filter(|e| {
			e.file_name()
				.to_str()
				.is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()))
		})
		.filter(|e| e.metadata().is_ok_and(|m| m.uid() == me))
		.filter_map(|e| sharer(&e.path(), names))
		.collect()
}

fn sharer(dir: &Path, names: &[(&str, Sign)]) -> Option<Sharer> {
	let comm = std::fs::read_to_string(dir.join("comm")).ok()?;
	let comm = comm.trim_end_matches('\n');
	// the kernel cuts a process name to 15 bytes
	let &(_, sign) = names
		.iter()
		.find(|(name, _)| name.get(..name.len().min(15)) == Some(comm))?;
	let nul_split = |bytes: &[u8]| -> Vec<String> {
		bytes
			.split(|b| *b == 0)
			.filter(|s| !s.is_empty())
			.map(|s| String::from_utf8_lossy(s).into_owned())
			.collect()
	};
	let args = nul_split(&std::fs::read(dir.join("cmdline")).ok()?);
	let env = std::fs::read(dir.join("environ"))
		.map(|e| nul_split(&e))
		.unwrap_or_default();
	let env_display = env.iter().find_map(|v| v.strip_prefix("DISPLAY="));
	let display = display_of(&args, env_display);
	let sockets = match sign {
		Sign::Holds => sockets_of(dir),
		Sign::RustDeskCm | Sign::NoMachineMonitor => Vec::new(),
	};
	Some(Sharer {
		sign,
		args,
		display,
		sockets,
	})
}

fn sockets_of(dir: &Path) -> Vec<u64> {
	let Ok(fds) = std::fs::read_dir(dir.join("fd")) else {
		return Vec::new();
	};
	fds.flatten()
		.filter_map(|fd| {
			let target = std::fs::read_link(fd.path()).ok()?;
			target
				.to_str()?
				.strip_prefix("socket:[")?
				.strip_suffix(']')?
				.parse()
				.ok()
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::fmt::Write;

	// Test ID: df3nf
	#[test]
	fn a_remote_session_is_told_by_its_variables_or_by_logind() {
		let none = |_: &str| false;
		let local = "UID=1001\nREMOTE=0\nTYPE=x11\nDISPLAY=:0\nSERVICE=lightdm\n";
		assert!(!detected_by(none, None));
		assert!(!detected_by(none, Some(local)));
		for v in REMOTE_VARS {
			assert!(detected_by(|name| name == v, Some(local)), "{v}");
		}
		assert!(!detected_by(|name| name == "DISPLAY", None));
		// NoMachine's virtual desktops say REMOTE=0
		assert!(detected_by(none, Some("REMOTE=0\nTYPE=x11\nSERVICE=nx\n")));
		assert!(detected_by(none, Some("REMOTE=1\nSERVICE=sshd\n")));
		assert!(detected_by(none, Some("SERVICE=xrdp-sesman\n")));
		assert!(!detected_by(none, Some("SERVICE=nxx\nREMOTE_HOST=1\n")));
	}

	const TCP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000:0FA0 00000000:0000 0A 00000000:00000000 00:00000000 00000000   125        0 26885 1 0 100 0 0 10 0
   1: 0100007F:170C 0100007F:D2F0 01 00000000:00000000 00:00000000 00000000 64055        0 501 1 0 20 4 30 10 -1
   2: 7B01A8C0:0FA0 7001A8C0:C350 01 00000000:00000000 00:00000000 00000000   125        0 502 1 0 20 4 30 10 -1
   3: 7B01A8C0:E9E8 2A00A8C0:01BB 01 00000000:00000000 00:00000000 00000000  1001        0 503 1 0 20 4 30 10 -1
   4: 7B01A8C0:E9E9 2A00A8C0:527D 01 00000000:00000000 00:00000000 00000000     0        0 504 1 0 20 4 30 10 -1
   5: 7B01A8C0:0D3D 7001A8C0:C351 06 00000000:00000000 00:00000000 00000000     0        0 0 1 0 20 4 30 10 -1
";

	// Test ID: df3p4
	#[test]
	fn the_tcp_table_gives_established_connections_on_remote_desktop_ports() {
		let links: Vec<Link> = links_in(TCP).collect();
		assert_eq!(
			links,
			[
				Link {
					server: Server::Vnc,
					inode: 501,
				},
				Link {
					server: Server::NoMachine,
					inode: 502,
				},
				Link {
					server: Server::RustDesk,
					inode: 504,
				},
			]
		);
		assert_eq!(links_in("header\n").count(), 0);
		assert_eq!(links_in("header\n   0: junk\n").count(), 0);
		for (name, n) in [
			(":0", Some(0)),
			(":0.0", Some(0)),
			("unix:1001", Some(1001)),
			("WAIT:7", Some(7)),
		] {
			assert_eq!(display_number(name), n, "{name}");
		}
		assert_eq!(display_number("wayland-0"), None);
		let snmp = "Tcp: RtoAlgorithm RtoMin RtoMax MaxConn ActiveOpens PassiveOpens AttemptFails EstabResets CurrEstab InSegs\nTcp: 1 200 120000 -1 310132 9 40 7 37 99\n";
		assert_eq!(tcp_counts(snmp), Some([310_141, 37]));
		assert_eq!(
			tcp_counts("Tcp: ActiveOpens PassiveOpens\nTcp: 5 6\n"),
			None
		);
		let args = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
		assert_eq!(
			display_of(&args(&["x11vnc", "-display", ":3"]), Some(":0")),
			Some(3)
		);
		assert_eq!(
			display_of(&args(&["x11vnc", "-forever"]), Some(":0")),
			Some(0)
		);
		assert_eq!(display_of(&args(&["x11vnc", "-display"]), None), None);
	}

	// Test ID: df3p5
	#[test]
	fn a_viewer_counts_only_when_its_server_has_a_peer_on_this_display() {
		let links: Vec<Link> = links_in(TCP).collect();
		let p = |sign, args: &[&str], display, sockets: &[u64]| Sharer {
			sign,
			args: args.iter().map(|s| (*s).to_owned()).collect(),
			display: Some(display),
			sockets: sockets.to_vec(),
		};
		let vnc = |display, sockets: &[u64]| p(Sign::Holds, &["x11vnc"], display, sockets);
		assert!(viewed_by(0, &links, &[vnc(0, &[7, 501])]));
		assert!(!viewed_by(0, &links, &[vnc(1, &[501])]));
		// waiting for a viewer, with a VM's console viewer on 5900 held by another user
		assert!(!viewed_by(0, &links, &[vnc(0, &[7])]));
		let cm = p(Sign::RustDeskCm, &["rustdesk", "--cm"], 0, &[]);
		let tray = p(Sign::RustDeskCm, &["rustdesk", "--tray"], 0, &[]);
		assert!(viewed_by(0, &links, std::slice::from_ref(&cm)));
		assert!(!viewed_by(1, &links, std::slice::from_ref(&cm)));
		assert!(!viewed_by(0, &links, &[tray]));
		assert!(!viewed_by(0, &[], &[cm]));
		// as b23's NoMachine node watches :0 with no client connected
		let nx = || {
			p(
				Sign::NoMachineMonitor,
				&["/usr/NX/bin/nxrunner.bin", "--monitor", "--pid", "362876"],
				0,
				&[],
			)
		};
		assert!(viewed_by(0, &links, &[nx()]));
		assert!(!viewed_by(1001, &links, &[nx()]));
		let not_nx: Vec<Link> = links
			.iter()
			.copied()
			.filter(|l| l.server != Server::NoMachine)
			.collect();
		assert!(!viewed_by(0, &not_nx, &[nx()]));
		assert!(!viewed_by(0, &links, &[]));
	}

	// Test ID: df3p6
	#[test]
	fn the_watch_looks_again_only_every_few_seconds() {
		let t0 = Instant::now();
		let mut w = Watch::of(Session::Local, Some(0));
		assert_eq!(w.session_by(t0, |_| true), Session::Remote);
		// not looked at again yet
		assert_eq!(
			w.session_by(t0 + CHECK_EVERY / 2, |_| false),
			Session::Remote
		);
		// a timer set to CHECK_EVERY that wakes a little early still looks
		let early = CHECK_EVERY.saturating_sub(CHECK_EVERY / 20);
		assert_eq!(w.session_by(t0 + early, |_| false), Session::Local);
		assert_eq!(w.session_by(t0 + early * 2, |d| d == 0), Session::Remote);
		let mut started = Watch::of(Session::Remote, Some(0));
		assert_eq!(started.session_by(t0, |_| false), Session::Remote);
		let mut headless = Watch::of(Session::Local, None);
		assert_eq!(headless.session_by(t0, |_| true), Session::Local);
	}

	// a stand-in for x11vnc: a copy of this test binary on display :77,
	// holding both ends of a connection on a VNC port
	// Test ID: df3p7
	#[test]
	#[cfg_attr(miri, ignore)]
	fn a_stand_in_viewer_on_a_vnc_port_is_seen_on_its_display_only() {
		use std::io::{BufRead, BufReader};
		use std::net::{TcpListener, TcpStream};
		use std::process::{Command, Stdio};
		const CHILD: &str = "TUKZMON_TEST_VIEWER";
		if std::env::var_os(CHILD).is_some() {
			let Some(listener) =
				(5990..=5999).find_map(|port| TcpListener::bind(("127.0.0.1", port)).ok())
			else {
				return;
			};
			let Ok(address) = listener.local_addr() else {
				return;
			};
			let Ok(peer) = TcpStream::connect(address) else {
				return;
			};
			let Ok((held, _)) = listener.accept() else {
				return;
			};
			println!("ready");
			// until the parent closes stdin
			let _ = std::io::stdin().lines().count();
			drop((peer, held));
			return;
		}
		let exe = std::env::current_exe().unwrap();
		let mut child = Command::new(exe)
			.args([
				"--exact",
				"remote::tests::a_stand_in_viewer_on_a_vnc_port_is_seen_on_its_display_only",
				"--nocapture",
				"--test-threads=1",
			])
			.env(CHILD, "1")
			.env("DISPLAY", ":77")
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.spawn()
			.unwrap();
		let mut out = BufReader::new(child.stdout.take().unwrap());
		let mut line = String::new();
		while out.read_line(&mut line).unwrap() > 0 && !line.contains("ready") {
			line.clear();
		}
		let name = std::fs::read_to_string(format!("/proc/{}/comm", child.id())).unwrap();
		let names = [(name.trim_end(), Sign::Holds)];
		let mut probe = Probe::default();
		let seen = probe.viewed(77, &names);
		let elsewhere = probe.viewed(78, &names);
		drop(child.stdin.take());
		child.wait().unwrap();
		assert!(line.contains("ready"), "no free VNC port in 5990 to 5999");
		assert!(seen);
		assert!(!elsewhere);
		assert!(!Probe::default().viewed(77, &[("no-such-sharer", Sign::Holds)]));
	}

	// Test ID: df4gs
	#[test]
	fn fuzz_snmp_gives_the_tcp_opens_and_connections() {
		use crate::fuzz::count;
		let tally = crate::fuzz::run("snmp", |dice| {
			let (active, passive, estab) = (count(dice), count(dice), count(dice) % 100_000);
			let text = format!(
				"Ip: Forwarding DefaultTTL InReceives InHdrErrors\nIp: 1 64 {} 0\n\
				Icmp: InMsgs InErrors\nIcmp: {} 0\n\
				Tcp: RtoAlgorithm RtoMin RtoMax MaxConn ActiveOpens PassiveOpens AttemptFails EstabResets CurrEstab InSegs\n\
				Tcp: 1 200 120000 -1 {active} {passive} {} 9 {estab} {}\n\
				Udp: InDatagrams NoPorts\nUdp: {} 0\n",
				count(dice),
				count(dice),
				count(dice),
				count(dice),
				count(dice)
			);
			let (text, whole) = crate::fuzz::maybe_hurt(dice, text);
			let opens = crate::lan::opens(&text);
			let counts = tcp_counts(&text);
			if whole {
				let sum = active.wrapping_add(passive);
				assert_eq!(opens, Some(sum), "{text:?}");
				assert_eq!(counts, Some([sum, estab]), "{text:?}");
			}
			assert!(
				counts.is_none_or(|[o, _]| Some(o) == opens),
				"{counts:?} {opens:?}"
			);
			counts.is_some()
		});
		tally.assert_mixed();
	}

	// Test ID: df4gt
	#[test]
	fn fuzz_tcp_table_gives_only_established_links_on_server_ports() {
		let tally = crate::fuzz::run("tcp table", |dice| {
			let mut text = String::from(
				"  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n",
			);
			let mut want = Vec::new();
			for sl in 0..dice.below(10) {
				let local = *dice.pick(&[5900, 5901, 3389, 4000, 21118, 22, 443, u16::MAX]);
				let remote = *dice.pick(&[21117, 40000, 80, 0]);
				let st = *dice.pick(&["01", "01", "0A", "06"]);
				let inode = crate::fuzz::count(dice);
				let wide = dice.one_in(2);
				let ip = if wide {
					"00000000000000000000000001000000"
				} else {
					"0100007F"
				};
				let _ = writeln!(
					text,
					"{sl:>4}: {ip}:{local:04X} {ip}:{remote:04X} {st} 00000000:00000000 00:00000000 00000000  1000        0 {inode} 1 0000000000000000 20 4 0 10 -1"
				);
				if let (Some(server), "01") = (server_of(local, remote), st) {
					want.push(Link { server, inode });
				}
			}
			let (text, whole) = crate::fuzz::maybe_hurt(dice, text);
			let got: Vec<Link> = links_in(&text).collect();
			if whole {
				assert_eq!(got, want, "{text:?}");
			}
			// no more links than rows
			let lines = text.lines().count();
			assert!(got.len() <= lines.saturating_sub(1), "{got:?}");
			!got.is_empty()
		});
		tally.assert_mixed();
	}

	// Test ID: df4gy
	#[test]
	fn fuzz_a_sharers_proc_files_give_its_display() {
		let tally = crate::fuzz::run("sharer", |dice| {
			let mut f = crate::fuzz::Files::new(dice);
			let comm = *dice.pick(&["x11vnc", "krfb", "freerdp-shadow-", "rustdesk", "bash"]);
			f.put(dice, "p/comm", format!("{comm}\n"));
			let (arg, env) = (dice.below(12), dice.below(12));
			let mut args = vec!["x11vnc", "-forever"];
			let shown = format!(":{arg}");
			if dice.one_in(2) {
				args.extend(["-display", &shown]);
			}
			args.push(if dice.one_in(2) { "--cm" } else { "-shared" });
			f.put(dice, "p/cmdline", args.join("\0") + "\0");
			f.put(
				dice,
				"p/environ",
				format!("HOME=/home/sam\0DISPLAY=:{env}.0\0LANG=C\0"),
			);
			let t = f.tree("sharer_fuzz");
			let got = sharer(Path::new(&t.at("p")), &SHARERS);
			if f.whole {
				let want = if args.contains(&"-display") { arg } else { env };
				assert_eq!(
					got.as_ref().map(|s| s.display),
					(comm != "bash").then_some(Some(want as u32)),
					"{args:?}"
				);
			}
			got.is_some_and(|s| s.display.is_some())
		});
		tally.assert_mixed();
	}
}
