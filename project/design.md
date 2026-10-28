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
	- The core says what to write, in which colors, and the room it has, which is the plot. The front end leaves it out when it doesn't fit, as on a short panel.

### XFCE plugin

- XFCE 4.20 is still GTK3, and its plugin API is C macros around `libxfce4panel-2.0`.

- The plugin is a Rust `cdylib`. The 20 or so GLib, GTK, cairo, and panel calls it makes are declared by hand in `xfce/src/sys.rs`, and it links the runtime libraries by soname. So one `cargo build` gives the `.so`, with no `-dev` packages.
	- The gtk3-rs bindings were archived in 2024.
	- Every widget is an opaque pointer, so no C struct layouts are copied, and a header change can't break it quietly.
	- The other way, a short C file built against the headers, needs `libxfce4panel-2.0-dev`. On b23 that would pull the panel library back from 4.20.7 to Debian's 4.20.4.
	- The panel runs external plugins in a separate wrapper process, so a crash takes down the plugin adn not the panel.
	- That's on X11. On Wayland, XFCE 4.20 loads the plugin into the panel's own process, since XEmbed is X11 only, so a panic in the plugin aborts the whole panel. Every instance shares that process too. That's safe, since the plugin's only globals are read-only caches of the settings spec, and it starts no threads.
	- So the plugin's messages go through `complain` in `xfce/src/lib.rs`, since `eprintln!` panics when stderr can't be written, such as to a full disk or a closed terminal. Test `defyn` keeps the print macros out of all the code the panel loads.
	- Nothing else in the plugin assumes X11. The drawing, the tip, the hover line, the right-click menu and Properties work the same on Wayland, and tukzmon-settings opens as a Wayland window.

