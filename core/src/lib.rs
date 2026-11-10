// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! The part of tukzmon every panel shares. No GUI toolkit in here.

#![warn(missing_docs)]

pub mod axis;
pub mod battery;
pub mod collect;
pub mod color;
pub mod config;
pub mod cpu;
pub mod disk;
pub mod gpu;
pub mod graphs;
pub mod history;
pub mod keep;
#[cfg(target_os = "linux")]
pub mod lan;
pub mod load;
pub mod mem;
pub mod net;
pub mod own;
#[cfg(unix)]
pub mod procs;
pub mod remote;
pub mod render;
pub mod temp;
pub mod when;
pub mod zswap;

use std::rc::Rc;
use std::time::{Instant, SystemTime};

use axis::{Ages, Axis, Basis, Ease, Timing};
use battery::BatteryReader;
use collect::Collector;
use config::{Look, Outline, Which};
use cpu::CpuReader;
use disk::DiskReader;
use gpu::{GpuReader, VramReader};
use history::History;
use load::LoadReader;
use mem::{MemoryReader, SwapReader};
use net::Traffic;
use own::OwnReader;
#[cfg(unix)]
use procs::ProcsReader;
use render::{Canvas, Frame, Paint, Peak, Scale, Scratch, Vertical};
use temp::TempReader;
use when::Said;
use zswap::ZswapReader;

/// No graph is read more often than this, the finest history tier.
pub const FASTEST_MS: u32 = 50;

/// A read due this soon counts as due now, since a timer may fire a little
/// early. So the wait to the next read can be up to this much over the rate.
pub const EARLY_MS: f64 = 10.0;

/// How far past now saved history may end, in seconds, and still load with
/// the clock moved up to it. A clock behind by a time zone, as a dual boot
/// that keeps local time in the hardware clock leaves it until it syncs, is
/// under this. Further ahead is a broken file, or a clock set back for good,
/// and that history starts over. See design.md History.
pub const AHEAD_MAX: f64 = 14.0 * 3600.0;

/// The CPUs this process can run on, read once.
pub(crate) fn cpus() -> f32 {
	static CPUS: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
	*CPUS.get_or_init(|| std::thread::available_parallelism().map_or(1.0, |n| n.get() as f32))
}

/// How often a moving graph draws, against the screen's rate. See design.md
/// Drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pace {
	/// As often as the screen allows, up to 60 a second: every frame at
	/// 60 Hz, every other at 120 and every third at 144.
	UpTo60,
	/// Every fourth frame, kept to 24 to 50 a second: every other frame at
	/// 60 Hz, every third at 75, every fourth from 96 to 200 Hz, and every
	/// fifth at 240.
	Quarter,
}

/// The pace the panel and the settings window draw a moving graph at.
/// [`Pace::UpTo60`] looked fine at 60 Hz, and Quarter costs less.
pub const PACE: Pace = Pace::Quarter;

impl Pace {
	/// Screen frames from one draw to the next on a screen of `hz`. Below
	/// the pace's slowest rate it's every frame, since none come faster.
	pub fn frames_apart(self, hz: f64) -> u32 {
		if !hz.is_finite() || hz <= 0.0 {
			return 1;
		}
		let apart = match self {
			// 61, so a 60.002 Hz screen still gets every frame
			Self::UpTo60 => (hz / 61.0).ceil(),
			// 1% slack each way, so 95.9 and 200.01 Hz still split by 4
			Self::Quarter => 4.0_f64.max((hz / 50.5).ceil()).min((hz / 23.75).floor()),
		};
		apart.clamp(1.0, 1000.0) as u32
	}
}

/// [`Pace::frames_apart`] at [`PACE`].
pub fn frames_apart(hz: f64) -> u32 {
	PACE.frames_apart(hz)
}

/// Milliseconds on one clock that every graph's beats go by. See
/// [`Graph::tick`].
pub fn clock_ms() -> f64 {
	static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
	START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}

fn unix_now() -> f64 {
	SystemTime::now()
		.duration_since(SystemTime::UNIX_EPOCH)
		.map_or(0.0, |d| d.as_secs_f64())
}

/// Milliseconds to the next graph's read, from the least of
/// [`Graph::tick`]'s answers, for a one-shot timer.
pub fn wait_ms(next: f64) -> u32 {
	if next.is_nan() {
		return FASTEST_MS;
	}
	next.ceil().clamp(1.0, f64::from(u32::MAX)) as u32
}

/// A collector as one graph draws and names it.
pub trait Source<const N: usize>: Collector<N> {
	/// Which graph it is, and so its settings in a [`Look`].
	const WHICH: Which;
	/// The graph's section in the history file.
	const SECTION: &'static str = Self::WHICH.key();
	/// The tip before there's anything read.
	const TITLE: &'static str;
	/// The hover tip's text for one read.
	fn text(&self, v: &[f32; N]) -> String;

	/// How the graph is colored, from its settings.
	fn paint(look: &Look) -> Paint<'_, N> {
		look.paint(Self::WHICH)
	}

	/// The graph's frame and background, from its settings.
	fn outline(look: &Look) -> Outline {
		look[Self::WHICH].outline
	}

	/// How the graph spreads time, its own or the Time settings'.
	fn timing(look: &Look) -> Timing {
		look.timing(look[Self::WHICH].time)
	}

	/// How often the graph is read, in ms. See [`Look::rate`].
	fn sample_ms(look: &Look) -> u32 {
		look.rate(look[Self::WHICH].sample)
	}

	/// Some for a source with no fixed top, such as bytes per second: the
	/// least its peak goes down to. See [`render::Peak`].
	fn peak_floor(&self) -> Option<f32> {
		None
	}

	/// How height goes up the graph, its own or Scale default's under
	/// Vertical axis.
	fn vertical(look: &Look) -> Vertical {
		look.scaling(look[Self::WHICH].vertical)
	}

	/// Takes up a new look's settings for the reader itself, such as which
	/// drives to count.
	fn follow(&mut self, _look: &Look) {}
}

fn percent(x: f32) -> i32 {
	(x * 100.0).round() as i32
}

impl Source<4> for CpuReader {
	const WHICH: Which = Which::Cpu;
	const TITLE: &'static str = "CPU";

	fn text(&self, v: &[f32; 4]) -> String {
		let busy: f32 = v.iter().sum();
		format!(
			"CPU {}%\nUser {}%\nSystem {}%\nNice {}%\nI/O wait {}%",
			percent(busy),
			percent(v[2]),
			percent(v[1]),
			percent(v[3]),
			percent(v[0])
		)
	}
}

impl Source<3> for MemoryReader {
	const WHICH: Which = Which::Mem;
	const TITLE: &'static str = "Memory";

	fn text(&self, v: &[f32; 3]) -> String {
		format!(
			"Memory {}% in use\nBuffers {}%\nCache {}%",
			percent(v[0]),
			percent(v[1]),
			percent(v[2])
		)
	}
}

impl Source<1> for GpuReader {
	const WHICH: Which = Which::Gpu;
	const TITLE: &'static str = "GPU";

	fn text(&self, v: &[f32; 1]) -> String {
		format!("GPU {}% busy", percent(v[0]))
	}
}

// binary, as a card's memory is sold
fn gib(bytes: f64) -> String {
	format!("{:.1}", bytes / f64::from(1u32 << 30))
}

impl Source<1> for VramReader {
	const WHICH: Which = Which::Vram;
	const TITLE: &'static str = "GPU memory";

