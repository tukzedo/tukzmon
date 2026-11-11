// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! The first GPU's busy time and memory, for the GPU and GPU memory graphs.
//! NVIDIA's through NVML, loaded at run time so a box without the driver
//! still starts, AMD's from amdgpu's files in sysfs, and Intel's busy time
//! from its sleep counter in sysfs. None of them needs root. See design.md,
//! Data sources on Linux.

use crate::collect::{Collector, Kept};

#[cfg(not(miri))]
mod nvml {
	use std::ffi::{c_int, c_uint, c_void};
	use std::sync::OnceLock;

	pub(super) type Device = *mut c_void;

	#[repr(C)]
	#[derive(Default)]
	pub(super) struct Utilization {
		pub(super) gpu: c_uint,
		memory: c_uint,
	}

	#[repr(C)]
	#[derive(Default)]
	pub(super) struct Memory {
		pub(super) total: u64,
		pub(super) free: u64,
		pub(super) used: u64,
	}

	// nvmlMemory_v2_t. Its used leaves out what the driver keeps back,
	// as nvidia-smi does (2026102915125135).
	#[repr(C)]
	#[derive(Default)]
	pub(super) struct MemoryV2 {
		pub(super) version: c_uint,
		pub(super) total: u64,
		pub(super) reserved: u64,
		pub(super) free: u64,
		pub(super) used: u64,
	}

	// NVML_STRUCT_VERSION(Memory, 2)
	pub(super) const MEMORY_V2: c_uint = size_of::<MemoryV2>() as c_uint | 2 << 24;

	pub(super) fn used_of(m: &MemoryV2) -> Option<(u64, u64)> {
		(m.total > 0).then_some((m.used, m.total))
	}

	/// The library's calls used here. It's loaded once a process, and never
	/// unloaded.
	#[derive(Debug)]
	pub(super) struct Lib {
		pub(super) init: unsafe extern "C" fn() -> c_int,
		pub(super) shutdown: unsafe extern "C" fn() -> c_int,
		pub(super) by_index: unsafe extern "C" fn(c_uint, *mut Device) -> c_int,
		pub(super) utilization: unsafe extern "C" fn(Device, *mut Utilization) -> c_int,
		pub(super) memory: unsafe extern "C" fn(Device, *mut Memory) -> c_int,
		pub(super) memory_v2: Option<unsafe extern "C" fn(Device, *mut MemoryV2) -> c_int>,
	}

	#[cfg(test)]
	thread_local! {
		pub(super) static STAND_IN: std::cell::Cell<Option<&'static Lib>> =
			const { std::cell::Cell::new(None) };
	}

	fn lib() -> Option<&'static Lib> {
		static LIB: OnceLock<Option<Lib>> = OnceLock::new();
		#[cfg(test)]
		if let Some(lib) = STAND_IN.get() {
			return Some(lib);
		}
		LIB.get_or_init(load).as_ref()
	}

	/// NVML, opened on the first device. NVML counts its opens, and the last
	/// one dropped shuts it down, so a GPU graph that's hidden doesn't keep a
	/// laptop's GPU awake (2026103115232174).
	#[derive(Debug)]
	pub(super) struct Nvml {
		lib: &'static Lib,
		device: Device,
	}

	impl Nvml {
		/// Whether the library loads. That touches no GPU.
		pub(super) fn loads() -> bool {
			lib().is_some()
		}

		pub(super) fn open() -> Option<Nvml> {
			let lib = lib()?;
			// SAFETY: the calls are as nvml.h has them, and the device
			// pointer is one for by_index to fill.
			unsafe {
				if (lib.init)() != 0 {
					return None;
				}
				// shut down by its drop from here on
				let mut n = Nvml {
					lib,
					device: std::ptr::null_mut(),
				};
				let r = (lib.by_index)(0, &raw mut n.device);
				(r == 0 && !n.device.is_null()).then_some(n)
			}
		}

		/// A share of the last sample period, 0 to 1.
		pub(super) fn busy(&self) -> Option<f32> {
			let mut u = Utilization::default();
			// SAFETY: the device came from NVML, and u is the struct it fills.
			let r = unsafe { (self.lib.utilization)(self.device, &raw mut u) };
			(r == 0).then(|| (u.gpu as f32 / 100.0).clamp(0.0, 1.0))
		}

		#[cfg(test)]
		pub(super) fn has_v2(&self) -> bool {
			self.lib.memory_v2.is_some()
		}

		/// Used and total, in bytes. Drivers older than 510 have no v2.
		pub(super) fn memory(&self) -> Option<(u64, u64)> {
			if let Some(v2) = self.lib.memory_v2 {
				let mut m = MemoryV2 {
					version: MEMORY_V2,
					..MemoryV2::default()
				};
				// SAFETY: as in busy, with the version NVML checks first.
				if unsafe { v2(self.device, &raw mut m) } == 0 {
					return used_of(&m);
				}
			}
			let mut m = Memory::default();
			// SAFETY: as in busy.
			let r = unsafe { (self.lib.memory)(self.device, &raw mut m) };
			(r == 0 && m.total > 0).then_some((m.used, m.total))
		}
	}

	impl Drop for Nvml {
		fn drop(&mut self) {
			// SAFETY: one shutdown for the init in open.
			unsafe { (self.lib.shutdown)() };
		}
	}

	fn sym(lib: *mut c_void, name: &std::ffi::CStr) -> Option<*mut c_void> {
		// SAFETY: lib is a live dlopen handle and name ends in a nul.
		let p = unsafe { libc::dlsym(lib, name.as_ptr()) };
		(!p.is_null()).then_some(p)
	}

	fn load() -> Option<Lib> {
		// SAFETY: a plain library open. It stays open for the process.
		let lib = unsafe { libc::dlopen(c"libnvidia-ml.so.1".as_ptr(), libc::RTLD_NOW) };
		if lib.is_null() {
			return None;
		}
		let init = sym(lib, c"nvmlInit_v2")?;
		let shutdown = sym(lib, c"nvmlShutdown")?;
		let by_index = sym(lib, c"nvmlDeviceGetHandleByIndex_v2")?;
		let utilization = sym(lib, c"nvmlDeviceGetUtilizationRates")?;
		let memory = sym(lib, c"nvmlDeviceGetMemoryInfo")?;
		let memory_v2 = sym(lib, c"nvmlDeviceGetMemoryInfo_v2");
		// SAFETY: each symbol has this signature in nvml.h.
		unsafe {
			Some(Lib {
				init: std::mem::transmute::<*mut c_void, unsafe extern "C" fn() -> c_int>(init),
				shutdown: std::mem::transmute::<*mut c_void, unsafe extern "C" fn() -> c_int>(
					shutdown,
				),
				by_index: std::mem::transmute::<
					*mut c_void,
					unsafe extern "C" fn(c_uint, *mut Device) -> c_int,
				>(by_index),
				utilization: std::mem::transmute::<
					*mut c_void,
					unsafe extern "C" fn(Device, *mut Utilization) -> c_int,
				>(utilization),
				memory: std::mem::transmute::<
					*mut c_void,
					unsafe extern "C" fn(Device, *mut Memory) -> c_int,
				>(memory),
				memory_v2: memory_v2.map(|p| {
					std::mem::transmute::<
						*mut c_void,
						unsafe extern "C" fn(Device, *mut MemoryV2) -> c_int,
					>(p)
				}),
			})
		}
	}
}

