// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! The part of tukzmon every panel shares. No GUI toolkit in here.

pub mod axis;
pub mod battery;
pub mod collect;
pub mod color;
pub mod config;
pub mod cpu;
pub mod disk;
pub mod graphs;
pub mod history;
pub mod keep;
pub mod load;
pub mod mem;
pub mod net;
pub mod own;
pub mod render;
pub mod temp;
pub mod when;
pub mod zswap;

use std::io;
use std::path::Path;
use std::time::{Instant, SystemTime};

use axis::{Ages, Axis, Basis, Ease, Timing};
use battery::BatteryReader;
use collect::Collector;
use config::Look;
use cpu::CpuReader;
use disk::DiskReader;
use history::History;
use load::LoadReader;
use mem::{MemoryReader, SwapReader};
use net::NetReader;
use own::OwnReader;
use render::{Frame, Paint, Peak, Scale};
use temp::TempReader;
use when::Said;
use zswap::ZswapReader;

/// No graph is read more often than this, the finest history tier.
pub const FASTEST_MS: u32 = 50;

/// A read due this soon counts as due now, since a timer may fire a little
/// early. So the wait to the next read can be up to this much over the rate.
pub const EARLY_MS: f64 = 10.0;

/// The CPUs this process can run on, read once.
pub(crate) fn cpus() -> f32 {
	static CPUS: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
	*CPUS.get_or_init(|| std::thread::available_parallelism().map_or(1.0, |n| n.get() as f32))
}

/// Milliseconds on one clock that every graph's beats go by. See
/// [`Graph::tick`].
pub fn clock_ms() -> f64 {
	static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
	START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
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
	/// The graph's section in the history file.
	const SECTION: &'static str;
	/// The tip before there's anything read.
	const TITLE: &'static str;
	fn text(&self, v: &[f32; N]) -> String;
	fn paint(look: &Look) -> Paint<'_, N>;
	fn timing(look: &Look) -> Timing;
	/// How often the graph is read, in ms. See [`Look::rate`].
	fn sample_ms(look: &Look) -> u32;

	/// Some for a source with no fixed top, such as bytes per second: the
	/// least its peak goes down to. See [`render::Peak`].
	fn peak_floor(&self) -> Option<f32> {
		None
	}

	/// A log scale up the graph, for a source with a peak floor.
	fn log_y(_look: &Look) -> bool {
		false
	}

	/// Takes up a new look's settings for the reader itself, such as which
	/// drives to count.
	fn follow(&mut self, _look: &Look) {}
}

fn pc(x: f32) -> i32 {
	(x * 100.0).round() as i32
}

impl Source<4> for CpuReader {
	const SECTION: &'static str = "cpu";
	const TITLE: &'static str = "CPU";

	fn text(&self, v: &[f32; 4]) -> String {
		let busy: f32 = v.iter().sum();
		format!(
			"CPU {}%\nUser {}%  System {}%\nNice {}%  I/O wait {}%",
			pc(busy),
			pc(v[2]),
			pc(v[1]),
			pc(v[3]),
			pc(v[0])
		)
	}

	fn paint(look: &Look) -> Paint<'_, 4> {
		look.cpu_paint()
	}

	fn timing(look: &Look) -> Timing {
		look.timing(look.cpu_time)
	}

	fn sample_ms(look: &Look) -> u32 {
		look.rate(look.cpu_sample_ms)
	}
}

impl Source<3> for MemoryReader {
	const SECTION: &'static str = "mem";
	const TITLE: &'static str = "Memory";

	fn text(&self, v: &[f32; 3]) -> String {
		format!(
			"Memory {}% in use\nBuffers {}%  Cache {}%",
			pc(v[0]),
			pc(v[1]),
			pc(v[2])
		)
	}

	fn paint(look: &Look) -> Paint<'_, 3> {
		look.mem_paint()
	}

	fn timing(look: &Look) -> Timing {
		look.timing(look.mem_time)
	}

	fn sample_ms(look: &Look) -> u32 {
		look.rate(look.mem_sample_ms)
	}
}

impl Source<1> for SwapReader {
	const SECTION: &'static str = "swap";
	const TITLE: &'static str = "Swap";

