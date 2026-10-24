<!-- markdownlint-disable MD007 -- Unordered list indentation -->
<!-- markdownlint-disable MD010 -- No hard tabs -->
<!-- markdownlint-disable MD033 -- No inline html -->
<!-- markdownlint-disable MD055 -- Table pipe style [Expected: leading_and_trailing; Actual: leading_only; Missing trailing pipe] -->
<!-- markdownlint-disable MD041 -- First line in a file should be a top-level heading -->
<!-- TOC ignore:true -->
# Design

Draft. A CPU-only proof of concept is built for XFCE. Goes with [backlog.md](backlog.md).

<!-- TOC ignore:true -->
## Table of contents

<!-- TOC -->

- [Summary](#summary)
- [Code layout](#code-layout)
	- [XFCE plugin](#xfce-plugin)
	- [Settings window](#settings-window)
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
	- Drawing into a plain pixel buffer. Stacked columns are filled by hand, with the top pixel of each series shaded by how much of it the series covers. [tiny-skia](https://crates.io/crates/tiny-skia) comes in once there are lines or curves to draw.
	- The settings spec, `core/src/settings.shcl`, which the plugin and the settings window both compile in.

- `knobs` is SilkTerm's settings model: settings declared in shcl, and what a change to one does to the others. It's MIT, copyright Bubbles, and the copy here is kept the same as SilkTerm's.

- `tukzmon-settings` is the settings window, a program of its own. See [Settings window](#settings-window).

- Each front end is as small as it can be. It creates the panel widget, runs a timer, and copies the library's pixel buffer onto the screen. A pixel buffer is the one thing every host can show: a cairo image surface on GTK, a `QImage` on Qt, a DIB on Windows, and a `CGImage` on macOS.

- Text overlays, such as the zswap ratio, are drawn by the front end with the panel's own font, so they match the rest of the panel.

### XFCE plugin

- XFCE 4.20 is still GTK3, and its plugin API is C macros around `libxfce4panel-2.0`.

- The plugin is a Rust `cdylib`. The 20 or so GLib, GTK, cairo, and panel calls it makes are declared by hand in `xfce/src/sys.rs`, and it links the runtime libraries by soname. So one `cargo build` gives the `.so`, with no `-dev` packages.
	- The gtk3-rs bindings were archived in 2024.
	- Every widget is an opaque pointer, so no C struct layouts are copied, and a header change can't break it quietly.
	- The other way, a short C file built against the headers, needs `libxfce4panel-2.0-dev`. On b23 that would pull the panel library back from 4.20.7 to Debian's 4.20.4.
	- The panel runs external plugins in a separate wrapper process, so a crash takes down the plugin adn not the panel.

- The drawing area has a window of its own, so it has to take button events and be an action widget, or a right-click never reaches the panel's menu.

- The plugin reads the settings file again when its modified time changes, checked at every sample. That's how a save in the settings window shows up.

- A `.desktop` file tells the panel where the plugin is. XFCE 4.20 reads them only from `/usr/share/xfce4/panel/plugins`, and the library only from `xfce4/panel/plugins` under the system lib dir. `XDG_DATA_DIRS` and `~/.local` aren't searched, so even a build for one user needs root once. `utility/dev-install.bash` installs to `~/.local/lib/tukzmon`, makes the 2 symlinks with sudo when they're missing, and restarts the panel.

- If Rust can't be used for some reason, the fallback is the same split, with the C file growing into the whole plugin.

### Settings window

- Settings are a separate program, `tukzmon-settings`, so every panel and platform shares one window. A panel only has to start it with `--config FILE`.

- It's Qt 6 Widgets. Widgets is the lightest Qt that's cross-platform and maintained, and Qt Quick would add QML and a scene graph for a form with a dozen rows. Rust reaches it through the [cxx](https://cxx.rs) crate, and `settings/src/qt.cpp` only builds widgets and copies state onto them. The model, the rules, and the file are all in Rust.

- Qt doesn't need a file format of its own for this, so settings stay in shcl, at `~/.config/tukzmon/tukzmon.shcl`. One file serves every graph for now. A file per plugin instance comes with more than one kind of graph.

- It follows SilkTerm's settings dialog, on top of knobs:
	- Tabs down the left, groups with a rule under the title, and a sub-group stepped in under the switch it depends on.
	- A revert arrow on every row, live when the value isn't its default.
	- Cancel, Apply, and Apply and close, with Cancel set a little apart. Enter is Apply and close and Esc is Cancel, even from inside a field.
	- A slider can reach less far than its number box takes. The box takes any sane value, and `slider_range` in `core/src/settings.shcl` sets how far the slider goes.
	- The selected tab is a tint of the theme's highlight under the normal text color, so it reads in a dark theme too.
	- A color chip opens a picker, and the hex value can be typed beside it.
	- The file gets only what was changed in the window, applied to the file as it is at save time, so a hand edit made meanwhile isn't lost.
	- A save that would drop a line it can't read, or a setting it doesn't know, is refused, and the error names the lines.

- Not there yet: a live preview in the panel before Apply, and the search box.

### Theme

- Graph colors are meant to start from the panel theme: the GTK style context on XFCE, the Qt palette on LXQt. That's how a tukzedo personality's theme reaches the graphs. For now the defaults are a customized multiload-ng setup instead, with the CPU system and nice colors changed since.

- Each color can be overridden in the settings window.

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
| Memory      | `/proc/meminfo`. User is total minus free, buffers, and cache. Cache is the `Cached` row alone, as multiload-ng counts it, so slab is in user.
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

- Every tier gets every sample, so there's no roll-up step.

- A sample is a share of the time since the read before it, so it's added over that whole stretch, to every bucket it covers. Half-second reads on quarter-second buckets would otherwise leave every other bucket empty.

- Each bucket keeps the min, max, and mean of each series, how many seconds of data it has, and where in the bucket that data starts and ends. Max is kept so a short spike can still be drawn once it's old. The start and end let the newest bucket, still filling, read as full where it has data.

- A bucket with no samples is a gap. Gaps are never counted as zero. A gap is drawn as blank, and it stays out of every average.

- Rough size at tier 4: 8,760 buckets a year, times about 25 series, times 12 bytes, so that way it comes to about 2.6 MB a year. Each bucket adds 20 bytes for its seconds and its start and end.

- History is saved under `$XDG_DATA_HOME/tukzmon/`, as `plugin-<id>.hist`, one file per plugin instance. On start the file is read back, and the time since the last sample becomes a gap.
	- It's written every 15 minutes and when the panel quits, to a temp file renamed over the old one. A crash loses at most the 15 minutes.
	- The file holds a named section per graph. A file from other tiers, or one that doesn't read whole, is skipped. The graph starts fresh, and the next save replaces it.
	- Times are Unix seconds, so runs line up. A clock set back to before the saved history is moved up to its end, so new samples aren't dropped as old.
	- "Keep across restarts" turns it off. Off stops saving and loading, and leaves the file as it is. Turned back on, the file is loaded again, so what was seen while it was off is a gap.
	- History older than the max age, 365 days by default, is dropped at each save and load.

## Time axis

- In log mode, the newest sample is at the right edge. Every pixel to the left covers more time than the one to its right, and the left edge is the oldest sample. Until the caps below kick in, the whole run is squeezed into the graph from the first second.

- The graph is split into 4 quarters with caps. From the right, they cover at most 1 minute, the next 59 minutes, the next 23 hours, and then everything older.

- Each quarter boundary is the smaller of where pure log mode would put it and its cap. Early on, no cap is reached and the graph is pure log. Once the right quarter would cover more than a minute, it's pinned at 1 minute and the other 3 quarters share the rest in log mode. The same happens at 1 hour and 1 day.
	- Pure log is scaled to the run: age at `u` of the width, from the right, is `T * ((1 + K)^u - 1) / K`, with `K` at 50. So the right quarter covers about 3% of the run, and the caps start to pin at about 30 minutes, 8 hours, and 2.7 days.
	- A fixed log, `ln(1 + age / a)` with `a` a second or so, never reaches the caps in a year, so it isn't used for the boundaries.

- Within a quarter, time is still log scaled.

- While running, the boundaries move smoothly by themselves, since each is a smooth function of the run's length. Easing over about half a second is still needed for jumps, such as a width change or history loaded from disk.

- There's no visible seam where one quarter meets the next. The curve between boundaries is a monotone cubic through them, in `ln(1 + age / 5 s)`, so a pixel covers the same time on both sides of a boundary. The move from 1 minute to 1 hour to 1 day to everything older looks like one smooth curve. Tests check both that and that time only ever runs older to the left.

- A linear mode is there too, for anyone who wants a plain scrolling graph. A pixel is one sample there.

## Drawing

- Each pixel column covers a span of time. Its value is the average of every bucket in that span, weighted by how much of each bucket falls in it. Buckets at the edges count in part. That's what keeps the graph from shimmering while boundaries move and columns slide across bucket edges.

- To keep a single-pixel spike visible, a column is drawn from its mean, plus part of the way toward its max. The part gets smaller as the column covers more time. A spike that lasted a long time stays visible far to the left, and a short one fades into the background level once its column covers enough time.
	- The part is the Spike emphasis setting times `1 / (1 + seconds / 20)`. A stack that comes out over 100% is scaled down to fit.

- A column with data for less than half its time is drawn as a gap.

- Stacked graphs, like CPU and memory, draw each series on top of the one below, in the order the backlog gives.

- Disk and network scale to their recent peak, and the peak is eased too. Disk can use a log Y axis.