// Miri can't load a library
#[cfg(miri)]
mod nvml {
	#[derive(Debug)]
	pub(super) enum Nvml {}

	impl Nvml {
		pub(super) fn loads() -> bool {
			false
		}

		pub(super) fn open() -> Option<Nvml> {
			None
		}

		pub(super) fn busy(&self) -> Option<f32> {
			match *self {}
		}

		pub(super) fn memory(&self) -> Option<(u64, u64)> {
			match *self {}
		}

		#[cfg(test)]
		pub(super) fn has_v2(&self) -> bool {
			match *self {}
		}
	}
}

use nvml::Nvml;
use std::cell::Cell;
use std::time::{Duration, Instant};

const DRM: &str = "/sys/class/drm";
const PCI: &str = "/sys/bus/pci/devices";

// How long a card that may sleep reads idle before NVML lets go of it, and
// how long it then stays shut past the card's own autosuspend delay. A set
// time, not a count of reads, so a fast sample doesn't rest it in a lull.
const IDLE_FOR: Duration = Duration::from_secs(5);
const REST_PAST_DELAY: Duration = Duration::from_secs(5);

thread_local! {
	// shared, so the GPU and GPU memory readers rest together and neither
	// keeps the card up for the other
	static REST_UNTIL: Cell<Option<Instant>> = const { Cell::new(None) };
}

#[cfg(test)]
thread_local! {
	static PCI_AT: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
	static DRM_AT: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
	static IDLE_FOR_AT: Cell<Option<Duration>> = const { Cell::new(None) };
}

fn idle_for() -> Duration {
	#[cfg(test)]
	if let Some(at) = IDLE_FOR_AT.get() {
		return at;
	}
	IDLE_FOR
}

fn pci() -> String {
	#[cfg(test)]
	if let Some(at) = PCI_AT.with_borrow(Clone::clone) {
		return at;
	}
	PCI.to_owned()
}

fn drm() -> String {
	#[cfg(test)]
	if let Some(at) = DRM_AT.with_borrow(Clone::clone) {
		return at;
	}
	DRM.to_owned()
}

/// Whether the first NVIDIA card is asleep, where the kernel may put it to
/// sleep. The GPU graphs then read 0 without asking NVML.
pub fn nvidia_asleep() -> bool {
	let mut buf = String::new();
	Power::find(&pci()).is_some_and(|mut p| p.may_sleep(&mut buf) && p.asleep(&mut buf))
}

/// Where the GPU's numbers come from.
#[derive(Debug)]
enum Card {
	Nvidia(Nvidia),
	Amd(Amd),
	Intel(Intel),
}