	fn text(&self, v: &[f32; 1]) -> String {
		format!("Swap {}% used", pc(v[0]))
	}

	fn paint(look: &Look) -> Paint<'_, 1> {
		look.swap_paint()
	}

	fn timing(look: &Look) -> Timing {
		look.timing(look.swap_time)
	}

	fn sample_ms(look: &Look) -> u32 {
		look.rate(look.swap_sample_ms)
	}
}

impl Source<3> for BatteryReader {
	const SECTION: &'static str = "battery";
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
		format!("Battery {}%, {state}", pc(v.iter().sum()))
	}

	fn paint(look: &Look) -> Paint<'_, 3> {
		look.battery_paint()
	}

	fn timing(look: &Look) -> Timing {
		look.timing(look.battery_time)
	}

	fn sample_ms(look: &Look) -> u32 {
		look.rate(look.battery_sample_ms)
	}
}

impl Source<1> for ZswapReader {
	const SECTION: &'static str = "zswap";
	const TITLE: &'static str = "Zswap";

	// the ratio isn't in the history, so it's the latest whatever time is hovered
	fn text(&self, v: &[f32; 1]) -> String {
		let used = format!("{} {}% used", self.name(), pc(v[0]));
		match self.ratio() {
			Some(r) => format!("{used}\nCompressed {r:.1}x now"),
			None => used,
		}
	}

	fn paint(look: &Look) -> Paint<'_, 1> {
		look.zswap_paint()
	}

	fn timing(look: &Look) -> Timing {
		look.timing(look.zswap_time)
	}

	fn sample_ms(look: &Look) -> u32 {
		look.rate(look.zswap_sample_ms)
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

impl Source<3> for NetReader {
	const SECTION: &'static str = "net";
	const TITLE: &'static str = "Network";

	// so a quiet line's odd packet doesn't fill the graph
	fn peak_floor(&self) -> Option<f32> {
		Some(10_000.0)
	}

	fn text(&self, v: &[f32; 3]) -> String {
		format!(
			"Network\nReceived {}  Sent {}\nLocal {}",
			per_sec(v[2]),
			per_sec(v[1]),
			per_sec(v[0])
		)
	}

	fn paint(look: &Look) -> Paint<'_, 3> {
		look.net_paint()
	}

	fn timing(look: &Look) -> Timing {
		look.timing(look.net_time)
	}

	fn sample_ms(look: &Look) -> u32 {
		look.rate(look.net_sample_ms)
	}
}

impl Source<1> for LoadReader {
	const SECTION: &'static str = "load";
	const TITLE: &'static str = "Load";

	fn text(&self, v: &[f32; 1]) -> String {
		format!("Load {:.2}", v[0].max(0.0))
	}

	fn paint(look: &Look) -> Paint<'_, 1> {
		look.load_paint()
	}

	fn timing(look: &Look) -> Timing {
		look.timing(look.load_time)
	}

	fn sample_ms(look: &Look) -> u32 {
		look.rate(look.load_sample_ms)
	}

	// full at a load of one a CPU, as the CPU graph is at 100%
	fn peak_floor(&self) -> Option<f32> {
		Some(cpus())
	}
}

impl Source<2> for DiskReader {
	const SECTION: &'static str = "disk";
	const TITLE: &'static str = "Disk";

	fn text(&self, v: &[f32; 2]) -> String {
		format!("Disk\nRead {}  Write {}", per_sec(v[1]), per_sec(v[0]))
	}

