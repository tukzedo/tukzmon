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

- The plugin draws its graphs in one drawing area, side by side, in the saved order. There's no gap between them, like multiload-ng's spacing of 0. On a vertical panel they're stacked, and share the height evenly. The graph under the pointer gets the hover line and the tip. `core/src/graphs.rs` has the layout.
	- Under Graphs on the Graph tab, each graph has a checkbox, at `cpu.show` and `mem.show`, and a width, at `cpu.width` and `mem.width`. A width of 0, the default, is the Width setting. Widths are in the panel's own pixels, so they scale with it.
	- A box where `/proc/meminfo` can't be read never shows the memory graph.
	- With every other graph off, CPU shows anyway, so the item never shrinks to nothing, and its right-click menu stays in reach.
	- A hidden graph isn't read. Its history is kept, and shows a gap for the time it was hidden.
	- Order on the Graph tab, at `graph.order`, names the graphs first to last, left to right or top to bottom: `cpu` and `mem`, by their history section names. The default is `cpu mem`.
		- Spaces or commas both work, since a hand edit like `order: mem, cpu` is a list to shcl.
		- A name it doesn't know is skipped. A graph it doesn't name goes at the end, in the default order, so that way a graph added later, such as network, shows up last in a saved order.
		- A hidden graph keeps its place for when it's shown again.
	- The live graph list, backlog 2026102408304639, takes over these rows. Its grippers set the order.

- The drawing area has a window of its own, so it has to take button events and be an action widget, or a right-click never reaches the panel's menu.

- The plugin reads the settings file again when its modified time changes, checked at every sample. That's how a save in the settings window shows up.

- The tip belongs to the drawing area, through `query-tooltip`, so it can say what's under the pointer. GTK shows a tip once the pointer has been still for 500 ms, and asking again restarts that wait. So the timer asks again only after the pointer has been still for a second, which keeps a still pointer's time current.

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
	- A tab with rows of its own and tabs under it, which is Themes, keeps those rows on top and shows its tabs as sub-tabs below them. Themes has the Name dropdown with Save, Save as, Rename, Delete and Randomize, and a sub-tab per graph's colors, plus Background for the colors they share. Like every other change, the theme buttons take effect at Apply.
	- Two keys in `core/src/settings.shcl` are tukzmon's, not knobs'. `shown_as: radio` draws a dropdown as radio buttons, and `shown_when: "id = key"` shows a row only while that dropdown is on that key. knobs' gates only take a checkbox.
	- A color chip opens a picker, and the hex value can be typed beside it. The picker is SilkTerm's, not the toolkit's: a saturation and brightness square, a hue strip, and boxes for red, green, blue, brightness, saturation and hex. The row behind it follows every change, and Cancel puts back all of it, the theme's "*" included. The color model is in `settings/src/pick.rs`.
	- The file gets only what was changed in the window, applied to the file as it is at save time, so a hand edit made meanwhile isn't lost.
	- A save that would drop a line it can't read, or a setting it doesn't know, is refused, and the error names the lines.

- Not there yet: a live preview in the panel before Apply, and the search box.

### Theme

- Graph colors are meant to start from the panel theme: the GTK style context on XFCE, the Qt palette on LXQt. That's how a tukzedo personality's theme reaches the graphs. For now the defaults are a customized multiload-ng setup instead, with the CPU system and nice colors changed since.

- Each color can be overridden in the settings window.

- Every color is in one knobs preset group, `theme`, picked with the Name dropdown on the Themes tab, at `theme.name`, and so is each graph's mode. The built-in themes are in `core/src/settings.shcl`: Default, then Green and Blue, each standard and pastel, then Fire.
	- One theme colors every graph, so no graph picks its own. A theme has a value for every graph's colors and mode, and picking one changes every sub-tab.
	- A theme is named for the color that leads, the user share. Nice, the border and the background are that hue, muted. I/O wait and system stay red and orange in every theme, so trouble reads the same whichever is picked.
	- Default is the earlier multiload-ng colors moved onto one hue, 150 degrees. System went from `#c64600` to `#c64200`, the border from `#40806a` to `#408060`, and the top from `#2a5446` to `#2a543f`.
	- Each color row's default is Default's color, so Custom starts from it. `config::tests::the_default_theme_is_the_row_defaults` holds the two together.
	- A color changed on a theme is kept apart from it, and the dropdown says "Name *", the way SilkTerm's does. Picking another theme drops those changes.
	- knobs reads a color line at its own path as the Custom set. A file with colors and no theme line reads as Custom, so a hand edit or an older file isn't hidden under Default. A save names the theme whenever Custom colors are stored, even Default, or the file would read back as Custom.