/// The NVIDIA card's runtime power files. Reading them doesn't wake the
/// card, as an NVML call does (2026110118380381).
#[derive(Debug)]
struct Power {
	control: Kept,
	status: Kept,
	rest: Duration,
}

impl Power {
	/// The first function bound to the nvidia driver that's a display
	/// card, in bus order, as NVML numbers them.
	fn find(pci: &str) -> Option<Power> {
		let mut names: Vec<String> = std::fs::read_dir(pci)
			.ok()?
			.filter_map(|e| e.ok()?.file_name().into_string().ok())
			.collect();
		names.sort();
		names
			.into_iter()
			.map(|n| format!("{pci}/{n}"))
			.find(|dir| is_nvidia_display(dir))
			.map(|dir| Power::new(&dir))
	}

	fn new(dir: &str) -> Power {
		// unreadable where the kernel doesn't autosuspend it
		let delay = std::fs::read_to_string(format!("{dir}/power/autosuspend_delay_ms"))
			.ok()
			.and_then(|s| s.trim().parse::<u64>().ok())
			.unwrap_or(0);
		Power {
			control: Kept::new(format!("{dir}/power/control")),
			status: Kept::new(format!("{dir}/power/runtime_status")),
			rest: Duration::from_millis(delay) + REST_PAST_DELAY,
		}
	}

	fn may_sleep(&mut self, buf: &mut String) -> bool {
		self.control.read(buf).is_some_and(|s| s.trim() == "auto")
	}

	fn asleep(&mut self, buf: &mut String) -> bool {
		self.status
			.read(buf)
			.is_some_and(|s| s.trim() == "suspended")
	}
}

fn is_nvidia_display(dir: &str) -> bool {
	let text = |f: &str| std::fs::read_to_string(format!("{dir}/{f}")).unwrap_or_default();
	text("vendor").trim() == "0x10de"
		&& text("class").starts_with("0x03")
		&& std::fs::read_link(format!("{dir}/driver"))
			.is_ok_and(|l| l.file_name().is_some_and(|n| n == "nvidia"))
}

/// NVML on the first NVIDIA card. Where the kernel may put the card to
/// sleep, NVML stays shut while it's asleep, and lets go of it once it's
/// been idle a while, since every NVML call keeps it awake.
#[derive(Debug)]
struct Nvidia {
	nvml: Option<Nvml>,
	power: Option<Power>,
	sleepy: bool,
	idle_since: Option<Instant>,
	used: u64,
	total: u64,
}

impl Nvidia {
	fn find(pci: &str) -> Option<Nvidia> {
		let mut n = Nvidia {
			nvml: None,
			power: Power::find(pci),
			sleepy: false,
			idle_since: None,
			used: 0,
			total: 0,
		};
		let mut buf = String::new();
		let asleep = n
			.power
			.as_mut()
			.is_some_and(|p| p.may_sleep(&mut buf) && p.asleep(&mut buf));
		if asleep {
			return Nvml::loads().then_some(n);
		}
		n.nvml = Some(Nvml::open()?);
		Some(n)
	}

	fn awake(&mut self, buf: &mut String) -> Awake<'_> {
		self.sleepy = self.power.as_mut().is_some_and(|p| p.may_sleep(buf));
		if self.sleepy && REST_UNTIL.get().is_some_and(|t| Instant::now() < t) {
			self.nvml = None;
			// once it sleeps the rest has done its job, and whatever wakes it
			// next is work to read, not something to wait out (2026110709074412)
			if self.power.as_mut().is_some_and(|p| p.asleep(buf)) {
				REST_UNTIL.set(None);
			}
			return Awake::Resting;
		}
		if self.nvml.is_none() {
			if self.sleepy && self.power.as_mut().is_some_and(|p| p.asleep(buf)) {
				return Awake::Resting;
			}
			self.nvml = Nvml::open();
			self.idle_since = None;
		}
		self.nvml.as_ref().map_or(Awake::Failed, Awake::Open)
	}

	fn busy(&mut self, buf: &mut String) -> Option<f32> {
		let n = match self.awake(buf) {
			Awake::Open(n) => n,
			// it went quiet before it rested, or it sleeps
			Awake::Resting => return Some(0.0),
			Awake::Failed => return None,
		};
		let busy = n.busy();
		self.rest_if_idle(busy);
		busy
	}

	fn memory(&mut self, buf: &mut String) -> Option<(u64, u64)> {
		let n = match self.awake(buf) {
			Awake::Open(n) => n,
			Awake::Resting => {
				// a card asleep holds nothing
				if self.power.as_mut().is_some_and(|p| p.asleep(buf)) {
					self.used = 0;
				}
				return Some((self.used, self.total));
			}
			Awake::Failed => return None,
		};
		let (used, total) = n.memory()?;
		let busy = n.busy();
		(self.used, self.total) = (used, total);
		self.rest_if_idle(busy);
		Some((used, total))
	}

	fn rest_if_idle(&mut self, busy: Option<f32>) {
		if !self.sleepy {
			return;
		}
		// a card with no busy share to give counts as idle
		if busy.is_some_and(|b| b > 0.0) {
			self.idle_since = None;
			return;
		}
		let now = Instant::now();
		if now.duration_since(*self.idle_since.get_or_insert(now)) >= idle_for() {
			self.nvml = None;
			self.idle_since = None;
			let rest = self.power.as_ref().map_or(REST_PAST_DELAY, |p| p.rest);
			REST_UNTIL.set(Some(now + rest));
		}
	}
}

