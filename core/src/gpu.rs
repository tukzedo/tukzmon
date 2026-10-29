// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! The first GPU's busy time and memory, for the GPU and GPU memory graphs.
//! NVIDIA's through NVML, loaded at run time so a box without the driver
//! still starts, and AMD's from amdgpu's files in sysfs. Neither needs root.
//! See design.md, Data sources on Linux.

use crate::collect::{Collector, slurp};

#[cfg(not(miri))]
mod nvml {
	use std::ffi::{c_int, c_uint, c_void};
	use std::sync::OnceLock;

	type Device = *mut c_void;

	#[repr(C)]
	#[derive(Default)]
	struct Utilization {
		gpu: c_uint,
		memory: c_uint,
	}

	#[repr(C)]
	#[derive(Default)]
	struct Memory {
		total: u64,
		free: u64,
		used: u64,
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

	#[derive(Debug)]
	pub(super) struct Nvml {
		device: Device,
		utilization: unsafe extern "C" fn(Device, *mut Utilization) -> c_int,
		memory: unsafe extern "C" fn(Device, *mut Memory) -> c_int,
		memory_v2: Option<unsafe extern "C" fn(Device, *mut MemoryV2) -> c_int>,
	}

	// SAFETY: NVML's calls are thread safe, and the device handle is only
	// passed back to them.
	unsafe impl Send for Nvml {}
	// SAFETY: as above.
	unsafe impl Sync for Nvml {}

	impl Nvml {
		/// Loaded once a process, and never unloaded.
		pub(super) fn get() -> Option<&'static Nvml> {
			static NVML: OnceLock<Option<Nvml>> = OnceLock::new();
			NVML.get_or_init(load).as_ref()
		}

		/// A share of the last sample period, 0 to 1.
		pub(super) fn busy(&self) -> Option<f32> {
			let mut u = Utilization::default();
			// SAFETY: the device came from NVML, and u is the struct it fills.
			let r = unsafe { (self.utilization)(self.device, &raw mut u) };
			(r == 0).then(|| (u.gpu as f32 / 100.0).clamp(0.0, 1.0))
		}

		#[cfg(test)]
		pub(super) fn has_v2(&self) -> bool {
			self.memory_v2.is_some()
		}

		/// Used and total, in bytes. Drivers older than 510 have no v2.
		pub(super) fn memory(&self) -> Option<(u64, u64)> {
			if let Some(v2) = self.memory_v2 {
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
			let r = unsafe { (self.memory)(self.device, &raw mut m) };
			(r == 0 && m.total > 0).then_some((m.used, m.total))
		}
	}

	fn sym(lib: *mut c_void, name: &std::ffi::CStr) -> Option<*mut c_void> {
		// SAFETY: lib is a live dlopen handle and name ends in a nul.
		let p = unsafe { libc::dlsym(lib, name.as_ptr()) };
		(!p.is_null()).then_some(p)
	}