- Each graph's sub-tab starts with its mode, Detailed or Gradient, at `cpu.mode` for the CPU graph and `mem.mode` for memory.
	- Detailed stacks a color per series, as before.
	- Gradient draws the column in one blend of Low, Medium and High. A pixel takes the blend at its height in the plot, Low at the bottom, Medium halfway and High at the top, so a busier column reaches further up the ramp and the colors stay put as it grows. The column's height is the same as the stack's.
	- Only the rows for the mode picked are shown. A mode changed by hand is a change on the theme, like a color.
	- Fire is a Gradient theme: dim red, yellow-white, then light blue, like a hot flame. Every other built-in is Detailed, with a gradient from its nice color through its user color to a pale tint of it.

- The memory graph's colors are on the Memory sub-tab: Programs, Buffers and Cache, at `mem.colors.user`, `.buffers` and `.cache`, plus its own Low, Medium and High.
	- Default's are multiload-ng's memory colors as they were, `#ffff80`, `#804080` and `#408060`, not moved onto Default's hue.
	- In the other built-ins, cache is the theme's nice color and buffers a muted color a step round the hue from it. Fire draws memory as a gradient too.
	- multiload-ng's memory border and background aren't used, since every graph shares the Background sub-tab.

- Saved themes are kept whole in the config file, under `themes.<key>`, with a `name` and every color and mode by its setting id, the same shape as a built-in in settings.shcl. The dropdown lists them after the built-ins, and before Custom.
	- They join the `theme` group as presets on a copy of the spec, through knobs' `Spec::finish`. The plugin reads them the same way, so knobs is unchanged.
	- The key comes from the name when the theme is made, and stays through a rename. A key that is a built-in's, or a color line that can't be read, stops a save like any other unreadable line. A missing color is Default's.
	- Save puts the "*" changes into the saved theme in force. Save as makes a new theme from the colors showing and picks it. Delete picks Default. Built-in themes can't be saved over, renamed or deleted, and no two themes share a name.
	- A save writes every saved theme. One this window didn't touch is written as the file has it, so a theme added by hand meanwhile isn't lost.

- Transparent, on the Background sub-tab at `graph.transparent`, leaves out the Top and Bottom colors so the panel shows through. It's one setting for every theme, outside the `theme` group, so a theme pick, a roll or Save as doesn't touch it. The border, the series and the hover line stay opaque. Pixels are premultiplied, so a row a column only half fills is half clear.

- Randomize rolls colors for every graph and sets them as changes on the theme in force, so the dropdown says "Name *". Save as keeps a roll, and picking a theme drops it. The rules are in `settings/src/dice.rs`.
	- The user share leads, at any hue from yellow-green round to magenta, which keeps it off red and orange. Nice and a gradient's Low are that hue muted, or one up to 30 degrees from it. The border and background are the lead hue or nice's, darker and grayer.
	- I/O wait stays red and system orange, as in the built-ins, since those are the ones to watch. Every color softens together, from as bright as Green standard to as pale as Green pastel.
	- A roll is always Detailed. Its gradient runs from nice through the user color to a pale tint, like the built-ins', so it's ready if Gradient is picked. The hover line stays black.
	- Buffers has a role of its own. It's muted like cache, but on the other side of the lead hue and at least 30 degrees from cache, so that way the 2 are told apart.
	- Every theme member needs a role in `dice::ROLES`. A new graph's members fail `dice::tests::every_theme_member_has_a_role` until they get one, such as buffers and cache as muted ones.

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
	- The file holds a named section per graph. A section that no graph shown owns, such as a hidden graph's, is kept at each save, so showing the graph again brings its history back. A file from other tiers, or one that doesn't read whole, is skipped. The graph starts fresh, and the next save replaces it.
	- Times are Unix seconds, so runs line up. A clock set back to before the saved history is moved up to its end, so new samples aren't dropped as old.
	- "Keep across restarts" turns it off. Off stops saving and loading, and leaves the file as it is. Turned back on, the file is loaded again, so what was seen while it was off is a gap.
	- History older than the max age, 365 days by default, is dropped at each save and load.