	fn paint(look: &Look) -> Paint<'_, 2> {
		look.disk_paint()
	}

	fn timing(look: &Look) -> Timing {
		look.timing(look.disk_time)
	}

	fn sample_ms(look: &Look) -> u32 {
		look.rate(look.disk_sample_ms)
	}

	// so a log file's odd flush doesn't fill the graph
	fn peak_floor(&self) -> Option<f32> {
		Some(100_000.0)
	}

	fn log_y(look: &Look) -> bool {
		look.disk_log
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
	const SECTION: &'static str = "temp";
	const TITLE: &'static str = "Temperature";

	fn text(&self, v: &[f32; 2]) -> String {
		format!(
			"Temperature {:.0} \u{b0}C\nCritical at {:.0} \u{b0}C",
			v[0] + v[1],
			self.limit()
		)
	}

	fn paint(look: &Look) -> Paint<'_, 2> {
		look.temp_paint()
	}

	fn timing(look: &Look) -> Timing {
		look.timing(look.temp_time)
	}

	fn sample_ms(look: &Look) -> u32 {
		look.rate(look.temp_sample_ms)
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
	const SECTION: &'static str = "self";
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
			_ => tip,
		}
	}

	fn paint(look: &Look) -> Paint<'_, 1> {
		look.self_paint()
	}

	fn timing(look: &Look) -> Timing {
		look.timing(look.self_time)
	}

	fn sample_ms(look: &Look) -> u32 {
		look.rate(look.self_sample_ms)
	}

	// full at every CPU busy, like the CPU graph
	fn peak_floor(&self) -> Option<f32> {
		Some(1.0)
	}
}

/// One graph: reads, history, and drawing. CPU unless said otherwise.
pub struct Graph<S: Source<N> = CpuReader, const N: usize = 4> {
	pub look: Look,
	hist: History<N>,
	src: S,
	clock: Instant,
	/// Unix time at `clock`'s start, so saved history lines up across runs.
	epoch: f64,
	read_at: Option<f64>,
	latest: Option<[f32; N]>,
	px: Vec<u32>,
	ease: Ease,
	/// The top, for a source with no fixed one.
	peak: Peak,
	/// Where the pointer is, in device pixels from the left, for the hover line.
	pub hover: Option<usize>,
	/// The beat of the last read from [`Graph::tick`], and its time on
	/// [`clock_ms`].
	beat: Option<(f64, f64)>,
}

impl Graph {
	pub fn new(look: Look) -> Graph {
		Graph::with(look, CpuReader::default())
	}
}

impl Graph<MemoryReader, 3> {
	/// None where memory can't be read, so the graph is left out.
	pub fn memory(look: Look) -> Option<Self> {
		MemoryReader::probe().map(|src| Graph::with(look, src))
	}
}

impl Graph<SwapReader, 1> {
	/// None on a box with no swap.
	pub fn swap(look: Look) -> Option<Self> {
		SwapReader::probe().map(|src| Graph::with(look, src))
	}
}

impl Graph<ZswapReader, 1> {
	/// None on a box with neither zswap on nor zram set up.
	pub fn zswap(look: Look) -> Option<Self> {
		ZswapReader::probe().map(|src| Graph::with(look, src))
	}
}

impl Graph<NetReader, 3> {
	/// None where `/proc/net/dev` can't be read.
	pub fn net(look: Look) -> Option<Self> {
		NetReader::probe().map(|src| Graph::with(look, src))
	}
}

impl Graph<LoadReader, 1> {
	/// None where `/proc/loadavg` can't be read.
	pub fn load_avg(look: Look) -> Option<Self> {
		LoadReader::probe().map(|src| Graph::with(look, src))
	}
}

impl Graph<DiskReader, 2> {
	/// None where `/proc/diskstats` can't be read, or with all disks picked
	/// on a box that has none.
	pub fn disk(look: Look) -> Option<Self> {
		DiskReader::new(look.drives()).map(|src| Graph::with(look, src))
	}
}

impl Graph<TempReader, 2> {
	/// None on a box with no CPU sensor it knows.
	pub fn temp(look: Look) -> Option<Self> {
		TempReader::probe().map(|src| Graph::with(look, src))
	}
}

impl Graph<OwnReader, 1> {
	/// None where the process's CPU time can't be read.
	pub fn own(look: Look) -> Option<Self> {
		OwnReader::probe().map(|src| Graph::with(look, src))
	}
}

impl Graph<BatteryReader, 3> {
	/// None on a box with no battery in.
	pub fn battery(look: Look) -> Option<Self> {
		BatteryReader::probe().map(|src| Graph::with(look, src))
	}
}