	fn load() -> Option<Nvml> {
		// SAFETY: a plain library open. It stays open for the process.
		let lib = unsafe { libc::dlopen(c"libnvidia-ml.so.1".as_ptr(), libc::RTLD_NOW) };
		if lib.is_null() {
			return None;
		}
		let init = sym(lib, c"nvmlInit_v2")?;
		let by_index = sym(lib, c"nvmlDeviceGetHandleByIndex_v2")?;
		let utilization = sym(lib, c"nvmlDeviceGetUtilizationRates")?;
		let memory = sym(lib, c"nvmlDeviceGetMemoryInfo")?;
		let memory_v2 = sym(lib, c"nvmlDeviceGetMemoryInfo_v2");
		// SAFETY: each symbol has this signature in nvml.h, and the calls
		// pass what it asks for.
		unsafe {
			let init = std::mem::transmute::<*mut c_void, unsafe extern "C" fn() -> c_int>(init);
			let by_index = std::mem::transmute::<
				*mut c_void,
				unsafe extern "C" fn(c_uint, *mut Device) -> c_int,
			>(by_index);
			if init() != 0 {
				return None;
			}
			let mut device: Device = std::ptr::null_mut();
			if by_index(0, &raw mut device) != 0 || device.is_null() {
				return None;
			}
			Some(Nvml {
				device,
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
		pub(super) fn get() -> Option<&'static Nvml> {
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

const DRM: &str = "/sys/class/drm";

/// Where the GPU's numbers come from.
#[derive(Debug)]
enum Card {
	Nvidia(&'static Nvml),
	/// An amdgpu card's `device` folder.
	Amd(String),
}

impl Card {
	fn find() -> Option<Card> {
		Nvml::get()
			.map(Card::Nvidia)
			.or_else(|| amd_in(DRM).map(Card::Amd))
	}

	fn busy(&self, buf: &mut String) -> Option<f32> {
		match self {
			Card::Nvidia(n) => n.busy(),
			Card::Amd(dir) => {
				let pc: f32 = slurp(&format!("{dir}/gpu_busy_percent"), buf)?
					.trim()
					.parse()
					.ok()?;
				Some((pc / 100.0).clamp(0.0, 1.0))
			}
		}
	}

	fn memory(&self, buf: &mut String) -> Option<(u64, u64)> {
		match self {
			Card::Nvidia(n) => n.memory(),
			Card::Amd(dir) => {
				let mut num = |name: &str| -> Option<u64> {
					slurp(&format!("{dir}/{name}"), buf)?.trim().parse().ok()
				};
				let used = num("mem_info_vram_used")?;
				let total = num("mem_info_vram_total")?;
				(total > 0).then_some((used, total))
			}
		}
	}
}

/// The first amdgpu card under `drm`, by its `device` folder. Connectors
/// such as `card0-DP-1` aren't cards.
fn amd_in(drm: &str) -> Option<String> {
	let mut cards: Vec<String> = std::fs::read_dir(drm)
		.ok()?
		.filter_map(|e| e.ok()?.file_name().into_string().ok())
		.filter(|n| {
			n.strip_prefix("card")
				.is_some_and(|k| !k.is_empty() && k.bytes().all(|b| b.is_ascii_digit()))
		})
		.collect();
	cards.sort_by_key(|n| n[4..].parse::<u32>().unwrap_or(u32::MAX));
	cards
		.into_iter()
		.map(|n| format!("{drm}/{n}/device"))
		.find(|dir| std::path::Path::new(&format!("{dir}/gpu_busy_percent")).exists())
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
		r.read().map(|_| r)
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
		Some([(used as f64 / total as f64).clamp(0.0, 1.0) as f32])
	}
}

#[cfg(test)]
pub(crate) fn amd_readers(dir: &str) -> (GpuReader, VramReader) {
	(
		GpuReader {
			card: Card::Amd(dir.into()),
			buf: String::new(),
		},
		VramReader {
			card: Card::Amd(dir.into()),
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
	fn amdgpu_reads_from_the_first_card_and_skips_connectors() {
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

	// Test ID: dekma
	#[test]
	#[cfg_attr(miri, ignore = "Miri can't load a library")]
	fn a_live_gpu_reads_as_a_share_where_there_is_one() {
		let (Some(mut gpu), Some(mut vram)) = (GpuReader::probe(), VramReader::probe()) else {
			return;
		};
		let [busy] = gpu.read().unwrap();
		let [used] = vram.read().unwrap();
		assert!((0.0..=1.0).contains(&busy), "{busy}");
		assert!(used > 0.0 && used <= 1.0 && vram.total() > 0, "{used}");
	}

	// Test ID: dektc
	#[test]
	#[cfg(not(miri))]
	fn gpu_memory_leaves_out_what_the_driver_keeps_back() {
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

	// Test ID: dektd
	#[test]
	#[cfg_attr(miri, ignore = "Miri can't load a library")]
	fn nvidia_memory_reads_through_v2_where_the_driver_has_it() {
		let Some(n) = Nvml::get() else {
			return;
		};
		assert!(
			n.has_v2(),
			"a driver from 510 on has nvmlDeviceGetMemoryInfo_v2"
		);
	}
}