	fn text(&self, v: &[f32; 1]) -> String {
		let total = self.total() as f64;
		if total == 0.0 {
			return format!("GPU memory {}% used", percent(v[0]));
		}
		format!(
			"GPU memory {}% used\n{} of {} GiB",
			percent(v[0]),
			gib(f64::from(v[0].clamp(0.0, 1.0)) * total),
			gib(total)
		)
	}
}

impl Source<1> for SwapReader {
	const WHICH: Which = Which::Swap;
	const TITLE: &'static str = "Swap";

	fn text(&self, v: &[f32; 1]) -> String {
		format!("Swap {}% used", percent(v[0]))
	}
}

impl Source<3> for BatteryReader {
	const WHICH: Which = Which::Battery;
	const TITLE: &'static str = "Battery";

	// full counts as charging, so the tip says what the power is doing
	fn text(&self, v: &[f32; 3]) -> String {
		let state = if v[2] > 0.0 {
			"critical"
		} else if v[1] > 0.0 {
			"on battery"
		} else {
			"plugged in"
		};
		format!("Battery {}%, {state}", percent(v.iter().sum()))
	}
}

impl Source<1> for ZswapReader {
	const WHICH: Which = Which::Zswap;
	const TITLE: &'static str = "Zswap";

	// the ratio isn't in the history, so it's the latest whatever time is hovered
	fn text(&self, v: &[f32; 1]) -> String {
		let used = format!("{} {}% used", self.name(), percent(v[0]));
		match self.ratio() {
			Some(r) => format!("{used}\nCompressed {r:.1}x now"),
			None => used,
		}
	}
}

// decimal, as a download is usually shown
fn per_sec(bytes: f32) -> String {
	const UNITS: [&str; 5] = ["B/s", "kB/s", "MB/s", "GB/s", "TB/s"];
	let (mut v, mut unit) = (bytes.max(0.0), 0);
	while v >= 999.5 && unit < UNITS.len() - 1 {
		v /= 1000.0;
		unit += 1;
	}
	if unit > 0 && v < 9.95 {
		format!("{v:.1} {}", UNITS[unit])
	} else {
		format!("{v:.0} {}", UNITS[unit])
	}
}

impl Source<5> for Traffic {
	const WHICH: Which = Which::Net;
	const TITLE: &'static str = "Network";

	// so a quiet line's odd packet doesn't fill the graph
	fn peak_floor(&self) -> Option<f32> {
		Some(10_000.0)
	}

	fn text(&self, v: &[f32; 5]) -> String {
		let [local, lan_sent, lan_received, sent, received] = v.map(per_sec);
		if !self.split() {
			return format!("Network\nReceived {received}\nSent {sent}\nLocal {local}");
		}
		format!(
			"Network\nInternet received {received}\nInternet sent {sent}\n\
			 LAN received {lan_received}\nLAN sent {lan_sent}\nLocal {local}"
		)
	}

	fn follow(&mut self, look: &Look) {
		self.set_split(look.net_split);
	}
}

impl Source<1> for LoadReader {
	const WHICH: Which = Which::Load;
	const TITLE: &'static str = "Load";

	fn text(&self, v: &[f32; 1]) -> String {
		format!("Load {:.2}", v[0].max(0.0))
	}

	// full at a load of one a CPU, as the CPU graph is at 100%
	fn peak_floor(&self) -> Option<f32> {
		Some(cpus())
	}
}

impl Source<2> for DiskReader {
	const WHICH: Which = Which::Disk;
	const TITLE: &'static str = "Disk";

	fn text(&self, v: &[f32; 2]) -> String {
		format!("Disk\nRead {}\nWrite {}", per_sec(v[1]), per_sec(v[0]))
	}

	// so a log file's odd flush doesn't fill the graph
	fn peak_floor(&self) -> Option<f32> {
		Some(100_000.0)
	}

	fn follow(&mut self, look: &Look) {
		let want = look.drives();
		if *self.drives() != want
			&& let Some(r) = DiskReader::new(want)
		{
			*self = r;
		}
	}
}

impl Source<2> for TempReader {
	const WHICH: Which = Which::Temp;
	const TITLE: &'static str = "Temperature";

	fn text(&self, v: &[f32; 2]) -> String {
		format!(
			"Temperature {:.0} \u{b0}C\nCritical at {:.0} \u{b0}C",
			v[0] + v[1],
			self.limit()
		)
	}

	// full at the critical level, and higher once it's over
	fn peak_floor(&self) -> Option<f32> {
		Some(self.limit())
	}
}

// percent with 2 figures or so, since tukzmon's own share is often under 0.1
fn fine_pc(p: f32) -> String {
	let p = p.max(0.0);
	if p >= 9.95 {
		format!("{p:.0}%")
	} else if p >= 0.995 {
		format!("{p:.1}%")
	} else if p >= 0.0995 {
		format!("{p:.2}%")
	} else {
		format!("{p:.3}%")
	}
}

impl Source<1> for OwnReader {
	const WHICH: Which = Which::Own;
	const TITLE: &'static str = "tukzmon";

	fn text(&self, v: &[f32; 1]) -> String {
		let all = v[0] * 100.0;
		let tip = format!(
			"{} {} of all CPUs\n{} of one CPU",
			self.host().name(),
			fine_pc(all),
			fine_pc(all * cpus())
		);
		match self.host() {
			own::Host::Panel => format!("{tip}\nThe whole panel, tukzmon in it"),
			own::Host::Own | own::Host::Settings => tip,
		}
	}

	// full at every CPU busy, like the CPU graph
	fn peak_floor(&self) -> Option<f32> {
		Some(1.0)
	}
}

#[cfg(unix)]
impl Source<3> for ProcsReader {
	const WHICH: Which = Which::Procs;
	const TITLE: &'static str = "Processes";

	fn text(&self, v: &[f32; 3]) -> String {
		let [system, others, me] = v.map(|x| x.max(0.0).round() as u32);
		format!(
			"Processes {}\nYours {me}\nOther users {others}\nSystem {system}",
			system + others + me
		)
	}

	// a count has no top, and a box runs more than this anyway
	fn peak_floor(&self) -> Option<f32> {
		Some(100.0)
	}
}

/// One graph: reads, history, and drawing. CPU unless said otherwise.
pub struct Graph<S: Source<N> = CpuReader, const N: usize = 4> {
	/// The same one for every graph of a panel item, or of the settings
	/// window, so a new look is one allocation, not one per graph.
	pub look: Rc<Look>,
	hist: History<N>,
	src: S,
	clock: Instant,
	/// Unix time at `clock`'s start, so saved history lines up across runs.
	/// Moved up to the wall clock at a read, since `clock` stops in a suspend.
	epoch: f64,
	/// How far [`Graph::skip`] moved the clock on.
	skipped: f64,
	/// How far a test set the wall clock on, or back, by itself.
	#[cfg(test)]
	wall_shift: f64,
	read_at: Option<f64>,
	latest: Option<[f32; N]>,
	px: Vec<u32>,
	scratch: Scratch<N>,
	ease: Ease,
	/// The top, for a source with no fixed one.
	peak: Peak,
	/// Where the pointer is, in device pixels from the left, for the hover line.
	pub hover: Option<usize>,
	/// The beat of the last read from [`Graph::tick`], and its time on
	/// [`clock_ms`].
	beat: Option<(f64, f64)>,
	/// Where the right edge was drawn when the newest sample came, and when.
	slide: Option<(f64, f64)>,
}

// By hand, to leave out the pixel buffer and the samples.
impl<S: Source<N>, const N: usize> std::fmt::Debug for Graph<S, N> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Graph")
			.field("which", &S::WHICH)
			.field("read_at", &self.read_at)
			.field("latest", &self.latest)
			.field("hover", &self.hover)
			.finish_non_exhaustive()
	}
}