enum Awake<'a> {
	Open(&'a Nvml),
	Resting,
	Failed,
}

/// An amdgpu card's files, in its `device` folder.
#[derive(Debug)]
struct Amd {
	busy: Kept,
	vram_used: Kept,
	vram_total: Kept,
}

impl Amd {
	fn new(dir: &str) -> Self {
		Amd {
			busy: Kept::new(format!("{dir}/gpu_busy_percent")),
			vram_used: Kept::new(format!("{dir}/mem_info_vram_used")),
			vram_total: Kept::new(format!("{dir}/mem_info_vram_total")),
		}
	}
}

// Where an Intel card's time asleep is, in ms, the first one there. i915
// has the second wherever it has RC6, and puts each GT's own at the first
// since 5.18. xe has the third.
const INTEL_ASLEEP: [&str; 3] = [
	"gt/gt0/rc6_residency_ms",
	"power/rc6_residency_ms",
	"device/tile0/gt0/gtidle/idle_residency_ms",
];

/// An Intel card's time in RC6, the sleep its graphics core drops into as
/// soon as it has nothing to run. Its busy share needs perf counters, which
/// need root, so busy is the share of the time it was awake. That reads a
/// little over the engines' own busy time (2026110920595600).
#[derive(Debug)]
struct Intel {
	asleep_ms: Kept,
	was: Option<(u64, Instant)>,
}

impl Intel {
	fn new(file: String) -> Self {
		Intel {
			asleep_ms: Kept::new(file),
			was: None,
		}
	}

	/// The first call only primes it.
	fn busy_at(&mut self, buf: &mut String, now: Instant) -> Option<f32> {
		let asleep: u64 = self.asleep_ms.read(buf)?.trim().parse().ok()?;
		let (was_asleep, then) = self.was.replace((asleep, now))?;
		// gone down, as after the driver is loaded again, so this read primes
		let slept = asleep.checked_sub(was_asleep)?;
		let awake_for = now.duration_since(then).as_secs_f64() * 1000.0;
		(awake_for > 0.0).then(|| (1.0 - slept as f64 / awake_for).clamp(0.0, 1.0) as f32)
	}
}

impl Card {
	fn find() -> Option<Card> {
		let drm = drm();
		Nvidia::find(&pci())
			.map(Card::Nvidia)
			.or_else(|| amd_in(&drm).map(|dir| Card::Amd(Amd::new(&dir))))
			.or_else(|| intel_in(&drm).map(|file| Card::Intel(Intel::new(file))))
	}

	/// Whether a source that only primes on its first read has done so.
	fn primed(&self) -> bool {
		matches!(self, Card::Intel(Intel { was: Some(_), .. }))
	}

	fn busy(&mut self, buf: &mut String) -> Option<f32> {
		match self {
			Card::Nvidia(n) => n.busy(buf),
			Card::Amd(amd) => {
				// f32 takes "NaN", which clamp passes through
				let percent: f32 = amd
					.busy
					.read(buf)?
					.trim()
					.parse()
					.ok()
					.filter(|p: &f32| !p.is_nan())?;
				Some((percent / 100.0).clamp(0.0, 1.0))
			}
			Card::Intel(intel) => intel.busy_at(buf, Instant::now()),
		}
	}

	fn memory(&mut self, buf: &mut String) -> Option<(u64, u64)> {
		match self {
			Card::Nvidia(n) => n.memory(buf),
			Card::Amd(amd) => {
				let mut num =
					|file: &mut Kept| -> Option<u64> { file.read(buf)?.trim().parse().ok() };
				let used = num(&mut amd.vram_used)?;
				let total = num(&mut amd.vram_total)?;
				(total > 0).then_some((used, total))
			}
			// an integrated card's memory is the system's, and nothing short
			// of root says how much of it the GPU holds
			Card::Intel(_) => None,
		}
	}
}

/// The first amdgpu card under `drm`, by its `device` folder. Connectors
/// such as `card0-DP-1` aren't cards.
fn amd_in(drm: &str) -> Option<String> {
	cards_in(drm)
		.into_iter()
		.map(|card| format!("{card}/device"))
		.find(|dir| std::path::Path::new(&format!("{dir}/gpu_busy_percent")).exists())
}

/// The first Intel card under `drm` with RC6 on, by the file that counts
/// its time asleep. With RC6 off that count stays put, and the card would
/// read as always busy.
fn intel_in(drm: &str) -> Option<String> {
	cards_in(drm).into_iter().find_map(|card| {
		let rc6 = std::fs::read_to_string(format!("{card}/power/rc6_enable"));
		if rc6.is_ok_and(|s| s.trim() == "0") {
			return None;
		}
		INTEL_ASLEEP
			.iter()
			.map(|file| format!("{card}/{file}"))
			.find(|file| std::path::Path::new(file).exists())
	})
}