impl<S: Source<N>, const N: usize> Graph<S, N> {
	fn with(look: Look, src: S) -> Self {
		Graph {
			look,
			hist: History::default(),
			src,
			clock: Instant::now(),
			epoch: SystemTime::now()
				.duration_since(SystemTime::UNIX_EPOCH)
				.map_or(0.0, |d| d.as_secs_f64()),
			read_at: None,
			latest: None,
			px: Vec::new(),
			ease: Ease::default(),
			peak: Peak::default(),
			hover: None,
			beat: None,
		}
	}

	fn now(&self) -> f64 {
		self.epoch + self.clock.elapsed().as_secs_f64()
	}

	/// Moves the clock on, so the soak test can run days in seconds.
	#[doc(hidden)]
	pub fn skip(&mut self, secs: f64) {
		self.epoch += secs;
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
	}

	/// Swaps in the saved history, if there is one that reads. The time since
	/// it was saved is a gap.
	///
	/// # Errors
	///
	/// As [`keep::read`], or `InvalidData` with no history of this graph's
	/// that reads. The history in memory is left as it was.
	pub fn load(&mut self, path: &Path) -> io::Result<()> {
		if self.adopt(&keep::read(path)?) {
			Ok(())
		} else {
			Err(io::Error::new(
				io::ErrorKind::InvalidData,
				format!("no {} history", S::SECTION),
			))
		}
	}