## Time axis

- In log mode, the newest sample is at the right edge. Every pixel to the left covers more time than the one to its right, and the left edge is the oldest sample. Until the caps below kick in, the whole run is squeezed into the graph from the first second.

- There are 3 log curves, picked under Log curve in settings. Smooth log is the default.
	- Smooth log is one curve over the whole run, `u = ln(1 + age / 5 s) / ln(1 + T / 5 s)`, with no caps. A year in, the right quarter covers about 4 minutes.
	- Log A and Log B put caps on 4 quarters, as below. They differ in the free curve that places a boundary before its cap is reached.
	- Log A is the first curve tukzmon had. Its free curve is scaled to the run, so the caps fill early.
	- Log B's free curve is Smooth log, so it stays the same as Smooth log until a quarter fills its cap. The right quarter fills at about 1.7 days, the third at about 36 days, and the second at about 128 days.

- With caps, the graph is split into 4 quarters. From the right, they cover at most 1 minute, the next 59 minutes, the next 23 hours, and then everything older.

- Each quarter boundary is the smaller of where pure log mode would put it and its cap. Early on, no cap is reached and the graph is pure log. Once the right quarter would cover more than a minute, it's pinned at 1 minute and the other 3 quarters share the rest in log mode. The same happens at 1 hour and 1 day.
	- For Log A, pure log is scaled to the run: age at `u` of the width, from the right, is `T * ((1 + K)^u - 1) / K`, with `K` at 50. So the right quarter covers about 3% of the run, and the caps pin at about 30 minutes, 18 hours, and 7 days.
	- For Log B, it's the fixed log `ln(1 + age / 5 s)` of Smooth log, which pins much later, as above.

- Within a quarter, time is still log scaled.

- While running, the boundaries move smoothly by themselves, since each is a smooth function of the run's length. Easing over about half a second is still needed for jumps, such as a width change or history loaded from disk.

- There's no visible seam where one quarter meets the next. The curve between boundaries is a monotone cubic through them, in `ln(1 + age / 5 s)`, so a pixel covers the same time on both sides of a boundary. The move from 1 minute to 1 hour to 1 day to everything older looks like one smooth curve. Tests check both that and that time only ever runs older to the left.

- A linear mode is there too, for anyone who wants a plain scrolling graph. A pixel is one sample there.

## Drawing

- Each pixel column covers a span of time. Its value is the average of every bucket in that span, weighted by how much of each bucket falls in it. Buckets at the edges count in part. That's what keeps the graph from shimmering while boundaries move and columns slide across bucket edges.

- To keep a single-pixel spike visible, a column is drawn from its mean, plus part of the way toward its max. The part gets smaller as the column covers more time. A spike that lasted a long time stays visible far to the left, and a short one fades into the background level once its column covers enough time.
	- The part is the Spike emphasis setting times `1 / (1 + seconds / 20)`. A stack that comes out over 100% is scaled down to fit.

- A column with data for less than half its time is drawn as a gap.

- With the pointer over the graph, a line one pixel wide is drawn under it, black unless set otherwise, over the plot and not the border. The tip gives the time under the line, either as "7 hrs ago" or as the local time, with the date for anything before today. The time and date follow the locale's short formats, so 12 or 24 hour is its call too. Under that are that graph's shares across the line's column, or "No data" for a gap.

- Stacked graphs, like CPU and memory, draw each series on top of the one below, in the order the backlog gives.

- Disk and network scale to their recent peak, and the peak is eased too. Disk can use a log Y axis.