impl Graph {
	/// A CPU graph, the one every panel item has.
	pub fn new(look: impl Into<Rc<Look>>) -> Graph {
		Graph::with(look, CpuReader::default())
	}
}

impl Graph<MemoryReader, 3> {
	/// None where memory can't be read, so the graph is left out.
	pub fn memory(look: impl Into<Rc<Look>>) -> Option<Self> {
		MemoryReader::probe().map(|src| Graph::with(look, src))
	}
}

impl Graph<SwapReader, 1> {
	/// None on a box with no swap.
	pub fn swap(look: impl Into<Rc<Look>>) -> Option<Self> {
		SwapReader::probe().map(|src| Graph::with(look, src))
	}
}

impl Graph<ZswapReader, 1> {
	/// None on a box with neither zswap on nor zram set up.
	pub fn zswap(look: impl Into<Rc<Look>>) -> Option<Self> {
		ZswapReader::probe().map(|src| Graph::with(look, src))
	}
}

impl Graph<Traffic, 5> {
	/// None where `/proc/net/dev` can't be read.
	pub fn net(look: impl Into<Rc<Look>>) -> Option<Self> {
		let look = look.into();
		Traffic::probe().map(|mut src| {
			src.set_split(look.net_split);
			Graph::with(look, src)
		})
	}
}

impl Graph<GpuReader, 1> {
	/// None on a box with no GPU it can read.
	pub fn gpu(look: impl Into<Rc<Look>>) -> Option<Self> {
		GpuReader::probe().map(|src| Graph::with(look, src))
	}
}

impl Graph<VramReader, 1> {
	/// None on a box with no GPU it can read.
	pub fn vram(look: impl Into<Rc<Look>>) -> Option<Self> {
		VramReader::probe().map(|src| Graph::with(look, src))
	}
}

impl Graph<LoadReader, 1> {
	/// None where `/proc/loadavg` can't be read.
	pub fn load_avg(look: impl Into<Rc<Look>>) -> Option<Self> {
		LoadReader::probe().map(|src| Graph::with(look, src))
	}
}

#[cfg(unix)]
impl Graph<ProcsReader, 3> {
	/// None where `/proc` can't be listed.
	pub fn procs(look: impl Into<Rc<Look>>) -> Option<Self> {
		ProcsReader::probe().map(|src| Graph::with(look, src))
	}
}

impl Graph<DiskReader, 2> {
	/// None where `/proc/diskstats` can't be read, or with all disks picked
	/// on a box that has none.
	pub fn disk(look: impl Into<Rc<Look>>) -> Option<Self> {
		let look = look.into();
		DiskReader::new(look.drives()).map(|src| Graph::with(look, src))
	}
}

impl Graph<TempReader, 2> {
	/// None on a box with no CPU sensor it knows.
	pub fn temp(look: impl Into<Rc<Look>>) -> Option<Self> {
		TempReader::probe().map(|src| Graph::with(look, src))
	}
}

impl Graph<OwnReader, 1> {
	/// None where the process's CPU time can't be read.
	pub fn own(look: impl Into<Rc<Look>>) -> Option<Self> {
		OwnReader::probe().map(|src| Graph::with(look, src))
	}
}

impl Graph<BatteryReader, 3> {
	/// None on a box with no battery in.
	pub fn battery(look: impl Into<Rc<Look>>) -> Option<Self> {
		BatteryReader::probe().map(|src| Graph::with(look, src))
	}
}

impl<S: Source<N>, const N: usize> Graph<S, N> {
	fn with(look: impl Into<Rc<Look>>, src: S) -> Self {
		Graph {
			look: look.into(),
			hist: History::default(),
			src,
			clock: Instant::now(),
			epoch: unix_now(),
			skipped: 0.0,
			#[cfg(test)]
			wall_shift: 0.0,
			read_at: None,
			latest: None,
			px: Vec::new(),
			scratch: Scratch::default(),
			ease: Ease::default(),
			peak: Peak::default(),
			hover: None,
			beat: None,
			slide: None,
		}
	}

	// what the monotonic clock has seen, so not a suspend
	fn steady(&self) -> f64 {
		self.clock.elapsed().as_secs_f64() + self.skipped
	}

	fn wall(&self) -> f64 {
		let t = unix_now() + self.skipped;
		#[cfg(test)]
		let t = t + self.wall_shift;
		t
	}

	/// Unix time. The wall clock, but never back: set back, the graph's time
	/// runs on from where it was.
	fn now(&self) -> f64 {
		(self.epoch + self.steady()).max(self.wall())
	}

	/// Moves the clock on, so the soak test can run days in seconds.
	#[doc(hidden)]
	pub fn skip(&mut self, secs: f64) {
		self.skipped += secs;
	}

	/// Forgets the last read, so the next one only primes and the time
	/// between is a gap. For a graph that isn't read for a while.
	pub fn pause(&mut self) {
		self.read_at = None;
		self.latest = None;
		self.slide = None;
	}

	/// Drops history older than the max age.
	pub fn trim(&mut self) {
		let max_age = f64::from(self.look.max_days) * 86400.0;
		self.hist.trim(self.now() - max_age);
	}

	/// Drops every sample, so the graph starts over from the next read.
	pub fn clear(&mut self) {
		self.hist = History::default();
		self.latest = None;
		self.peak = Peak::default();
		self.slide = None;
	}

	/// Swaps in the saved history, if there is one that reads. The time since
	/// it was saved is a gap.
	///
	/// # Errors
	///
	/// As [`keep::read`], or `InvalidData` with no history of this graph's
	/// that reads. The history in memory is left as it was.
	#[cfg(test)]
	pub fn load(&mut self, path: &std::path::Path) -> std::io::Result<()> {
		if self.adopt(&keep::read(path)?) {
			Ok(())
		} else {
			Err(std::io::Error::new(
				std::io::ErrorKind::InvalidData,
				format!("no {} history", S::SECTION),
			))
		}
	}

	/// Swaps in this graph's saved history from a file's sections, read once
	/// for every graph in it. The time since it was saved is a gap. False,
	/// with nothing changed, when none of them is this graph's, or its
	/// history ends more than [`AHEAD_MAX`] after now.
	pub fn adopt(&mut self, sections: &[(String, Vec<u8>)]) -> bool {
		let now = self.now();
		let Some(saved) = sections
			.iter()
			.find(|(name, _)| name == S::SECTION)
			.and_then(|(_, body)| History::decode(body))
			.filter(|h| h.last().is_none_or(|last| last <= now + AHEAD_MAX))
		else {
			return false;
		};
		// a clock set back mustn't hide new samples behind old ones
		if let Some(last) = saved.last()
			&& last > now
		{
			self.epoch = last - self.steady();
		}
		self.hist = saved;
		self.read_at = None;
		self.slide = None;
		self.trim();
		true
	}

	/// The graph's history file section, for [`keep::write`].
	pub fn section(&self) -> (&'static str, Vec<u8>) {
		let mut body = Vec::new();
		self.hist.encode(&mut body);
		(S::SECTION, body)
	}