- The plugin draws its graphs in one drawing area, side by side, in the saved order. They have 2 px clear above and below, and 4 px between each pair, in logical pixels, so the panel shows through. On a vertical panel or a deskbar they're stacked, and share the height evenly, with the 2 px either side instead. Each asks for half the panel's width in height, at least 16 px, so the widths don't apply there. The graph under the pointer gets the hover line and the tip. The 2 px edges count as the graph beside them, so a pointer pushed against the screen's edge still finds it, and a gap between 2 graphs finds neither. `core/src/graphs.rs` has the layout.
	- Under Graphs on the Graph tab is the graph list, a row per graph in the saved order. Each row has a checkbox, at `cpu.show`, `mem.show`, `net.show`, `swap.show`, `zswap.show`, `load.show`, `disk.show` and `battery.show`, the live graph, a width, at `cpu.width`, `mem.width`, `net.width`, `swap.width`, `zswap.width`, `load.width`, `disk.width` and `battery.width`, a time axis, a revert arrow and a gripper. A width of 0, the default, is the Width setting. Widths are in the panel's own pixels, so they scale with it.
	- Each row draws its graph live, at its own width, with the window's settings, so a change shows there before Apply. The window reads the graphs itself from when it opens, so a row starts empty. A hidden graph is drawn too. The widest graph sets the column, so the boxes after it line up, and one wider than 512 px is squeezed to that.
	- The row under the pointer gets the hover line and the tip, the same as in the panel, through the panel's own `graphs::point`. On a squeezed row the line goes at the pointer's place in the whole graph. A tip left up follows each sample.
	- The time axis is at `cpu.time`, `mem.time`, `net.time`, `swap.time`, `zswap.time`, `load.time`, `disk.time` and `battery.time`. See [Time axis](#time-axis).
	- A graph gets a row once `core/src/settings.shcl` has its `<name>.show`, `.width` and `.time` settings.
	- A box where `/proc/meminfo` can't be read never shows the memory graph.
	- The network, swap, zswap, load, disk and battery graphs are off by default, so an existing panel doesn't grow wider. A box with no swap, with neither zswap on nor zram set up, with no disk, or with no battery in, never shows that one.
	- Under Disk graph, below the order, Disk drives at `disk.drives` picks what the disk graph counts: `all`, the default, or `/proc/diskstats` names such as `sda nvme0n1`, with spaces or commas. A new pick takes effect at Apply and keeps the graph's history. Disk log scale, at `disk.log`, draws it on a log scale. See [Drawing](#drawing).
	- With every other graph off, CPU shows anyway, so the item never shrinks to nothing, and its right-click menu stays in reach.
	- A hidden graph isn't read. Its history is kept, and shows a gap for the time it was hidden.
	- Dragging a row's gripper, or Up and Down on it, sets the order. It's saved at `graph.order`, which names the graphs first to last, left to right or top to bottom: `cpu`, `mem`, `net`, `swap`, `zswap`, `load`, `disk` and `battery`, by their history section names. The default is `cpu mem net swap zswap load disk battery`, so network sits after memory and load and disk after swap, as in multiload-ng, and zswap beside swap. A saved order from before a graph was added puts it last.
		- Spaces or commas both work, since a hand edit like `order: mem, cpu` is a list to shcl.
		- A name it doesn't know is skipped. A graph it doesn't name goes at the end, in the default order, so that way a graph added later, such as network, shows up last in a saved order.
		- A hidden graph keeps its place for when it's shown again.
	- A name the list doesn't know stays in `graph.order` after a drag, at the end, so a file from a newer tukzmon keeps it.
	- The row's revert arrow puts its checkbox, width and time axis back. The order has no arrow, since dragging back undoes it.

- The drawing area has a window of its own, so it has to take button events and be an action widget, or a right-click never reaches the panel's menu.

- The plugin reads the settings file again when its modified time changes, checked at every sample. That's how a save in the settings window shows up.

- The tip belongs to the drawing area, through `query-tooltip`, so it can say what's under the pointer. GTK shows a tip once the pointer has been still for 500 ms, and asking again restarts that wait. So the timer asks again only after the pointer has been still for a second, which keeps a still pointer's time current.

- A `.desktop` file tells the panel where the plugin is. XFCE 4.20 reads them only from `/usr/share/xfce4/panel/plugins`, and the library only from `xfce4/panel/plugins` under the system lib dir. `XDG_DATA_DIRS` and `~/.local` aren't searched, so even a build for one user needs root once. `utility/dev-install.bash` installs to `~/.local/lib/tukzmon`, makes the 2 symlinks with sudo when they're missing, and restarts the panel.

- The Nix package is for tukzedo-linux's NixOS edition. `flake.nix` has it, an overlay, and a NixOS module, `programs.tukzmon.enable`. `nix/package.nix` builds it.
	- XFCE 4.20 built with its own prefix, as Nix builds it, pairs each `XDG_DATA_DIRS` entry `<p>/share` with `<p>/lib`. It reads `.desktop` files from `<p>/share/xfce4/panel/plugins`, and the library from `<p>/lib/xfce4/panel/plugins`. So the package puts them there, and NixOS links both into `/run/current-system/sw`. Its XFCE desktop module does that already, and the tukzmon module does it too, for a panel run under some other desktop.
	- The plugin links its libraries by soname, which Nix's linker wrapper gives no rpath for, so the package passes the rpath itself.
	- A link to `tukzmon-settings` sits beside the library, so the plugin runs the one from its own package rather than whatever is first on PATH.
	- The build number is the flake's commit time, since Nix's default `SOURCE_DATE_EPOCH` is 1980.
	- `cicd/cicd.bash --long`, or its `nix` area, builds the package in a nixos/nix container, then has nixpkgs' xfce4-panel load and embed the plugin from a NixOS system with the module. `cicd/nix/check.bash` is the check. Its store is a docker volume kept between runs, since the first build fetches about 5 GB. It's left out of the other runs for that reason.

- If Rust can't be used for some reason, the fallback is the same split, with the C file growing into the whole plugin.

### Settings window

- Settings are a separate program, `tukzmon-settings`, so every panel and platform shares one window. A panel only has to start it with `--config FILE`.

- It's Qt 6 Widgets. Widgets is the lightest Qt that's cross-platform and maintained, and Qt Quick would add QML and a scene graph for a form with a dozen rows. Rust reaches it through the [cxx](https://cxx.rs) crate, and `settings/src/qt.cpp` only builds widgets and copies state onto them. The model, the rules, and the file are all in Rust.

- Qt doesn't need a file format of its own for this, so settings stay in shcl, under `~/.config/tukzmon/`.
	- Each panel item has its own file, `plugin-<id>.shcl` by the panel's unique id, named like its history file. Its Properties opens that file, so 2 items on a panel can show different graphs.
	- `tukzmon.shcl` is the shared file. An item with no file of its own yet starts from a copy of it, so a panel set up before items had their own keeps its look. The shared file is never changed by an item, and `tukzmon-settings` with no `--config` edits it, which sets what a new item starts from.
	- With no shared file either, a new item starts at the defaults, and writes no file until a save.

- It follows SilkTerm's settings dialog, on top of knobs:
	- Tabs down the left, groups with a rule under the title, and a sub-group stepped in under the switch it depends on. A row is greyed while that switch is off, but its revert arrow still works.
	- A revert arrow on every row, live when the value isn't its default.
	- Cancel, Apply, and OK, with Cancel set a little apart. Enter is OK and Esc is Cancel, even from inside a field.
	- A slider can reach less far than its number box takes. The box takes any sane value, and `slider_range` in `core/src/settings.shcl` sets how far the slider goes.
	- The selected tab is a tint of the theme's highlight under the normal text color, so it reads in a dark theme too.
	- A tab with rows of its own and tabs under it, which is Themes, keeps those rows on top of whichever of its sub-tabs is picked. The sub-tabs are listed under it in the tab list, stepped in, and picking Themes itself shows the first one. Themes has the Name dropdown with Save, Save as, Rename, Delete and Randomize, and a sub-tab per graph's colors, plus Background for the colors they share. Like every other change, the theme buttons take effect at Apply.
	- Two keys in `core/src/settings.shcl` are tukzmon's, not knobs'. `shown_as: radio` draws a dropdown as radio buttons, and `shown_when: "id = key"` shows a row only while that dropdown is on that key. knobs' gates only take a checkbox.
	- Graphs on the Graph tab is a list with a row per graph, not a row per setting. See [XFCE plugin](#xfce-plugin).
	- A color chip opens a picker, and the hex value can be typed beside it. The picker is SilkTerm's, not the toolkit's: a saturation and brightness square, a hue strip, and boxes for red, green, blue, brightness, saturation and hex. The row behind it follows every change, and Cancel puts back all of it, the theme's "*" included. The color model is in `settings/src/pick.rs`.
	- A tip is broken into lines of at most 70 characters, the width GTK wraps its own tips at, so the panel's and the window's read alike. Qt would show a plain tip as one line. `wrap_tip` in `settings/src/bridge.rs` does it, and an event filter in `qt.cpp` sends every tip in the window through it.
	- The file gets only what was changed in the window, applied to the file as it is at save time, so a hand edit made meanwhile isn't lost.
	- A save that would drop a line it can't read, or a setting it doesn't know, is refused, and the error names the lines.

- Reset history sits on the left of the bottom strip, under the pages, and starts every graph over, in every panel item. With Keep across restarts on, it asks first, since that deletes the saved history too. Like the theme buttons it takes effect at Apply, but the window's own graphs start over at once.
	- At Apply the window writes `reset` in the history folder. When Keep across restarts is on, it deletes every `plugin-*.hist` there first, so an item that isn't running doesn't bring old history back.
	- A running plugin checks that file's modified time at each sample, as it does the settings file's. It drops what it holds, and when history is kept it deletes its own file again, since it may have saved in between.
	- With Keep across restarts off, the file is left as it is, the same as Off always does.

- Not there yet: a live preview in the panel before Apply, and the search box.

### Theme

- Graph colors are meant to start from the panel theme: the GTK style context on XFCE, the Qt palette on LXQt. That's how a tukzedo personality's theme reaches the graphs. For now the defaults are a customized multiload-ng setup instead, with the CPU system and nice colors changed since.

- Each color can be overridden in the settings window.

- Every color is in one knobs preset group, `theme`, picked with the Name dropdown on the Themes tab, at `theme.name`, and so is each graph's mode. The built-in themes are in `core/src/settings.shcl`: Default, Primary, Pastel, Neon, Earth and Fire.
	- One theme colors every graph, so no graph picks its own. A theme has a value for every graph's colors and mode, and picking one changes every sub-tab.
	- A theme's name says nothing of color. Each graph has a hue of its own in every theme, so graphs side by side are told apart. A graph's quiet series and its gradient's Low are its hue, muted. The border and background are neutral, or lean to the theme's mood.
	- I/O wait and system stay red and orange in every theme, so trouble reads the same whichever is picked. No graph's own hue is red or orange, except in Fire and Earth, where they're warm but not the alarms'.
	- Default is the earlier multiload-ng colors moved onto one hue, 150 degrees. System went from `#c64600` to `#c64200`, the border from `#40806a` to `#408060`, and the top from `#2a5446` to `#2a543f`.
	- Each color row's default is Default's color, so Custom starts from it. `config::tests::the_default_theme_is_the_row_defaults` holds the two together.
	- A color changed on a theme is kept apart from it, and the dropdown says "Name *", the way SilkTerm's does. Picking another theme drops those changes.
	- knobs reads a color line at its own path as the Custom set. A file with colors and no theme line reads as Custom, so a hand edit or an older file isn't hidden under Default. A save names the theme whenever Custom colors are stored, even Default, or the file would read back as Custom.

- Each graph's sub-tab starts with its mode, Detailed or Gradient, at `cpu.mode` for the CPU graph, `mem.mode` for memory, `net.mode` for network, `swap.mode` for swap, `zswap.mode` for zswap, `load.mode` for load, `disk.mode` for disk and `battery.mode` for the battery.
	- Detailed stacks a color per series, as before.
	- Gradient draws the column in one blend of Low, Medium and High. A pixel takes the blend at its height in the plot, Low at the bottom, Medium halfway and High at the top, so a busier column reaches further up the ramp and the colors stay put as it grows. The column's height is the same as the stack's.
	- Only the rows for the mode picked are shown. A mode changed by hand is a change on the theme, like a color.
	- Fire is a Gradient theme, and each graph burns a flame of its own. CPU is dim red, yellow-white, then light blue, like a hot flame. Memory is a violet flame, swap a blue gas flame, the network a pink lithium one, zswap a cyan one, load a yellow-green barium one, the disk an indigo one, and the battery a green copper one. Every other built-in is Detailed, and each graph's gradient runs from its muted color through its own lead to a pale tint of it.

- The memory graph's colors are on the Memory sub-tab: Programs, Buffers and Cache, at `mem.colors.user`, `.buffers` and `.cache`, plus its own Low, Medium and High.
	- Default's are multiload-ng's memory colors as they were, `#ffff80`, `#804080` and `#408060`, not moved onto Default's hue.
	- In the other built-ins, programs take memory's own hue, cache is that hue muted, and buffers a muted color a step round from it. Fire draws memory as a gradient too.
	- multiload-ng's memory border and background aren't used, since every graph shares the Background sub-tab.

- The swap graph's colors are on the Swap sub-tab: In use, at `swap.colors.used`, plus its own Low, Medium and High. Every built-in gives swap a hue and gradient of its own. Default's is sky blue, `#40a0ff`, and Fire draws swap as a gradient too.

- The network graph's colors are on the Network sub-tab: Local, Sent and Received, at `net.colors.local`, `.sent` and `.received`, plus its own Low, Medium and High. Received is the network's own hue, local is that hue muted, the same as Low, and sent is a pale tint of it, the same as High. Default's received is lime, `#90ff40`.
	- The tip gives the 3 rates in bytes a second, in decimal units, such as "Received 1.5 MB/s  Sent 30 kB/s" over "Local 2.0 kB/s".

- The load graph's colors are on the Load sub-tab: Load, at `load.colors.load`, plus its own Low, Medium and High. Default's is cyan, `#40ffff`. Its tip reads like "Load 1.25".

- The disk graph's colors are on the Disk sub-tab: Written and Read, at `disk.colors.write` and `.read`, plus its own Low, Medium and High. Read is the disk's own hue, and written a pale tint of it, the same as High. Default's read is rose, `#ff4080`.
	- The tip gives both rates the way the network's does, such as "Read 1.5 MB/s  Write 30 kB/s".

- The zswap graph's colors are on the Zswap sub-tab: In use, at `zswap.colors.used`, and Ratio, at `zswap.colors.ratio`, plus its own Low, Medium and High. Every built-in gives zswap a hue and gradient of its own, and Ratio is a pale tint of it. Default's is magenta, `#ff80ff`.
	- The graph is zram's on a box with zram and no zswap, and its tip says which, such as "Zram 40% used".
	- The ratio from the latest read is written at the plot's top left, such as `4.7x`, in Ratio with a rim of the background's Bottom, so it reads over the pool's own color too. An empty pool has no ratio, so nothing is written.

- The battery graph's colors are on the Battery sub-tab: Plugged in, On battery and Critical, at `battery.colors.charging`, `.discharging` and `.critical`, plus its own Low, Medium and High. In every built-in, plugged in is a hue of the battery's own, and on battery and critical are the theme's system and I/O wait colors, so on battery reads as a warning and critical as an alarm. The gradient is the battery's own. Default's plugged in is lavender, `#b080ff`.
	- The tip says plugged in rather than charging, since a full battery with the power in counts as charging.

- Saved themes are kept whole in the config file, under `themes.<key>`, with a `name` and every color and mode by its setting id, the same shape as a built-in in settings.shcl. The dropdown lists them after the built-ins, and before Custom.
	- They join the `theme` group as presets on a copy of the spec, through knobs' `Spec::finish`. The plugin reads them the same way, so knobs is unchanged.
	- The key comes from the name when the theme is made, and stays through a rename. A key that is a built-in's, or a color line that can't be read, stops a save like any other unreadable line. A missing color is Default's.
	- Save puts the "*" changes into the saved theme in force. Save as makes a new theme from the colors showing and picks it. Delete picks Default. Built-in themes can't be saved over, renamed or deleted, and no two themes share a name.
	- A save writes every saved theme. One this window didn't touch is written as the file has it, so a theme added by hand meanwhile isn't lost.

- Transparent, on the Background sub-tab at `graph.transparent`, leaves out the Top and Bottom colors so the panel shows through. It's one setting for every theme, outside the `theme` group, so a theme pick, a roll or Save as doesn't touch it. The border, the series and the hover line stay opaque. Pixels are premultiplied, so a row a column only half fills is half clear.

- Randomize rolls colors for every graph and sets them as changes on the theme in force, so the dropdown says "Name *". Save as keeps a roll, and picking a theme drops it. The rules are in `settings/src/dice.rs`.
	- Each graph leads with a hue of its own, the user share for CPU, at any hue from yellow-green round to magenta, which keeps it off red and orange. The 8 lead hues are at least 25 degrees apart, in a random order. A graph's quiet colors and its gradient's Low are its hue muted, or one up to 30 degrees from it. The border and background are CPU's lead hue or nice's, darker and grayer.
	- I/O wait stays red and system orange, as in the built-ins, since those are the ones to watch. Every color softens together, from as bright as Primary to as pale as Pastel.
	- Every graph shares one saturation and brightness, so a roll reads as one set and only the hues differ.
	- A roll is always Detailed. Each graph's gradient runs from its quiet color through its lead to a pale tint, like the built-ins', so it's ready if Gradient is picked. The hover line stays black.
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
| Memory      | `/proc/meminfo`. User is total minus free, buffers, and cache. Cache is `Cached` plus `SReclaimable`, as `free` counts it and as multiload-ng does with `graph-mem-procps-compliant` on. Slab that can't be reclaimed is in user.
| Network     | `/proc/net/dev`, in bytes per second. Local is what the loopback device, `lo`, received, since it receives all it sends. Sent and received add up every other interface. An interface coming or going, or a counter that went down, makes that sample a gap.
| Swap        | `/proc/meminfo`. Used is `SwapTotal` less `SwapFree`, against `SwapTotal`. A box with no swap at start has no Swap graph, and swap turned off later is a gap.
| Zswap       | `Zswap` and `Zswapped` in `/proc/meminfo`, on kernels new enough to have them. Used is the pool, `Zswap`, against the cap zswap stops taking pages at, `max_pool_percent` of `MemTotal`. The ratio is `Zswapped / Zswap`. `/sys/kernel/debug/zswap` needs root, so it isn't used.
| Zram        | `/sys/block/zram*/mm_stat`, summed over every device there at start, when zswap is off. Used is `orig_data_size` against `disksize`, and the ratio is `orig_data_size / compr_data_size`. A box with neither, or with zram devices that have no size, has no Zswap graph.
| Load        | `/proc/loadavg`, its first column, the 1 minute load.
| Disk        | `/proc/diskstats`, one row per device, as 512 byte sectors read and written. All disks means each device with a `device` link under `/sys/block`. That leaves out partitions and what's stacked on a disk, such as LVM, LUKS, RAID, loop and zram, so nothing is counted twice. A chosen list counts just the names on it. A counted drive coming or going, or a counter that went down, makes a gap.
| Temperature | `hwmon` under `/sys/class/hwmon`, picking the CPU package sensor: `Package id` on `coretemp`, `Tdie` or else `Tctl` on `k10temp` and `zenpower`, and the first sensor on an Arm board's `cpu_thermal`. With more than one socket it's the hottest. `temp*_crit` gives the critical level when the sensor has one, the lowest of them with more than one. Other drivers, such as `acpitz`, `nvme` and the board's, aren't the CPU's, so a box with none of these has no Temperature graph.
| Battery     | `/sys/class/power_supply/BAT*`. A mouse's or a headset's battery has another name, so it isn't counted, and neither is a `BAT*` whose `type` isn't `Battery`, or one with `present` at 0. The charge is `energy_now / energy_full`, or else `charge_now / charge_full`, or else `capacity`. More than one battery adds up as one, or gives the mean of their shares when they count in different units. The charge goes in one series by state. It's discharging when any battery's `status` says so, and Full, `Not charging` and `Unknown` count as charging. Critical is discharging with every battery's `capacity_level` at `Critical`, or at 4% or less when a battery has no level, as multiload-ng does. A box with no battery in has no Battery graph.

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
	| 0    | 50 ms  | 256 seconds
	| 1    | 250 ms | 2048 seconds, about 34 minutes
	| 2    | 2 s    | 3 hours
	| 3    | 30 s   | 3 days
	| 4    | 5 min  | 90 days
	| 5    | 1 hour | Forever

- Every tier gets every sample, so there's no roll-up step.

- Tiers 0 and 1 are each kept for 1024 times the next tier's bucket. 1024 px is the top of the Width slider, so a linear graph that wide keeps one sample a pixel at any rate down to 50 ms. Past that width, or on a 2x screen at a slow rate, the left end can reach a coarser tier.

- A sample is a share of the time since the read before it, so it's added over that whole stretch, to every bucket it covers. Half-second reads on quarter-second buckets would otherwise leave every other bucket empty.

- Each bucket keeps the min, max, and mean of each series, how many seconds of data it has, and where in the bucket that data starts and ends. Max is kept so a short spike can still be drawn once it's old. The start and end let the newest bucket, still filling, read as full where it has data.

- A bucket with no samples is a gap. Gaps are never counted as zero. A gap is drawn as blank, and it stays out of every average.

- Rough size at tier 5: 8,760 buckets a year, times about 25 series, times 12 bytes, so that way it comes to about 2.6 MB a year. Each bucket adds 20 bytes for its seconds and its start and end.

- History is saved under `$XDG_DATA_HOME/tukzmon/`, as `plugin-<id>.hist`, one file per plugin instance. On start the file is read back, and the time since the last sample becomes a gap.
	- It's written every 15 minutes and when the panel quits, to a temp file renamed over the old one. A crash loses at most the 15 minutes.
	- The file holds a named section per graph. A section that no graph shown owns, such as a hidden graph's, is kept at each save, so showing the graph again brings its history back. A file from other tiers, or one that doesn't read whole, is skipped. A file from before tier 0 went in, which has one tier less, still reads, with tier 0 empty. The graph starts fresh, and the next save replaces it.
	- Times are Unix seconds, so runs line up. A clock set back to before the saved history is moved up to its end, so new samples aren't dropped as old.
	- "Keep across restarts" turns it off. Off stops saving and loading, and leaves the file as it is. Turned back on, the file is loaded again, so what was seen while it was off is a gap.
	- History older than the max age, 365 days by default, is dropped at each save and load.
	- Reset history in the settings window drops all of it, or only what's on screen while Keep across restarts is off. See [Settings window](#settings-window).

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

- While running, the boundaries move smoothly by themselves, since each is a smooth function of the run's length. A jump eases in over half a second instead, from what was on screen to where the axis should be.
	- A jump is anything but time going on: history loaded from disk, the first sample after a gap, a new curve or time axis, or a linear graph's span changing with its width or sample rate. A log axis is in fractions of the width, so a width change only stretches it.
	- The two are mixed in `ln(1 + age / 5 s)`, so time still only runs older to the left. A jump part way through an ease starts from what's on screen.
	- While an axis eases, the panel and the settings window's graph list draw about 30 times a second, then go back to once a sample.

- There's no visible seam where one quarter meets the next. The curve between boundaries is a monotone cubic through them, in `ln(1 + age / 5 s)`, so a pixel covers the same time on both sides of a boundary. The move from 1 minute to 1 hour to 1 day to everything older looks like one smooth curve. Tests check both that and that time only ever runs older to the left.

- A linear mode is there too, for anyone who wants a plain scrolling graph. A pixel is one sample there.

- Each graph can pick its own time axis under Graphs: Linear, or one of the 3 log curves. The default, As set under Time, follows Log time axis and Log curve, so a settings file from before the pick keeps its axis, and one setting still changes every graph that has no pick of its own.
	- A graph's pick is `cpu.time`, `mem.time`, `net.time`, `swap.time`, `zswap.time` or `battery.time`: `same`, `linear`, `smooth`, `fixed_a` or `fixed_b`.
	- Spike emphasis sits under Log time axis, but it applies to any graph on a log curve.

## Drawing

- Each pixel column covers a span of time. Its value is the average of every bucket in that span, weighted by how much of each bucket falls in it. Buckets at the edges count in part. That's what keeps the graph from shimmering while boundaries move and columns slide across bucket edges.

- To keep a single-pixel spike visible, a column is drawn from its mean, plus part of the way toward its max. The part gets smaller as the column covers more time. A spike that lasted a long time stays visible far to the left, and a short one fades into the background level once its column covers enough time.
	- The part is the Spike emphasis setting times `1 / (1 + seconds / 20)`. A stack that comes out over 100% is scaled down to fit.

- A column with data for less than half its time is drawn as a gap.

- With the pointer over the graph, a line one pixel wide is drawn under it, black unless set otherwise, over the plot and not the border. The tip gives the time under the line, either as "7 hrs ago" or as the local time, with the date for anything before today. The time and date follow the locale's short formats, so 12 or 24 hour is its call too. Under that are that graph's shares across the line's column, or "No data" for a gap.

- A still pointer's tip keeps its age and moves its clock time. The right edge is always now and the graph scrolls left under the pointer, so each column is a fixed age, while the time in that column advances a sample each tick. Both describe the column under the line. The line and tip follow the column, not the moment first pointed at.

- Stacked graphs, like CPU and memory, draw each series on top of the one below, in the order the backlog gives.

- Disk and network scale to their recent peak, and the peak is eased too. Disk can use a log Y axis.
	- The network graph's top is the tallest stack in view, spikes included, as drawn. It jumps up at once, so nothing is cut off, and eases down over about 2 seconds once the peak scrolls away or squeezes into a wider column. `render::Peak` has it, so disk and load can share it.
	- The top never goes under 10 kB/s, so the odd packet on a quiet line doesn't fill the graph.
	- With no labels on the graph, the tip gives the rates.
	- The disk graph does the same, with a floor of 100 kB/s, so a log file's flush doesn't fill it.
	- The load graph's floor is the number of CPUs, so it fills only once there's more work queued than CPUs to run it.
	- With Disk log scale on, a column's height is `ln(1 + v/u) / ln(1 + top/u)`, with `u` a thousandth of the floor, 100 B/s. So 1 kB/s under a 4 MB/s top is about a fifth of the height, where a straight scale gives it 1/4000. Written and read keep their shares of that height.
