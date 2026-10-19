<!-- markdownlint-disable MD007 -- Unordered list indentation -->
<!-- markdownlint-disable MD010 -- No hard tabs -->
<!-- markdownlint-disable MD033 -- No inline html -->
<!-- markdownlint-disable MD055 -- Table pipe style [Expected: leading_and_trailing; Actual: leading_only; Missing trailing pipe] -->
<!-- markdownlint-disable MD041 -- First line in a file should be a top-level heading -->
<!-- TOC ignore:true -->
# Design

Draft. Nothing here is built yet. Goes with [backlog.md](backlog.md).

<!-- TOC ignore:true -->
## Table of contents

<!-- TOC -->

- [Summary](#summary)
- [Code layout](#code-layout)
	- [XFCE plugin](#xfce-plugin)
	- [Theme](#theme)
- [Panels and trays](#panels-and-trays)
- [Data sources on Linux](#data-sources-on-linux)
- [History](#history)
- [Time axis](#time-axis)
- [Drawing](#drawing)

<!-- /TOC -->

## Summary

tukzmon draws small system graphs in a desktop panel. Most of the work is the same on every panel: reading the numbers, keeping years of history in a small file, mapping time onto a log axis, and drawing. That part is one Rust library. Each panel gets a thin front end that hands the library a place to draw.

XFCE comes first. The other panels and platforms are in the backlog.

## Code layout

Settled on 2026-10-19: a Rust core, with C only where a panel's API needs it.

- `tukzmon-core` is a Rust library with no GUI toolkit in it.
	- Collectors for each platform, behind one trait.
	- The history store.
	- The time axis mapping.
	- Drawing into a plain RGBA pixel buffer with [tiny-skia](https://crates.io/crates/tiny-skia), which does antialiasing and has no system dependencies.

- Each front end is as small as it can be. It creates the panel widget, runs a timer, and copies the library's pixel buffer onto the screen. A pixel buffer is the one thing every host can show: a cairo image surface on GTK, a `QImage` on Qt, a DIB on Windows, and a `CGImage` on macOS.

- Text overlays, such as the zswap ratio, are drawn by the front end with the panel's own font, so they match the rest of the panel.

### XFCE plugin

- XFCE 4.20 is still GTK3, and its plugin API is C macros around `libxfce4panel-2.0`.

- The plugin is a Rust `cdylib`. A short C file registers the plugin and makes a `GtkDrawingArea`, and `build.rs` compiles it with the `cc` crate against `pkg-config` flags. So one `cargo build` gives the `.so`.
	- The gtk3-rs bindings were archived in 2024, so all GTK code is in that C file.
	- The panel runs external plugins in a separate wrapper process, so a crash takes down the plugin adn not the panel.

- A `.desktop` file tells the panel where the plugin is. It installs to `xfce4/panel/plugins` under the system data dir.

- If Rust can't be used for some reason, the fallback is the same split, with the C file growing into the whole plugin.

### Theme

- Graph colors start from the panel theme: the GTK style context on XFCE, the Qt palette on LXQt. That's how a tukzedo personality's theme reaches the graphs.

- Each color can be overridden in the plugin's settings. Settings are saved as shcl.

## Panels and trays

Can a status bar indicator be wide? For a tray icon, mostly no. For a panel plugin, yes.

| Host                        | Wide graph | How
| :---                        | :---       | :---
| XFCE panel                  | Yes        | Native plugin, any width.
| LXQt panel                  | Yes        | Native plugin in C++ and Qt.
| KDE Plasma                  | Yes        | A plasmoid in QML, with a C++ part that calls the library.
| GNOME Shell                 | Yes        | An extension in JavaScript. It can't load the library, so it reads from a small helper process.
| Cinnamon                    | Yes        | An applet in JavaScript, with the same helper as GNOME.
| MATE                        | Yes        | An applet in C.
| Budgie                      | Yes        | An applet in C or Vala.
| Wayfire panel               | Yes        | A widget in C++.
| Waybar                      | Yes        | A CFFI module in C.
| Tray icons on Linux and BSD | Mostly no  | StatusNotifierItem hosts put each icon in a square slot. Some show a short text label beside it, which fits a number but not a graph.
| Windows 11 tray             | No         | Small square icons only. Windows 11 dropped taskbar toolbars, so a wide graph has to be a small window placed over the taskbar.
| macOS menu bar              | Yes        | A status item can be any length.

On panels without a native plugin, tukzmon falls back to a tray icon with one small graph, and a click opens a full window.

## Data sources on Linux

| Graph       | Source
| :---        | :---
| CPU         | `/proc/stat`. The I/O wait, system, user, and nice columns.
| Memory      | `/proc/meminfo`. User is total minus free, buffers, and cache.
| Network     | `/proc/net/dev`. Local is the loopback device, `lo`.
| Swap        | `/proc/meminfo`.
| Zswap       | `Zswap` and `Zswapped` in `/proc/meminfo`, on kernels new enough to have them. The ratio is `Zswapped / Zswap`. `/sys/kernel/debug/zswap` needs root, so it isn't used.
| Zram        | `/sys/block/zram*/mm_stat`.
| Load        | `/proc/loadavg`.
| Disk        | `/proc/diskstats`, one row per device, with partitions left out of the total.
| Temperature | `hwmon` under `/sys/class/hwmon`, picking the CPU package sensor. `temp*_crit` gives the critical level when the sensor has one.
| Battery     | `/sys/class/power_supply/BAT*`.

Splitting network traffic into LAN and internet is optional, backlog item 2026101916274800. It's off by default, and the user picks the way in settings. `/proc/net/dev` counts per interface, not per destination, so it needs one of these:

- An nftables counter set that matches private address ranges. Its overhead is close to none, but it needs root to install once.

- Polling every TCP socket through `sock_diag` and summing bytes by remote address. It needs no root, but it misses UDP, which includes QUIC, and sockets that close between polls.

- eBPF. It's exact, and it needs root too. It isn't offered unless the other two fall short.

The BSDs use `sysctl`, Windows uses PDH counters, and macOS uses `host_statistics64` and IOKit. Those come later.

## History

The graph needs data at a finer grain than its own pixels, so it can be widened without going blocky, and so averaging has something to work with. Keeping every sample forever is too much for a box that runs for a year. So older data is kept at coarser steps.

- Samples go into tiers. Each tier keeps fixed-size buckets for a set time, and buckets roll up into the next tier as they age out.

	| Tier | Bucket | Kept for
	| :--- | :---   | :---
	| 0    | 250 ms | 5 minutes
	| 1    | 2 s    | 3 hours
	| 2    | 30 s   | 3 days
	| 3    | 5 min  | 90 days
	| 4    | 1 hour | Forever

- Each bucket keeps the min, max, and mean of each series, and how many samples went in. Max is kept so a short spike can still be drawn once it's old.

- A bucket with no samples is a gap. Gaps are never counted as zero. A gap is drawn as blank, and it stays out of every average.

- Rough size at tier 4: 8,760 buckets a year, times about 25 series, times 12 bytes, so that way it comes to about 2.6 MB a year.

- History is saved under `$XDG_DATA_HOME/tukzmon/`, one file per plugin instance. On start the file is read back, and the time since the last sample becomes a gap. Both reloads can be turned off in settings.

## Time axis

- In log mode, the newest sample is at the right edge. Every pixel to the left covers more time than the one to its right, and the left edge is the oldest sample. Until the caps below kick in, the whole run is squeezed into the graph from the first second.

- The graph is split into 4 quarters with caps. From the right, they cover at most 1 minute, the next 59 minutes, the next 23 hours, and then everything older.

- Each quarter boundary is the smaller of where pure log mode would put it and its cap. Early on, no cap is reached and the graph is pure log. Once the right quarter would cover more than a minute, it's pinned at 1 minute and the other 3 quarters share the rest in log mode. The same happens at 1 hour and 1 day.

- Within a quarter, time is still log scaled.

- When a boundary changes, it's eased over about half a second, so nothing jumps.

- There's no visible seam where one quarter meets the next. Each quarter's log curve is fit so a pixel covers the same time on both sides of a boundary. The move from 1 minute to 1 hour to 1 day to everything older looks like one smooth curve.

- A linear mode is there too, for anyone who wants a plain scrolling graph.

## Drawing

- Each pixel column covers a span of time. Its value is the average of every bucket in that span, weighted by how much of each bucket falls in it. Buckets at the edges count in part. That's what keeps the graph from shimmering while boundaries move and columns slide across bucket edges.

- To keep a single-pixel spike visible, a column is drawn from its mean, plus part of the way toward its max. The part gets smaller as the column covers more time. A spike that lasted a long time stays visible far to the left, and a short one fades into the background level once its column covers enough time.

- Stacked graphs, like CPU and memory, draw each series on top of the one below, in the order the backlog gives.

- Disk and network scale to their recent peak, and the peak is eased too. Disk can use a log Y axis.