	/// # Errors
	///
	/// As [`keep::write`].
	#[cfg(test)]
	pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
		let (name, body) = self.section();
		keep::write(path, &[(name, &body)])
	}

	/// Read the source and keep it. The first call only primes it.
	pub fn sample(&mut self) {
		let steady = self.steady();
		// time the monotonic clock didn't see, as in a suspend or the clock set on
		let lost = (self.wall() - self.epoch - steady).max(0.0);
		self.epoch += lost;
		if lost > f64::from(self.sample_ms()) / 1000.0 {
			self.read_at = None;
		}
		let now = self.epoch + steady;
		let shares = self.src.read();
		if let (Some(v), Some(was)) = (shares, self.read_at) {
			let shown = self.edge();
			self.hist.add(was, now, &v);
			self.latest = Some(v);
			// a gap or a jump is the axis ease's, not a slide
			self.slide = shown
				.filter(|&from| self.slides() && now - from <= self.slack())
				.map(|from| (from, self.now()));
		}
		self.read_at = Some(now);
	}

	fn slides(&self) -> bool {
		self.look.slides(cpus())
	}

	// a sample may come late by most of a tick, and a tick may be missed
	fn slack(&self) -> f64 {
		2.0 * self.period() + 0.25
	}

	fn period(&self) -> f64 {
		f64::from(self.sample_ms()) / 1000.0
	}

	/// The time at the right edge as drawn: the newest sample's end, or on
	/// the way there while it slides in. None with no history yet.
	fn edge(&self) -> Option<f64> {
		let last = self.hist.last()?;
		let Some((from, at)) = self.slide.filter(|_| self.slides()) else {
			return Some(last);
		};
		// a steady speed, so the next slide starts as this one ends
		let k = ((self.now() - at) / self.period()).clamp(0.0, 1.0);
		Some(from + (last - from) * k)
	}

	/// True while the newest sample is still sliding in.
	pub fn sliding(&self) -> bool {
		self.slides()
			&& self
				.slide
				.is_some_and(|(_, at)| self.now() - at < self.period())
	}

	/// True while the graph moves between samples, as it slides or its axis
	/// eases, so the panel should draw again soon.
	pub fn moving(&self) -> bool {
		self.sliding() || self.easing()
	}

	/// How often this graph is read, in ms.
	pub fn sample_ms(&self) -> u32 {
		S::sample_ms(&self.look)
	}

	/// Reads the source when its next beat has come, and gives the ms from
	/// `at` to the beat after. Beats are whole multiples of the sample rate on
	/// [`clock_ms`], so graphs whose rates divide each other read on the same
	/// timer tick. The first call only primes it, as [`Self::sample`] does.
	pub fn tick(&mut self, at: f64) -> f64 {
		let rate = f64::from(self.sample_ms());
		let beat = ((at + EARLY_MS) / rate).floor();
		// a span much shorter than a beat is mostly noise, as after a new rate
		let due = self
			.beat
			.is_none_or(|(b, was)| b != beat && at - was >= rate / 2.0);
		if due {
			self.sample();
			self.beat = Some((beat, at));
		}
		(beat + 1.0) * rate - at
	}

	/// When [`Self::tick`] last read the source, on [`clock_ms`], so a caller
	/// can tell which graphs a tick read.
	pub fn last_read_ms(&self) -> Option<f64> {
		self.beat.map(|(_, at)| at)
	}

	/// The axis for a graph `columns` wide, as of the latest read.
	pub fn axis(&self, columns: usize) -> Axis {
		Axis::of(&self.basis(columns))
	}

	fn basis(&self, columns: usize) -> Basis {
		match S::timing(&self.look) {
			Timing::Log(curve) => Basis::Log {
				curve,
				span: self.hist.first().zip(self.edge()),
			},
			Timing::Linear => Basis::Linear(columns as f64 * f64::from(self.sample_ms()) / 1000.0),
		}
	}

	/// True while the axis is still easing in after a jump, so the panel
	/// should draw again soon.
	pub fn easing(&self) -> bool {
		self.ease.easing(self.now())
	}

	fn columns(&self, w: usize, scale: usize) -> usize {
		w.saturating_sub(2 * self.look.border as usize * scale)
	}

	// as Frame::draw clamps it
	fn border_px(&self, w: usize, h: usize, scale: usize) -> usize {
		(self.look.border as usize * scale).min(w / 2).min(h / 2)
	}

	/// The time under the hover line at device column `x`, oldest end first, as
	/// `render` at that size draws it. None with no history yet.
	pub fn span_at(&self, x: usize, w: usize, h: usize, scale: usize) -> Option<(f64, f64)> {
		let now = self.edge()?;
		let border = self.border_px(w, h, scale);
		if w <= 2 * border || h <= 2 * border {
			return None;
		}
		let plot_w = w - 2 * border;
		let thick = scale.clamp(1, plot_w);
		let x0 = x.clamp(border, border + plot_w - thick) - border;
		let axis = self.axis(self.columns(w, scale));
		let axis = self.ease.view(&axis, self.now());
		let u = |cx: usize| (plot_w - cx) as f64 / plot_w as f64;
		Some((now - axis.age(u(x0)), now - axis.age(u(x0 + thick))))
	}

	/// The tip for the pointer at device column `x`: when that is, and what
	/// the source read then.
	pub fn hover_tip(&self, x: usize, w: usize, h: usize, scale: usize) -> String {
		let Some((a, b)) = self.span_at(x, w, h, scale) else {
			return self.tooltip();
		};
		let t = f64::midpoint(a, b);
		let now = self.now().max(t);
		let when = match self.look.said {
			Said::Ago => when::ago(now - t),
			Said::Clock => when::clock(t, now).unwrap_or_else(|| when::ago(now - t)),
		};
		let span = self.hist.span(a, b);
		if span.covered < render::MIN_COVER {
			return format!("{when}\nNo data");
		}
		format!("{when}\n{}", self.src.text(&span.mean))
	}

	/// The graph as cairo ARGB32 pixels, `w` by `h`.
	pub fn render(&mut self, w: usize, h: usize, scale: usize) -> &[u32] {
		let mut px = std::mem::take(&mut self.px);
		px.resize(w * h, 0);
		if let Some(mut canvas) = Canvas::whole(&mut px, w, h) {
			self.render_in(&mut canvas, scale);
		}
		self.px = px;
		&self.px
	}

	/// As [`Self::render`], into a part of a bigger image.
	pub fn render_in(&mut self, canvas: &mut Canvas<'_>, scale: usize) {
		let (w, h) = (canvas.w(), canvas.h());
		let basis = self.basis(self.columns(w, scale));
		let axis = Axis::of(&basis);
		let t = self.now();
		let edge = self.edge().unwrap_or(0.0);
		let sample = self.period();
		self.ease.update(&axis, basis, t, self.slack());
		let peak = self.src.peak_floor().map(|floor| Scale {
			peak: &mut self.peak,
			floor,
		});
		Frame {
			hist: &self.hist,
			axis: &self.ease.view(&axis, t),
			now: edge,
			sample,
			look: &self.look,
			paint: S::paint(&self.look),
			outline: S::outline(&self.look),
			vertical: S::vertical(&self.look),
		}
		.draw_in(canvas, scale, peak, &mut self.scratch);
		if let Some(x) = self.hover {
			let border = self.border_px(w, h, scale);
			canvas.line(border, x, scale, self.look.hover);
		}
	}

	/// The latest read, as the panel's tooltip.
	pub fn tooltip(&self) -> String {
		let Some(v) = self.latest else {
			return S::TITLE.into();
		};
		self.src.text(&v)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use config::Motion;

	// Test ID: debj4
	#[test]
	fn samples_and_draws() {
		let mut g = Graph::new(Look::default());
		assert_eq!(g.tooltip(), "CPU");
		g.sample();
		std::thread::sleep(std::time::Duration::from_millis(40));
		g.sample();
		let px = g.render(128, 26, 1);
		assert_eq!(px.len(), 128 * 26);
		assert!(px.iter().all(|p| p >> 24 == 0xff));
		assert!(g.tooltip().starts_with("CPU "));
	}

	// Test ID: dehzn
	#[test]
	fn network_rates_read_in_decimal_units() {
		for (bytes, want) in [
			(0.0, "0 B/s"),
			(-3.0, "0 B/s"),
			(f32::NAN, "0 B/s"),
			(999.0, "999 B/s"),
			(999.6, "1.0 kB/s"),
			(1234.0, "1.2 kB/s"),
			(12_345.0, "12 kB/s"),
			(3e6, "3.0 MB/s"),
			(5e15, "5000 TB/s"),
		] {
			assert_eq!(per_sec(bytes), want, "{bytes}");
		}
		assert_eq!(
			Traffic::default().text(&[2000.0, 0.0, 0.0, 30_000.0, 1.5e6]),
			"Network\nReceived 1.5 MB/s\nSent 30 kB/s\nLocal 2.0 kB/s"
		);
	}

	// Test ID: demw3
	#[test]
	fn a_network_history_from_before_the_split_starts_over() {
		let mut old = History::<3>::default();
		let now = SystemTime::now()
			.duration_since(SystemTime::UNIX_EPOCH)
			.unwrap()
			.as_secs_f64();
		for i in 0..600 {
			let t = now - 600.0 + f64::from(i);
			old.add(t, t + 1.0, &[1.0, 2.0, 3.0]);
		}
		let mut body = Vec::new();
		old.encode(&mut body);
		let sections = [("net".to_string(), body)];
		let mut g = Graph::with(Look::default(), Traffic::default());
		assert!(!g.adopt(&sections));
		let mut five = Graph::with(Look::default(), Traffic::default());
		five.hist.add(now - 1.0, now, &[1.0; 5]);
		let sections = [five.section()].map(|(n, b)| (n.to_string(), b));
		assert!(g.adopt(&sections));
	}

	// Test ID: demwh
	#[test]
	fn a_version_1_file_with_3_network_series_is_still_refused() {
		let mut old = History::<3>::default();
		let now = SystemTime::now()
			.duration_since(SystemTime::UNIX_EPOCH)
			.unwrap()
			.as_secs_f64();
		for i in 0..600 {
			let t = now - 600.0 + f64::from(i);
			old.add(t, t + 1.0, &[1.0, 2.0, 3.0]);
		}
		let mut body = Vec::new();
		old.encode(&mut body);
		// as the plugin wrote it before sections had a series count
		let mut v1 = b"tukzhist".to_vec();
		for x in [1u32, 1] {
			v1.extend_from_slice(&x.to_le_bytes());
		}
		v1.push(3);
		v1.extend_from_slice(b"net");
		v1.extend_from_slice(&(body.len() as u64 - 4).to_le_bytes());
		v1.extend_from_slice(&body[4..]);
		let path = std::env::temp_dir().join(format!("test_tukzmon_net_v1_{}", std::process::id()));
		std::fs::write(&path, &v1).unwrap();
		let mut g = Graph::with(Look::default(), Traffic::default());
		assert!(g.load(&path).is_err());
		// by its layout, which still fits 3
		let back = keep::read(&path).unwrap();
		assert_eq!(History::<3>::decode(&back[0].1).unwrap().last(), old.last());
		std::fs::remove_file(&path).unwrap();
	}

	// Test ID: demvz
	#[test]
	#[cfg_attr(miri, ignore = "Miri has no netlink sockets")]
	fn the_split_tip_names_the_lan_and_the_internet() {
		let mut t = Traffic::default();
		t.set_split(config::Split::Sockets);
		assert!(t.split(), "sock_diag needs no root");
		assert_eq!(
			t.text(&[2000.0, 1000.0, 5e5, 30_000.0, 1.5e6]),
			"Network\nInternet received 1.5 MB/s\nInternet sent 30 kB/s\n\
			 LAN received 500 kB/s\nLAN sent 1.0 kB/s\nLocal 2.0 kB/s"
		);
		t.set_split(config::Split::Off);
		assert!(!t.split());
		assert!(t.text(&[0.0; 5]).starts_with("Network\nReceived"));
	}

	// Test ID: dekmb
	#[test]
	fn gpu_tips_read_busy_and_memory_in_gib() {
		let t = collect::Tree::new(
			"gpu_tip",
			&[
				("device/gpu_busy_percent", "44\n"),
				("device/mem_info_vram_used", "2684354560\n"),
				("device/mem_info_vram_total", "8589934592\n"),
			],
		);
		let (mut gpu, mut vram) = gpu::amd_readers(&t.at("device"));
		let v = gpu.read().unwrap();
		assert_eq!(gpu.text(&v), "GPU 44% busy");
		let v = vram.read().unwrap();
		assert_eq!(vram.text(&v), "GPU memory 31% used\n2.5 of 8.0 GiB");
		assert_eq!(gpu.peak_floor(), None);
		assert_eq!(vram.peak_floor(), None);
	}

	// Test ID: dej07
	#[test]
	fn load_reads_as_a_number_and_disk_in_rates() {
		let load = LoadReader::default();
		assert_eq!(load.text(&[1.254]), "Load 1.25");
		assert_eq!(load.text(&[-1.0]), "Load 0.00");
		let cpus = std::thread::available_parallelism().unwrap().get() as f32;
		assert_eq!(load.peak_floor(), Some(cpus));
		let disk = DiskReader::new(disk::Drives::Only(Vec::new())).unwrap();
		assert_eq!(
			disk.text(&[30_000.0, 1.5e6]),
			"Disk\nRead 1.5 MB/s\nWrite 30 kB/s"
		);
		assert_eq!(disk.peak_floor(), Some(100_000.0));
		let log = Look::default().with(Which::Disk, |e| e.vertical = Some(Vertical::Log10));
		assert_eq!(DiskReader::vertical(&log), Vertical::Log10);
		assert_eq!(DiskReader::vertical(&Look::default()), Vertical::Linear);
		assert_eq!(LoadReader::vertical(&log), Vertical::Linear);
		assert!(Traffic::default().peak_floor().is_some());
	}

	// Test ID: dejb6
	#[test]
	fn own_use_reads_in_fine_percents_and_fills_at_every_cpu() {
		for (p, want) in [
			(0.0, "0.000%"),
			(-1.0, "0.000%"),
			(f32::NAN, "0.000%"),
			(0.0144, "0.014%"),
			(0.0996, "0.10%"),
			(0.46, "0.46%"),
			(1.234, "1.2%"),
			(9.96, "10%"),
			(100.0, "100%"),
		] {
			assert_eq!(fine_pc(p), want, "{p}");
		}
		let mut own = OwnReader::default();
		let one = 0.0046 / cpus();
		assert_eq!(
			own.text(&[one]),
			format!(
				"tukzmon {} of all CPUs\n0.46% of one CPU",
				fine_pc(one * 100.0)
			)
		);
		own.host = own::Host::Panel;
		assert!(own.text(&[one]).starts_with("Panel "));
		assert!(
			own.text(&[one])
				.ends_with("\nThe whole panel, tukzmon in it")
		);
		assert_eq!(own.peak_floor(), Some(1.0));
		assert_eq!(OwnReader::vertical(&Look::default()), Vertical::Linear);
	}

	// Test ID: dej0t
	#[test]
	fn temperature_reads_in_degrees_up_to_its_crit() {
		let t = TempReader::default();
		assert_eq!(
			t.text(&[95.0, 3.4]),
			"Temperature 98 \u{b0}C\nCritical at 95 \u{b0}C"
		);
		assert_eq!(t.peak_floor(), Some(temp::CRIT_UNKNOWN));
		assert_eq!(TempReader::vertical(&Look::default()), Vertical::Linear);
	}

	// Test ID: dep7b
	#[test]
	#[cfg_attr(miri, ignore = "Miri has no netlink sockets")]
	fn every_tip_gives_one_pair_a_line() {
		assert_eq!(
			CpuReader::default().text(&[0.01, 0.02, 0.03, 0.04]),
			"CPU 10%\nUser 3%\nSystem 2%\nNice 4%\nI/O wait 1%"
		);
		assert_eq!(
			MemoryReader::default().text(&[0.4, 0.02, 0.3]),
			"Memory 40% in use\nBuffers 2%\nCache 30%"
		);
		let t = collect::Tree::new(
			"one_pair_tip",
			&[
				("device/gpu_busy_percent", "44\n"),
				("device/mem_info_vram_used", "2684354560\n"),
				("device/mem_info_vram_total", "8589934592\n"),
			],
		);
		let (gpu, vram) = gpu::amd_readers(&t.at("device"));
		let mut split = Traffic::default();
		split.set_split(config::Split::Sockets);
		let disk = DiskReader::new(disk::Drives::Only(Vec::new())).unwrap();
		let mut tips = vec![
			CpuReader::default().text(&[0.01, 0.02, 0.03, 0.04]),
			MemoryReader::default().text(&[0.4, 0.02, 0.3]),
			gpu.text(&[0.44]),
			vram.text(&[0.31]),
			SwapReader.text(&[0.1]),
			BatteryReader::default().text(&[0.5, 0.1, 0.0]),
			ZswapReader::zswap_on("/proc/no/such").text(&[0.35]),
			Traffic::default().text(&[2000.0, 0.0, 0.0, 30_000.0, 1.5e6]),
			split.text(&[2000.0, 1000.0, 5e5, 30_000.0, 1.5e6]),
			LoadReader::default().text(&[1.25]),
			disk.text(&[30_000.0, 1.5e6]),
			TempReader::default().text(&[60.0, 0.0]),
			OwnReader::default().text(&[0.001]),
		];
		#[cfg(unix)]
		tips.push(ProcsReader::at("/proc/no/such", 1000).text(&[100.0, 50.0, 50.0]));
		for tip in &tips {
			// pairs used to share a line, 2 spaces apart
			assert!(tip.lines().all(|l| !l.contains("  ")), "{tip:?}");
			// and a value a line, however the pairs are joined. "2 of 8 GiB" is one
			for line in tip.lines() {
				let values = line
					.replace(" of ", "/")
					.split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '/'))
					.filter(|w| w.starts_with(|c: char| c.is_ascii_digit()))
					.count();
				assert!(values <= 1, "{line:?} in {tip:?}");
			}
		}
	}

	// Test ID: defpr
	#[test]
	#[cfg_attr(miri, ignore = "Miri runs too slow for the real clock's margins")]
	fn a_new_time_axis_eases_in_and_then_stops() {
		let mut g = Graph::new(Look::default());
		g.sample();
		std::thread::sleep(std::time::Duration::from_millis(40));
		g.sample();
		g.render(64, 20, 1);
		assert!(!g.easing());
		Rc::make_mut(&mut g.look).time = Timing::Linear;
		g.render(64, 20, 1);
		assert!(g.easing());
		g.skip(axis::EASE);
		assert!(!g.easing());
		// so the panel stops drawing extra frames
		g.render(64, 20, 1);
		assert!(!g.easing());
	}

	// Test ID: debj5
	#[test]
	fn linear_axis_follows_the_sample_rate() {
		let g = Graph::new(
			Look {
				time: Timing::Linear,
				sample_ms: 250,
				..Look::default()
			}
			.with(Which::Cpu, |e| e.sample = 100),
		);
		assert_eq!(g.axis(100), Axis::linear(25.0));
		// a graph's own rate rules its own axis
		let g = Graph::new(
			Look {
				time: Timing::Linear,
				sample_ms: 250,
				..Look::default()
			}
			.with(Which::Cpu, |e| e.sample = 400),
		);
		assert_eq!(g.axis(100), Axis::linear(100.0));
	}

	// Test ID: dej9z
	#[test]
	fn a_graph_reads_on_the_beats_of_its_own_rate() {
		let mut g = Graph::new(
			Look {
				sample_ms: 500,
				..Look::default()
			}
			.with(Which::Cpu, |e| e.sample = 100),
		);
		assert_eq!(g.sample_ms(), 500);
		// the first call primes it, wherever it falls
		assert_eq!(g.tick(1230.0), 270.0);
		assert_eq!(g.beat, Some((2.0, 1230.0)));
		// a timer a little early still reads, and the next beat is a whole one on
		assert_eq!(g.tick(1495.0), 505.0);
		assert_eq!(g.beat, Some((3.0, 1495.0)));
		// a wake for another graph's beat reads nothing here
		assert_eq!(g.tick(1700.0), 300.0);
		assert_eq!(g.beat, Some((3.0, 1495.0)));
		// a late one reads, and the beat after stays on the grid
		assert_eq!(g.tick(2040.0), 460.0);
		assert_eq!(g.beat, Some((4.0, 2040.0)));

		// its own percentage of Sample ms default, and a gap under half a beat
		// after the change is too short to read
		Rc::make_mut(&mut g.look)[Which::Cpu].sample = 400;
		assert_eq!(g.sample_ms(), 2000);
		assert_eq!(g.tick(2500.0), 1500.0);
		assert_eq!(g.beat, Some((4.0, 2040.0)));
		assert_eq!(g.tick(4000.0), 2000.0);
		assert_eq!(g.beat, Some((2.0, 4000.0)));
		// as fast as the finest history tier, and no faster
		Rc::make_mut(&mut g.look)[Which::Cpu].sample = 2;
		assert_eq!(g.sample_ms(), FASTEST_MS);
		Rc::make_mut(&mut g.look)[Which::Cpu].sample = 100;
		assert_eq!(g.sample_ms(), 500);
		Rc::make_mut(&mut g.look)[Which::Cpu].sample = 33;
		assert_eq!(g.sample_ms(), 165);
		assert_eq!(
			(wait_ms(0.2), wait_ms(504.1), wait_ms(f64::NAN)),
			(1, 505, 50)
		);
	}

	// Test ID: decby
	#[test]
	fn log_axis_takes_the_chosen_curve() {
		let mut g = Graph::new(Look::default());
		g.hist.add(0.0, 1.0, &[0.1; 4]);
		g.hist.add(7200.0, 7201.0, &[0.1; 4]);
		assert_eq!(g.axis(100), Axis::log(axis::Curve::Smooth, 7201.0));
		Rc::make_mut(&mut g.look).time = Timing::Log(axis::Curve::FixedA);
		assert_eq!(g.axis(100), Axis::log(axis::Curve::FixedA, 7201.0));
		assert_ne!(g.axis(100), Axis::log(axis::Curve::Smooth, 7201.0));
	}

	// a busy hour, then an idle one, ending now
	fn two_hours() -> Graph {
		let mut g = Graph::new(Look {
			said: Said::Ago,
			..Look::default()
		});
		let now = g.now();
		g.hist
			.add(now - 7200.0, now - 3600.0, &[0.0, 0.0, 0.8, 0.0]);
		g.hist.add(now - 3600.0, now, &[0.0, 0.0, 0.1, 0.0]);
		g
	}

	// Test ID: decc6
	#[test]
	#[cfg_attr(miri, ignore = "the clock tip calls nl_langinfo, which is C")]
	fn the_hover_tip_says_when_and_what() {
		let mut g = two_hours();
		// 130 px wide, 1 px border: 128 columns, as render would draw them
		let (w, h) = (130, 20);
		let (a, b) = g.span_at(1, w, h, 1).unwrap();
		assert!(
			near_secs(a, g.hist.first().unwrap()),
			"left edge is the first sample"
		);
		assert!(b > a);
		let (a, b) = g.span_at(128, w, h, 1).unwrap();
		assert!(near_secs(b, g.hist.last().unwrap()), "right edge is now");
		assert!(a < b);
		// the border reads as the plot's edge
		assert_eq!(g.span_at(0, w, h, 1), g.span_at(1, w, h, 1));
		assert_eq!(g.span_at(500, w, h, 1), g.span_at(128, w, h, 1));
		let left = g.hover_tip(1, w, h, 1);
		assert!(left.starts_with("1 hr ago\nCPU 80%\nUser 80%"), "{left}");
		let right = g.hover_tip(128, w, h, 1);
		assert!(right.starts_with("just now\nCPU 10%"), "{right}");
		Rc::make_mut(&mut g.look).said = Said::Clock;
		let tip = g.hover_tip(128, w, h, 1);
		assert_eq!(tip.lines().next().map(str::len), Some(8), "{tip}");
		// device columns on a scale 2 screen fall on the same times
		assert_eq!(g.span_at(2, 2 * w, 2 * h, 2), g.span_at(1, w, h, 1));
	}

	// Test ID: defz1
	#[test]
	#[cfg_attr(miri, ignore = "Miri runs too slow for the real clock's margins")]
	fn a_still_pointer_keeps_its_age_while_its_clock_time_moves_on() {
		let mut g = Graph::new(
			Look {
				time: Timing::Linear,
				sample_ms: 500,
				said: Said::Ago,
				..Look::default()
			}
			.with(Which::Cpu, |e| e.sample = 100),
		);
		let now = g.now();
		g.hist.add(now - 300.0, now, &[0.1; 4]);
		// 128 columns of 0.5 s: column 65 is 31.75 s back, clear of a rounding edge
		let (w, h) = (130, 20);
		let age = |g: &Graph| {
			let (a, b) = g.span_at(65, w, h, 1).unwrap();
			(
				f64::midpoint(a, b),
				g.hist.last().unwrap() - f64::midpoint(a, b),
			)
		};
		let (t0, age0) = age(&g);
		let tip0 = g.hover_tip(65, w, h, 1);
		assert!(tip0.starts_with("31 sec ago\n"), "{tip0}");
		for _ in 0..4 {
			let from = g.hist.last().unwrap();
			g.skip(1.0);
			g.hist.add(from, from + 1.0, &[0.1; 4]);
			assert!(
				near_secs(age(&g).1, age0),
				"the column under the line is a fixed age"
			);
			assert!(g.hover_tip(65, w, h, 1).starts_with("31 sec ago\n"));
		}
		assert!(
			near_secs(age(&g).0 - t0, 4.0),
			"the clock time under the line moves on with each sample"
		);
	}

	fn near_secs(a: f64, b: f64) -> bool {
		(a - b).abs() < 1e-6
	}

	// Test ID: df3e4
	#[test]
	fn frames_are_an_even_split_of_the_screen_rate_from_30_to_60() {
		for (hz, apart) in [
			(60.0, 1),
			(59.94, 1),
			(60.0024, 1),
			(75.0, 2),
			(120.0, 2),
			(144.0, 3),
			(165.0, 3),
			(240.0, 4),
			(360.0, 6),
			(30.0, 1),
			(0.0, 1),
			(f64::NAN, 1),
		] {
			assert_eq!(Pace::UpTo60.frames_apart(hz), apart, "{hz} Hz");
		}
		for hz in 60..=500 {
			let fps = f64::from(hz) / f64::from(Pace::UpTo60.frames_apart(f64::from(hz)));
			assert!((30.0..=61.0).contains(&fps), "{hz} Hz draws {fps} a second");
		}
	}

	// Test ID: df3me
	#[test]
	fn a_quarter_pace_splits_the_screen_rate_evenly_from_24_to_50() {
		for (hz, apart) in [
			(60.0, 2),
			(59.94, 2),
			(75.0, 3),
			(90.0, 3),
			(95.9, 4),
			(99.99, 4),
			(100.0, 4),
			(120.0, 4),
			(144.0, 4),
			(165.0, 4),
			(200.01, 4),
			(240.0, 5),
			(360.0, 8),
			(48.0, 2),
			(30.0, 1),
			(24.0, 1),
			(20.0, 1),
			(0.0, 1),
			(f64::NAN, 1),
		] {
			assert_eq!(Pace::Quarter.frames_apart(hz), apart, "{hz} Hz");
		}
		for hz in 48..=1000 {
			let apart = Pace::Quarter.frames_apart(f64::from(hz));
			let fps = f64::from(hz) / f64::from(apart);
			assert!(
				(23.75..=50.5).contains(&fps),
				"{hz} Hz draws {fps} a second"
			);
			if (96..=200).contains(&hz) {
				assert_eq!(apart, 4, "{hz} Hz");
			}
		}
	}

	// Test ID: df3e1
	#[test]
	#[cfg_attr(miri, ignore = "Miri runs too slow for the real clock's margins")]
	fn a_new_sample_slides_in_at_a_steady_speed_but_a_gap_does_not() {
		let look = |motion| {
			Look {
				motion,
				sample_ms: 1000,
				..Look::default()
			}
			.with(Which::Mem, |e| e.sample = 100)
		};
		let near = |a: f64, b: f64| (a - b).abs() < 0.02;
		// memory, since CPU reads nothing when no real time went by
		let mut g = Graph::with(look(Motion::On), MemoryReader::default());
		g.sample();
		g.skip(1.0);
		g.sample();
		g.skip(1.0);
		g.sample();
		let (from, _) = g.slide.unwrap();
		let last = g.hist.last().unwrap();
		assert!(near(last - from, 1.0));
		assert!(
			near(g.edge().unwrap(), from),
			"it starts where it was drawn"
		);
		assert!(g.sliding() && g.moving());
		g.skip(0.5);
		assert!(
			near(g.edge().unwrap(), from + 0.5),
			"half way at half the time"
		);
		g.skip(0.5);
		assert_eq!(g.edge(), Some(last));
		assert!(!g.sliding());
		g.skip(10.0);
		g.sample();
		assert_eq!(g.edge(), g.hist.last(), "a gap is the ease's");
		assert!(!g.sliding());

		let mut off = Graph::with(look(Motion::Off), MemoryReader::default());
		for _ in 0..2 {
			off.sample();
			off.skip(1.0);
		}
		off.sample();
		assert_eq!(off.edge(), off.hist.last());
		assert!(!off.sliding());
	}

	// Test ID: df3e2
	#[test]
	#[cfg_attr(miri, ignore = "Miri runs too slow for the real clock's margins")]
	fn the_right_column_fills_as_the_sample_slides_in_and_the_tip_follows() {
		let mut g = Graph::new(
			Look {
				motion: Motion::On,
				time: Timing::Linear,
				sample_ms: 1000,
				spikes: 0.0,
				..Look::default()
			}
			.with(Which::Cpu, |e| e.sample = 100),
		);
		let now = g.now();
		g.hist.add(now - 10.0, now - 1.0, &[0.0; 4]);
		g.hist.add(now - 1.0, now, &[1.0, 0.0, 0.0, 0.0]);
		g.slide = Some((now - 1.0, g.now()));
		// 10 columns of 1 s inside a 1 px border
		let (w, h) = (12, 20);
		let right = w - 2;
		let column = |g: &mut Graph| {
			let px = g.render(w, h, 1);
			(1..h - 1).map(|y| px[y * w + right]).collect::<Vec<_>>()
		};
		let start = column(&mut g);
		g.skip(0.5);
		let half = column(&mut g);
		let (a, b) = g.span_at(right, w, h, 1).unwrap();
		assert!(
			(b - (now - 0.5)).abs() < 0.02 && (b - a - 1.0).abs() < 1e-6,
			"{a} {b}"
		);
		g.skip(0.6);
		let end = column(&mut g);
		let user = end[end.len() - 1];
		let filled = |c: &[u32]| c.iter().filter(|&&p| p == user).count();
		assert_eq!(filled(&start), 0, "{start:x?}");
		assert!((7..=11).contains(&filled(&half)), "{half:x?}");
		assert!(filled(&end) >= 17, "{end:x?}");
	}

	// Test ID: decc7
	#[test]
	fn the_hover_tip_shows_gaps_and_no_history() {
		let g = Graph::new(Look::default());
		assert_eq!(g.span_at(5, 130, 20, 1), None);
		assert_eq!(g.hover_tip(5, 130, 20, 1), "CPU");
		let mut g = Graph::new(Look {
			time: Timing::Linear,
			..Look::default()
		});
		let now = g.now();
		g.hist.add(now - 10.0, now, &[0.1; 4]);
		// linear at 1000 ms covers 128 s, so the left edge is before the first sample
		assert!(g.hover_tip(1, 130, 20, 1).ends_with("\nNo data"));
		assert!(g.hover_tip(128, 130, 20, 1).contains("CPU 40%"));
	}

	// Test ID: decc8
	#[test]
	fn the_hover_line_is_drawn_where_the_pointer_is() {
		let mut g = two_hours();
		Rc::make_mut(&mut g.look).hover = config::Rgb(1, 2, 3);
		let (w, h) = (40, 10);
		let plain = g.render(w, h, 1).to_vec();
		assert!(!plain.contains(&0xff01_0203));
		g.hover = Some(17);
		let px = g.render(w, h, 1).to_vec();
		for r in 0..h {
			for x in 0..w {
				let lit = px[r * w + x] == 0xff01_0203;
				assert_eq!(lit, x == 17 && (1..h - 1).contains(&r), "{x},{r}");
				if !lit {
					assert_eq!(px[r * w + x], plain[r * w + x]);
				}
			}
		}
		g.hover = None;
		assert_eq!(g.render(w, h, 1), plain.as_slice());
	}

	fn scratch(name: &str) -> std::path::PathBuf {
		std::env::temp_dir().join(format!(
			"test_tukzmon_graph_{}_{name}.hist",
			std::process::id()
		))
	}

	// Test ID: decbp
	#[test]
	fn saved_history_comes_back_with_a_gap() {
		let path = scratch("gap");
		let mut g = Graph::new(Look::default());
		g.sample();
		std::thread::sleep(std::time::Duration::from_millis(30));
		g.sample();
		let (first, last) = (g.hist.first().unwrap(), g.hist.last().unwrap());
		g.save(&path).unwrap();
		std::thread::sleep(std::time::Duration::from_millis(30));
		let mut back = Graph::new(Look::default());
		back.load(&path).unwrap();
		assert_eq!(
			(back.hist.first(), back.hist.last()),
			(Some(first), Some(last))
		);
		// the first read after a load only primes, so the time between is a gap
		back.sample();
		assert_eq!(back.hist.last(), Some(last));
		assert!(back.read_at.unwrap() >= last + 0.03);
		std::fs::remove_file(&path).unwrap();
	}

	// Test ID: decbq
	#[test]
	fn a_clock_set_back_still_takes_new_samples() {
		let path = scratch("back");
		let mut g = Graph::new(Look::default());
		g.hist.add(g.now() + 3600.0, g.now() + 3601.0, &[0.1; 4]);
		let saved = g.hist.last().unwrap();
		g.save(&path).unwrap();
		let mut back = Graph::new(Look::default());
		back.load(&path).unwrap();
		back.sample();
		std::thread::sleep(std::time::Duration::from_millis(30));
		back.sample();
		assert!(back.hist.last().unwrap() > saved);
		std::fs::remove_file(&path).unwrap();
	}

	// Test ID: deqqs
	#[test]
	#[cfg_attr(miri, ignore = "Miri runs too slow for the real clock's margins")]
	fn a_suspend_moves_the_clock_to_the_wall_and_is_a_gap() {
		let mut g = Graph::with(Look::default(), MemoryReader::default());
		g.sample();
		g.skip(1.0);
		g.sample();
		let had = g.hist.last().unwrap();
		g.wall_shift = 3600.0;
		assert!(
			(g.now() - g.wall()).abs() < 0.01,
			"tips go by the wall clock"
		);
		g.sample();
		assert_eq!(g.hist.last(), Some(had), "the first read after only primes");
		g.skip(1.0);
		g.sample();
		assert!(g.hist.last().unwrap() > had + 3600.0);
		assert_eq!(g.hist.span(had + 1.0, had + 3599.0).covered, 0.0);
		// a step under a sample's worth is only a late read
		g.wall_shift += f64::from(g.sample_ms()) / 2000.0;
		g.skip(1.0);
		g.sample();
		assert!((g.hist.last().unwrap() - g.wall()).abs() < 0.01);
		assert!(g.hist.span(had + 3601.0, had + 3602.0).covered > 0.99);
	}

	// Test ID: deqqt
	#[test]
	#[cfg_attr(miri, ignore = "Miri runs too slow for the real clock's margins")]
	fn a_clock_set_back_while_running_never_runs_back() {
		let mut g = Graph::with(Look::default(), MemoryReader::default());
		g.sample();
		g.skip(1.0);
		g.sample();
		let had = g.hist.last().unwrap();
		g.wall_shift = -3600.0;
		assert!(g.now() >= had);
		g.skip(1.0);
		g.sample();
		let now = g.hist.last().unwrap();
		assert!((now - had - 1.0).abs() < 0.01, "{}", now - had);
		assert!(g.hist.span(had, now).covered > 0.99);
	}

	// Test ID: deqs6
	#[test]
	fn history_saved_hours_ahead_moves_the_clock_but_a_day_ahead_starts_over() {
		let ahead = |secs: f64| {
			let mut g = Graph::with(Look::default(), MemoryReader::default());
			let end = g.now() + secs;
			let mut h = History::<3>::default();
			h.add(end - 60.0, end, &[0.5; 3]);
			let mut body = Vec::new();
			h.encode(&mut body);
			let took = g.adopt(&[(MemoryReader::SECTION.to_string(), body)]);
			(took, g.now() - end)
		};
		let (took, off) = ahead(8.0 * 3600.0);
		assert!(took && (0.0..60.0).contains(&off), "{off}");
		let (took, off) = ahead(86400.0);
		assert!(!took && off < -86000.0, "{off}");
	}

	// Test ID: decbr
	#[test]
	fn load_trims_to_the_max_age_and_refuses_junk() {
		let path = scratch("trim");
		let mut g = Graph::new(Look::default());
		let now = g.now();
		g.hist
			.add(now - 3.0 * 86400.0, now - 3.0 * 86400.0 + 60.0, &[0.1; 4]);
		g.hist.add(now - 60.0, now - 59.0, &[0.2; 4]);
		g.save(&path).unwrap();
		let mut back = Graph::new(Look {
			max_days: 1,
			..Look::default()
		});
		back.load(&path).unwrap();
		assert!(back.hist.first().unwrap() > now - 86401.0);
		assert_eq!(back.hist.last(), Some(now - 59.0));
		std::fs::write(&path, b"junk").unwrap();
		assert!(back.load(&path).is_err());
		assert_eq!(back.hist.last(), Some(now - 59.0), "kept what it had");
		std::fs::remove_file(&path).unwrap();
	}
}