	/// [`Self::load`] from a file's sections, read once for every graph in
	/// it. False, with nothing changed, when none of them is this graph's.
	pub fn adopt(&mut self, sections: &[(String, Vec<u8>)]) -> bool {
		let Some(saved) = sections
			.iter()
			.find(|(name, _)| name == S::SECTION)
			.and_then(|(_, body)| History::decode(body))
		else {
			return false;
		};
		// a clock set back mustn't hide new samples behind old ones
		if let Some(last) = saved.last() {
			self.epoch += (last - self.now()).max(0.0);
		}
		self.hist = saved;
		self.read_at = None;
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
	pub fn save(&self, path: &Path) -> io::Result<()> {
		let (name, body) = self.section();
		keep::write(path, &[(name, &body)])
	}

	/// Read the source and keep it. The first call only primes it.
	pub fn sample(&mut self) {
		let now = self.now();
		let shares = self.src.read();
		if let (Some(v), Some(was)) = (shares, self.read_at) {
			self.hist.add(was, now, &v);
			self.latest = Some(v);
		}
		self.read_at = Some(now);
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

	pub fn axis(&self, columns: usize) -> Axis {
		Axis::of(&self.basis(columns))
	}

	fn basis(&self, columns: usize) -> Basis {
		match S::timing(&self.look) {
			Timing::Log(curve) => Basis::Log {
				curve,
				span: self.hist.first().zip(self.hist.last()),
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
		let now = self.hist.last()?;
		let bw = self.border_px(w, h, scale);
		if w <= 2 * bw || h <= 2 * bw {
			return None;
		}
		let pw = w - 2 * bw;
		let thick = scale.clamp(1, pw);
		let x0 = x.clamp(bw, bw + pw - thick) - bw;
		let axis = self.axis(self.columns(w, scale));
		let axis = self.ease.view(&axis, self.now());
		let u = |cx: usize| (pw - cx) as f64 / pw as f64;
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
		let basis = self.basis(self.columns(w, scale));
		let axis = Axis::of(&basis);
		let t = self.now();
		// a sample may come late by most of a tick, and a tick may be missed
		let slack = 2.0 * f64::from(self.sample_ms()) / 1000.0 + 0.25;
		self.ease.update(&axis, basis, t, slack);
		let mut px = std::mem::take(&mut self.px);
		let log = S::log_y(&self.look);
		let peak = self.src.peak_floor().map(|floor| Scale {
			peak: &mut self.peak,
			floor,
			log,
		});
		Frame {
			hist: &self.hist,
			axis: &self.ease.view(&axis, t),
			now: self.hist.last().unwrap_or(0.0),
			look: &self.look,
			paint: S::paint(&self.look),
		}
		.draw_to(&mut px, w, h, scale, peak);
		if let Some(x) = self.hover {
			let bw = self.border_px(w, h, scale);
			render::line(&mut px, w, h, bw, x, scale, self.look.hover);
		}
		self.px = px;
		&self.px
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
			NetReader::default().text(&[2000.0, 30_000.0, 1.5e6]),
			"Network\nReceived 1.5 MB/s  Sent 30 kB/s\nLocal 2.0 kB/s"
		);
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
			"Disk\nRead 1.5 MB/s  Write 30 kB/s"
		);
		assert_eq!(disk.peak_floor(), Some(100_000.0));
		let log = Look {
			disk_log: true,
			..Look::default()
		};
		assert!(DiskReader::log_y(&log) && !DiskReader::log_y(&Look::default()));
		assert!(!LoadReader::log_y(&log) && NetReader::default().peak_floor().is_some());
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
		assert!(!OwnReader::log_y(&Look::default()));
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
		assert!(!TempReader::log_y(&Look::default()));
	}

	// Test ID: defpr
	#[test]
	fn a_new_time_axis_eases_in_and_then_stops() {
		let mut g = Graph::new(Look::default());
		g.sample();
		std::thread::sleep(std::time::Duration::from_millis(40));
		g.sample();
		g.render(64, 20, 1);
		assert!(!g.easing());
		g.look.time = Timing::Linear;
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
		let g = Graph::new(Look {
			time: Timing::Linear,
			sample_ms: 250,
			..Look::default()
		});
		assert_eq!(g.axis(100), Axis::linear(25.0));
		// a graph's own rate rules its own axis
		let g = Graph::new(Look {
			time: Timing::Linear,
			sample_ms: 250,
			cpu_sample_ms: 1000,
			..Look::default()
		});
		assert_eq!(g.axis(100), Axis::linear(100.0));
	}

	// Test ID: dej9z
	#[test]
	fn a_graph_reads_on_the_beats_of_its_own_rate() {
		let mut g = Graph::new(Look {
			sample_ms: 500,
			..Look::default()
		});
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

		// its own rate over Sample ms default, and a gap under half a beat
		// after the change is too short to read
		g.look.cpu_sample_ms = 2000;
		assert_eq!(g.sample_ms(), 2000);
		assert_eq!(g.tick(2500.0), 1500.0);
		assert_eq!(g.beat, Some((4.0, 2040.0)));
		assert_eq!(g.tick(4000.0), 2000.0);
		assert_eq!(g.beat, Some((2.0, 4000.0)));
		// as fast as the finest history tier, and no faster
		g.look.cpu_sample_ms = 10;
		assert_eq!(g.sample_ms(), FASTEST_MS);
		g.look.cpu_sample_ms = 0;
		assert_eq!(g.sample_ms(), 500);
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
		g.look.time = Timing::Log(axis::Curve::FixedA);
		assert_eq!(g.axis(100), Axis::log(axis::Curve::FixedA, 7201.0));
		assert_ne!(g.axis(100), Axis::log(axis::Curve::Smooth, 7201.0));
	}

	// a busy hour, then an idle one, ending now
	fn two_hours() -> Graph {
		let mut g = Graph::new(Look::default());
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
		g.look.said = Said::Clock;
		let tip = g.hover_tip(128, w, h, 1);
		assert_eq!(tip.lines().next().map(str::len), Some(8), "{tip}");
		// device columns on a scale 2 screen land on the same times
		assert_eq!(g.span_at(2, 2 * w, 2 * h, 2), g.span_at(1, w, h, 1));
	}

	// Test ID: defz1
	#[test]
	fn a_still_pointer_keeps_its_age_while_its_clock_time_moves_on() {
		let mut g = Graph::new(Look {
			time: Timing::Linear,
			..Look::default()
		});
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
		// linear at 500 ms covers 64 s, so the left edge is before the first sample
		assert!(g.hover_tip(1, 130, 20, 1).ends_with("\nNo data"));
		assert!(g.hover_tip(128, 130, 20, 1).contains("CPU 40%"));
	}

	// Test ID: decc8
	#[test]
	fn the_hover_line_is_drawn_where_the_pointer_is() {
		let mut g = two_hours();
		g.look.hover = config::Rgb(1, 2, 3);
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