/// The `card*` folders under `drm`, in number order. Connectors such as
/// `card0-DP-1` aren't cards.
fn cards_in(drm: &str) -> Vec<String> {
	let Ok(dir) = std::fs::read_dir(drm) else {
		return Vec::new();
	};
	let mut cards: Vec<String> = dir
		.filter_map(|e| e.ok()?.file_name().into_string().ok())
		.filter(|n| {
			n.strip_prefix("card")
				.is_some_and(|k| !k.is_empty() && k.bytes().all(|b| b.is_ascii_digit()))
		})
		.collect();
	cards.sort_by_key(|n| n[4..].parse::<u32>().unwrap_or(u32::MAX));
	cards.into_iter().map(|n| format!("{drm}/{n}")).collect()
}

/// How busy the GPU is, 0 to 1.
#[derive(Debug)]
pub struct GpuReader {
	card: Card,
	buf: String,
}

impl Collector<1> for GpuReader {
	const SERIES: [&'static str; 1] = ["busy"];

	fn probe() -> Option<Self> {
		let mut r = GpuReader {
			card: Card::find()?,
			buf: String::new(),
		};
		let busy = r.read();
		(busy.is_some() || r.card.primed()).then_some(r)
	}

	fn read(&mut self) -> Option<[f32; 1]> {
		self.card.busy(&mut self.buf).map(|x| [x])
	}
}

/// The share of the GPU's memory in use, 0 to 1.
#[derive(Debug)]
pub struct VramReader {
	card: Card,
	buf: String,
	total: u64,
}

impl VramReader {
	/// Bytes, as of the last read.
	pub fn total(&self) -> u64 {
		self.total
	}
}

impl Collector<1> for VramReader {
	const SERIES: [&'static str; 1] = ["used"];

	fn probe() -> Option<Self> {
		let mut r = VramReader {
			card: Card::find()?,
			buf: String::new(),
			total: 0,
		};
		r.read().map(|_| r)
	}

	fn read(&mut self) -> Option<[f32; 1]> {
		let (used, total) = self.card.memory(&mut self.buf)?;
		self.total = total;
		// no total yet from a card asleep since start
		if total == 0 {
			return Some([0.0]);
		}
		Some([(used as f64 / total as f64).clamp(0.0, 1.0) as f32])
	}
}

#[cfg(test)]
pub(crate) fn amd_readers(dir: &str) -> (GpuReader, VramReader) {
	(
		GpuReader {
			card: Card::Amd(Amd::new(dir)),
			buf: String::new(),
		},
		VramReader {
			card: Card::Amd(Amd::new(dir)),
			buf: String::new(),
			total: 0,
		},
	)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::collect::Tree;

	// Test ID: dekm9
	#[test]
	fn amdgpu_first_card() {
		let t = Tree::new(
			"gpu_amd",
			&[
				("card0-DP-1/device/gpu_busy_percent", "1\n"),
				("card10/device/gpu_busy_percent", "2\n"),
				("card1/device/", ""),
				("card2/device/gpu_busy_percent", "37\n"),
				("card2/device/mem_info_vram_used", "2147483648\n"),
				("card2/device/mem_info_vram_total", "8589934592\n"),
				("renderD128/", ""),
			],
		);
		let dir = amd_in(&t.at("")).unwrap();
		assert!(dir.ends_with("/card2/device"), "{dir}");
		let (mut gpu, mut vram) = amd_readers(&dir);
		assert_eq!(gpu.read(), Some([0.37]));
		assert_eq!(vram.read(), Some([0.25]));
		assert_eq!(vram.total(), 8_589_934_592);
		let (mut gone, mut none) = amd_readers(&t.at("card1/device"));
		assert_eq!((gone.read(), none.read()), (None, None));
		assert_eq!(amd_in(&t.at("card1")), None);
		let only = Tree::new(
			"gpu_amd_connector",
			&[("card0-DP-1/device/gpu_busy_percent", "1\n")],
		);
		assert_eq!(amd_in(&only.at("")), None);
	}

	// Test ID: df3kw
	#[test]
	fn intel_busy_from_rc6() {
		let t = Tree::new("gpu_intel_rc6", &[("rc6_residency_ms", "1000\n")]);
		let set = |ms: &str| std::fs::write(t.at("rc6_residency_ms"), ms).unwrap();
		let mut intel = Intel::new(t.at("rc6_residency_ms"));
		let mut buf = String::new();
		let t0 = Instant::now();
		let at = |ms: u64| t0 + Duration::from_millis(ms);
		assert_eq!(
			intel.busy_at(&mut buf, at(0)),
			None,
			"the first read primes"
		);
		set("1750\n");
		assert_eq!(intel.busy_at(&mut buf, at(1000)), Some(0.25));
		set("1750\n");
		assert_eq!(intel.busy_at(&mut buf, at(1500)), Some(1.0));
		// asleep a little longer than the clock says, from when each was read
		set("2260\n");
		assert_eq!(intel.busy_at(&mut buf, at(2000)), Some(0.0));
		// the driver loaded again, so it primes again
		set("10\n");
		assert_eq!(intel.busy_at(&mut buf, at(3000)), None);
		set("510\n");
		assert_eq!(intel.busy_at(&mut buf, at(4000)), Some(0.5));
		assert_eq!(intel.busy_at(&mut buf, at(4000)), None, "no time between");
		set("junk\n");
		assert_eq!(intel.busy_at(&mut buf, at(5000)), None);
	}

	// Test ID: df3kx
	#[test]
	#[cfg(not(miri))]
	fn intel_gpu_graph_only() {
		let t = Tree::new(
			"gpu_intel",
			&[
				("card0-eDP-1/power/rc6_residency_ms", "5\n"),
				("card0/power/rc6_enable", "0\n"),
				("card0/power/rc6_residency_ms", "0\n"),
				("card1/device/", ""),
				("card2/power/rc6_enable", "1\n"),
				("card2/power/rc6_residency_ms", "100\n"),
				("card2/gt/gt0/rc6_residency_ms", "200\n"),
				("card3/device/tile0/gt0/gtidle/idle_residency_ms", "300\n"),
				("renderD128/", ""),
			],
		);
		let found = intel_in(&t.at("")).unwrap();
		assert!(found.ends_with("/card2/gt/gt0/rc6_residency_ms"), "{found}");
		let legacy = Tree::new(
			"gpu_intel_legacy",
			&[("card0/power/rc6_residency_ms", "1\n")],
		);
		let found = intel_in(&legacy.at("")).unwrap();
		assert!(found.ends_with("/card0/power/rc6_residency_ms"), "{found}");
		let xe = Tree::new(
			"gpu_intel_xe",
			&[("card0/device/tile0/gt0/gtidle/idle_residency_ms", "3\n")],
		);
		let found = intel_in(&xe.at("")).unwrap();
		assert!(found.ends_with("/gtidle/idle_residency_ms"), "{found}");
		let off = Tree::new(
			"gpu_intel_off",
			&[
				("card0/power/rc6_enable", "0\n"),
				("card0/power/rc6_residency_ms", "0\n"),
			],
		);
		assert_eq!(intel_in(&off.at("")), None);

		// no NVIDIA card and no amdgpu one, as on a box with only Intel's
		nvml::STAND_IN.set(Some(&stand_in::LIB));
		stand_in::NO_DEVICE.set(true);
		PCI_AT.set(Some("/nonexistent".into()));
		DRM_AT.set(Some(t.at("")));
		let gpu = GpuReader::probe();
		let vram = VramReader::probe();
		stand_in::NO_DEVICE.set(false);
		nvml::STAND_IN.set(None);
		PCI_AT.set(None);
		DRM_AT.set(None);
		let mut gpu = gpu.expect("the GPU graph, from the first read priming it");
		assert!(vram.is_none(), "no GPU memory graph");
		std::fs::write(t.at("card2/gt/gt0/rc6_residency_ms"), "200\n").unwrap();
		std::thread::sleep(Duration::from_millis(20));
		assert_eq!(gpu.read(), Some([1.0]), "awake the whole time");
	}

	// Test ID: dekma
	#[test]
	#[cfg_attr(miri, ignore = "Miri can't load a library")]
	fn live_gpu_share_where_present() {
		let (Some(mut gpu), Some(mut vram)) = (GpuReader::probe(), VramReader::probe()) else {
			return;
		};
		let [busy] = gpu.read().unwrap();
		let [used] = vram.read().unwrap();
		assert!((0.0..=1.0).contains(&busy), "{busy}");
		if vram.total() == 0 {
			// asleep since start, so NVML wasn't asked
			return;
		}
		assert!(used > 0.0 && used <= 1.0 && vram.total() > 0, "{used}");
	}

	// Test ID: dektc
	#[test]
	#[cfg(not(miri))]
	fn gpu_memory_excludes_reserved() {
		const MIB: u64 = 1 << 20;
		let m = nvml::MemoryV2 {
			version: nvml::MEMORY_V2,
			total: 8192 * MIB,
			reserved: 351 * MIB,
			free: 4962 * MIB,
			used: 2879 * MIB,
		};
		assert_eq!(nvml::used_of(&m), Some((2879 * MIB, 8192 * MIB)));
		assert_eq!(nvml::MEMORY_V2, 0x0200_0028);
	}

	// counts what's open, on this thread only, as each test has its own
	#[cfg(not(miri))]
	mod stand_in {
		use super::nvml::{Device, Lib, Memory, Utilization};
		use std::cell::Cell;
		use std::ffi::{c_int, c_uint};

		thread_local! {
			pub(super) static OPEN: Cell<i32> = const { Cell::new(0) };
			pub(super) static INITS: Cell<u32> = const { Cell::new(0) };
			pub(super) static NO_DEVICE: Cell<bool> = const { Cell::new(false) };
			pub(super) static BUSY: Cell<c_uint> = const { Cell::new(40) };
		}

		extern "C" fn init() -> c_int {
			OPEN.set(OPEN.get() + 1);
			INITS.set(INITS.get() + 1);
			0
		}

		extern "C" fn shutdown() -> c_int {
			OPEN.set(OPEN.get() - 1);
			0
		}

		extern "C" fn by_index(_: c_uint, device: *mut Device) -> c_int {
			if NO_DEVICE.get() {
				return 2;
			}
			// SAFETY: Nvml::open passes its own field.
			unsafe { *device = std::ptr::NonNull::<u8>::dangling().as_ptr().cast() };
			0
		}

		extern "C" fn utilization(_: Device, u: *mut Utilization) -> c_int {
			// SAFETY: as above.
			unsafe { (*u).gpu = BUSY.get() };
			0
		}

		extern "C" fn memory(_: Device, m: *mut Memory) -> c_int {
			// SAFETY: as above.
			unsafe {
				(*m).total = 8 << 30;
				(*m).used = 2 << 30;
			}
			0
		}

		pub(super) static LIB: Lib = Lib {
			init,
			shutdown,
			by_index,
			utilization,
			memory,
			memory_v2: None,
		};
	}

	// Test ID: der6j
	#[test]
	#[cfg(not(miri))]
	fn nvml_open_while_graph_shown() {
		use crate::config::{Look, Which};
		use crate::graphs::Graphs;
		use stand_in::{INITS, NO_DEVICE, OPEN};
		nvml::STAND_IN.set(Some(&stand_in::LIB));
		PCI_AT.set(Some("/nonexistent".into()));
		let shown = |gpu: bool, vram: bool| {
			Look::default()
				.with(Which::Gpu, |e| e.show = gpu)
				.with(Which::Vram, |e| e.show = vram)
		};
		let mut g = Graphs::new(shown(false, false));
		assert_eq!((OPEN.get(), INITS.get()), (0, 0));
		g.set_look(shown(true, true), None).unwrap();
		assert!(g.has(Which::Gpu) && g.has(Which::Vram));
		assert_eq!(OPEN.get(), 2);
		for _ in 0..3 {
			g.sample();
		}
		assert_eq!((OPEN.get(), INITS.get()), (2, 2), "opened once, not a read");
		g.set_look(shown(false, true), None).unwrap();
		assert_eq!(OPEN.get(), 1);
		g.set_look(shown(false, false), None).unwrap();
		assert_eq!(OPEN.get(), 0);
		g.set_look(shown(true, false), None).unwrap();
		assert_eq!(OPEN.get(), 1);
		drop(g);
		assert_eq!(OPEN.get(), 0);
		// a driver with no device is let go at once
		NO_DEVICE.set(true);
		assert!(Nvml::open().is_none());
		assert_eq!(OPEN.get(), 0);
		nvml::STAND_IN.set(None);
		PCI_AT.set(None);
	}

	// Test ID: desfa
	#[test]
	#[cfg(not(miri))]
	fn sleeping_card_left_alone() {
		use stand_in::{BUSY, INITS, OPEN};
		let t = Tree::new(
			"gpu_power",
			&[
				("0000:00:02.0/vendor", "0x8086\n"),
				("0000:00:02.0/class", "0x030000\n"),
				("0000:00:10.0/vendor", "0x10de\n"),
				("0000:00:10.0/class", "0x030000\n"),
				("0000:01:00.0/vendor", "0x10de\n"),
				("0000:01:00.0/class", "0x030200\n"),
				("0000:01:00.0/power/control", "auto\n"),
				("0000:01:00.0/power/runtime_status", "suspended\n"),
				("0000:01:00.1/vendor", "0x10de\n"),
				("0000:01:00.1/class", "0x040300\n"),
			],
		);
		let links = [
			("0000:00:10.0", "vfio-pci"),
			("0000:01:00.0", "nvidia"),
			("0000:01:00.1", "nvidia"),
		];
		for (dev, driver) in links {
			std::os::unix::fs::symlink(
				format!("../../../bus/pci/drivers/{driver}"),
				t.at(&format!("{dev}/driver")),
			)
			.unwrap();
		}
		let set = |file: &str, text: &str| {
			std::fs::write(t.at(&format!("0000:01:00.0/power/{file}")), text).unwrap();
		};
		nvml::STAND_IN.set(Some(&stand_in::LIB));
		PCI_AT.set(Some(t.at("")));
		let is = |dev: &str| is_nvidia_display(&t.at(dev));
		assert!(is("0000:01:00.0") && !is("0000:00:10.0") && !is("0000:01:00.1"));

		let (mut gpu, mut vram) = (GpuReader::probe().unwrap(), VramReader::probe().unwrap());
		assert_eq!((gpu.read(), vram.read()), (Some([0.0]), Some([0.0])));
		assert_eq!(
			(OPEN.get(), INITS.get(), vram.total()),
			(0, 0, 0),
			"asleep at start"
		);
		assert!(nvidia_asleep());

		set("runtime_status", "active\n");
		assert!(!nvidia_asleep());
		assert_eq!((gpu.read(), vram.read()), (Some([0.4]), Some([0.25])));
		assert_eq!(OPEN.get(), 2);

		// idle reads alone don't rest it, however many come in a lull
		BUSY.set(0);
		IDLE_FOR_AT.set(Some(Duration::from_secs(3600)));
		for _ in 0..20 {
			assert_eq!((gpu.read(), vram.read()), (Some([0.0]), Some([0.25])));
		}
		assert_eq!((OPEN.get(), INITS.get()), (2, 2));

		// idle long enough, so both let go together, and stay shut while it's
		// still up
		IDLE_FOR_AT.set(Some(Duration::ZERO));
		assert_eq!((gpu.read(), vram.read()), (Some([0.0]), Some([0.25])));
		assert_eq!(OPEN.get(), 0);
		assert_eq!((gpu.read(), vram.read()), (Some([0.0]), Some([0.25])));
		assert_eq!((OPEN.get(), INITS.get()), (0, 2));

		// asleep, so the rest is over, and work that wakes it reads at once
		set("runtime_status", "suspended\n");
		assert!(REST_UNTIL.get().is_some());
		for _ in 0..3 {
			assert_eq!((gpu.read(), vram.read()), (Some([0.0]), Some([0.0])));
		}
		assert_eq!(REST_UNTIL.get(), None);
		assert_eq!((OPEN.get(), INITS.get()), (0, 2), "nothing woke it");

		// woken by something else
		set("runtime_status", "active\n");
		BUSY.set(40);
		assert_eq!((gpu.read(), vram.read()), (Some([0.4]), Some([0.25])));
		assert_eq!(OPEN.get(), 2);

		// runtime power management off, so idle reads keep it open
		set("control", "on\n");
		BUSY.set(0);
		for _ in 0..10 {
			gpu.read();
			vram.read();
		}
		assert_eq!(OPEN.get(), 2);
		drop((gpu, vram));
		assert_eq!(OPEN.get(), 0);

		BUSY.set(40);
		REST_UNTIL.set(None);
		IDLE_FOR_AT.set(None);
		nvml::STAND_IN.set(None);
		PCI_AT.set(None);
		for (dev, _) in links {
			std::fs::remove_file(t.at(&format!("{dev}/driver"))).unwrap();
		}
	}

	// Test ID: dektd
	#[test]
	#[cfg_attr(miri, ignore = "Miri can't load a library")]
	fn nvidia_memory_via_v2() {
		let Some(n) = Nvml::open() else {
			return;
		};
		assert!(
			n.has_v2(),
			"a driver from 510 on has nvmlDeviceGetMemoryInfo_v2"
		);
	}

	// Test ID: df4gx
	#[test]
	fn fuzz_gpu_sysfs_shares() {
		use crate::fuzz::count;
		// seed 665 found it, past the fixed seeds
		let nan = Tree::new("gpu_nan", &[("card/gpu_busy_percent", "NaN\n")]);
		assert_eq!(amd_readers(&nan.at("card")).0.read(), None);
		drop(nan);
		let tally = crate::fuzz::run("gpu sysfs", |dice| {
			let mut f = crate::fuzz::Files::new(dice);
			let busy = dice.below(101);
			f.put(dice, "card/gpu_busy_percent", format!("{busy}\n"));
			let (used, total) = (count(dice), count(dice));
			f.put(dice, "card/mem_info_vram_used", format!("{used}\n"));
			f.put(dice, "card/mem_info_vram_total", format!("{total}\n"));
			let asleep = count(dice);
			f.put(dice, "rc6", format!("{asleep}\n"));
			let delay = count(dice);
			f.put(dice, "dev/power/autosuspend_delay_ms", format!("{delay}\n"));
			let t = f.tree("gpu_fuzz");
			let in_range = |v: Option<[f32; 1]>| v.is_none_or(|[x]| (0.0..=1.0).contains(&x));
			let (mut gpu, mut vram) = amd_readers(&t.at("card"));
			let got = gpu.read();
			if f.whole {
				assert_eq!(got, Some([busy as f32 / 100.0]));
			}
			assert!(in_range(got), "{got:?}");
			let got_vram = vram.read();
			assert!(in_range(got_vram), "{got_vram:?}");
			let mut intel = Intel::new(t.at("rc6"));
			let mut buf = String::new();
			let t0 = Instant::now();
			intel.busy_at(&mut buf, t0);
			let later = asleep.wrapping_add(dice.below(2000) as u64).to_string();
			std::fs::write(t.at("rc6"), crate::fuzz::maybe_hurt(dice, later).0).unwrap();
			let since = Duration::from_millis(dice.below(3000) as u64);
			let rc6 = intel.busy_at(&mut buf, t0 + since).map(|x| [x]);
			assert!(in_range(rc6), "{rc6:?}");
			let _ = Power::new(&t.at("dev"));
			got.is_some()
		});
		tally.assert_mixed();
	}
}
